import { execFileSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'
import os from 'node:os'

const PLACE_ICONS = `on run argv
  tell application "Finder"
    tell disk (item 1 of argv)
      open
      repeat with i from 2 to (count of argv) by 3
        set position of item (item i of argv) to {(item (i + 1) of argv) as integer, (item (i + 2) of argv) as integer}
      end repeat
      close
    end tell
  end tell
end run`

// Finder now and then saves the whole window a few points lower, or leaves a new file where it auto-placed it. Finder only has to create the position records; the coordinates are then written straight into the volume's .DS_Store.
export function pinIconPositions(dsStorePath, icons) {
  const store = fs.readFileSync(dsStorePath)
  for (const { name, at } of icons) {
    const nameLength = Buffer.alloc(4)
    nameLength.writeUInt32BE(name.length)
    const record = Buffer.concat([nameLength, Buffer.from(name, 'utf16le').swap16(), Buffer.from('Ilocblob'), Buffer.from([0, 0, 0, 16])])
    let found = 0
    for (let i = store.indexOf(record); i !== -1; i = store.indexOf(record, i + 1)) {
      store.writeUInt32BE(at[0], i + record.length)
      store.writeUInt32BE(at[1], i + record.length + 4)
      found++
    }
    if (!found) throw new Error(`no icon position record for "${name}" in ${dsStorePath}`)
  }
  fs.writeFileSync(dsStorePath, store)
}

// Tauri's macOS.dmg bundler has no "extra files" option, so the built DMG is converted to read-write, the files are copied in, and it is resealed. A file with `at: [x, y]` gets that icon position in the window Tauri laid out; `pinned` restates the positions of the icons already there. Finder is scripted without `activate`, so it never takes the keyboard.
export function injectIntoDmg(dmgPath, files, pinned = []) {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'dmg-inject-'))
  const rwDmg = path.join(tmpDir, 'rw.dmg')

  execFileSync('hdiutil', ['convert', dmgPath, '-format', 'UDRW', '-o', rwDmg], { stdio: 'inherit' })
  const attached = execFileSync('hdiutil', ['attach', rwDmg, '-mountrandom', '/Volumes', '-readwrite', '-noverify', '-noautoopen', '-nobrowse'], { encoding: 'utf8' })
  const mountPoint = attached.match(/\/Volumes\/\S+/)[0]
  try {
    for (const { src } of files) {
      const dest = path.join(mountPoint, path.basename(src))
      fs.copyFileSync(src, dest)
      fs.chmodSync(dest, fs.statSync(src).mode & 0o777)
    }
    const placed = files.filter((f) => f.at)
    if (placed.length) {
      execFileSync('osascript', ['-e', PLACE_ICONS, path.basename(mountPoint), ...placed.flatMap((f) => [path.basename(f.src), ...f.at.map(String)])], { stdio: 'inherit' })
      execFileSync('sync')
      pinIconPositions(path.join(mountPoint, '.DS_Store'), [...pinned, ...placed.map((f) => ({ name: path.basename(f.src), at: f.at }))])
    }
  } finally {
    execFileSync('hdiutil', ['detach', mountPoint, '-force'], { stdio: 'inherit' })
  }

  fs.rmSync(dmgPath)
  execFileSync('hdiutil', ['convert', rwDmg, '-format', 'UDZO', '-o', dmgPath], { stdio: 'inherit' })
  fs.rmSync(tmpDir, { recursive: true, force: true })
}
