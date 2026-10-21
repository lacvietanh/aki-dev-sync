#!/bin/sh
# Usage: dev/html-render/render.sh <page> <width> <height> <out.jpg> [out-width] — renders dev/html-render/<page> (a query string is allowed) in a <width>×<height> window at 2× with headless Chrome, then scales it to <out-width> (default <width>) as JPEG
set -e
here=$(cd "$(dirname "$0")" && pwd)
tmp=$(mktemp -d -t html-render)
"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless=new --disable-gpu --hide-scrollbars --force-device-scale-factor=2 --allow-file-access-from-files --virtual-time-budget=6000 --user-data-dir="$tmp/profile" --window-size="$2,$3" --screenshot="$tmp/shot.png" "file://$here/$1" >/dev/null 2>&1
sips --resampleWidth "${5:-$2}" -s format jpeg -s formatOptions 90 "$tmp/shot.png" --out "$4" >/dev/null
rm -rf "$tmp"
