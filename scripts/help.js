// `npm run help`: prints every package.json script with its one-line purpose; a script with no entry here fails the run so the map cannot drift.
import fs from 'node:fs'

const INFO = {
  help: 'this list',
  dev: 'Vite frontend only (no Rust)',
  'dev:debug': 'full app with AKI_DEBUG logs (= tauri dev)',
  tauri: 'tauri CLI wrapper (use: npm run tauri dev)',
  build: 'Vite frontend build only',
  preview: 'preview the Vite build',
  'build:app': 'arm64 .app + artifact rename (Finder reveal unless NO_REVEAL=1)',
  'build:rmad': 'arm64 .dmg',
  'build:rmud': 'universal .dmg (release build)',
  'install:app': 'build + sign + replace /Applications/Aki Dev Sync.app (owner-local)',
  'lint:scripts': 'lint the remote shell scripts',
  'lint:simpleview': 'check the SimpleView boundary',
  'audit:ui': 'UI architecture audit',
  'test:statusline': 'Rust statusline tests',
  'test:ui-audit': 'UI audit tests',
  'test:replay': 'terminal replay hydration tests',
  'test:config': 'project config store tests',
  'gen-icon': 'regenerate icons from public/icon.png',
}

const scripts = Object.keys(JSON.parse(fs.readFileSync(new URL('../package.json', import.meta.url), 'utf8')).scripts)
const missing = scripts.filter((s) => !INFO[s])
const stale = Object.keys(INFO).filter((s) => !scripts.includes(s))
if (missing.length || stale.length) {
  console.error(`scripts/help.js out of sync with package.json - missing: ${missing.join(', ') || '-'}; stale: ${stale.join(', ') || '-'}`)
  process.exit(1)
}
const width = Math.max(...scripts.map((s) => s.length))
for (const s of scripts) console.log(`npm run ${s.padEnd(width)}  ${INFO[s]}`)
