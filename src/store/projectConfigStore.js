// Per-project settings mirrored in-memory copy of <local_path>/.akidevsync/project.json (docs/plan/settings-and-state-layout.md).
// Same shape and reasoning as projectNotesStore.js: a map of entities, one entry per project id.
import { ref } from 'vue'

/**
 * `{ [projectId]: { status, name, production_url, pull_excludes, push_excludes, commands, error } }`
 * status: Rust ProjectConfigStatus ('ok' | 'missing' | 'unavailable' | 'corrupt') + 'unknown'.
 */
export const projectConfigs = ref({})

/** Frozen entry shape for unread id to prevent accidental mutation — every entry must arrive via setProjectConfigEntry. */
const UNKNOWN_ENTRY = Object.freeze({
  status: 'unknown',
  name: '',
  production_url: '',
  pull_excludes: [],
  push_excludes: [],
  commands: Object.freeze({ dev: '', build: '' }),
  error: '',
})

// ── The three id-scoped accessors ───────────────────────────────────────────────────────────────
// REGRESSION GUARD (CLAUDE.md multi-entity): projectConfigs is a MAP OF ENTITIES. Functions affect only the given id; whole-store wipes (projectConfigs.value = {}) are forbidden.

/** Replace ONE project's entry. */
export function setProjectConfigEntry(id, entry) {
  if (!id) return
  projectConfigs.value = { ...projectConfigs.value, [id]: { ...UNKNOWN_ENTRY, ...entry } }
}

/** Read ONE project's entry, or the frozen unknown default (never returns undefined). */
export function getProjectConfigEntry(id) {
  return projectConfigs.value[id] || UNKNOWN_ENTRY
}

/**
 * Resets every already-known entry's status to 'unknown' - called by `loadData` right before it replaces
 * `projects.value` with a fresh load, so a cached 'ok' can never outlive the project object it was read
 * against (T1, docs/plan/settings-and-state-layout.md: a Refresh must not let PUSH/PULL/status-check run
 * against a stale-`ok`, freshly-stripped object during the window before `hydrateProjectConfig` completes).
 * Every id is touched because this runs once per whole-list reload, not as a scoped single-entity action.
 */
export function markAllProjectConfigStatusesUnknown() {
  const next = {}
  for (const [id, entry] of Object.entries(projectConfigs.value)) {
    next[id] = { ...entry, status: 'unknown' }
  }
  projectConfigs.value = next
}

/** Remove ONE project's entry (scoped by id, called from removeProject). */
export function dropProjectConfigEntry(id) {
  bumpProjectConfigGeneration(id)
  if (!id || !(id in projectConfigs.value)) return
  const next = { ...projectConfigs.value }
  delete next[id]
  projectConfigs.value = next
}

// ── Per-id generation token: guards against a late async read_project_config result overwriting a newer one ──────────────────────────
const generation = Object.create(null)

/** Invalidate every read for this id that is currently in flight, and return the new token. */
export function bumpProjectConfigGeneration(id) {
  generation[id] = (generation[id] || 0) + 1
  return generation[id]
}

/** Is `token` still the current generation for this id, i.e. may this read's result be applied? */
export function isCurrentProjectConfigGeneration(id, token) {
  return (generation[id] || 0) === token
}

// ── Per-project write queue for `.akidevsync/project.json` ─────────────────────────────────────
// Serialises `write_project_config` calls per project id so a rapid Save + a companion's Save (or, since
// Slice E, the pycache exclude migration) never interleave two writes for the same project. Lives here
// (not in remoteActions.js, its original home) so `useProjectConfig.js` can share the same queue without a
// circular import - remoteActions.js already imports from useProjectConfig.js statically.
const configWriteChains = new Map()
export function queueConfigWrite(projectId, run) {
  const prev = configWriteChains.get(projectId) || Promise.resolve()
  // `.then(run, run)` — failed write must not poison chain for subsequent edits.
  const next = prev.then(run, run)
  configWriteChains.set(projectId, next.catch(() => {}))
  return next
}
