#!/usr/bin/env bash
set -euo pipefail

# `npm run build:app`: builds the arm64 .app unless the built one is current (NO_REVEAL=1: no Finder pop-up), signs it, quits the running app, replaces /Applications. FORCE=1 always builds.
# Lookup: docs/ref/install-desktop.md

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IDENTITY="Aki Dev Sync Dev"
PRODUCT="Aki Dev Sync"
DEST="/Applications/${PRODUCT}.app"

fail() {
  echo "ERROR: $1" >&2
  exit 1
}

[ "$(uname -s)" = "Darwin" ] || fail "install-desktop.sh is macOS-only"

identity_present() {
  security find-identity -p codesigning 2>/dev/null | grep -F -q "$IDENTITY"
}

ensure_identity() {
  if identity_present; then
    echo "identity ready: $IDENTITY"
    return
  fi

  echo "creating self-signed codesigning identity: $IDENTITY (once)"
  local tmp
  tmp="$(mktemp -d "${TMPDIR:-/tmp}/aki-dev-sync-codesign.XXXXXX")"
  printf '%s\n' \
    'basicConstraints=CA:FALSE' \
    'keyUsage=digitalSignature' \
    'extendedKeyUsage=codeSigning' > "$tmp/ext.cnf"

  /usr/bin/openssl genrsa -out "$tmp/key.pem" 2048
  /usr/bin/openssl req -new -key "$tmp/key.pem" -out "$tmp/csr.pem" -subj "/CN=${IDENTITY}"
  /usr/bin/openssl x509 -req -in "$tmp/csr.pem" -signkey "$tmp/key.pem" \
    -out "$tmp/cert.pem" -days 3650 -extfile "$tmp/ext.cnf"
  /usr/bin/openssl pkcs12 -export -inkey "$tmp/key.pem" -in "$tmp/cert.pem" \
    -out "$tmp/cert.p12" -passout pass:tmp -name "$IDENTITY"
  security import "$tmp/cert.p12" -P tmp -A \
    -T /usr/bin/codesign -T /usr/bin/security \
    || { rm -rf "$tmp"; fail "security import of $IDENTITY failed"; }
  rm -rf "$tmp"

  identity_present || fail "identity '$IDENTITY' not in the login keychain after import"
  echo "identity created: $IDENTITY"
}

echo "== 1/4: codesigning identity =="
ensure_identity

TARGET_ROOT="${CARGO_TARGET_DIR:-$REPO_ROOT/src-tauri/target}"
APP_SRC="$TARGET_ROOT/aarch64-apple-darwin/release/bundle/macos/${PRODUCT}.app"
# Signing and xattr -cr leave Info.plist untouched, so its mtime is the time of the last build.
APP_STAMP="$APP_SRC/Contents/Info.plist"
# Everything that ends up in the bundle: the in-app changelog reads CHANGELOG.md, and the Rust crate embeds the files outside src-tauri listed here.
SOURCES=(src src-tauri public share index.html vite.config.js package.json package-lock.json CHANGELOG.md scripts/tauri-runner.js scripts/sync-version.js scripts/get-antigravity-usage.sh scripts/get-claudecode-usage.sh scripts/provision-claudecode.sh ':(exclude,glob)src/**/*.md' ':(exclude,glob)src-tauri/**/*.md')

app_version() { /usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP_STAMP" 2>/dev/null || true; }

build_reason() {
  if [ "${FORCE:-}" = "1" ]; then echo "FORCE=1"; return 0; fi
  if [ ! -f "$APP_STAMP" ]; then echo "no built .app yet"; return 0; fi
  local want f
  want="$(node -p "require('$REPO_ROOT/package.json').version")"
  if [ "$(app_version)" != "$want" ]; then echo "built .app is $(app_version), package.json is $want"; return 0; fi
  while IFS= read -r -d '' f; do
    if [ "$REPO_ROOT/$f" -nt "$APP_STAMP" ]; then echo "$f changed after the last build"; return 0; fi
  done < <(git -C "$REPO_ROOT" ls-files -co --exclude-standard -z -- "${SOURCES[@]}")
  return 0
}

echo "== 2/4: arm64 .app =="
reason="$(build_reason)"
if [ -n "$reason" ]; then
  echo "building ($reason): npm run build:rmaa"
  (
    cd "$REPO_ROOT"
    NO_REVEAL=1 npm run build:rmaa
  ) || fail "npm run build:rmaa failed"
else
  echo "build skipped: the built .app is $(app_version) and newer than every source file (FORCE=1 rebuilds)"
fi
[ -d "$APP_SRC" ] || fail "built .app not found at $APP_SRC"

echo "== 3/4: sign $APP_SRC =="
codesign --force --deep --sign "$IDENTITY" \
  --preserve-metadata=entitlements \
  "$APP_SRC" || fail "codesign failed (no sudo — identity must be in the login keychain)"
xattr -cr "$APP_SRC" 2>/dev/null || true

echo "== 4/4: replace $DEST =="
if [ -e "$DEST" ]; then
  owner="$(stat -f '%Su' "$DEST")"
  me="$(whoami)"
  if [ "$owner" != "$me" ]; then
    fail "$DEST is owned by $owner. Once: sudo chown -R $me $DEST — then re-run without sudo"
  fi
fi
# Started from the app's own terminal, quitting the app hangs up this shell and closes its output: the swap must still finish.
trap '' HUP
was_running=""
killall "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$APP_SRC/Contents/Info.plist")" 2>/dev/null && was_running=1
sleep 0.5
rm -rf "$DEST"
ditto "$APP_SRC" "$DEST"
xattr -cr "$DEST" 2>/dev/null || true
[ -z "$was_running" ] || open "$DEST" || true

{
  echo "installed: $DEST"
  echo "signed as: $IDENTITY"
  codesign -dv --verbose=2 "$DEST" 2>&1 | grep -E '^(Authority|Identifier|Signature)='
} || true
