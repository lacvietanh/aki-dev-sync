// Seam A: action() wrapper split out from services/intents.js (docs/plan/done/remote-control.md §3, R-2).
// Host dispatch and store-glob registry remain in intents.js, which stores do not import.
import { isHost, send } from './bridge'
import { FRAME_INTENT } from '../constants/protocol'

// Per-screen action setters run locally on companion to avoid mutating host viewport state (§3.12).
const PER_SCREEN_ACTION_KEYS = new Set(['usageSlotStore.setSlotTarget'])

// Security allowlist: HOST_ACTIONS registers only functions explicitly wrapped with action(key, fn).
const HOST_ACTIONS = new Map()

/** Host-side action lookup for intents.js; returns undefined for unregistered keys. */
export function getHostAction(key) {
  return HOST_ACTIONS.get(key)
}

/**
 * Wraps a store action: passes through on host, dispatches FRAME_INTENT on companion (ACT-1; §3.2).
 * Keys in PER_SCREEN_ACTION_KEYS bypass remote dispatch and execute locally.
 */
export function action(key, fn) {
  if (typeof key !== 'string' || typeof fn !== 'function') {
    throw new Error(
      `[action] requires (key: string, fn: function) — e.g. action('projectStore.setDryRun', fn). ` +
        `Got (${typeof key}, ${typeof fn}).`
    )
  }
  if (isHost) {
    // Per-screen actions run locally on companion, so they are excluded from host intent registration.
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

// Surfaces dropped intents via Toast on disconnection (§3.13; CLAUDE.md UI Extreme Narrow).
// Dynamic import avoids store -> action -> store circular dependency during bootstrap.
let lastUndeliveredAt = 0

function reportUndelivered(key) {
  // Rate-limit to one toast per burst to prevent stacked alerts on repeated taps.
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
