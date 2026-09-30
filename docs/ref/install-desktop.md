# Local desktop install

Owner-local arm64 `.app` → `/Applications`, signed as `Aki Dev Sync Dev` so TCC grants survive rebuilds. Not a release, not a DMG, no sudo.

```sh
npm run install:app   # same as ./scripts/install-desktop.sh
```

When the owner says `build:app`, run this script. Do not stop at `npm run build:app` (produce-only; this script calls it). `SKIP_BUILD=1` reuses an existing bundle.

First run creates `Aki Dev Sync Dev` in the login keychain; later runs reuse it. The script preserves entitlements, replaces `/Applications/Aki Dev Sync.app`, and clears quarantine. If the destination is root-owned, it prints the one-time `chown` command and exits.

Apple/TCC mechanism: `~/.aki/akidevrule/docs/ref/macos-codesign-tcc.md`. Aki install pattern: `/Volumes/DEV/Frameworks/Tauri/AkiTauri/ref/install-desktop.md`.

The script builds with `NO_REVEAL=1`, so `scripts/post-build.js` does not `open -R` the bundle in Finder when the next step is the install; a plain `npm run build:app`/`build:rmud` still reveals it.
