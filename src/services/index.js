// Single boot entry for the remote-control seams (Wave 2 wires this into App.vue — see that
// file's own header comment / docs/plan/done/remote-control.md for the call site).
//
// Call once, unconditionally, on BOTH host and companion — role branching lives inside each
// seam module (ENV-1, §9), never in the caller. Order matters: watchers/listeners must be
// registered before the socket opens, so mirror/intents init before connect().
import { connect } from './bridge'
import { initMirror } from './mirror'
import { initIntents } from './intents'
import { initHostInvoke } from './hostInvoke'
import { initPtyBridge } from './ptyBridge'

export { isHost, connectionState, assetBase, pairDevice, hasDeviceToken } from './bridge'

let started = false

/**
 * Boot the bridge + state mirror + intent dispatch. Idempotent — safe to call more than once
 * (e.g. from HMR); only the first call does anything.
 *
 * Wave 2 call site: `src/App.vue`, inside `onMounted`, alongside the existing `loadData()` /
 * `initGlobalNote()` / `refreshClaudeMode()` boot calls:
 *
 *   import { initRemote } from './services'
 *   onMounted(() => {
 *     initRemote()
 *     loadData(sshHosts, false)
 *     ...
 *   })
 */
export function initRemote() {
  if (started) return
  started = true
  initMirror()
  initIntents()
  initHostInvoke()
  // In-app terminal relay (docs/plan/1.20.0-terminal-and-remote-sync.md §4.4). Host-only inside,
  // like initHostInvoke — registered here, before connect(), for the same reason every other seam
  // is: its onFrame listener must exist before the socket can deliver a frame to it.
  initPtyBridge()
  connect()
}
