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
//
// It publishes the HEAD OF THE QUEUE below — exactly one dialog at a time, same shape as before, so
// the wire protocol and DialogHost are unchanged.
export const pendingDialog = ref(null)

// FIFO of dialogs waiting to be answered; `_queue[0]` is what `pendingDialog` publishes.
//
// WHY A QUEUE: `askConfirm` used to overwrite `pendingDialog` unconditionally, so a second confirm
// (say the phone starting a `--delete` PUSH on project B while the Mac is mid-confirm on project A)
// made A's dialog vanish unanswered. Its promise never settled, its `_waiters` entry leaked, and
// `projectRuntime[A].syncing` stayed `true` for the rest of the session — project A could not be
// synced again without restarting the app. This is a shape fix, not a guard: with one slot, the
// second dialog had nowhere to go, so no amount of caller-side checking could have saved the first.
//
// Deliberately a plain array, not a ref: services/mirror.js registers every `isRef` export of every
// `src/store/*.js`, and the queue is host-local bookkeeping (like `_waiters`), not shared state. The
// companion never runs this code — `resolveDialog` is an `action()`, so on a phone it only sends an
// intent — so the queue exists on exactly one process, the same one that holds the waiters.
//
// FIFO, and a queued dialog waits SILENTLY — no counter, no "1 more pending" badge. Someone
// answering a delete confirmation is concentrating on one irreversible decision and reading a file
// list; telling them another question is stacked behind it splits exactly the attention that dialog
// exists to capture. The next one simply appears when the first is answered, which is what a person
// expects from a queue anyway (and it is the only option compatible with UI Extreme Narrow —
// a badge would have to live somewhere new). Last-in-wins was rejected outright: it discards a
// decision the user had already started making.
const _queue = []

// dialog id -> the resolve() fn of the Promise askConfirm() returned. This map is process-local
// (never mirrored, never sent over the wire) — it only ever has entries on whichever process is
// actually awaiting, which by construction is the HOST (see askConfirm below).
const _waiters = new Map()

let _seq = 0

/** Single writer for `pendingDialog`: it is ALWAYS the head of the queue, never anything else. */
function publishHead() {
  pendingDialog.value = _queue.length > 0 ? _queue[0] : null
}

/**
 * HOST-ONLY helper: publish a dialog as mirrored state and resolve once either screen answers.
 *
 * MUST only be called from code already running on the host — i.e. from inside an action()
 * body (directly, like useSync.js's startSync via remoteActions.requestSync), or from a plain
 * function only ever reached through one (remoteActions.requestRemoveProject). Calling this from
 * a companion would write `pendingDialog` directly, outside an action — the exact ACT-1 violation
 * PERSIST-1 elsewhere in this release exists to eliminate — and the companion's own `_waiters`
 * entry could never be fulfilled anyway, since `resolveDialog` only ever runs for real on the host.
 *
 * Enqueues rather than replaces: if another dialog is already awaiting an answer, this one becomes
 * visible the moment that one is answered. Nothing is ever dropped, so no awaited promise is left
 * unsettled and no `_waiters` entry leaks.
 */
export function askConfirm(spec) {
  const id = `dlg-${Date.now()}-${++_seq}`
  return new Promise((resolve) => {
    _waiters.set(id, resolve)
    _queue.push({ ...spec, id })
    publishHead()
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
  // Only the head is answerable, and `pendingDialog` only ever shows the head — so an answer for
  // anything else is a stale id from the losing screen (first-answer-wins) and is dropped exactly
  // as before. Matching on the queue head rather than on `pendingDialog` keeps the two from being
  // able to disagree.
  if (_queue.length === 0 || _queue[0].id !== id) return
  _queue.shift()
  publishHead() // promotes the next queued dialog, or clears the slot when none is left
  const waiter = _waiters.get(id)
  if (waiter) {
    _waiters.delete(id)
    waiter(answer || { confirmed: false })
  }
})
