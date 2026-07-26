# Remote Control (companion) — feature

Control the Mac app from a phone (or any browser) on the same LAN or over Tailscale. The Mac
webview stays the single source of truth; the phone is a thin mirror that shows the same state and
sends intents back over one WebSocket. Full architecture: `docs/plan/done/remote-control.md`.

> **Status:** foundation + host entry point + **companion control (R-2)** + pairing gate shipped.
> The Rust relay (`src-tauri/src/web_server.rs`) must be built on the Mac before it works
> end-to-end. A phone can now push, pull, flip DRY, refresh and toggle sync-check; the full pairing
> modal (QR + device management) is still Wave 2. Every companion state and failure case is
> enumerated in **"Companion states — every case"** below.
>
> **1.20.0 adds** (code complete, unverified on a Mac): decision **dialogs are mirrored** so a
> confirm triggered from the phone appears and is answerable on both screens; **task/note/reorder
> edits from a phone now stick** (PERSIST-1); and a shared **in-app terminal** the phone can type
> into. See "1.20.0 — the two classes that were still broken" below, plus
> `docs/plan/1.20.0-terminal-and-remote-sync.md`.

## How a user turns it on and pairs a phone

1. Open the hamburger **menu** (top-left) → **Remote Control** → flip the toggle to **On**.
   - Under the hood: `start_companion_server()` flips the relay's `enabled` gate on and mints a
     fresh 6-digit pairing code. The TCP listener itself was bound once at app start.
2. The menu now shows:
   - a **Pair code** (6 digits), and
   - one or more **IP:PORT** rows — `lan` (e.g. `http://192.168.1.23:1421`) and, if Tailscale is up,
     `tailscale` (e.g. `http://100.x.y.z:1421`). Click a row to copy it.
3. On the phone, open one of those URLs in a browser. First time only, it asks for the code → type
   the 6 digits → the Mac mints a **per-device token** the phone stores in `localStorage`.
4. Done. That phone reconnects silently after that (no code, no QR) — across app restarts on both
   ends — until you revoke it.

Flip the toggle **Off** (`stop_companion_server()`) to cut every live phone immediately and reject
new joins; the code and addresses disappear from the menu so a stale code can't be read off it.

The menu reflects the **relay's** real state, not the window's: reloading the app while remote control
is on shows it still on, with the same code (`get_companion_status()`).

## Where it lives in code

| Piece | File |
| :-- | :-- |
| Host menu section (toggle, code, IP:PORT rows) | `src/components/AppHeader.vue` |
| Host control logic (start/stop/URLs) | `src/composables/useRemoteControl.js` |
| Role detection (`isHost`), WS transport, pairing fetch, token clear | `src/services/bridge.js` |
| State mirror (host→companion) | `src/services/mirror.js` |
| `action()` wrapper — host runs it / companion sends an intent | `src/services/action.js` |
| Host-side intent dispatch + store-glob registry | `src/services/intents.js` |
| Companion-actionable store entry points (push/pull/DRY/refresh) | `src/store/remoteActions.js` |
| `invoke` wrapper — companion half (real IPC on host, RPC send on companion) | `src/utils/tauri.js` |
| `invoke` responder — host half (runs the real IPC, replies `invoke_result`) | `src/services/hostInvoke.js` |
| Pairing gate (code entry / connecting) | `src/components/PairingGate.vue` + `src/composables/useCompanionPairing.js` |
| Wire-protocol constants | `src/constants/protocol.js` |
| Rust relay + pairing + address discovery | `src-tauri/src/web_server.rs` |
| Role stamp | `src/boot/roleStamp.js` (first import in `main.js`) |
| Mirrored decision dialogs (1.20.0) | `src/store/dialogStore.js` + `src/components/DialogHost.vue` |
| In-app terminal — PTY backend (1.20.0) | `src-tauri/src/pty.rs` |
| In-app terminal — xterm surface / role wiring / host relay | `src/components/TerminalView.vue` · `src/composables/usePtyTerminal.js` · `src/services/ptyBridge.js` |
| In-app terminal — panel tab + the "open project in the in-app terminal" gesture | `src/composables/useTerminalPanel.js` (module-level refs, deliberately **not** in `src/store/` so the mirror does not sync one screen's tab choice onto the other) |

> **Why `action()` is split from `intents.js` (REGISTRY-1).** `remoteActions.js` and
> `syncCheckStore.js` live in `src/store/` and need `action()` at their definition site. If they
> imported it from `intents.js` — which does `import.meta.glob('../store/*.js')` — that would form a
> hard `store → intents → (glob) → store` import cycle evaluated at bootstrap, the classic cause of
> a blank page on the phone. `action.js` depends only on `bridge` + `protocol` (neither touches a
> store), so `store → action → bridge` has no back-edge. The glob-based dispatch registry stays in
> `intents.js`, which **no store imports**, and is built lazily on first dispatch for good measure.

## Security model (summary)

- **Off means off.** While the toggle is off, `:1421` serves *nothing* to the LAN — not the page, not
  (in dev) the proxied dev server. Everything returns 503 until the user turns it on.
- The server binds all interfaces but is **useless without a token**. A stranger on the same wifi
  hitting `:1421` gets only the pairing page; they need the 6-digit code shown on the Mac screen.
- **10 wrong codes in a row disable remote control** and wipe the code — the guess space of a 6-digit
  code is small enough to walk otherwise. Turn it back on from the menu for a fresh code.
- Tokens are **per-device and revocable** (`list_paired_devices()` / `revoke_device(id)` — Wave 2 UI).
  Revoking one device leaves every other paired device untouched (scoped-clear rule).
- LAN and Tailscale use the **same** token — the token is the identity, not the network path.
- This is **not multi-tenant**: every paired device is a mirror of the one Mac session, not a
  separate account (SCOPE-1).

## Platform / build notes

- **One address in dev and release (PORT-1).** The phone always uses `http://<ip>:1421`. In a release
  build axum serves the embedded frontend on 1421; in `npm run tauri dev` axum reverse-proxies the
  page to the Vite dev server (localhost) on the same 1421, so DX matches production and hot-reload
  still works. See `docs/plan/done/remote-control.md` §7.2.
- macOS-only, like the rest of the app. Tailscale is only *noticed* (via `if-addrs` reading the
  `100.64/10` CGNAT range) and offered as an address — never installed or configured for the user.
- **Minimal PWA (install as a standalone app).** `index.html` carries the favicon (`/icon.png`),
  the apple-* / `mobile-web-app-capable` meta tags and a `manifest.webmanifest` (`display:
  standalone`, 192/512 icons); `public/sw.js` is a network-passthrough service worker (caches
  nothing — this tool needs the live Mac; no stale-shell risk) registered from `main.js`
  **companion-only + secure-context-only**. Standalone by platform: **iOS** "Add to Home Screen"
  works over plain http (apple meta tags); **desktop Chrome** "Create shortcut → Open as window"
  works via the favicon; **Android Chrome** needs **HTTPS** (e.g. a Tailscale funnel) for a true
  standalone WebAPK — over plain LAN http it stays a browser shortcut. Bundled into the `.app` only
  on a Mac rebuild (`index.html`/`public/` are Vite-built into the embedded frontend).
- **HTTPS over Tailscale (unlocks the Android standalone PWA) — in-app toggle.** axum serves plain
  http on 1421, so `https://100.x:1421` fails (no TLS there). `tailscale serve` terminates TLS with a
  real cert for `<machine>.<tailnet>.ts.net` on 443 and proxies to `http://127.0.0.1:1421` (page +
  `/ws` + `/pair`, same origin). This is wired into the app as a menu toggle:
  - Rust: `web_server.rs` `get_tailscale_https` / `set_tailscale_https` (async + `spawn_blocking`;
    resolve the `tailscale` binary via explicit install paths then PATH; `serve --bg http://127.0.0.1:PORT`
    to enable, `serve --https=443 off` to disable). Registered in `lib.rs`. An enable that fails
    because the tailnet has no HTTPS certs returns tailscale's own error (with the admin URL) verbatim.
  - Frontend: `useRemoteControl.js` `httpsEnabled`/`httpsUrl`/`toggleHttps` + the **HTTPS (PWA)** row
    in `AppHeader.vue`, shown only when tailscale is present. Turning Remote Control **off also turns
    serve off** — nothing left proxying in the background.
  - `bridge.js` `wsUrl()`/`pairDevice()` are **origin-relative** (`wss://<host>/ws` from an https
    page, `ws://<host>:1421/ws` from http) — a hardcoded `ws://…:1421` would be blocked as mixed
    content from the https origin. The device token is per-origin, so the `.ts.net` origin needs a
    one-time re-pair.
  - **The one manual step the app can't do:** enabling HTTPS certs for the tailnet is an admin-console
    account setting (MagicDNS + HTTPS on) — done once. `scripts/tailscale-serve-https.sh` remains as a
    CLI fallback but is no longer required once the toggle ships.
- The host role stamp is `if (window.__TAURI_INTERNALS__) window.__AKI_ROLE__='host'` in
  `src/boot/roleStamp.js`, imported first in `main.js` (a module, not an inline script — the host
  CSP `script-src 'self'` blocks inline). A phone browser never has `__TAURI_INTERNALS__`, so it
  defaults to companion. See `docs/plan/done/remote-control.md` §9 (S-1) for why this replaced the
  originally-planned Rust init-script.

## Companion states — every case

Exhaustive map of what a phone/browser can be doing and what it shows. `ready = isHost ||
connectionState === 'open'`; the whole dashboard mounts behind `v-if="ready"` (READY-1) and the
`PairingGate` covers everything while `!ready`. **The Vite HMR websocket (`ws://<ip>:1421/?token=…`,
no `/ws` path, no `role=`) is unrelated to any of this** — its "connection failed → fallback →
connected" console line is a benign dev-only warning about Vite's own hot-reload channel, not the
pairing socket (`ws://<ip>:1421/ws?role=companion&token=…`).

### Role detection

| # | Situation | Result |
| :-- | :-- | :-- |
| A1 | Tauri webview (the Mac app) | `__AKI_ROLE__='host'` → `isHost` true → `ready` always true → dashboard, gate never shown. |
| A2 | Phone/browser served by axum | no `__TAURI_INTERNALS__` → companion (the safe default). |
| A3 | Phone mis-detected as host | **impossible by construction** — the stamp requires Tauri's injected global, which a browser never has. |

### Connection lifecycle (companion) — `idle → connecting → {open | unpaired | closed | error}`

| # | Situation | What the phone shows / does |
| :-- | :-- | :-- |
| B1 | No stored token | connect sends an empty token → relay closes **4001** → `unpaired` → **code-entry form**. |
| B2 | Valid token, remote control **on** | socket `open` → `ready` → dashboard mounts and mirrors host state. |
| B3 | Stale / revoked token, remote **on** | relay closes **4001** → token is **cleared** (ROBUST-1) → code-entry form, no reconnect loop on the dead credential. |
| B4 | Remote control **off** on the Mac | relay closes **4001** "disabled" → code-entry form + "Not paired, or remote control is off on the Mac." |
| B5 | Relay unreachable (app closed / wrong IP) | WS errors or closes with a non-4001 code → `error`/`closed` → **code form still shown** with "No connection to the Mac — is the app running?"; auto-reconnect with backoff. |
| B6 | Socket hangs at `connecting` (no TCP response) | **code form still shown** (this was the old trap — a token used to hide it and leave the user stuck on "Connecting…"). |
| B7 | Was `open`, then dropped | `ready` flips false → dashboard unmounts → gate reappears with the form → backoff reconnect → on `open`, dashboard returns. |
| B8 | Ping timeout (15s interval, 5s grace) | bridge force-closes the socket → same path as B7. |

### Host ↔ relay (the invisible half — a companion can't see this)

The host runs its OWN websocket to the relay (`ws://127.0.0.1:1421/ws?role=host`). If that link is
down, `host_tx` is null on the relay, so a companion connects and pairs fine but **never receives an
`init`/`delta`** (empty dashboard) and its intents are dropped. Diagnose from the phone console:
`[bridge] socket OPEN (role=companion)` with **no** following `[mirror] applied init — N key(s)`
means exactly this — the host isn't feeding the relay.

| # | Situation | Result |
| :-- | :-- | :-- |
| H1 | Host WS connected | on each companion join the relay sends `companion-connected` → host `broadcastFull()` → companion applies `init`. |
| H2 | **Host WS rejected by its own loopback guard (fixed)** | The listener used to bind `[::]` (dual-stack); the host dials IPv4 `127.0.0.1`, which arrived as the IPv4-mapped `::ffff:127.0.0.1`. `Ipv6Addr::is_loopback()` matches only `::1`, so the guard wrongly returned 4001 → host never connected → every companion sat empty. **Fixed by binding IPv4-only `0.0.0.0`** (`serve_forever` in `web_server.rs`): inbound addresses are then plain IPv4 and `is_loopback()` is correct directly — the whole IPv4-mapped class is gone. Cost: a pure-IPv6 Tailscale peer is no longer served (the IPv6 companion URL was dropped); Tailscale still works over its `100.x` IPv4. Plus the host now **reconnects on a 4001** instead of going `unpaired` (`bridge.js`) — the host has no pairing, so a 4001 there is always a fault to retry. |
| H3 | Host WS drops mid-session | host bridge reconnects with backoff; on reopen its `connectionState` watch re-broadcasts a full snapshot to everyone. |

### Pairing submit (code entry)

| # | Situation | Result |
| :-- | :-- | :-- |
| C1 | Correct 6-digit code | `POST /pair` 200 → token stored → `connect()` → `open`. |
| C2 | Wrong code | `POST /pair` 401 → "Invalid pairing code"; form stays, input cleared. |
| C3 | Fewer than 6 digits | blocked client-side; the Pair button stays disabled. |
| C4 | 10 wrong codes in a row | relay disables remote control + wipes the code (429). The phone must wait for the user to re-enable it on the Mac for a fresh code. |

### Readiness gate & premature work (READY-1)

| # | Situation | Result |
| :-- | :-- | :-- |
| D1 | Companion not yet `open` | dashboard components **do not mount** → none of their `onMounted` `invoke`s fire over a closed socket (no `send dropped, socket not open` burst, no `check_for_updates` throw). |
| D2 | Companion `open` | dashboard mounts once; the `init` snapshot has already populated every mirrored store. |
| D3 | Host | dashboard always mounted; `onHostBoot`/`hostInterval` run the boot sequence and timers only here. |

### Bootstrap / load order (REGISTRY-1)

| # | Situation | Result |
| :-- | :-- | :-- |
| E1 | App bundle loads on the phone | `store → action → bridge` has no import cycle, so no eval-order/TDZ crash → the app actually renders (this is what a blank page + Vite-only console logs would have meant). |
| E2 | Host dispatch registry | built lazily on the first inbound intent, from the store glob in `intents.js`, which no store imports. |

### Control actions (R-2)

| # | Situation | Result |
| :-- | :-- | :-- |
| F1 | Companion clicks PUSH / PULL / DRY / Refresh / global-refresh / sync-check power | sends an `intent` (project **id** + args) → host runs the real action → the resulting state change mirrors back to every screen. |
| F2 | Host clicks the same buttons | `action(fn) === fn` on the host → byte-identical to before R-2 (the only change is an id→object resolve the host does anyway). |
| F3 | Companion triggers the native "Upload (select files)" | the file picker and its overwrite confirm (`useSync.js:325`, plain `Swal.fire`) run **on the Mac** — host-only by design, being bound to the native macOS dialog. |
| F3b | Companion triggers a `--delete` sync | as of 1.20.0 the typed `--delete` confirm is a **mirrored** dialog (`useSync.js:159` `askConfirm` → `dialogStore`/`DialogHost.vue`), answerable from the phone, typed value re-validated on the host. Before 1.20.0 it was host-only like F3. The normal no-dialog push/pull/DRY path is fully phone-usable either way. |
| F4 | Companion clicks while the host is mid-sync | the PUSH/PULL fieldset is disabled via the mirrored `syncing` flag; `startSync` also guards host-side, so a racing intent is a no-op. |
| F5 | Intent with an unknown key reaches the host | logged as a warning, never executed. |

### Invoke RPC — Seam N, companion → host (both halves wired)

The companion half (`utils/tauri.js`) always shipped: `invoke()` sends `{t:'invoke', cmd, args, id}`
and awaits a matching `invoke_result`. The **host half** (`services/hostInvoke.js`, booted host-only
in `initRemote`) is what was missing — until it existed, every companion `invoke()` (`get_agent_usage`,
`log_frontend`, git-detail reads, `check_for_updates`, path resolves) hung until the 20 s watchdog in
`bridge.request()` fired: the `NO invoke_result from the host … seam N: the host-side invoke responder
is not wired` errors on the phone console.

| # | Situation | Result |
| :-- | :-- | :-- |
| G1 | Companion calls `invoke(cmd, args)` | host `initHostInvoke` runs the real Tauri IPC and replies `{t:'invoke_result', id, ok}` → the companion Promise resolves with the real value. |
| G2 | The host command **throws** | host replies `{id, err}` with the real Tauri error text → the companion Promise rejects with that message (not a generic "rejected"), logged as `[invoke] companion RPC "<cmd>" failed`. |
| G3 | Void command (returns `undefined`) | `JSON.stringify` drops the `undefined` `ok`, so the frame carries neither `ok` nor `err` → companion resolves `undefined`, correct. |
| G4 | Host reply never comes (relay drop, host mid-restart) | the 20 s `bridge.request()` watchdog rejects with a named error rather than hanging forever. Long ops don't rely on this path — `run_sync`/delete-preview go through an **intent** (F1), not an invoke, so 20 s only ever bounds quick reads. |

### Manual recovery (if a phone ever gets wedged)

Clear the stored token and reload — the code-entry form always returns:

```
localStorage.removeItem('aki-companion-device-token')
```

With ROBUST-1 this should never be *necessary* (a rejected token self-clears), but it is the
one-liner to force a clean re-pair.

---

## Control inventory — every button, cross-check matrix (SCOPE-1)

The single reference to check every control against while editing. Goal (plan §0): *what the Mac
shows, the browser shows; what the browser does, the Mac does* — one session mirrored, **no
per-device view**. A control is correct only if it is right in BOTH columns.

**The one root-cause pattern behind every BUG below** (plan §0, ACT-1): a mutation that
`mutate-local-copy + invoke('save_*')`. On the companion that mutates the *phone's* copy of a
mirrored ref (which never travels back — the mirror is host→companion only) and RPC-persists to the
Mac's **disk**, but never mutates the Mac's **reactive** ref — so nothing mirrors back and the Mac UI
is stale until a reload re-reads disk. The fix is always the same: the data mutation must be an
`action()` living in a `store/*.js` module (so the host runs it, mutates host reactive state, and the
change mirrors to every screen). UI-only side effects (Toast, closeModal, field normalization) stay
local to the clicker.

**Mechanism legend** — `MIRROR` state is a mirrored `store/*.js` ref (auto H→C) · `ACTION` gesture
wrapped in `action()` (C→H intent, runs on host) · `RPC` `invoke()` runs the command on the Mac
one-shot · `LOCAL` browser-only effect (clipboard / open-link-in-clicker / transient modal-open).

**Verdict** — `OK` correct both ways · `BUG` a data change that does not reach the Mac's live UI ·
`LOCAL-OK` an interaction surface that is legitimately local to the clicker (opens a modal the phone
user then interacts with; the *save inside* is what must be an ACTION) · `RPC-OK` a one-shot host
side effect with no shared state to mirror.

### Project table (`ProjectTable.vue`) + config (`useProjectConfig.js`)

| Control | Handler:line | Mechanism | Verdict |
| :-- | :-- | :-- | :-- |
| PUSH / PULL | `requestSync(p.id,dir)` :239/266 | ACTION `remoteActions` | OK |
| DRY checkbox | `setDryRun(p.id,v)` :252 | ACTION | OK |
| Per-project refresh | `requestRefresh(p.id)` :217 | ACTION | OK |
| Sync-check power | `toggleSyncCheck()` :30 | ACTION | OK |
| Edit config → Save | `saveConfig`→`remoteActions.applyProjectConfig` | ACTION (data) + LOCAL Toast/close | **FIXED** — host applies to its reactive `projects`, mirrors to every screen |
| New project | `createNewProject`→`saveConfig`→`applyProjectConfig` | native folder dialog (Mac) + ACTION save | **FIXED** (save path); folder picker still opens on the Mac (RPC-OK) |
| Remove project | `confirmRemove`→`remoteActions.requestRemoveProject`→`removeProject` | MIRRORED DIALOG + ACTION | **FIXED (1.20.0)** — the confirm is mirrored state answerable from either screen; the removal mutates the host's `projects` and mirrors out. The companion's own config modal does not self-close afterwards (R-1, `showConfigModal` still per-screen) |
| Open Git modal | `openGitModal(p)` :114 | RPC (git info) + LOCAL modal | LOCAL-OK |
| Open config modal | `openConfig(p)` :280 | LOCAL modal-open | LOCAL-OK |
| Toggle project log | `toggleProjectLog(p.id)` :275 | LOCAL (which log is shown) | LOCAL-OK |
| Open REPORT.html | `openReportHtml(p)` :147 | RPC (`resolve_report_html`+`macos_open`) | RPC-OK (opens on Mac) |
| Open IDE local/remote | `openIdeLocal/Remote` :160-202 | RPC | RPC-OK (opens on Mac) |
| Run DEV / BUILD | `runProjectDev/Command` :176/179 | RPC | RPC-OK (opens a Mac `Terminal.app` window, still not visible to a phone — redirecting these into the 1.20.0 in-app terminal needs per-project cwd + multi-session first, see the 1.20.0 plan T-6) |
| Upload (select files) | `openSelectDialog(p)` :207 | RPC native dialog (Mac) | RPC-OK (host-only, F3) — its overwrite confirm stays a host-local Swal on purpose, being bound to the native picker |
| Copy local/remote path | `copyLocalPath/RemotePath` :156/189 | LOCAL clipboard | LOCAL-OK |
| Open production URL | `openUrl(p.production_url)` :87 | RPC `macos_open` | RPC-OK |

### Header (`AppHeader.vue`)

| Control | Handler:line | Mechanism | Verdict |
| :-- | :-- | :-- | :-- |
| Refresh all | `handleRefresh`→`requestRefreshAll` :234 | ACTION | OK |
| Tier count 1 / 2 | `setTierCount(n)` :122/130 | MIRROR + ACTION | **FIXED** — `setTierCount` now an action |
| **Remote on/off toggle** | `toggleRemote` :66 | host relay control (`useRemoteControl`) | **REVIEW — companion toggling the server it rides on; should be host-only** |
| Update check | `triggerManualUpdateCheck` :19 | RPC + LOCAL modal | RPC-OK |
| Window presets width/place | `applyViewSafe/ComboSafe` :158-195 | RPC window API on Mac window | RPC-OK (controls the Mac window) |
| Pin / Minimize / Close | `togglePin/minimize/closeWin` :261-268 | RPC window API | RPC-OK (controls the Mac window) |
| SSH config → Save/Undo/Redo | `saveSshConfig`→`remoteActions.applySshHostsChange` | LOCAL modal/RPC file-write + ACTION (host reconcile) | **FIXED** — host re-reads `sshHosts`/undo-redo flags + migrates affected projects on its reactive state; the missing-host replacement dialog is decided host-side and, since 1.20.0, mirrored to the phone (`askConfirm` `kind: 'select'`) |
| Refresh-settings modal | `save`→`refreshStore.setRefreshSettings` :237 | LOCAL modal + ACTION (data) | **FIXED** — save routes through an action; host sets `refreshSettings`, re-drives Mac timers, mirrors back |
| Global note | note save via `noteStore.saveNote` :227 | LOCAL modal + ACTION (persist) | **FIXED** — `noteContent` moved into `store/noteStore.js` (mirrors H→C); save is an action that mutates it + writes disk on the host |
| Changelog / Update / Intro / Profile / Statusline modals | `show*Modal=true` :11-55 | LOCAL modal-open | LOCAL-OK |
| Copy remote URL | `copyRemoteUrl` :79 | LOCAL clipboard | LOCAL-OK |
| Open repo/donate links | `openLink(url)` :101/200/230 | RPC `macos_open` | RPC-OK |
| Install AkiClaudeDoc / SSH color | `installAkiClaudeDoc`/`enableSshTerminalColor` | RPC | RPC-OK |
| Remember-view toggle | `toggleRememberView` :146 | `useAppWindow` local pref | REVIEW (window pref of the Mac) |

### Usage (`AgentUsageSection.vue` / `AgentUsageSlot.vue` / `AgentUsage.vue`)

| Control | Handler:line | Mechanism | Verdict |
| :-- | :-- | :-- | :-- |
| Power AG / CC (any scope) | `monitor.toggle()`→`setMonitorEnabled(id, …)` (`usageMonitorStore`) | MIRROR + ACTION | **FIXED** — flags moved to store; toggle is an action. Since 1.20.0 one keyed map, one entry per `agentId@host` (`monitorId`), not four fixed source flags — "ccRemote" is no longer a thing |
| Remote host select | `@change=setSlotTarget(slotId,{remoteHost})` Slot:40-41 | MIRROR (`usageSlotStore`) + ACTION | **FIXED** — `:value`+`@change`→action, not v-model. Per **slot** since 1.20.0; `sshStore.selectedSshHost` is only the fallback |
| Reload / retry | `$emit('retry')`→`checkUsage`→`get_agent_usage` RPC | RPC | RPC-OK (refetches the phone's view) |
| Logout AG (IDE/CLI) | `logoutAntigravity`→`logout_antigravity*` RPC | Swal on phone + RPC | RPC-OK (logs out on Mac) |
| Open Antigravity | `handleIconClick`→`macos_open` RPC | RPC | RPC-OK |
| Tab LOCAL / REMOTE | `setSlotTarget(slotId,{scope})` Slot:6/13 | MIRROR (`usageSlotStore`) + ACTION | **FIXED (1.20.0)** — the slot's target became store state; the old "component-local + localStorage" REVIEW is settled |
| Source tab AG / CC | `setAgent()`→`setSlotTarget(slotId,{localAgent\|remoteAgent})` Slot:31/116 | MIRROR + ACTION | **FIXED (1.20.0)** — same |
| Account view dropdown | `select-account`→`selectAccount` | composable-local view | **REVIEW — which account is *shown*; currently local** |
| Email show/hide | `toggle-email`→`showEmail` Slot:112 | slot-local | **REVIEW — same** |

### 1.20.0 — the two classes that were still broken

Both are documented in full in `docs/plan/1.20.0-terminal-and-remote-sync.md` (§2, §3).

- **PERSIST-1 — a companion must never write the projects array.** `saveProjectsList()` was a bare
  `invoke('save_projects', {projects: projects.value})`, the one mutating persistence path not wrapped
  in `action()`. From a phone it shipped the *phone's* array to disk while the Mac's reactive
  `projects` stayed on the old value — so the next `broadcastFull()` (fired on **every** companion
  reconnect: screen lock, backgrounded tab, LAN blip) replayed the stale copy back over the edit.
  That is the "task note reverts after a while" report. Fixed by `remoteActions.applyTaskEdit(id, patch)`
  and `remoteActions.reorderProjects(orderedIds)`; the seven bare call sites (task add/toggle/remove,
  notes, task title, task detail, drag-reorder) now all route through them. `saveProjectsList` carries
  the invariant as a comment at its definition: it is a **host-side persist of the host's own state**
  and may only be reached from inside an action body. No guard was added inside it — by the time it
  runs the wrong array is already in hand, so a guard would police the symptom.
- **Dialogs are mirrored state (§3.4, designed in 1.19.0, built in 1.20.0).** Four decision dialogs
  moved out of host-local `Swal.fire` into `store/dialogStore.js` (`pendingDialog` ref + `resolveDialog`
  action + a host-side `askConfirm()` promise helper) rendered by `components/DialogHost.vue` on both
  screens: the typed `--delete` confirm, the preview-failed prompt, Remove Project, and the
  missing-SSH-host replacement picker. First-answer-wins via the id guard; the typed value travels with
  the answer and is **re-validated on the host**, so a phone cannot skip the check. No new frame type
  and no relay change — it rides the two existing seams. Still plain `Swal.fire` on purpose: the
  file-picker overwrite confirm (`useSync.js`, bound to the native macOS dialog, host-only by design),
  the Antigravity logout confirm (`AgentUsage.vue`, runs in the clicking screen's own handler), and
  every toast.

Still open after 1.20.0: a companion's **Config modal does not self-close** after a remove it triggered
(the removal itself succeeds and mirrors — only `editingProject`/`showConfigModal` are still per-screen
composable refs, R-1). Usage numbers are still the one area the mirror does not carry (each screen
fetches its own). `projectIcons` is still filled only at boot.

### Fix order — progress

- **DONE** — Config save (`applyProjectConfig`), Remove project (`removeProject`), New-project save path.
- **DONE** — `setTierCount` action; usage power toggles (now `usageMonitorStore` +
  `setMonitorEnabled`, one entry per `agentId@host`); remote host select (now per-slot,
  `usageSlotStore.setSlotTarget`).
- **DONE (same class)** — all three former TODOs, identical pattern (data mutation → an `action()` in
  a `store/*.js`):
  - **`saveSshConfig`/`undoSshConfig`/`redoSshConfig`** → `remoteActions.applySshHostsChange`. The RPC
    file writes stay on the clicker; the reactive reconcile (re-read `get_ssh_hosts` → set the
    mirrored `sshHosts`, refresh `hasSshUndo`/`hasSshRedo`, migrate any project pinned to a
    now-missing host + `saveProjectsList`) runs host-side. The many-to-many missing-host replacement
    dialog is **mirrored** since 1.20.0 (`remoteActions.js:247`, `askConfirm` `kind: 'select'`): it is
    decided host-side, as it always was, but is now visible and answerable from a companion screen
    too. `oldHosts` is read from the live host `sshHosts`, so nothing crosses the intent wire.
    useSsh/SshConfigModal no longer forward `saveProjectsList` (the action owns the save).
  - **Refresh-settings interval** → `refreshStore.setRefreshSettings`. `RefreshSettingsModal.save()`
    calls the action instead of writing `refreshSettings.value` directly; the host's existing deep
    `watch` persists to its localStorage and the change mirrors back.
  - **Global note** → `noteContent` moved from the composable into `store/noteStore.js` (so it mirrors
    H→C); `noteStore.saveNote` is the C→H action that mutates it + `write_global_note` on the host.
    `useGlobalNote` re-exports `noteContent` (importers unchanged) and keeps the transient
    `showGlobalNote`/`noteSaving`/debounce clicker-local.
- **REVIEW (product call, documented not guessed)** — `toggleRemote` (companion toggling the server
  it rides on → likely host-only); window/view controls (`applyView*`, pin/min/close → they act on
  the Mac window; fine as RPC); usage *view* state (tab LOCAL/REMOTE, source AG/CC, account-shown,
  email show/hide) — SCOPE-1 says one session mirrored, so these arguably should mirror the *shown*
  view across screens, but that means moving per-slot view refs into a store. Decide before wiring.
