import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { execSync } from 'node:child_process'

const BRAND_SLUG = 'Aki-DevSync'

const appRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const version = JSON.parse(fs.readFileSync(path.join(appRoot, 'package.json'), 'utf8')).version
const productName = JSON.parse(fs.readFileSync(path.join(appRoot, 'src-tauri/tauri.conf.json'), 'utf8')).productName
const now = new Date()
const buildNum = process.env.BUILD_NUM || `${String(now.getHours()).padStart(2, '0')}${String(now.getMinutes()).padStart(2, '0')}`
const targetRoot = process.env.CARGO_TARGET_DIR || path.join(appRoot, 'src-tauri/target')

const SLOTS = [
  { rel: 'aarch64-apple-darwin/release/bundle/dmg', nativeArch: 'aarch64', arch: 'arm' },
  { rel: 'x86_64-apple-darwin/release/bundle/dmg', nativeArch: 'x86_64', arch: 'x64' },
  { rel: 'universal-apple-darwin/release/bundle/dmg', nativeArch: 'universal', arch: 'uni' },
  { rel: 'release/bundle/dmg', nativeArch: null, arch: null },
]

const NATIVE_ARCH = { aarch64: 'arm', x86_64: 'x64', universal: 'uni' }

function reveal(absPath) {
  if (process.platform !== 'darwin') return
  try { execSync(`open -R ${JSON.stringify(absPath)}`) } catch { /* Finder is optional */ }
}

function renameNative(dmgDir, nativeName, arch) {
  const from = path.join(dmgDir, nativeName)
  if (!fs.existsSync(from)) return false
  const to = path.join(dmgDir, `${BRAND_SLUG}-v${version}.${buildNum}-${arch}.dmg`)
  fs.renameSync(from, to)
  console.log(`Renamed: ${from} → ${to}`)
  reveal(to)
  return true
}

let found = false
for (const { rel, nativeArch, arch } of SLOTS) {
  const dmgDir = path.join(targetRoot, rel)
  if (!fs.existsSync(dmgDir)) continue
  if (nativeArch) {
    found = renameNative(dmgDir, `${productName}_${version}_${nativeArch}.dmg`, arch) || found
    continue
  }
  for (const [raw, short] of Object.entries(NATIVE_ARCH)) {
    found = renameNative(dmgDir, `${productName}_${version}_${raw}.dmg`, short) || found
  }
}

if (!found) {
  const appRels = [
    'aarch64-apple-darwin/release/bundle/macos',
    'x86_64-apple-darwin/release/bundle/macos',
    'universal-apple-darwin/release/bundle/macos',
    'release/bundle/macos',
  ]
  for (const rel of appRels) {
    const appDir = path.join(targetRoot, rel)
    if (!fs.existsSync(appDir)) continue
    for (const file of fs.readdirSync(appDir)) {
      if (!file.endsWith('.app')) continue
      const appPath = path.join(appDir, file)
      console.log(`Built .app: ${appPath}`)
      reveal(appPath)
      found = true
    }
  }
}

if (!found) {
  console.error(`No native ${productName}_${version}_*.dmg (or .app) under ${targetRoot}`)
  process.exit(1)
}
