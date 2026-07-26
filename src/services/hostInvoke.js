// Seam N — HOST half (docs/plan/done/remote-control.md §4, §13.2 `invoke`/`invoke_result`).
//
// The companion half lives in utils/tauri.js: on the companion, invoke() sends `{t:'invoke', cmd,
// args, id}` over the bridge and awaits a matching `invoke_result`. This module is the ONLY thing
// that answers those frames — without it a companion invoke() hangs until the 20s watchdog in
// bridge.request() fires (exactly the `get_agent_usage` / `log_frontend` timeouts seen on the phone).
//
// Symmetry with Seam A (intents.js): host-only, one onFrame listener, unknown/failed calls reply
// with a concrete error rather than going silent. Difference from an intent — an invoke expects a
// value back, so we always emit an `invoke_result` for the companion's `id` (ok on success, err on
// throw); an intent is fire-and-forget and emits nothing.
//
// We call `invoke` from utils/tauri.js, NOT `@tauri-apps/api/core` directly: on the host that
// export IS the raw tauriInvoke, so this respects R-3's single-import-point rule (utils/tauri.js is
// the only module allowed to touch the native core import).
import { isHost, onFrame, send } from './bridge'
import { invoke } from '../utils/tauri'
import { FRAME_INVOKE, FRAME_INVOKE_RESULT } from '../constants/protocol'

/** Run one inbound companion invoke on the host and reply with its result. Any throw becomes an
 *  `err` string on the reply (the companion's request() Promise rejects with it) — never an
 *  unanswered frame, which would strand the companion on the watchdog timeout. */
async function respondToInvoke(frame) {
  const { id, cmd, args } = frame
  try {
    const ok = await invoke(cmd, args)
    // JSON.stringify drops an `undefined` value, so a void command serializes to a frame with
    // neither `ok` nor `err` — the companion reads that as resolve(undefined), which is correct.
    send({ t: FRAME_INVOKE_RESULT, id, ok })
  } catch (e) {
    // Preserve the host's real error text so the phone console shows the actual Tauri failure,
    // not a generic "rejected". Tauri command errors are usually plain strings already.
    const err = e && e.message ? e.message : String(e)
    console.error(`[hostInvoke] command "${cmd}" failed`, e)
    send({ t: FRAME_INVOKE_RESULT, id, err })
  }
}

/** Boot this seam. Host-only: wires incoming `invoke` frames to the real Tauri IPC. No-op on the
 *  companion (it is the SENDER of invoke frames, via utils/tauri.js). */
export function initHostInvoke() {
  if (!isHost) return
  onFrame((frame) => {
    if (frame && frame.t === FRAME_INVOKE) respondToInvoke(frame)
  })
}
