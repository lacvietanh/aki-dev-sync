// Seam A — host-side intent dispatch (docs/plan/done/remote-control.md §3, §13.2 `intent`).
//
// An intent is a *gesture*, not a forwarded `invoke` (§3.1): the companion never mutates its own
// copy of mirrored state, it asks the host to perform the real action in the host's own reactive
// context; the resulting change reaches every screen back through the mirror (services/mirror.js).
//
// The `action()` WRAPPER lives in services/action.js, NOT here — see that file for why (breaking
// the store ⇄ intents import cycle). This module owns only the HOST side: the store-glob registry
// and the dispatch of inbound intent frames. No store module imports this file, so its eager glob
// below can never be triggered mid-store-evaluation.
import { isHost, onFrame } from './bridge'
import { FRAME_INTENT } from '../constants/protocol'

const mods = import.meta.glob('../store/*.js', { eager: true })

function basename(path) {
  return path.split('/').pop().replace(/\.js$/, '')
}

// key ("<storeFile>.<export>") -> the real host function. Built LAZILY, on first dispatch — never
// at module-eval time: iterating a store namespace (`Object.entries(mod)`) during bootstrap could
// read an export still in its temporal dead zone if module init order ever put a store mid-eval.
// dispatchIntent only runs at runtime (an inbound frame), long after every module finished
// evaluating, so deferring the build sidesteps that entirely.
let REGISTRY = null

function registry() {
  if (REGISTRY) return REGISTRY
  REGISTRY = new Map()
  for (const [path, mod] of Object.entries(mods)) {
    const store = basename(path)
    for (const [name, val] of Object.entries(mod)) {
      if (typeof val === 'function') REGISTRY.set(`${store}.${name}`, val)
    }
  }
  return REGISTRY
}

/** Host-side dispatch of one incoming intent frame. Unknown key -> warn, never execute. */
export function dispatchIntent(frame) {
  const fn = registry().get(frame && frame.key)
  if (typeof fn !== 'function') {
    console.warn(`[intents] unknown intent key "${frame && frame.key}" — ignored`)
    return
  }
  try {
    fn(...(frame.args || []))
  } catch (e) {
    console.error(`[intents] action "${frame.key}" threw`, e)
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
