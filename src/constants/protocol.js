// Wire-protocol contract SSoT shared by Rust relay (src-tauri/src/web_server.rs) and JS seams (bridge, mirror, intents, tauri).
// @docs docs/plan/done/remote-control.md §13 (wire protocol contract frozen).

// Relay WS/HTTP port (dev and prod; docs/plan/done/remote-control.md §7.1a).
export const REMOTE_PORT = 1421

// Optional addressing fields: to (host->relay target connection), from (relay->host opaque connection ID).
// Per-connection isolation prevents duplicate scrollbacks and request ID collisions across tabs (live frames remain broadcast).

// Frame types (docs/plan/done/remote-control.md §13.2).
export const FRAME_INIT = 'init'                   // host -> companion: full snapshot on join
export const FRAME_DELTA = 'delta'                 // host -> companion: changed mirrored keys
export const FRAME_INTENT = 'intent'                // companion -> host: run this action
export const FRAME_INVOKE = 'invoke'                // companion -> host: RPC, expects a reply
export const FRAME_INVOKE_RESULT = 'invoke_result'  // host -> companion: reply to one `invoke` id
export const FRAME_PING = 'ping'                    // both: liveness
export const FRAME_PONG = 'pong'                    // both: liveness reply

// relay -> host: companion authenticated/joined notification carrying connection `{ id }`.
export const FRAME_COMPANION_CONNECTED = 'companion-connected'

// In-app terminal streaming frames (docs/plan/done/1.20.0-terminal-and-remote-sync.md §4; tab_id multiplexing).

// companion -> host: keystroke/paste chunk from companion xterm (data is base64; src-tauri/src/pty.rs).
export const FRAME_PTY_INPUT = 'pty_input'

// host -> companion: PTY output chunk for tab_id (base64; optional reset: true with cols/rows for replay).
export const FRAME_PTY_OUTPUT = 'pty_output'

// host -> companion ONLY: T-4 authoritative PTY resize event carrying `{ tab_id, cols, rows }`.
export const FRAME_PTY_RESIZE = 'pty_resize'

// host -> companion ONLY: PTY exit notification carrying `{ tab_id }` when shell process terminates.
export const FRAME_PTY_EXIT = 'pty_exit'

// companion -> host: manual resize authority claim (docs/plan/done/wish-terminal-manual-resize-authority.md).
export const FRAME_PTY_RESIZE_REQUEST = 'pty_resize_request'

// WS close codes mirrored in src-tauri/src/web_server.rs.
// Token unknown/revoked: clear stored token (do not retry).
export const CLOSE_UNPAIRED = 4001

// Remote control server off: keep stored token and reconnect with exponential backoff.
export const CLOSE_SERVER_DISABLED = 4002

// Host role connection refused (non-loopback or invalid host token): host webview re-reads token and retries.
export const CLOSE_HOST_ROLE_REJECTED = 4003

// localStorage key for paired companion device token (docs/plan/done/remote-control.md §7.1).
export const DEVICE_TOKEN_STORAGE_KEY = 'aki-companion-device-token'
