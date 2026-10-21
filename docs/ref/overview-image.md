# The overview image

One image shows the app on the websites: `dev/html-render/overview.jpg` (1920×1080) is the source of truth, built from `dev/html-render/overview.html` out of three screenshots in `dev/html-render/shots/` (main window, header menu, Statusline Customizer). It carries no text, so the Vietnamese and English pages share it.

| Step | Command |
|---|---|
| Rebuild and copy to the sites | `dev/html-render/publish-overview.sh` (Chrome headless, then `sips` to JPEG) |
| After a UI change | replace the matching file in `dev/html-render/shots/` with a fresh screenshot, then rebuild |

The script copies the same bytes to `app.akinet.me` (`public/img/articles/aki-dev-sync/aki-dev-sync-overview.jpg`) and `akitao.com` (`public/pj/aki-dev-sync-shots/overview.jpg`). Each site then needs its own commit and deploy. Edit the image only here: a copy edited on a site is overwritten by the next rebuild.
