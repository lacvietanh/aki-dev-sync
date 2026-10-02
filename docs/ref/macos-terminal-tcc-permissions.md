# macOS TCC — in-app terminal (Aki Dev Sync)

Bundle `aki.devsync`. In-app PTY: app → zsh → CLI (`claude`, `agy`, `find`, `fd`, `rg`). TCC charges this **bundle**, not Terminal.app.

Apple switches, rebuild/CDHash, sticky denial: `~/.aki/akidevrule/docs/ref/fact-macos-codesign-tcc.md` (concise rule: `tauri.B7`). Local `.app` install: [install-desktop.md](install-desktop.md).

## Folder prompt for this app

Pop-up `"Aki Dev Sync" would like to access files in your Documents folder` → **Allow**. Record: System Settings → Privacy & Security → Files and Folders → Aki Dev Sync.

Don't Allow is sticky. Recovery (then relaunch and Allow again):

```sh
tccutil reset SystemPolicyDocumentsFolder aki.devsync
```

Or `tccutil reset All aki.devsync`.

Developer Tools does not stop this dialog.

## After a rebuild, grants look gone

Not a `tccutil` loop. Run `npm run build:app` (= `./scripts/install-desktop.sh`); it reuses `Aki Dev Sync Dev` and replaces the launched `.app` with the same designated requirement. Mechanism: `~/.aki/akidevrule/docs/ref/fact-macos-codesign-tcc.md`.

## Spawn scope

TCC pop-ups are often an unscoped walk. Bind every search to the workspace:

- ❌ `find ~ -name "*.rs"`, `fd keyword /Users/aki/`, `grep -r "pattern" $HOME/`
- ✅ relative `./` or the project path (e.g. `/Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/`)

If the walk must start above the project:

```sh
fd --exclude "Documents" --exclude "Desktop" --exclude "Downloads" --exclude "Library" <pattern> <path>
```

Claude Code / AGY CLI: give the workspace path. Do not let an agent discover from `$HOME` or `/`.
