# Driving the dev app by code

How an agent or script runs Aki Dev Sync from source beside the installed app and controls it without touching the owner's screen. Design and reasoning: `docs/research/agent-control-channel.md`.

## Start and stop

| Step | Command |
|---|---|
| Start, hidden and unable to take focus | `npm run dev:agent` (run it in the background; the first build compiles Rust for minutes) |
| Ready when this answers | `npm run ctl -- status` (a killed instance can leave a stale `control.sock`; the next launch replaces it) |
| Stop | `pkill -f "target/debug/aki-dev-sync"`, then end the `npm run dev:agent` process |

What a debug build has of its own, so the installed app is never touched:

| Thing | Debug build | Installed app |
|---|---|---|
| Bundle identifier (single-instance socket) | `aki.devsync.dev` | `aki.devsync` |
| Data dir | `~/.aki/devsync-dev` (first launch copies `projects.json` from `~/.aki/devsync`) | `~/.aki/devsync` |
| Companion relay port | 1431 | 1421 |
| Control socket | `~/.aki/devsync-dev/control.sock`, mode 0600 | none |

Agent mode also sets `backgroundThrottling: disabled` on the window: WebKit otherwise suspends a hidden webview's tasks, so the host stops answering the relay and a paired browser receives nothing until something wakes the page.

`npm run tauri dev` and `npm run dev:debug` get the same isolation, but with a visible window.

## Operations

`npm run ctl -- <op> [arg]` prints the reply's `data` as JSON, or `error: …` with exit code 1. The wire format on the socket is one JSON object per line: `{"id":1,"op":"status"}` → `{"id":1,"ok":true,"data":…}`.

| Op | Does | Returns |
|---|---|---|
| `status` | nothing | version, identifier, data dir, Remote Control state |
| `remote.on` | switches Remote Control on with a fresh pairing code | `enabled`, `pairing_code`, `pairLinkToken`, `port`, `urls` |
| `remote.off` | switches it off and drops paired sessions | same shape |
| `remote.status` | nothing | same shape |
| `eval "<JS function body>"` | runs the body in the app window, e.g. `return document.title` | the returned value (synchronous only; a Promise comes back as `{}`) |

## Companion test from a browser

1. `npm run ctl -- remote.on` and read `pairing_code`.
2. Open `http://127.0.0.1:1431` in a Chrome profile driven over CDP (akimcp `chrome_launch`), not a window on the owner's screen.
3. Enter the code; the page then runs the same UI the phone gets.
