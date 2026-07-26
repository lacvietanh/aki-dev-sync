// Seam A — host-side intent dispatch (docs/plan/done/remote-control.md §3, §13.2 `intent`).
//
// An intent is a *gesture*, not a forwarded `invoke` (§3.1): the companion never mutates its own
// copy of mirrored state, it asks the host to perform the real action in the host's own reactive
// context; the resulting change reaches every screen back through the mirror (services/mirror.js).
//
// The `action()` WRAPPER lives in services/action.js, NOT here — see that file for why (breaking
// the store ⇄ intents import cycle). It also OWNS the registry now: every `action(key, fn)` call
// registers itself there at wrap time, and this module can only dispatch what is in it. The old
// design globbed `src/store/*.js` here and registered every function export, which exposed plenty of
// internals that were never meant to be remotely callable (`dialogStore.askConfirm`, whose `html`
// option is rendered with innerHTML inside the Tauri webview, was the worst of them).
import { isHost, onFrame } from './bridge'
import { getHostAction } from './action'
import { FRAME_INTENT } from '../constants/protocol'

// Side-effect import only — nothing here reads these namespaces. Evaluating every store module is
// what runs its top-level `action(...)` calls, which is what fills the registry in action.js. No
// export is touched, so there is no temporal-dead-zone hazard in doing it eagerly.
import.meta.glob('../store/*.js', { eager: true })

/** Host-side dispatch of one incoming intent frame. Anything that is not an `action()`-wrapped
 *  function -> warn, never execute. */
export function dispatchIntent(frame) {
  const key = frame && frame.key
  const fn = getHostAction(key)
  if (typeof fn !== 'function') {
    console.warn(`[intents] unknown intent key "${key}" — ignored`)
    return
  }
  // Args arrive as JSON from an untrusted peer: anything but an array is a malformed frame, and
  // spreading a non-array would throw before the action ever ran.
  const args = Array.isArray(frame.args) ? frame.args : []
  try {
    fn(...args)
  } catch (e) {
    console.error(`[intents] action "${key}" threw`, e)
  }
}

/** Boot this seam. Host-only: wires incoming `intent` frames to dispatchIntent(). No-op on the
 *  companion (it only ever sends intents, via the action() stubs from services/action.js). */
export function initIntents() {
  if (!isHost) return
  onFrame((frame) => {
    if (frame.t === FRAME_INTENT) dispatchIntent(frame)
  })
}
