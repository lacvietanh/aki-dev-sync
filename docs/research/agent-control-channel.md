# Agent control channel — letting AIs and scripts drive the app by code

**Start time:** 2026-10-20

## Initial purpose

The owner asked for a way for AIs and scripts to operate Aki Dev Sync the way apps with an MCP server can be operated, at least for a dev build: the trigger was an agent that could not run the companion test (backlog ledger #1) without someone clicking ☰ → Remote Control and reading the 6-digit code off the Mac, and that then fell back to screenshot-and-click GUI automation, which stole the owner's focus (now banned in `CLAUDE.md`).

Context at the time: release 1.33.1 installed and running all day with the owner's in-app terminals (quitting it is forbidden, `CLAUDE.md`); `tauri` 2.11.3; `tauri-plugin-single-instance` 2; macOS-only shipping.

## Strategy

`/akithink` self-run, three rounds until the decision stopped moving: goal chain → facts read from the code and the vendored crates → options → critique (steelman, attack, inversion, pre-mortem, second-order) → decision. Every fact below was read from source or run, never assumed.

## Checklist

- [x] Map every way into the running app today (HTTP routes, IPC, single-instance, files re-read at runtime)
- [x] Read `tauri-plugin-single-instance` macOS source for how a second launch behaves
- [x] Read `tauri-utils`, `tao`, `tauri` for window focus and launch activation controls
- [x] Survey existing Tauri MCP bridges and read the most-used one's source (`tauri-plugin-mcp-bridge` 0.13.1)
- [x] Critique and converge

## Result

### R1. The installed app has no external control surface

- The relay on `127.0.0.1:1421` / LAN has two routes, `/ws` and `/pair` (`src-tauri/src/web_server.rs` `.route(...)`), and answers 503 while Remote Control is off, so it cannot be used to switch itself on.
- Every other action is a `#[tauri::command]` reachable only from the app's own webview IPC (88 entries in `src-tauri/src/lib.rs` `generate_handler!`).
- `companion-server.json` (on/off + ingress) is read only at startup (`web_server.rs` `init`), so editing it needs a restart.
- The single-instance callback ignores the forwarded args and only shows and focuses the main window (`lib.rs`).

### R2. A dev build cannot run beside the installed app today

- `tauri-plugin-single-instance` on macOS derives its socket from the bundle identifier: `/tmp/<identifier with . and - as _>_si.sock`. A launch that can connect to that socket notifies the running instance and calls `std::process::exit(0)` (`platform_impl/macos.rs` `init`). Dev and release share `aki.devsync`, so `npm run tauri dev` while the release app runs exits at once and focuses the release window.
- The relay port is a constant, `PORT: u16 = 1421` (`web_server.rs`), reserved by `scripts/tauri-runner.js` `RELAY_PORT`. Two instances would collide on it.
- The data dir is one fixed funnel, `app_paths::app_data_dir()` → `~/.aki/devsync`, with no override. Two instances would share `projects.json`, notes and `companion-devices.json`, and the last writer wins.
- A stale `/tmp/aki_devsync_devcheck_si.sock` (no listener) shows that a build with a different identifier has run here before, so changing the identifier is enough to separate the two instances.

### R3. Launching any GUI process activates it

`tao` 0.35 sets `activate_ignoring_other_apps: true` by default and applies it at launch (`platform_impl/macos/app_delegate.rs`, `app_state.rs`). A window `"focus": false` (`tauri-utils` `WindowConfig::focus`, "Whether the window will be initially focused") only covers the window. `tauri` 2.11 exposes `set_activation_policy` (`app.rs`) and `set_dock_visibility`. Whether `ActivationPolicy::Accessory` plus a hidden window keeps the owner's focus at launch is **unverified**: it can only be settled by launching it and checking the frontmost app before and after.

### R4. Existing bridge: `tauri-plugin-mcp-bridge` (hypothesi/mcp-server-tauri)

MIT, 0.13.1, updated 2026-10-13, ~379k downloads. A Rust plugin opens a WebSocket that a Node MCP server (`@hypothesi/tauri-mcp-server`) drives. It has 20 tools: DOM snapshot, click and type via JS events, `execute_js`, native WKWebView screenshot, IPC execute and monitor, logs, window management. Read from the crate source:
- `Config::default()` binds **`0.0.0.0`**, base port 9223; `localhost_only()` exists but is opt-in (`src/config.rs`).
- The WebSocket server has no auth and no Origin check (`src/websocket.rs`), so any LAN host, or any web page that can open a WebSocket to localhost, can reach it.
- `ipc_execute_command` and `execute_js` reach every command, and this app's commands include PTY write, i.e. a shell as the user.
- Input is synthetic JS (`src/bridge.js`), screenshots are native, and there is an explicit `focus` window op (`websocket.rs`). It does not steal focus unless asked.
- The README registers it under `#[cfg(debug_assertions)]` only.
- A smaller crate, `DaveDev42/tauri-plugin-mcp` (Unix socket / named pipe, 14 tools), exists. Its details were not verified: the directory page carries no README content.

### R5. The companion client is already a full UI client

With Remote Control on and a device paired, the companion SPA in any browser drives the same frontend through the relay. A Chrome profile under akimcp's CDP tools (`devtools_eval`, `chrome_interact`) is therefore a complete, focus-free UI driver for every remote-capable flow. The missing pieces are only "switch Remote Control on" and "hand me the pairing code".

### Critique (round 3)

- **Steelman adopting `mcp-bridge` now:** 20 maintained tools, native screenshots, zero custom code, and it is what "like an MCP app" literally means. **Against:** an unauthenticated, browser-reachable socket into an app that can write to a shell, a 0.x dependency with weekly churn, and 90% of its surface unused. Its value grows only if we need visual checks (screenshots) that our own channel lacks. That is the reopen trigger, not today's need.
- **Steelman a release-build control channel ("apps have MCP"):** an agent could trigger sync or read badges in daily use. **Against:** agents already run `git`, `rsync` and `ssh` directly, and no daily-use operation has been named that only the app can do. A release channel is a permanent attack surface with no current consumer (YAGNI).
- **Attack the favored design:** a hand-written op set can drift into a second API that duplicates every command (`pattern.A1`). Mitigation: typed ops only where a Rust function already exists, and everything UI-side goes through `eval` into the real frontend, never re-implemented.
- **Inversion / pre-mortem:** six months on, it failed because (a) the dev instance still could not run beside the release app (R2 skipped), so nothing was ever used; (b) the dev instance clobbered the owner's `projects.json` via the shared data dir; (c) launching the dev instance still stole focus (R3 unverified and ignored). All three are prerequisites in the decision below, not follow-ups.
- **Second-order effects:**
  - A new dev identifier gets its own TCC grants (`tauri.B7`): expect one Files & Folders prompt for `/Volumes/DEV` per new signature.
  - A separate dev data dir starts empty unless seeded.
  - Pairing on the dev instance never touches the owner's paired devices, which is the desired effect.

## Decision

**Action**, in three steps, each verifiable alone. It becomes a plan doc when scheduled (owner decision D5).

1. **Dev isolation (prerequisite):**
   - `tauri dev` merges a dev config with its own identifier (`aki.devsync.dev`) and product name.
   - `app_data_dir()` honours an override env var, and dev defaults to `~/.aki/devsync-dev`, seeded once with a copy of `projects.json`.
   - The relay port is read from one env-overridable source shared by `web_server.rs`, `tauri-runner.js` and `src/constants/protocol.js`.
   - An agent-run mode starts with the window hidden and `ActivationPolicy::Accessory`. Verify R3 by comparing the frontmost app before and after launch.
2. **Control socket, debug builds only:**
   - `control.sock` inside the instance's data dir, mode 0600, newline-delimited JSON `{id, op, args}` → `{id, ok, data | error}`.
   - Typed ops call the existing Rust functions: `status`, `remote.on`, `remote.off`, `remote.status`. `remote.on` returns the pairing code, the pair link and the URLs.
   - `eval` runs JS in the main webview via `WebviewWindow::eval` and returns its result through a debug-only reply command, which also needs its capability entry (`tauri.B2`).
   - A Unix socket is used because no browser page or LAN host can open it, and same-user processes already hold everything it exposes.
3. **CLI:** `scripts/devsync-ctl.mjs` (`npm run ctl -- remote on`). The companion test then runs as: `ctl remote on` → akimcp Chrome profile opens the dev relay URL → pair → multi-tab reconnect checks, with no owner action and no focus change.

**Rejected:**

| Option | Why rejected |
|---|---|
| GUI automation (screenshot + coordinate clicks, System Events, `open -a`) | Steals the owner's focus; banned in `CLAUDE.md` |
| Loopback HTTP control port | Reachable from any browser page; needs token + DNS-rebinding defence |
| Deep link | `open` activates the app; one-way |
| Single-instance args | One-way, and the second launch must reach the release app's socket |
| WebDriver / `tauri-driver` | No macOS support |
| `tauri-plugin-mcp-bridge` now | R4 |
| A release-build control channel now | No consumer |

**Reopen if:**
- a concrete daily-use operation needs the installed app → design a release channel, still a Unix socket, opt-in;
- visual verification (screenshots, layout) becomes a recurring need → adopt `tauri-plugin-mcp-bridge` with `localhost_only()` and `#[cfg(debug_assertions)]`, or add a native snapshot op to the socket;
- R3 shows the dev launch still steals focus → find another launch strategy before step 2.

**Cross-references:**
- `CLAUDE.md` (the no-GUI line)
- `docs/plan/backlog.md` (ledger #1 and the ingress line)
- `src-tauri/src/lib.rs` (single-instance callback)
- `docs/arch/remote-ingress.md` (the relay this channel switches on)
