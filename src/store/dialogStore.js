// Mirrored decision dialog state (SYNC-1; docs/plan/done/remote-control.md §3.4, docs/plan/done/1.20.0-terminal-and-remote-sync.md §3).
// First-answer-wins across screens via mirror/intents glob discovery; depends ONLY on vue + action (REGISTRY-1 / ENV-1 cycle guard).
import { ref } from 'vue'
import { action } from '../services/action'

// Publishes queue head as JSON-encodable data (SER-1: id, kind, plain strings/arrays; never refs/nodes).
export const pendingDialog = ref(null)

// Host-local FIFO queue (plain array, not mirrored ref) to prevent concurrent confirms from overwriting dialogs and leaking waiters.
const _queue = []

// Host-local map of dialog id -> Promise resolve() for pending askConfirm() calls.
const _waiters = new Map()

let _seq = 0

/** Single writer for pendingDialog: always publishes the head of _queue. */
function publishHead() {
  pendingDialog.value = _queue.length > 0 ? _queue[0] : null
}

/** Host-only: enqueues dialog spec into FIFO queue and returns a Promise settling on answer (ACT-1/PERSIST-1). */
export function askConfirm(spec) {
  const id = `dlg-${Date.now()}-${++_seq}`
  return new Promise((resolve) => {
    _waiters.set(id, resolve)
    _queue.push({ ...spec, id })
    publishHead()
  })
}

/** Resolves pending dialog; first-answer-wins via id guard, delivering payload to host-side askConfirm waiter. */
export const resolveDialog = action('dialogStore.resolveDialog', (id, answer) => {
  if (_queue.length === 0 || _queue[0].id !== id) return
  _queue.shift()
  publishHead()
  const waiter = _waiters.get(id)
  if (waiter) {
    _waiters.delete(id)
    waiter(answer || { confirmed: false })
  }
})

