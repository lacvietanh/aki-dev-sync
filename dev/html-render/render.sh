#!/bin/sh
# Usage: dev/html-render/render.sh <name> <width> <height> <out.jpg> — renders dev/html-render/<name>.html at 1:1 with headless Chrome, then converts to JPEG
set -e
here=$(cd "$(dirname "$0")" && pwd)
png=$(mktemp -t html-render).png
"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless=new --disable-gpu --hide-scrollbars --force-device-scale-factor=1 --allow-file-access-from-files --window-size="$2,$3" --screenshot="$png" "file://$here/$1.html" >/dev/null 2>&1
sips -s format jpeg -s formatOptions 90 "$png" --out "$4" >/dev/null
rm -f "$png"
