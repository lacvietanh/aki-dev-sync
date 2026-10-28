#!/bin/bash
# RUN BETA: double-click to install or update Aki Dev Sync; README.png beside this file shows the steps. It signs the app with a stable local identity ("Aki Dev Sync Dev"), same as scripts/install-desktop.sh, so the permissions you grant survive every update.
set -Eeuo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
IDENTITY="Aki Dev Sync Dev"
PRODUCT="Aki Dev Sync"
APP_SRC="$HERE/${PRODUCT}.app"

pause() { echo; read -r -n 1 -s -p "Press any key to close..." || true; echo; }
fail() { echo "ERROR: $1" >&2; pause; exit 1; }
trap 'echo "ERROR: a step failed (line $LINENO)." >&2; pause' ERR

[ "$(uname -s)" = "Darwin" ] || fail "macOS only"
[ -d "$APP_SRC" ] || fail "${PRODUCT}.app not found next to this script ($APP_SRC)"

if [ -d "/Applications/${PRODUCT}.app" ]; then
  DEST="/Applications/${PRODUCT}.app"
elif [ -d "$HOME/Applications/${PRODUCT}.app" ]; then
  DEST="$HOME/Applications/${PRODUCT}.app"
elif [ -w /Applications ]; then
  DEST="/Applications/${PRODUCT}.app"
else
  DEST="$HOME/Applications/${PRODUCT}.app"
fi

EXECUTABLE="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$APP_SRC/Contents/Info.plist")"
if [ "$(sysctl -n hw.optional.arm64 2>/dev/null)" != "1" ] && ! /usr/bin/file "$APP_SRC/Contents/MacOS/$EXECUTABLE" | grep -q x86_64; then
  fail "this build runs on Apple Silicon Macs only and this Mac has an Intel chip. Ask for the universal build (-uni.dmg)"
fi

# By hash, not name: two certificates with the same name make `codesign --sign <name>` fail as ambiguous.
identity_hash() {
  security find-identity -p codesigning 2>/dev/null | awk -v id="\"$IDENTITY\"" 'index($0, id) { print $2; exit }'
}

signed_by_identity() {
  codesign -d -r- "$1" 2>&1 | grep -q 'certificate leaf = H"'
}

ensure_identity() {
  if [ -n "$(identity_hash)" ]; then
    echo "identity ready: $IDENTITY"
    return
  fi
  echo "creating the signing identity: $IDENTITY (once)"
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
    || { rm -rf "$tmp"; fail "importing the signing identity failed"; }
  rm -rf "$tmp"
  [ -n "$(identity_hash)" ] || fail "the signing identity is not in your login keychain after import"
  echo "identity created: $IDENTITY"
}

echo "== 1/3: signing identity =="
ensure_identity
HASH="$(identity_hash)"

echo "== 2/3: sign a working copy =="
WORK="$(mktemp -d "${TMPDIR:-/tmp}/aki-dev-sync-install.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
ditto "$APP_SRC" "$WORK/${PRODUCT}.app"
xattr -cr "$WORK/${PRODUCT}.app" 2>/dev/null || true
codesign --force --deep --sign "$HASH" \
  --preserve-metadata=entitlements \
  "$WORK/${PRODUCT}.app" || fail "codesign failed"

echo "== 3/3: install to $DEST =="
mkdir -p "$(dirname "$DEST")"
[ -w "$(dirname "$DEST")" ] || fail "cannot write to $(dirname "$DEST")"
asks_once_more=0
if [ -e "$DEST" ]; then
  file_user="$(stat -f '%Su' "$DEST")"
  [ "$file_user" = "$(whoami)" ] || fail "$DEST belongs to $file_user. Run once: sudo chown -R $(whoami) \"$DEST\" then double-click RUN BETA again"
  signed_by_identity "$DEST" || asks_once_more=1
fi
killall "$EXECUTABLE" 2>/dev/null || true
sleep 0.5
rm -rf "$DEST"
ditto "$WORK/${PRODUCT}.app" "$DEST"
xattr -cr "$DEST" 2>/dev/null || true

signed_by_identity "$DEST" || fail "the installed app is not signed with $IDENTITY"
if xattr -r "$DEST" 2>/dev/null | grep -q 'com.apple.quarantine'; then
  fail "macOS still marks the installed app as downloaded"
fi

echo
echo "Done, opening ${PRODUCT} (${DEST})."
if [ "$asks_once_more" = "1" ]; then
  echo "macOS asks for folder permissions one last time. After that every update keeps them."
fi
open "$DEST" || true
pause
