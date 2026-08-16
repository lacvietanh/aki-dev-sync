// Per-project tasks & notes, as the app holds them in memory.
//
// The file itself lives in the project's own repo (`<local_path>/.akidevsync/notes.json`,
// docs/plan/done/1.22.0-notes-json-ssot.md). This module is the mirrored in-memory copy of what Rust
// last read from it.
//
// WHY IT IS A `src/store/*.js` MODULE AND NOT A COMPOSABLE — this IS the companion read answer.
// services/mirror.js globs every `isRef` export under `src/store/` and mirrors it host→companion
// with no per-key wiring. A paired phone has no filesystem and may not invoke the read commands
// (they are deliberately absent from COMPANION_ALLOWED_COMMANDS), so putting the data here is what
// makes it appear on the phone for free. Nothing else was needed for that case, and nothing else
// should be added for it.
import { ref } from 'vue'

/**
 * `{ [projectId]: { status, notes, tasks, updated_at, error } }`
 *
 * `status` is the Rust `ProjectNotesStatus`: `'ok' | 'missing' | 'unavailable' | 'corrupt'`, plus
 * `'unknown'` for an id nothing has read yet. Those distinctions are the point — see
 * isProjectNotesWritable(): only `ok` and `missing` may be written to, and everything else must
 * make the UI read-only BEFORE the user types, or the app shows an empty note and then saves that
 * emptiness over the user's real one.
 */
export const projectNotes = ref({})

/** The entry shape for an id that has never been read. Frozen so a caller cannot accidentally make
 *  it the mutable backing object for a real project — every entry must arrive via
 *  `setProjectNotesEntry`. */
const UNKNOWN_ENTRY = Object.freeze({
  status: 'unknown',
  notes: '',
  tasks: [],
  updated_at: 0,
  error: '',
})

// ── The three id-scoped accessors ───────────────────────────────────────────────────────────────
//
// REGRESSION GUARD (CLAUDE.md, the 1.9.3 multi-entity bug): `projectNotes` is a MAP OF ENTITIES.
// No function in this module may touch more than the one id it is handed, and none of them is
// allowed to assign `projectNotes.value = {}`. That whole-store wipe, reached through a function
// whose name implied a narrower scope, is exactly what deleted a user's entire multi-account
// history once already. Each function below is named for the single id it affects.

/** Replace ONE project's entry. */
export function setProjectNotesEntry(id, entry) {
  if (!id) return
  // New object identity, not an in-place field write: mirror.js watches this ref and a nested mutation on the same object would not always produce a delta for the companion.
  projectNotes.value = { ...projectNotes.value, [id]: { ...UNKNOWN_ENTRY, ...entry } }
}

/** Read ONE project's entry, or the frozen unknown default. Never returns undefined, so no caller
 *  has to guard, and no caller can be tempted to create the missing entry itself. */
export function getProjectNotesEntry(id) {
  return projectNotes.value[id] || UNKNOWN_ENTRY
}

/** Remove ONE project's entry — called from removeProject. Named for its scope: never
 *  `clearProjectNotes()`, which is what a future reader would take as licence to wipe the map. */
export function dropProjectNotesEntry(id) {
  bumpProjectNotesGeneration(id)
  if (!id || !(id in projectNotes.value)) return
  const next = { ...projectNotes.value }
  delete next[id]
  projectNotes.value = next
}

// ── Per-id generation token: the guard that makes a LATE read harmless ──────────────────────────
//
// `read_project_notes` is `spawn_blocking` precisely because it can stall for tens of seconds on an unhealthy network mount. So this sequence is not theoretical:
//
//   modal opens → refresh starts → user edits → applyTaskEdit writes and re-seeds the entry
//                                            → THEN the pre-edit read lands
//
// Without a guard that stale read overwrites the store with the file as it was BEFORE the edit: the
// edit vanishes from the screen, and the next edit persists that reverted content back over the good
// file. That is the 1.20.0 "task note reverts" shape all over again, in a new place.
//
// Every mutation of an id's entry bumps that id's counter; every async read captures the counter it
// started with and discards its own result if the world has moved on. Plain object, deliberately not
// reactive — this is bookkeeping about reads, never something the UI renders.
const generation = Object.create(null)

/** Invalidate every read for this id that is currently in flight, and return the new token. */
export function bumpProjectNotesGeneration(id) {
  generation[id] = (generation[id] || 0) + 1
  return generation[id]
}

/** Is `token` still the current generation for this id, i.e. may this read's result be applied? */
export function isCurrentProjectNotesGeneration(id, token) {
  return (generation[id] || 0) === token
}
