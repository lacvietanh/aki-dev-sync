# Local desktop install

Owner-local arm64 `.app` → `/Applications`, signed as `Aki Dev Sync Dev` so TCC grants survive rebuilds. Not a release, not a DMG, no sudo.

```sh
npm run build:app             # = ./scripts/install-desktop.sh
FORCE=1 npm run build:app     # build even when the built .app is current
npm run build:rmaa            # the .app only: nothing signed, nothing installed, the running app untouched
```

`build:app` runs four steps: create or reuse the `Aki Dev Sync Dev` identity in the login keychain, build the arm64 `.app` (`NO_REVEAL=1 npm run build:rmaa`, so Finder does not pop up), sign it with that identity keeping its entitlements, then quit the running app and replace `/Applications/Aki Dev Sync.app`. An app that was running is reopened on the new build. If the destination is root-owned, it prints the one-time `chown` command and exits.

The build step is skipped, and the run only signs and installs, when the built `.app` is current: its version equals `package.json` and its `Contents/Info.plist` is newer than every git-tracked or untracked-not-ignored file in the list `SOURCES` in `scripts/install-desktop.sh`: `src/`, `src-tauri/`, `public/`, `share/` (Markdown excluded under `src/` and `src-tauri/`), `index.html`, `vite.config.js`, `package.json`, `package-lock.json`, `CHANGELOG.md` (the in-app changelog), the build wrappers `scripts/tauri-runner.js` and `scripts/sync-version.js`, and the three `scripts/*.sh` the Rust crate embeds. Signing and `xattr -cr` do not touch `Info.plist`, so its time is the time of the last build. The script prints the reason it builds (`src/App.vue changed after the last build`) or that it skipped. A source file deleted without any other change is not detected; `FORCE=1` covers that.

**It quits the running app**, and every in-app terminal with it. Started from one of those terminals or from the BUILD button, the script ignores the hang-up and still finishes the swap.

The bundle id is written in one place, `src-tauri/tauri.conf.json` (`aki.devsync`): `codesign` reads it from the built app's `Info.plist`, and no script repeats it.

Apple/TCC mechanism: `~/.aki/akidevrule/docs/ref/fact-macos-codesign-tcc.md`. Aki install pattern: `/Volumes/DEV/Frameworks/Tauri/AkiTauri/ref/install-desktop.md`.

## DMG to hand to a person

```sh
npm run build:rmad:friend   # arm64
npm run build:rmud:friend   # universal (Intel + Apple Silicon)
```

Output: `src-tauri/target/{aarch64|universal}-apple-darwin/release/bundle/dmg/Aki-DevSync-Installer-v{version}.{HHMM}-{arm|uni}.dmg`. It is the DMG Tauri builds, with its background and window, plus two files from `scripts/installer/`: `READ ME.txt` centred above the two icons and `Install Aki Dev Sync.command` centred below them.

`FRIEND=1` (set by both scripts) makes `scripts/post-build.js` give the DMG the `-Installer-` name and call `scripts/inject-dmg-file.js`: it copies the two files in, lets Finder create their icon records, then writes all four icon positions straight into the volume's `.DS_Store`, because Finder alone sometimes saves the window a few points off. Two Finder windows open during the build (Tauri's layout, then this step); leave them alone until they close.

The recipient double-clicks the `.command`: it creates the `Aki Dev Sync Dev` identity on their Mac, signs the app, installs it to `/Applications` and launches it, so the grants they give the app survive the next DMG they receive. It uses only tools that ship with macOS, so Xcode and the Command Line Tools are not needed. On an Intel Mac the arm64 DMG stops with a message asking for the universal build. `READ ME.txt` (Vietnamese and English) covers the macOS block on first open and what the installer changes.

Not a release artifact: the GitHub Release carries the `build:rmud` `.dmg`. In this app that release DMG carries the same installer and readme (it replaced an ad-hoc helper whose signature changed on every update, so folder grants were lost each time); `FRIEND=1` only changes the name.
