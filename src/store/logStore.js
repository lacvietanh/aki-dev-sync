import { ref } from 'vue'

// Retained log cap (2,000/entity, head-dropped). Single trimming owner (C-2, docs/plan/done/1.20.1-flow-audit-fixes.md §3.14).
export const LOG_CAP = 2000

export const globalLogs = ref([])
export const projectLogs = ref({})

// Per-screen dock state: unmirrored (excluded via PER_SCREEN_KEYS, §3.12) so device panels toggle independently.
export const activeLogProjectId = ref(null)
export const isLogExpanded = ref(false)
// Invariant: console DOM and transient flash live in useLogs.js; store refs are auto-mirrored by mirror.js.
export let globalListener = null
export function setGlobalListener(fn) { globalListener = fn }

// Monotonic append cursor: unaffected by head-drop, used by mirror.js delta diffing (§3.14).
const _appended = { global: 0, projects: {} }

/** Snapshot of the append cursors. Plain data, safe to put on the wire. */
export function logAppendCounts() {
  return { global: _appended.global, projects: { ..._appended.projects } }
}

function pushCapped(arr, lines) {
  for (const line of lines) arr.push(line)
  if (arr.length > LOG_CAP) arr.splice(0, arr.length - LOG_CAP)
}

/** Append to the global event log, capped. The single funnel — see LOG_CAP. */
export function appendGlobalLogLines(lines) {
  if (!lines || lines.length === 0) return
  pushCapped(globalLogs.value, lines)
  _appended.global += lines.length
}

/** Append to one project's raw log, capped. The single funnel — see LOG_CAP. */
export function appendProjectLogLines(projectId, lines) {
  if (!projectId || !lines || lines.length === 0) return
  if (!projectLogs.value[projectId]) projectLogs.value[projectId] = []
  pushCapped(projectLogs.value[projectId], lines)
  _appended.projects[projectId] = (_appended.projects[projectId] || 0) + lines.length
}

/** Drop removed project log lines & append cursor (scoped to id, CLAUDE.md Multi-entity State). */
export function dropProjectLogs(projectId) {
  if (!projectId) return
  if (projectLogs.value[projectId]) {
    const next = { ...projectLogs.value }
    delete next[projectId]
    projectLogs.value = next
  }
  delete _appended.projects[projectId]
}
