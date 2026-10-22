#!/bin/sh
# Usage: dev/html-render/publish-images.sh — rebuilds the mock styles from src/, renders overview.html to overview.jpg (the SSoT, 2560×1440) and customizer.html to the Statusline Customizer image, and copies both to the two websites that show them
set -e
here=$(cd "$(dirname "$0")" && pwd)
app=/Volumes/DEV/www/app.akinet.me/public/img/articles/aki-dev-sync
akitao=/Volumes/DEV/www/akitao.com/public/pj/aki-dev-sync-shots
node "$here/mock/build-css.mjs"
"$here/render.sh" overview.html 1920 1080 "$here/overview.jpg" 2560
cp "$here/overview.jpg" "$app/aki-dev-sync-overview.jpg"
cp "$here/overview.jpg" "$akitao/overview.jpg"
"$here/render.sh" customizer.html 836 600 "$app/aki-dev-sync-statusline-customizer.jpg" 1254
cp "$app/aki-dev-sync-statusline-customizer.jpg" "$akitao/statusline-customizer.jpg"
