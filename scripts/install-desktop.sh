#!/usr/bin/env bash
set -euo pipefail

# Lookup: docs/ref/install-desktop.md

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IDENTITY="Aki Dev Sync Dev"
PRODUCT="Aki Dev Sync"
BUNDLE_ID="aki.devsync"
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

echo "== 2/4: arm64 .app (npm run build:app) =="
if [ "${SKIP_BUILD:-}" = "1" ]; then
  echo "SKIP_BUILD=1 — using existing bundle"
else
  (
    cd "$REPO_ROOT"
    npm run build:app
  ) || fail "npm run build:app failed"
fi

TARGET_ROOT="${CARGO_TARGET_DIR:-$REPO_ROOT/src-tauri/target}"
APP_SRC="$TARGET_ROOT/aarch64-apple-darwin/release/bundle/macos/${PRODUCT}.app"
[ -d "$APP_SRC" ] || fail "built .app not found at $APP_SRC — run without SKIP_BUILD=1"

echo "== 3/4: sign $APP_SRC =="
codesign --force --deep --sign "$IDENTITY" \
  --identifier "$BUNDLE_ID" \
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
killall "$PRODUCT" 2>/dev/null || true
sleep 0.5
rm -rf "$DEST"
ditto "$APP_SRC" "$DEST"
xattr -cr "$DEST" 2>/dev/null || true

echo "installed: $DEST"
echo "signed as: $IDENTITY"
codesign -dv --verbose=2 "$DEST" 2>&1 | grep -E '^(Authority|Identifier|Signature)=' || true
