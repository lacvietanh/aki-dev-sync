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

// Frame `t` values — §13.2.
export const FRAME_INIT = 'init'                   // host -> companion: full snapshot on join
export const FRAME_DELTA = 'delta'                 // host -> companion: changed mirrored keys
export const FRAME_INTENT = 'intent'                // companion -> host: run this action
export const FRAME_INVOKE = 'invoke'                // companion -> host: RPC, expects a reply
export const FRAME_INVOKE_RESULT = 'invoke_result'  // host -> companion: reply to one `invoke` id
export const FRAME_PING = 'ping'                    // both: liveness
export const FRAME_PONG = 'pong'                    // both: liveness reply

// relay -> host: a companion just authenticated and joined. Carries `{ id }` (the joining
// device id) but no app state — the relay stays content-blind (§13.6); this is awareness of its
// OWN connections, not state. The host reacts by broadcasting a full `init` to everyone. This is
// the ONLY frame the relay originates. Distinct tag (not an overloaded `init`) so Terminal View
// (next round) can also route per-companion PTYs by `id`.
export const FRAME_COMPANION_CONNECTED = 'companion-connected'

// In-app terminal (docs/plan/1.20.0-terminal-and-remote-sync.md §4, docs/plan/remote-views-
// roadmap.md § Terminal View). Deliberately NOT routed through FRAME_DELTA/FRAME_INTENT
// (services/mirror.js / services/intents.js) — raw terminal bytes are a firehose and do not fit
// a JSON-diffed state model; these are new top-level frames on the same socket instead.
//
// companion -> host: one keystroke/paste chunk from the companion's xterm. `data` is base64 (see
// src-tauri/src/pty.rs module doc comment "BINARY-SAFE TRANSPORT" for why). Consumed only by
// services/ptyBridge.js, which calls `pty_write` on the host.
export const FRAME_PTY_INPUT = 'pty_input'
// host -> companion: a chunk of PTY output. `data` is base64. Carries an optional `reset: true`
// (plus `cols`/`rows`) when the payload is a full scrollback replay (companion just joined) rather
// than a live incremental chunk — the receiving xterm must clear + resize(cols, rows) before
// writing a `reset` payload so a reconnect/rejoin does not duplicate on-screen history or show
// stale dimensions.
export const FRAME_PTY_OUTPUT = 'pty_output'
// host -> companion ONLY (T-4: the host is the sole resize authority — a companion never sends
// or acts on anything BUT this). Carries `{ cols, rows }`, the PTY's actual authoritative size
// right after the host called the `pty_resize` command. The companion's xterm calls
// `term.resize(cols, rows)` directly from this frame — it never computes or requests its own
// PTY size. Added alongside PTY_INPUT/OUTPUT rather than reusing either, since it is host-to-
// companion-only metadata, not terminal bytes.
export const FRAME_PTY_RESIZE = 'pty_resize'
// host -> companion ONLY: the shared shell just ended (the user typed `exit`, it crashed, or
// someone hit KILL). Carries no payload — it flips the companion's own alive state so it can show
// the restart affordance. Deliberately NOT inferred from the `[process exited]` notice bytes in
// the output stream: parsing our own cosmetic text to drive state would break the moment the
// wording changes, and a companion joining after the fact learns the same thing from
// `pty_get_scrollback`'s `alive` field instead.
export const FRAME_PTY_EXIT = 'pty_exit'

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
