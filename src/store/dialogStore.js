// Seam A+S together — a decision dialog as mirrored state (docs/plan/done/remote-control.md §3.4,
// docs/plan/1.20.0-terminal-and-remote-sync.md §3).
//
// Per SYNC-1 a dialog awaiting a decision is a DATA EVENT, not host-local UI: it must be visible
// and answerable from either screen, whichever answers first wins. It rides the two seams that
// already exist — no new frame type, no relay change. `pendingDialog` is discovered by
// services/mirror.js because it's an `isRef` export of a `src/store/*.js` module; `resolveDialog`
// is discovered by services/intents.js because it's a function export of the same glob.
//
// Import discipline (REGISTRY-1 / ENV-1): this module depends ONLY on vue + services/action —
// the same rule syncCheckStore/remoteActions already follow. No role-token import, no import of
// intents.js, so it cannot reintroduce the store <-> intents eager-glob cycle documented in
// services/action.js, and ENV-1 holds (no role token literal anywhere under src/store/**).
import { ref } from 'vue'
import { action } from '../services/action'

// pendingDialog must stay JSON-encodable (SER-1): id, kind, and plain strings/arrays/objects —
// never a function, DOM node, Vue component ref, or a project object with cyclic links. Every
// call site below passes ids/strings/plain data, never a live store object.
export const pendingDialog = ref(null)

// dialog id -> the resolve() fn of the Promise askConfirm() returned. This map is process-local
// (never mirrored, never sent over the wire) — it only ever has entries on whichever process is
// actually awaiting, which by construction is the HOST (see askConfirm below).
const _waiters = new Map()

let _seq = 0

/**
 * HOST-ONLY helper: publish a dialog as mirrored state and resolve once either screen answers.
 *
 * MUST only be called from code already running on the host — i.e. from inside an action()
 * body (directly, like useSync.js's startSync via remoteActions.requestSync), or from a plain
 * function only ever reached through one (remoteActions.requestRemoveProject). Calling this from
 * a companion would write `pendingDialog` directly, outside an action — the exact ACT-1 violation
 * PERSIST-1 elsewhere in this release exists to eliminate — and the companion's own `_waiters`
 * entry could never be fulfilled anyway, since `resolveDialog` only ever runs for real on the host.
 */
export function askConfirm(spec) {
  const id = `dlg-${Date.now()}-${++_seq}`
  return new Promise((resolve) => {
    _waiters.set(id, resolve)
    pendingDialog.value = { ...spec, id }
  })
}

/**
 * Answer a pending dialog (remote-control.md §3.4). First-answer-wins via the `id` guard: once
 * one screen's answer clears `pendingDialog`, the losing screen's own eventual answer carries a
 * stale `id` and DialogHost drops it as a silent no-op — this function does not need to know
 * which screen called it.
 *
 * The typed/select value travels IN `answer` and is NOT judged here — this function only
 * delivers it to whatever host-side code is awaiting `askConfirm()`. That awaiting code is what
 * re-validates it (e.g. useSync.js checks `answer.typed === project.name`) — so the safety check
 * always runs on the host, never on the strength of what the phone claims.
 */
export const resolveDialog = action('dialogStore.resolveDialog', (id, answer) => {
  const d = pendingDialog.value
  if (!d || d.id !== id) return // already answered (or never asked) — ignore
  pendingDialog.value = null
  const waiter = _waiters.get(id)
  if (waiter) {
    _waiters.delete(id)
    waiter(answer || { confirmed: false })
  }
})
