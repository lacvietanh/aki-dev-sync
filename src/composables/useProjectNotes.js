// Read/hydrate layer for per-project tasks & notes (docs/plan/done/1.22.0-notes-json-ssot.md §3).
// Host-only: reads/hydrates notes.json; writes funnel through applyTaskEdit in remoteActions.js (PERSIST-1).
// Companions read mirrored projectNotesStore.js and refresh via requestProjectNotesRefresh action.
import { invoke } from '../utils/tauri'
import {
  setProjectNotesEntry,
  getProjectNotesEntry,
  bumpProjectNotesGeneration,
  isCurrentProjectNotesGeneration,
} from '../store/projectNotesStore'

/** Flattens Rust `ProjectNotesRead` into store entry; non-ok status yields safe empty content. */
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

/** Single predicate for write permission; non-ok reads are read-only to prevent overwriting empty content. Missing is writable. */
export function isProjectNotesWritable(id) {
  const s = getProjectNotesEntry(id).status
  return s === 'ok' || s === 'missing'
}

/** What the task/notes UI renders for one project. */
export function projectNotesFor(id) {
  const e = getProjectNotesEntry(id)
  return { notes: e.notes, tasks: e.tasks }
}

/** Boot hydrate: single read_project_notes_map round-trip for all projects rather than N. */
export async function hydrateProjectNotes(projectList) {
  const targets = (projectList || [])
    .filter((p) => p?.id && p?.local_path)
    .map((p) => ({ id: p.id, local_path: p.local_path }))
  if (targets.length === 0) return
  // Same staleness guard as single read below, one token per id against in-flight race.
  const tokens = new Map(targets.map((t) => [t.id, bumpProjectNotesGeneration(t.id)]))
  try {
    const map = await invoke('read_project_notes_map', { targets })
    for (const [id, read] of Object.entries(map || {})) {
      if (!isCurrentProjectNotesGeneration(id, tokens.get(id))) continue
      setProjectNotesEntry(id, toEntry(read))
    }
  } catch (e) {
    // Failed batch does not mark projects unavailable to avoid false read-only state; entries stay unknown.
    console.error('[projectNotes] hydrate failed', e)
  }
}

/** Re-read ONE project on modal open or local_path change; scoped to id (multi-entity guard). */
export async function refreshProjectNotes(id, localPath) {
  if (!id || !localPath) return
  // Discard stale read resolving after an edit to prevent reverting UI and overwriting content.
  const token = bumpProjectNotesGeneration(id)
  try {
    const read = await invoke('read_project_notes', { localPath })
    if (!isCurrentProjectNotesGeneration(id, token)) return
    setProjectNotesEntry(id, toEntry(read))
  } catch (e) {
    // Single failed read marks entry unavailable so UI goes read-only instead of saving unreadable state.
    console.error('[projectNotes] refresh failed', e)
    if (!isCurrentProjectNotesGeneration(id, token)) return
    // Merge over existing entry so transient IPC error preserves displayed notes while marking unavailable.
    setProjectNotesEntry(id, {
      ...getProjectNotesEntry(id),
      status: 'unavailable',
      error: String(e?.message || e),
    })
  }
}

/** Idempotent one-time migration of legacy projects.json tasks/notes into repo notes.json file. */
export async function migrateLegacyProjectNotes(projectList) {
  let changed = false
  for (const p of projectList || []) {
    const legacyTasks = Array.isArray(p?.tasks) ? p.tasks : []
    const legacyNotes = typeof p?.notes === 'string' ? p.notes : ''
    const hasLegacyKey =
      Object.prototype.hasOwnProperty.call(p || {}, 'tasks') ||
      Object.prototype.hasOwnProperty.call(p || {}, 'notes')
    if (!hasLegacyKey) continue
    // Drop legacy keys if empty so project stops being a migration candidate.
    if (legacyTasks.length === 0 && legacyNotes === '') {
      delete p.tasks
      delete p.notes
      changed = true
      continue
    }

    const entry = getProjectNotesEntry(p.id)
    // Never migrate against unreadable dir: write is refused and deleting legacy fields destroys the only copy.
    if (entry.status !== 'ok' && entry.status !== 'missing') continue

    // Existing repo file content wins over legacy fields (SSOT).
    const fileHasContent = entry.status === 'ok' && (entry.tasks.length > 0 || entry.notes !== '')
    if (fileHasContent) {
      delete p.tasks
      delete p.notes
      changed = true
      continue
    }

    // Dynamic import avoids cycle; applyTaskEdit maintains the single-writer funnel (PERSIST-1).
    const { applyTaskEdit } = await import('../store/remoteActions')
    const res = await applyTaskEdit(p.id, { notes: legacyNotes, tasks: legacyTasks })
    // Delete legacy fields only after write succeeds; failed writes keep legacy copy intact.
    if (res) {
      delete p.tasks
      delete p.notes
      changed = true
    }
  }
  return changed
}
