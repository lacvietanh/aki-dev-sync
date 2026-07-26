// Seam N — native call (docs/plan/done/remote-control.md §4).
//
// The ONLY module allowed to import from `@tauri-apps/api/core` (R-3: 19 call sites elsewhere
// swap their import to this file instead, mechanically, in the Wave-2 refactor pass).
//
// `invoke()` has the same signature and semantics on both sides, so nothing in the app needs to
// know where it actually runs — no skip-list, no "host-only command" concept:
//   HOST:      the real Tauri IPC call.
//   COMPANION: an RPC over the WS bridge — the host runs the real command and replies (or
//              rejects with the host's error), via `bridge.request()` / `{t:'invoke'}` (§13.2).
// No client-side timeout here: some commands (e.g. `run_sync`) legitimately take minutes.
// Liveness is the bridge's own ping/pong; a dropped socket rejects all in-flight calls at once.
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
