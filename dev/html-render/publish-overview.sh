#!/bin/sh
# Usage: dev/html-render/publish-overview.sh — rebuilds the mock styles from src/, renders overview.html to overview.jpg (the SSoT, 2560×1440) and copies it to the two websites that show it
set -e
here=$(cd "$(dirname "$0")" && pwd)
node "$here/mock/build-css.mjs"
"$here/render.sh" overview.html 1920 1080 "$here/overview.jpg" 2560
cp "$here/overview.jpg" /Volumes/DEV/www/app.akinet.me/public/img/articles/aki-dev-sync/aki-dev-sync-overview.jpg
cp "$here/overview.jpg" /Volumes/DEV/www/akitao.com/public/pj/aki-dev-sync-shots/overview.jpg
