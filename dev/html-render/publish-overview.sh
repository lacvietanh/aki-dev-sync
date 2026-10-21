#!/bin/sh
# Usage: dev/html-render/publish-overview.sh — renders overview.html to overview.jpg (the SSoT) and copies it to the two websites that show it
set -e
here=$(cd "$(dirname "$0")" && pwd)
"$here/render.sh" overview 1920 1080 "$here/overview.jpg"
cp "$here/overview.jpg" /Volumes/DEV/www/app.akinet.me/public/img/articles/aki-dev-sync/aki-dev-sync-overview.jpg
cp "$here/overview.jpg" /Volumes/DEV/www/akitao.com/public/pj/aki-dev-sync-shots/overview.jpg
