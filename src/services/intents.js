// Seam A — host-side intent dispatch (docs/plan/done/remote-control.md §3, §13.2 `intent`).
// An intent is a gesture, not a forwarded invoke (§3.1): the companion asks the host to perform the real action in the host's reactive context, and changes reach screens through mirror (services/mirror.js).
// The action() wrapper lives in services/action.js (breaks store ⇄ intents cycle) and owns the registry populated by action(key, fn).
import { isHost, onFrame } from './bridge'
import { getHostAction } from './action'
import { FRAME_INTENT } from '../constants/protocol'

// Side-effect import: evaluating store modules executes top-level action() calls to populate the registry in action.js.
import.meta.glob('../store/*.js', { eager: true })

/** Host-side dispatch of incoming intent frame; ignores unregistered actions. */
export function dispatchIntent(frame) {
  const key = frame && frame.key
  const fn = getHostAction(key)
  if (typeof fn !== 'function') {
    console.warn(`[intents] unknown intent key "${key}" — ignored`)
    return
  }
  // Guard untrusted peer payload: non-array args would throw on spread.
  const args = Array.isArray(frame.args) ? frame.args : []
  try {
    fn(...args)
  } catch (e) {
    console.error(`[intents] action "${key}" threw`, e)
  }
}

/** Host-only boot: wires incoming FRAME_INTENT frames to dispatchIntent. */
export function initIntents() {
  if (!isHost) return
  onFrame((frame) => {
    if (frame.t === FRAME_INTENT) dispatchIntent(frame)
  })
}
