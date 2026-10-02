#!/bin/bash
# Aki Dev Sync installer — double-click me; "READ ME.txt" beside this file covers a macOS block.
# Copies the app next to this file into /Applications and signs it with a self-signed identity ("Aki Dev Sync Dev"), same as scripts/install-desktop.sh, so folder permissions survive the next update.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
IDENTITY="Aki Dev Sync Dev"
PRODUCT="Aki Dev Sync"
APP_SRC="$HERE/${PRODUCT}.app"
DEST="/Applications/${PRODUCT}.app"

pause() { echo; read -r -n 1 -s -p "Press any key to close..." || true; echo; }
fail() { echo "ERROR: $1" >&2; pause; exit 1; }

[ "$(uname -s)" = "Darwin" ] || fail "macOS only"
[ -d "$APP_SRC" ] || fail "${PRODUCT}.app not found next to this script ($APP_SRC)"

EXECUTABLE="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$APP_SRC/Contents/Info.plist")"
if [ "$(sysctl -n hw.optional.arm64 2>/dev/null)" != "1" ] && ! /usr/bin/file "$APP_SRC/Contents/MacOS/$EXECUTABLE" | grep -q x86_64; then
  fail "this build runs on Apple Silicon Macs only and this Mac has an Intel chip — ask for the universal build"
fi

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

echo "== 1/3: codesigning identity =="
ensure_identity

echo "== 2/3: sign a working copy =="
WORK="$(mktemp -d "${TMPDIR:-/tmp}/aki-dev-sync-install.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
ditto "$APP_SRC" "$WORK/${PRODUCT}.app"
xattr -cr "$WORK/${PRODUCT}.app" 2>/dev/null || true
codesign --force --deep --sign "$IDENTITY" \
  --preserve-metadata=entitlements \
  "$WORK/${PRODUCT}.app" || fail "codesign failed"

echo "== 3/3: install to $DEST =="
if [ -e "$DEST" ]; then
  owner="$(stat -f '%Su' "$DEST")"
  me="$(whoami)"
  [ "$owner" = "$me" ] || fail "$DEST is owned by $owner. Run once: sudo chown -R $me \"$DEST\" then re-run this script"
fi
killall "$EXECUTABLE" 2>/dev/null || true
sleep 0.5
rm -rf "$DEST"
ditto "$WORK/${PRODUCT}.app" "$DEST"
xattr -cr "$DEST" 2>/dev/null || true

echo "installed: $DEST"
codesign -dv --verbose=2 "$DEST" 2>&1 | grep -E '^(Authority|Identifier|Signature)=' || true
open "$DEST" || true
pause
