import { ref } from 'vue'

// Hard cap on retained log lines — 2,000 per project and 2,000 global, dropped from the HEAD
// (docs/plan/1.20.1-flow-audit-fixes.md §3.14, contract C-2).
//
// C-2, one cap one owner: this store is the ONLY place log lines are trimmed. `AppConsole.vue`
// must not add its own trimming — it only decides how much of the (already capped) array it
// renders. Nothing tells the user the log was truncated: what a person wants after a 5,000-line
// rsync is the tail — the error and what came just before it — which the cap preserves by
// construction, and a "log truncated" row would cost a row (CLAUDE.md *UI Extreme Narrow*).
export const LOG_CAP = 2000

// Shared: the same lines are meant to be readable on the Mac and on a paired phone.
export const globalLogs = ref([])
export const projectLogs = ref({})

// PER-SCREEN, deliberately NOT mirrored (§3.12). Which panel a screen has open is that screen's
// own choice: `services/mirror.js` PER_SCREEN_KEYS excludes both from the wire, so a phone
// connecting can no longer yank the Mac's panel shut (and vice versa). Nothing here needs an
// `action()` wrapper for the same reason — wrapping would send the toggle to the Mac, which is
// exactly the cross-screen bleed §3.12 removes.
export const activeLogProjectId = ref(null)
export const isLogExpanded = ref(false)
export const consoleRef = ref(null)
export const copied = ref(false)
export let globalListener = null
export function setGlobalListener(fn) { globalListener = fn }

// Monotonic count of lines ever appended through the helpers below — it does NOT decrease when the
// cap drops lines from the head, which is what makes it usable as a delta cursor. `services/mirror.js`
// diffs against it to send only the lines added since the last frame instead of re-encoding the whole
// log map per line (the quadratic transport in §3.14).
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
