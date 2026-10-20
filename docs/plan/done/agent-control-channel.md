# Agent control channel — execution plan

Executes `docs/research/agent-control-channel.md` § Decision. Owner approved on 2026-10-20 ("làm gì thì làm đi"). Done 2026-10-20, unreleased.

## Steps

- [x] **Dev isolation**
  - `scripts/tauri-runner.js` gives `tauri dev` the identifier `aki.devsync.dev`, the relay port 1431 in the CSP, and (agent mode) a hidden, unfocused main window.
  - `app_paths::app_data_dir()` returns `~/.aki/devsync-dev` in debug builds, seeded once with the installed app's `projects.json`; the legacy-dir migration runs in release builds only.
  - `web_server::PORT` is 1431 in debug builds; the host bridge reads the port from `get_companion_status` instead of a JS constant.
- [x] **Control socket** (`src-tauri/src/control.rs`, debug builds only)
  - `<data dir>/control.sock`, mode 0600, one JSON request per line → one JSON reply per line.
  - Ops: `status`, `remote.on`, `remote.off`, `remote.status`, `eval`.
  - `AKI_DEVSYNC_AGENT=1` sets the macOS activation policy to Prohibited and stops the single-instance callback from showing the window.
- [x] **CLI**: `scripts/devsync-ctl.mjs`, `npm run dev:agent`, `npm run ctl -- <op> [arg]`, entries in `scripts/help.js`.
- [x] **Docs**: `docs/ref/agent-control.md` (how to use it), `docs/arch/settings-and-state.md` (dev data dir), `CHANGELOG.md` `[Unreleased]`.
- [x] **Verify at runtime**
  - Frontmost app is unchanged across the `dev:agent` launch.
  - `ctl status` answers; `ctl eval` returns `document.title`.
  - `ctl remote on` returns a code.
  - A Chrome profile under akimcp opens `http://127.0.0.1:1431`, pairs, and shows the project list.
  - Agent mode also needs `backgroundThrottling: disabled`: a hidden webview is suspended and stops answering the relay (research § Amendments).

## Found while verifying

- The companion received every mirrored key except the project list: the mirror encoder rejected an object reached twice (`hooks` aliased by `targets.<host>.hooks`) as a cycle. Fixed in `src/services/mirror.js`; CHANGELOG `[Unreleased]` § Fixed.
- The installed app has the same default throttling policy, so a minimized or hidden release window may stop serving the phone. Not changed here: it is a release behaviour change.
