import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { execSync } from 'node:child_process'
import { injectIntoDmg } from './inject-dmg-file.js'

const BRAND_SLUG = 'Aki-DevSync'
// Every DMG of this app carries the installer and its readme, the release one included. FRIEND=1 (build:rmad:friend / build:rmud:friend) only gives the DMG to hand to a person a name that can never be taken for a release artifact.
const FRIEND = process.env.FRIEND === '1'

const appRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const version = JSON.parse(fs.readFileSync(path.join(appRoot, 'package.json'), 'utf8')).version
const tauriConf = JSON.parse(fs.readFileSync(path.join(appRoot, 'src-tauri/tauri.conf.json'), 'utf8'))
const productName = tauriConf.productName
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
  if (process.platform !== 'darwin' || process.env.NO_REVEAL === '1') return
  try { execSync(`open -R ${JSON.stringify(absPath)}`) } catch { /* Finder is optional */ }
}

function renameNative(dmgDir, nativeName, arch) {
  const from = path.join(dmgDir, nativeName)
  if (!fs.existsSync(from)) return false
  const to = path.join(dmgDir, `${BRAND_SLUG}${FRIEND ? '-Installer' : ''}-v${version}.${buildNum}-${arch}.dmg`)
  fs.renameSync(from, to)
  console.log(`Renamed: ${from} → ${to}`)
  const { windowSize, appPosition, applicationFolderPosition } = tauriConf.bundle.macOS.dmg
  const centerX = Math.round(windowSize.width / 2)
  injectIntoDmg(to, [
    { src: path.join(appRoot, 'scripts/installer/READ ME.txt'), at: [centerX, 70] },
    { src: path.join(appRoot, `scripts/installer/Install ${productName}.command`), at: [centerX, 288] },
  ], [
    { name: `${productName}.app`, at: [appPosition.x, appPosition.y] },
    { name: 'Applications', at: [applicationFolderPosition.x, applicationFolderPosition.y] },
  ])
  console.log('Injected: installer + readme')
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
