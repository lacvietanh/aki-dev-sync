#!/bin/bash
DIR="$(cd "$(dirname "$0")" && pwd)"
SRC="$DIR/Aki Dev Sync.app"
DEST="/Applications/Aki Dev Sync.app"

if ! rm -rf "$DEST" 2>/dev/null || ! cp -R "$SRC" "$DEST" 2>/dev/null; then
  osascript -e "do shell script \"rm -rf '$DEST' && cp -R '$SRC' '$DEST'\" with administrator privileges with prompt \"Aki Dev Sync needs your password to install.\""
fi

xattr -cr "$DEST"
codesign --force --deep -s - "$DEST"
open "$DEST"
