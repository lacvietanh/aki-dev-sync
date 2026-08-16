// Read/hydrate layer for per-project tasks & notes (docs/plan/done/1.22.0-notes-json-ssot.md §3).
//
// Knows about the FILE's statuses; knows nothing about the UI. The write side is deliberately not
// here — it lives in `applyTaskEdit` (src/store/remoteActions.js), which is the one funnel PERSIST-1
// requires every task/notes mutation to pass through. Splitting reads here and the single write
// there is what makes acceptance criterion 1 ("exactly one call site for write_project_notes")
// checkable with a grep.
//
// HOST-ONLY BY CONSTRUCTION: every function below invokes a command that is not in
// COMPANION_ALLOWED_COMMANDS. Companions get this data through the mirrored
// `src/store/projectNotesStore.js` instead, and their refresh goes through
// `requestProjectNotesRefresh` (an action, so it runs on the Mac).
import { invoke } from '../utils/tauri'
import {
  setProjectNotesEntry,
  getProjectNotesEntry,
  bumpProjectNotesGeneration,
  isCurrentProjectNotesGeneration,
} from '../store/projectNotesStore'

/** Flattens Rust's `ProjectNotesRead` (`{ status, file, error }`) into the flat store entry the UI
 *  reads. `file` is `Some` only for `ok`, so every other status yields empty content — which is
 *  safe precisely because `isProjectNotesWritable` then refuses the write that would persist it. */
function toEntry(read) {
  const status = read?.status || 'unavailable'
  const f = read?.file
  return {
    status,
    notes: f?.notes || '',
    tasks: Array.isArray(f?.tasks) ? f.tasks : [],
    updated_at: f?.updated_at || 0,
    error: read?.error || '',
  }
}

/**
 * THE single predicate for "may this project's notes be written right now?".
 *
 * Every disabled/read-only state in the UI reads this one function rather than re-deriving the rule
 * from `status` — the rule is "a read that did not succeed makes the surface read-only", and it must
 * have exactly one spelling or one component will eventually get it backwards and re-open the
 * empty-then-overwrite hole.
 *
 * `missing` IS writable: a project that has never taken a note is the normal state, and the write
 * creates the directory and the file.
 */
export function isProjectNotesWritable(id) {
  const s = getProjectNotesEntry(id).status
  return s === 'ok' || s === 'missing'
}

/** What the task/notes UI renders for one project. */
export function projectNotesFor(id) {
  const e = getProjectNotesEntry(id)
  return { notes: e.notes, tasks: e.tasks }
}

/** Boot hydrate: ONE `read_project_notes_map` round-trip for the whole list rather than N.
 *  A project whose directory is unreadable gets its own `unavailable` entry — one unmounted volume
 *  must never blank every other project's badges. */
export async function hydrateProjectNotes(projectList) {
  const targets = (projectList || [])
    .filter((p) => p?.id && p?.local_path)
    .map((p) => ({ id: p.id, local_path: p.local_path }))
  if (targets.length === 0) return
  // Same staleness guard as the single read below, one token per id — a boot hydrate on a slow network mount can easily still be in flight when the user opens a modal and starts typing.
  const tokens = new Map(targets.map((t) => [t.id, bumpProjectNotesGeneration(t.id)]))
  try {
    const map = await invoke('read_project_notes_map', { targets })
    for (const [id, read] of Object.entries(map || {})) {
      if (!isCurrentProjectNotesGeneration(id, tokens.get(id))) continue
      setProjectNotesEntry(id, toEntry(read))
    }
  } catch (e) {
    // A failed BATCH says nothing about any individual project, so nothing is marked unavailable —
    // that would paint every project read-only over one transport hiccup. The entries stay
    // `unknown` (also non-writable) and the next refresh corrects them.
    console.error('[projectNotes] hydrate failed', e)
  }
}

/** Re-read ONE project — on modal open, and after a `local_path` change. Scoped to that id; no
 *  other project's entry is touched (multi-entity guard). */
export async function refreshProjectNotes(id, localPath) {
  if (!id || !localPath) return
  // See `bumpProjectNotesGeneration`'s doc comment: a read that resolves after an edit has already
  // been written must be DISCARDED, not applied — applying it reverts the edit on screen and then
  // persists the reverted content on the next save.
  const token = bumpProjectNotesGeneration(id)
  try {
    const read = await invoke('read_project_notes', { localPath })
    if (!isCurrentProjectNotesGeneration(id, token)) return
    setProjectNotesEntry(id, toEntry(read))
  } catch (e) {
    // Unlike the batch above, a single failed read IS about this project — record it so the UI goes read-only rather than offering to save into something we could not read.
    console.error('[projectNotes] refresh failed', e)
    if (!isCurrentProjectNotesGeneration(id, token)) return
    // MERGED over the existing entry, never spread over the empty default: whatever text is on
    // screen may be the user's only remaining copy at this moment, and a transient IPC hiccup must
    // not blank it (plan §4 property 2). Only the status and the reason change; the content stays
    // visible and selectable, and `unavailable` is what stops it being saved.
    setProjectNotesEntry(id, {
      ...getProjectNotesEntry(id),
      status: 'unavailable',
      error: String(e?.message || e),
    })
  }
}

/**
 * One-time migration of the legacy `projects.json` `tasks`/`notes` fields into the repo file.
 *
 * Runs on every `loadData`, beside `migratePushOnlyPaths` — this project's established home for a
 * one-time project-record migration, and the decision needs the on-disk read status, which the JS
 * side has in hand right after `hydrateProjectNotes`.
 *
 * IDEMPOTENT WITHOUT A FLAG, by construction: a project record with no `tasks`/`notes` key is by
 * definition already migrated (or was created after this shipped), so that branch touches nothing.
 * Rust enforces the one-way property — both fields are `Option` with
 * `skip_serializing_if = "Option::is_none"`, so once deleted they can never be re-materialized on
 * disk, not even by a stale companion array. No localStorage flag may be added here: a volatile flag
 * guarding durable data is the incident `migratePushOnlyPaths` documents.
 *
 * Returns whether the caller should persist the project list.
 */
export async function migrateLegacyProjectNotes(projectList) {
  let changed = false
  for (const p of projectList || []) {
    const legacyTasks = Array.isArray(p?.tasks) ? p.tasks : []
    const legacyNotes = typeof p?.notes === 'string' ? p.notes : ''
    const hasLegacyKey =
      Object.prototype.hasOwnProperty.call(p || {}, 'tasks') ||
      Object.prototype.hasOwnProperty.call(p || {}, 'notes')
    if (!hasLegacyKey) continue
    // Present but empty: nothing to move, just drop the keys so this project stops being a migration candidate on every launch.
    if (legacyTasks.length === 0 && legacyNotes === '') {
      delete p.tasks
      delete p.notes
      changed = true
      continue
    }

    const entry = getProjectNotesEntry(p.id)
    // NEVER migrate against a directory we could not read: writing is refused there anyway, and deleting the legacy fields on a failed write would destroy the only copy. Retry next launch.
    if (entry.status !== 'ok' && entry.status !== 'missing') continue

    // The file wins when it already has content — local repo is SSOT (plan §1). An `ok` but empty file is treated exactly like `missing`: there is nothing there to lose.
    const fileHasContent = entry.status === 'ok' && (entry.tasks.length > 0 || entry.notes !== '')
    if (fileHasContent) {
      delete p.tasks
      delete p.notes
      changed = true
      continue
    }

    // Through `applyTaskEdit`, NOT a direct `invoke('write_project_notes')`. PERSIST-1's funnel has
    // exactly one writer and a migration is not an exception to it — going around it would mean the
    // writable-guard, the store re-seed and the clobber report all have a second, divergent
    // implementation. Dynamic import because remoteActions.js imports this module (see its
    // isProjectNotesWritable import); the codebase resolves that direction lazily everywhere.
    const { applyTaskEdit } = await import('../store/remoteActions')
    const res = await applyTaskEdit(p.id, { notes: legacyNotes, tasks: legacyTasks })
    // Only after the write actually landed (`applyTaskEdit` returns null when refused or failed).
    // A failed write leaves the legacy fields in place — that is the copy the user still has.
    if (res) {
      delete p.tasks
      delete p.notes
      changed = true
    }
  }
  return changed
}
