// Wire-protocol contract — SSoT, shared by the Rust relay (src-tauri/src/web_server.rs) and
// every JS seam (services/bridge.js, services/mirror.js, services/intents.js, utils/tauri.js).
//
// FROZEN per docs/plan/remote-control.md §13 — do not diverge, do not add per-feature frame
// types. Both the host and the companion import these same constants so a typo can't produce
// two sides that silently fail to recognize each other's frames.
//
// @docs docs/plan/remote-control.md §13 (wire protocol contract)

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

// Reserved for the next round (Terminal View, docs/plan/remote-views-roadmap.md). Do NOT
// implement these frame types yet — reserved here purely so the names are never reused for
// something else in the meantime.
export const FRAME_PTY_INPUT = 'pty_input'
export const FRAME_PTY_OUTPUT = 'pty_output'

// WS close code the relay uses when a companion connects with a bad/absent pairing token
// (§13.1). The companion UI must treat exactly this code as "show the pairing screen", not a
// generic disconnect/retry.
export const CLOSE_UNPAIRED = 4001

// localStorage key the companion persists its paired device token under (§7.1). Read/written
// only by services/bridge.js.
export const DEVICE_TOKEN_STORAGE_KEY = 'aki-companion-device-token'
