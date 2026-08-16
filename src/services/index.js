// Single boot entry for remote-control seams (docs/plan/done/remote-control.md). Call once unconditionally on host and companion (role branching is internal); listeners register before connect().
import { connect } from './bridge'
import { initMirror } from './mirror'
import { initIntents } from './intents'
import { initHostInvoke } from './hostInvoke'
import { initPtyBridge } from './ptyBridge'

export { isHost, connectionState, assetBase, pairDevice, hasDeviceToken } from './bridge'

let started = false

/** Boot bridge + state mirror + intent dispatch. Idempotent across HMR/re-renders. */
export function initRemote() {
  if (started) return
  started = true
  initMirror()
  initIntents()
  initHostInvoke()
  // Terminal relay (docs/plan/done/1.20.0-terminal-and-remote-sync.md §4.4): host-only listener registered before connect().
  initPtyBridge()
  connect()
}
