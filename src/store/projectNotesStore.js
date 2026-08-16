// Per-project tasks & notes mirrored in-memory copy of <local_path>/.akidevsync/notes.json (docs/plan/done/1.22.0-notes-json-ssot.md).
// WHY STORE (NOT COMPOSABLE): services/mirror.js auto-mirrors src/store/ isRef exports host→companion; paired phone lacks filesystem/read commands so store placement provides phone notes for free.
import { ref } from 'vue'

/**
 * `{ [projectId]: { status, notes, tasks, updated_at, error } }`
 * status: Rust ProjectNotesStatus ('ok' | 'missing' | 'unavailable' | 'corrupt') + 'unknown'. Only 'ok' and 'missing' are writable (isProjectNotesWritable) — others force read-only to prevent saving empty notes over real ones.
 */
export const projectNotes = ref({})

/** Frozen entry shape for unread id to prevent accidental mutation — every entry must arrive via setProjectNotesEntry. */
const UNKNOWN_ENTRY = Object.freeze({
  status: 'unknown',
  notes: '',
  tasks: [],
  updated_at: 0,
  error: '',
})

// ── The three id-scoped accessors ───────────────────────────────────────────────────────────────
// REGRESSION GUARD (CLAUDE.md multi-entity): projectNotes is a MAP OF ENTITIES. Functions affect only the given id; whole-store wipes (projectNotes.value = {}) are forbidden.

/** Replace ONE project's entry. */
export function setProjectNotesEntry(id, entry) {
  if (!id) return
  // New object identity, not an in-place field write: mirror.js watches this ref and a nested mutation on the same object would not always produce a delta for the companion.
  projectNotes.value = { ...projectNotes.value, [id]: { ...UNKNOWN_ENTRY, ...entry } }
}

/** Read ONE project's entry, or the frozen unknown default (never returns undefined). */
export function getProjectNotesEntry(id) {
  return projectNotes.value[id] || UNKNOWN_ENTRY
}

/** Remove ONE project's entry (scoped by id, called from removeProject). */
export function dropProjectNotesEntry(id) {
  bumpProjectNotesGeneration(id)
  if (!id || !(id in projectNotes.value)) return
  const next = { ...projectNotes.value }
  delete next[id]
  projectNotes.value = next
}

// ── Per-id generation token: guards against late async read_project_notes results overwriting newer edits (1.20.0 stale read regression) ──────────────────────────
// Deliberately non-reactive plain object — internal read bookkeeping, never rendered by UI.
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

