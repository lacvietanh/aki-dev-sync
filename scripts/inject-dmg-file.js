import { execSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'
import os from 'node:os'

// Tauri's macOS.dmg bundler has no "extra files" option, so we convert the built DMG to read-write, copy the file in, and reseal it.
export function injectFileIntoDmg(dmgPath, filePath) {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'dmg-inject-'))
  const rwDmg = path.join(tmpDir, 'rw.dmg')
  const mountPoint = path.join(tmpDir, 'mnt')
  fs.mkdirSync(mountPoint)

  execSync(`hdiutil convert ${JSON.stringify(dmgPath)} -format UDRW -o ${JSON.stringify(rwDmg)}`, { stdio: 'inherit' })
  execSync(`hdiutil attach ${JSON.stringify(rwDmg)} -mountpoint ${JSON.stringify(mountPoint)} -nobrowse -noautoopen`, { stdio: 'inherit' })
  try {
    const dest = path.join(mountPoint, path.basename(filePath))
    fs.copyFileSync(filePath, dest)
    fs.chmodSync(dest, 0o755)
  } finally {
    execSync(`hdiutil detach ${JSON.stringify(mountPoint)} -force`, { stdio: 'inherit' })
  }

  fs.rmSync(dmgPath)
  execSync(`hdiutil convert ${JSON.stringify(rwDmg)} -format UDZO -o ${JSON.stringify(dmgPath)}`, { stdio: 'inherit' })
  fs.rmSync(tmpDir, { recursive: true, force: true })
}
