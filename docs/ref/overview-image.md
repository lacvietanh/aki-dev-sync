# The overview image

One image shows the app on every page about it: `dev/html-render/overview.jpg` (2560×1440, English text) is the source of truth, rendered from `dev/html-render/overview.html`. The README, akitao.com and app.akinet.me all show these bytes.

Every app screen in it is a mock, never a capture of the real app: `dev/html-render/mock/app.html` draws each screen with the app's own stylesheets and invented data, so nothing private reaches the image and no running app is needed.

| Piece | What it holds |
|---|---|
| `overview.html` | the 1920×1080 layout: header, the push → agent → pull → commit loop strip, the benefit rail, and one `.card` per screen (an iframe of `mock/app.html`, scaled by `--s`, cropped at `--cx`/`--cy` inside a `--fw`×`--fh` window) |
| `mock/app.html?view=…` | one screen per view: `main`, `menu`, `wide` (right-dock terminal), `phone`, `sync` (Sync Changes popover), `open` (Open popover), `customizer`, `config`, `git`, `tasks` |
| `mock/views.js` | the markup, one function per real template (named in its comment), using the template's real class names |
| `mock/data.js` | the one invented story every screen tells: project `storefront`, Claude Code fixing it on the SSH host `devbox`, 12 files to pull |
| `mock/build-css.mjs` | writes `mock/sfc.css` (gitignored) from every component's `<style>` block in `src/`, so the mocks wear the current styles |
| `mock/mock.css` | what a static page must fake: frozen animations, the menu held open, xterm's canvas as text |

| Step | Command |
|---|---|
| Rebuild and copy to the sites | `dev/html-render/publish-overview.sh` (rebuilds `sfc.css`, renders at 2× in headless Chrome, scales to 2560 wide with `sips`, copies to both sites) |
| Look at one screen | open `dev/html-render/mock/app.html?view=<view>` in Chrome started with `--allow-file-access-from-files` |
| After a UI change | styles follow on the next rebuild; a changed template needs the matching function in `mock/views.js` changed by hand, then check the card's crop still ends at a whole row |

The script copies the same bytes to `app.akinet.me` (`public/img/articles/aki-dev-sync/aki-dev-sync-overview.jpg`) and `akitao.com` (`public/pj/aki-dev-sync-shots/overview.jpg`). Each site then needs its own commit and push, which deploys it. Edit the image only here: a copy edited on a site is overwritten by the next rebuild.

Every claim in the image's copy must hold in `README.md` or `src/components/modals/IntroModal.vue`; the invented numbers must agree across screens (the statusline repeats the second quota slot, the 12 files appear in the table, the terminal and the Sync Changes popover).
