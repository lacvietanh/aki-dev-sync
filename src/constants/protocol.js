// Wire-protocol contract — SSoT, shared by the Rust relay (src-tauri/src/web_server.rs) and
// every JS seam (services/bridge.js, services/mirror.js, services/intents.js, utils/tauri.js).
//
// FROZEN per docs/plan/done/remote-control.md §13 — do not diverge, do not add per-feature frame
// types. Both the host and the companion import these same constants so a typo can't produce
// two sides that silently fail to recognize each other's frames.
//
// @docs docs/plan/done/remote-control.md §13 (wire protocol contract)

// The relay's WS/HTTP port (dev and prod alike — see §7.1a "Dev vs prod" table).
export const REMOTE_PORT = 1421

// ── `to` / `from` — OPTIONAL ADDRESSING FIELDS ON EXISTING FRAMES (1.21.1) ────────────────────
//
// NO NEW FRAME TAG WAS ADDED FOR THIS; §13's freeze holds. A frame carrying neither field routes
// exactly as it always has (broadcast), so an older companion bundle — which sets neither and reads
// neither — is unaffected in both directions.
//
//   `to`   (host -> relay, optional string): the ONE companion CONNECTION this frame is for. The
//          relay's `dispatch` enqueues it into that connection's outbox only; absent means every
//          companion.
//   `from` (relay -> host, optional string): the connection a companion frame arrived on. STAMPED BY
//          THE RELAY from its own connection counter, never supplied by the companion and never shown
//          to one, so it can be neither forged nor guessed. The host echoes it back as `to` on the
//          reply. Treat the value as OPAQUE — echo it, never parse it, never construct one.
//
// A CONNECTION, NOT A DEVICE, and the difference is not academic: two browser tabs on one paired
// phone are two connections, two outboxes, and two independent request counters that BOTH start at 1.
// Addressing by device id leaves both of the bugs below fully alive within that one device — the
// reply for tab A resolves tab B's unrelated id-1 call, and the joining tab's scrollback replay is
// duplicated into every tab's outbox, which is more than the relay's per-companion byte budget can
// hold (INVARIANT R, src-tauri/src/web_server.rs). Cross-device isolation falls out of it for free.
//
// ONLY TWO SENDERS SET `to`, and widening that list is a protocol decision, not a local convenience:
//   * the scrollback replay (services/ptyBridge.js) — a `reset: true` pty_output belongs to the ONE
//     connection that just joined or just drained a congested queue. Broadcasting it cleared and
//     resized the xterm of every OTHER connected screen, mid-command.
//   * `invoke_result` (services/hostInvoke.js) — a reply to one id on one PAGE's PRIVATE request
//     counter, which starts at 1 on every page. Broadcast, two pages with an id-1 call in flight each
//     resolved the other's answer: silent wrong data, not a timeout.
//
// EVERYTHING ELSE STAYS A BROADCAST ON PURPOSE. Live `pty_output` chunks (one shared PTY, every
// screen shows the same bytes), `pty_exit` / `pty_resize` (shared liveness and size — addressing them
// would rebuild the 1.20.0 §2.4 desync bug), `delta` (mirror state; a mirrored confirm dialog
// answerable from any screen is a designed property), `init` (idempotent) and `ping`/`pong`.

// Frame `t` values — §13.2.
export const FRAME_INIT = 'init'                   // host -> companion: full snapshot on join
export const FRAME_DELTA = 'delta'                 // host -> companion: changed mirrored keys
export const FRAME_INTENT = 'intent'                // companion -> host: run this action
export const FRAME_INVOKE = 'invoke'                // companion -> host: RPC, expects a reply
export const FRAME_INVOKE_RESULT = 'invoke_result'  // host -> companion: reply to one `invoke` id
export const FRAME_PING = 'ping'                    // both: liveness
export const FRAME_PONG = 'pong'                    // both: liveness reply

// relay -> host: a companion just authenticated and joined (and again whenever a companion whose
// queue was coalesced has drained it — same frame, same handling). Carries `{ id }` and no app state
// — the relay stays content-blind (§13.6); this is awareness of its OWN connections, not state.
//
// `id` IS THE CONNECTION KEY, the same opaque value the relay stamps as `from`, and it is what
// services/ptyBridge.js echoes back as `to` on the scrollback replay. It is emitted PER CONNECTION
// and answered with one full replay each, so it has to name a connection: a device id here fanned one
// join's replay into every outbox that device had open. services/mirror.js consumes the tag alone
// (broadcast a full `init`) and ignores the field.
export const FRAME_COMPANION_CONNECTED = 'companion-connected'

// In-app terminal (docs/plan/done/1.20.0-terminal-and-remote-sync.md §4, docs/plan/remote-views-
// roadmap.md § Terminal View). Deliberately NOT routed through FRAME_DELTA/FRAME_INTENT
// (services/mirror.js / services/intents.js) — raw terminal bytes are a firehose and do not fit
// a JSON-diffed state model; these are new top-level frames on the same socket instead.
//
// ── `tab_id` ON ALL FOUR PTY FRAMES ──────────────────────────────────────────────────────────
// The terminal is multi-tab (one PTY per tab, src-tauri/src/pty.rs). NO NEW FRAME TYPE WAS ADDED
// FOR IT — this file's own freeze rule stands: the four tags below are unchanged and each simply
// carries one more field, `tab_id` (number, ABSENT or undefined means tab 0).
//
// WHY THE FIELD IS NOT OPTIONAL IN PRACTICE, even though it defaults: the relay is content-blind
// and COALESCES `pty_output` BY TAG ALONE (src-tauri/src/web_server.rs drops queued pty_output
// frames under congestion without ever looking inside them). Nothing between the host's PTY and the
// companion's xterm knows which tab a frame belongs to except this field, so a frame that omitted
// it would default to 0 and write tab A's bytes into tab B's terminal — corruption that renders as
// a shell apparently going haywire, with no error anywhere. Every sender stamps it; every receiver
// filters on it (`(frame.tab_id ?? 0) !== myTabId → ignore`).
//
// THE TAB LIST ITSELF IS NOT A FRAME. It is ordinary shared state and rides the normal mirror
// (src/store/terminalTabsStore.js → FRAME_DELTA), exactly like every other list in this app.
// Only per-tab BYTES and LIVENESS travel on the raw frames below, because only those are a firehose.
//
// companion -> host: one keystroke/paste chunk from the companion's xterm. `data` is base64 (see
// src-tauri/src/pty.rs module doc comment "BINARY-SAFE TRANSPORT" for why). Also carries `tab_id`
// — which of the companion's terminal tabs was typed into. Consumed only by services/ptyBridge.js,
// which calls `pty_write` on the host with that same tab.
export const FRAME_PTY_INPUT = 'pty_input'
// host -> companion: a chunk of PTY output for ONE tab (`tab_id`). `data` is base64. Carries an
// optional `reset: true` (plus `cols`/`rows`) when the payload is a full scrollback replay
// (companion just joined, or a congestion resync) rather than a live incremental chunk — the
// receiving xterm must clear + resize(cols, rows) before writing a `reset` payload so a
// reconnect/rejoin does not duplicate on-screen history or show stale dimensions. A replay is one
// such frame PER TAB (services/ptyBridge.js's pushAllScrollbacks), never one for "the terminal".
export const FRAME_PTY_OUTPUT = 'pty_output'
// host -> companion ONLY (T-4: the host is the sole resize authority — a companion never sends
// or acts on anything BUT this). Carries `{ tab_id, cols, rows }`, that tab's PTY's authoritative size
// right after the host called the `pty_resize` command. The companion's xterm calls
// `term.resize(cols, rows)` directly from this frame — it never computes or requests its own
// PTY size. Added alongside PTY_INPUT/OUTPUT rather than reusing either, since it is host-to-
// companion-only metadata, not terminal bytes.
export const FRAME_PTY_RESIZE = 'pty_resize'
// host -> companion ONLY: ONE tab's shell just ended (the user typed `exit`, it crashed, someone
// hit KILL, or the tab was closed). Carries `{ tab_id }` and nothing else — it flips that tab's
// alive state on the companion so it can show the restart affordance. The tab_id is what keeps one
// tab's exit from marking every other tab dead. Deliberately NOT inferred from the `[process
// exited]` notice bytes in
// the output stream: parsing our own cosmetic text to drive state would break the moment the
// wording changes, and a companion joining after the fact learns the same thing from
// `pty_get_scrollback`'s `alive` field instead.
export const FRAME_PTY_EXIT = 'pty_exit'

// companion -> host: "I am claiming resize authority for this tab, apply this size now"
// (docs/plan/wish-terminal-manual-resize-authority.md). Carries `{ tab_id, cols, rows }`. Honored
// UNCONDITIONALLY on arrival — no negotiation, no permission check beyond "this came from a
// companion" — because safety here comes entirely from WHEN a companion is allowed to send this
// (only in direct response to one explicit user tap on a key-row button, never from a
// ResizeObserver or any automatic/background trigger), not from the host refusing it. The relay
// stamps `from` on every companion frame regardless of type (src-tauri/src/web_server.rs
// stamp_from), so the host already knows which connection to record as the new
// `terminalTabsStore.js` `resizeOwner` without this frame needing to carry any identity itself.
//
// A DELIBERATE, NARROW EXCEPTION TO THIS FILE'S OWN FREEZE: this is the fifth member of the
// already-exempted PTY firehose-adjacent frame family (raw bytes/liveness/size, not JSON-diffed
// state), not a new per-feature category — see the design doc above for why reusing
// FRAME_PTY_RESIZE bidirectionally was considered and rejected (every frame here has one
// documented fixed direction; overloading one tag with direction-dependent meaning breaks that).
// `resizeOwner` ITSELF is ordinary shared state and rides the normal FRAME_DELTA mirror, exactly
// like the tab list — only the imperative "apply this size now" action needed a new raw frame.
export const FRAME_PTY_RESIZE_REQUEST = 'pty_resize_request'

// ── WS close codes (mirrored in src-tauri/src/web_server.rs — keep the two in step) ───────────
//
// These three were one undifferentiated 4001 until 1.20.0, and that ambiguity WAS the bug: a
// companion could not tell "your token is dead" from "the Mac's relay is not accepting right now",
// so it took the destructive reading and wiped a perfectly good token on every app restart and
// every Off — breaking the shipped promise that a phone "reconnects silently across app restarts …
// until revoked". Each code now has exactly one meaning and exactly one client response.

// The token this device presented is unknown / absent / revoked. The ONLY code that may clear the
// stored token: it would fail identically on every retry, so retrying is pointless.
export const CLOSE_UNPAIRED = 4001
// Remote control is off (or was just switched off) on the Mac. Says NOTHING about the token — keep
// it and reconnect with backoff; the phone reconnects by itself the moment the Mac comes back on.
export const CLOSE_SERVER_DISABLED = 4002
// A `role=host` connection was refused (not loopback, or a stale/absent process host token). Only
// the Mac's own webview sees this; it re-reads the token and retries.
export const CLOSE_HOST_ROLE_REJECTED = 4003

// localStorage key the companion persists its paired device token under (§7.1). Read/written
// only by services/bridge.js.
export const DEVICE_TOKEN_STORAGE_KEY = 'aki-companion-device-token'
