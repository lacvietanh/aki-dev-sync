// `npm run help`: prints every package.json script with its one-line purpose; a script with no entry here fails the run so the map cannot drift.
import fs from 'node:fs'

const INFO = {
  help: 'this list: the install command first, the build-name key last',
  dev: 'frontend only, in the browser (no app window, no Rust)',
  'dev:debug': 'run the full app window from source, with debug logs',
  tauri: 'Tauri CLI (npm run tauri dev = run the full app window from source)',
  build: 'build the frontend only (no app); build:app and the DMG builds run it for you',
  preview: 'serve the built frontend in the browser',
  'build:app': 'INSTALL on this Mac: build the arm64 .app (skipped when nothing changed; FORCE=1 rebuilds) + sign + quit the running app + replace /Applications/Aki Dev Sync.app',
  'build:rmaa': 'build the arm64 .app without installing it, to check a change builds; the installed app is untouched (shows it in Finder unless NO_REVEAL=1)',
  'build:rmad': 'arm64 .dmg (Apple Silicon only), not installed',
  'build:rmud': 'universal .dmg (Intel + Apple Silicon): the release file',
  'build:rmad:friend': '.dmg to hand to a person with an Apple Silicon Mac: the app plus a double-click installer and a readme',
  'build:rmud:friend': '.dmg to hand to a person with any Mac (Intel or Apple Silicon): the app plus a double-click installer and a readme',
  'lint:scripts': 'check the shell scripts the app runs on remote hosts still work under plain sh; run after editing one',
  'lint:simpleview': 'check the simple terminal view stays text-only (no xterm, no cursor handling); run after editing it',
  'audit:ui': 'report hardcoded colours/sizes and duplicated CSS rules in the UI; run before a UI cleanup',
  'test:statusline': 'test the status-line script the app installs for Claude Code / Antigravity; run after changing it',
  'test:ui-audit': 'test that audit:ui itself reports correctly; run after editing audit:ui',
  'test:replay': 'test that in-app terminal tabs restore their content after a reload; run after terminal changes',
  'test:config': 'test reading and saving per-project settings; run after changing project settings code',
  'gen-icon': 'rebuild every app icon from public/icon.png; run after replacing that image',
}

const scripts = Object.keys(JSON.parse(fs.readFileSync(new URL('../package.json', import.meta.url), 'utf8')).scripts)
const missing = scripts.filter((s) => !INFO[s])
const stale = Object.keys(INFO).filter((s) => !scripts.includes(s))
if (missing.length || stale.length) {
  console.error(`scripts/help.js out of sync with package.json - missing: ${missing.join(', ') || '-'}; stale: ${stale.join(', ') || '-'}`)
  process.exit(1)
}
const width = Math.max(...scripts.map((s) => s.length))
console.log('Install Aki Dev Sync on this Mac: npm run build:app\n')
for (const s of scripts) console.log(`npm run ${s.padEnd(width)}  ${INFO[s]}`)
console.log('\nbuild:<mode><os><arch><pack>: mode r release · os m macOS · arch a arm64, u universal · pack a .app, d .dmg · :friend = the DMG plus an installer, for another person')
