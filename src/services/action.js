// Seam A — the `action()` wrapper, split out from services/intents.js (docs/plan/done/remote-control.md
// §3, R-2 / REGISTRY-1).
//
// WHY ITS OWN MODULE: store modules (syncCheckStore, remoteActions) need `action()` at their
// definition site. If they imported it from `intents.js` — which does `import.meta.glob('../store/
// *.js')` — that would form a hard import cycle store → intents → (glob) → store, evaluated eagerly
// at bootstrap. On the host that cycle crashed nothing, but it is exactly the kind of load-order
// hazard that renders a blank page on the phone. This module depends ONLY on `bridge` and
// `protocol` (neither imports any store), so store → action → bridge has no back-edge and no cycle.
//
// Dispatch (host side) and the store-glob registry stay in intents.js, which no store imports.
import { isHost, send } from './bridge'
import { FRAME_INTENT } from '../constants/protocol'

/**
 * Wrap a store action so it behaves identically whether it runs on the host or is invoked from a
 * companion (§3.2, ACT-1).
 *
 *   export const setDryRun = action('projectStore.setDryRun', (id, v) => { ... })
 *
 * HOST:      returns `fn` unchanged — zero overhead, byte-identical behaviour to not wrapping.
 * COMPANION: returns a stub that never runs `fn` locally; it sends `{t:'intent', key, args}` so the
 *            host runs the real action, whose state change then flows back through the mirror.
 *
 * An explicit `key` is REQUIRED (it must match the host's registry key, `"<storeFile>.<export>"`).
 * The old identity-lookup form (`action(fn)`) is gone with the cycle: it needed the store glob,
 * which is what this split removes. Every call site in the app already passes an explicit key.
 */
export function action(key, fn) {
  if (typeof key !== 'string' || typeof fn !== 'function') {
    throw new Error(
      `[action] requires (key: string, fn: function) — e.g. action('projectStore.setDryRun', fn). ` +
        `Got (${typeof key}, ${typeof fn}).`
    )
  }
  if (isHost) return fn
  return function actionStub(...args) {
    send({ t: FRAME_INTENT, key, args })
    return undefined
  }
}
