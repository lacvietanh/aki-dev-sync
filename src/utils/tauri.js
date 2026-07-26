// Seam N — native call (docs/plan/done/remote-control.md §4).
//
// The ONLY module allowed to import from `@tauri-apps/api/core` (R-3: 19 call sites elsewhere
// swap their import to this file instead, mechanically, in the Wave-2 refactor pass).
//
// `invoke()` has the same signature and semantics on both sides, so nothing in the app needs to
// know where it actually runs:
//   HOST:      the real Tauri IPC call — no timeout, no allowlist, nothing between it and Tauri.
//   COMPANION: an RPC over the WS bridge — the host runs the real command and replies (or
//              rejects with the host's error), via `bridge.request()` / `{t:'invoke'}` (§13.2).
// Two things apply to the COMPANION path only, and neither lives here:
//   * an allowlist — services/hostInvoke.js refuses any command not in
//     COMPANION_ALLOWED_COMMANDS, so a companion invoke of e.g. `run_sync` rejects by design.
//   * a watchdog — bridge.request() rejects a call the host never answers, on a per-command budget
//     (INVOKE_TIMEOUT_MS_BY_CMD; 20s default, longer or none for SSH/network-bound commands).
// Liveness on top of that is the bridge's own ping/pong; a dropped socket rejects all in-flight
// calls at once.
import { invoke as tauriInvoke } from '@tauri-apps/api/core'
import { isHost, request } from '../services/bridge'
import { FRAME_INVOKE } from '../constants/protocol'

export const invoke = isHost
  ? tauriInvoke
  : (cmd, args) =>
      request({ t: FRAME_INVOKE, cmd, args }).catch((e) => {
        // Concrete, named error so a broken companion invoke is visible in the phone console
        // instead of hanging silently. The underlying reason (timeout / socket drop / host error)
        // is in `e.message`; this line pins which command it was.
        console.error(`[invoke] companion RPC "${cmd}" failed:`, e && e.message ? e.message : e)
        throw e
      })
