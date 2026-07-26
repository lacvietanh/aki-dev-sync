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

// Setters for state that is deliberately per-screen (services/mirror.js PER_SCREEN_KEYS, plan
// §3.12). They run LOCALLY on the companion, exactly as they do on the host — the ref they write is
// not on the wire, so sending an intent would change the Mac's slots and leave the phone's own
// untouched: the tap would appear to do nothing here and something unwanted there. Which host a
// screen's usage slots watch is that screen's own choice, and the write also lands in that screen's
// own localStorage, which is where a per-screen preference belongs.
const PER_SCREEN_ACTION_KEYS = new Set(['usageSlotStore.setSlotTarget'])

// The host's intent registry: key -> the real function, filled by `action()` at wrap time.
//
// SECURITY (this is the whole point of the map): it used to be built in intents.js by globbing
// `src/store/*.js` and registering EVERY function export. That made 16 functions that are not
// actions — `dialogStore.askConfirm` (its `html` option is written into the dialog with innerHTML),
// `logStore.setGlobalListener`, `projectStore.markProjectRemoved`/`bumpEpoch` — remotely callable by
// any paired companion. Registering at wrap time instead means the remotely callable set is exactly
// the set a developer opted in by writing `action(key, fn)`, and it cannot drift as stores grow.
const HOST_ACTIONS = new Map()

/** Host-side lookup for services/intents.js. Returns undefined for anything never wrapped in
 *  `action()` — the caller treats that as "unknown intent" and never executes. */
export function getHostAction(key) {
  return HOST_ACTIONS.get(key)
}

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
 *
 * A key in PER_SCREEN_ACTION_KEYS is the one exception: it runs locally on the companion too.
 */
export function action(key, fn) {
  if (typeof key !== 'string' || typeof fn !== 'function') {
    throw new Error(
      `[action] requires (key: string, fn: function) — e.g. action('projectStore.setDryRun', fn). ` +
        `Got (${typeof key}, ${typeof fn}).`
    )
  }
  if (isHost) {
    // A per-screen action never travels (the companion runs it locally, see above), so it is
    // deliberately NOT reachable from an inbound intent frame either.
    if (!PER_SCREEN_ACTION_KEYS.has(key)) {
      if (HOST_ACTIONS.has(key)) {
        console.error(`[action] duplicate intent key "${key}" — the later definition now owns it`)
      }
      HOST_ACTIONS.set(key, fn)
    }
    return fn
  }
  if (PER_SCREEN_ACTION_KEYS.has(key)) return fn
  return function actionStub(...args) {
    if (!send({ t: FRAME_INTENT, key, args })) reportUndelivered(key)
    return undefined
  }
}

// §3.13: a dropped intent used to be a `console.debug` in bridge.js and an ignored return value
// here — the user taps PUSH on the phone and NOTHING happens, with no way to tell a dead socket
// from a broken button. It rides the app's existing Toast (CLAUDE.md *UI Extreme Narrow*: no new
// element, no banner, no status row).
//
// The import is dynamic on purpose: every `src/store/*.js` module imports THIS one, so a static
// `import { Toast } from '../store/projectStore'` would close the store → action → store cycle this
// module was split out of intents.js to break. Resolved lazily, only on a failure, long after
// bootstrap.
let lastUndeliveredAt = 0

function reportUndelivered(key) {
  // One toast per burst: a mis-tap on a dead socket can fire several intents in the same second,
  // and three stacked toasts say nothing the first did not.
  const now = Date.now()
  if (now - lastUndeliveredAt < 3000) return
  lastUndeliveredAt = now
  import('../store/projectStore')
    .then(({ Toast }) => {
      Toast.fire({
        icon: 'error',
        title: 'Not sent — no connection to the Mac',
        text: `"${key}" was dropped. It will work again once the phone reconnects.`,
      })
    })
    .catch((e) => console.error('[action] could not surface the dropped intent', e))
}
