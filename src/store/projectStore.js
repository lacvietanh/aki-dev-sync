import { ref, computed } from 'vue'
import Swal from 'sweetalert2'
import { invoke } from '../utils/tauri'

export const Toast = Swal.mixin({
  toast: true,
  position: 'bottom',
  showConfirmButton: false,
  timer: 3000,
  timerProgressBar: true,
  background: '#131317',
  color: '#e2e8f0',
  didOpen: (toast) => {
    toast.addEventListener('mouseenter', Swal.stopTimer)
    toast.addEventListener('mouseleave', Swal.resumeTimer)
  }
})

// Persisted config - synced with projects.json via save_projects
export const projects = ref([])

// Ephemeral runtime - never serialized, lost on restart (intentional: all derived)
// Shape: { [id]: { git_status, git_log, remote_url, syncing } }
export const projectRuntime = ref({})

export const isReloading = ref(false)

// Ids of projects removed during this session (docs/plan/1.20.1-flow-audit-fixes.md §3.3). A
// config modal that was already open when its project was removed — typically on the OTHER screen —
// still holds a full copy of it, and Save would re-enter `applyProjectConfig`'s "new project"
// branch and silently resurrect it. This is the record that lets that write be REJECTED instead:
// "id I have never seen" and "id the user just deleted" are otherwise indistinguishable.
//
// A ref export of a `src/store/*.js` module, so the mirror carries it to every companion for free
// (services/mirror.js encodes Set natively) — the phone can therefore make the same judgement its
// own modal needs without an extra round-trip. Ids are `project-<epoch-ms>`, never reused, and
// removals are hand gestures, so the set stays tiny; it is deliberately session-only (not
// persisted) — after a restart no modal is open, so nothing can resurrect anything.
export const removedProjectIds = ref(new Set())

/** Record that ONE project id was removed. Scoped to that id — never clears the set (multi-entity
 *  regression guard, CLAUDE.md): another project's removal record is not this removal's business. */
export function markProjectRemoved(id) {
  if (!id) return
  removedProjectIds.value.add(id)
}

export function isProjectRemoved(id) {
  return !!id && removedProjectIds.value.has(id)
}

// Preloaded IDE availability and cache-busting timestamp for icons
export const ideAvailability = ref(null)
export const iconTimestamp = ref(Date.now())

// ICON-1 (docs/plan/done/remote-control.md §7.0): { [projectId]: dataUri | null }, a COMPLETE map —
// every project id is present, with an explicit null when it has no icon, so a companion never
// retries or 404s on a missing key. Lives in the store precisely so the mirror carries it to the
// phone for free: the `aki-devsync-icon://` custom protocol the host `<img>`s use exists only
// inside the Tauri webview and resolves to nothing in a phone browser.
export const projectIcons = ref({})

/** Host-side fill. Companion never calls this — its copy arrives through the state mirror. */
export async function refreshProjectIcons() {
  try {
    projectIcons.value = await invoke('get_project_icons_map')
  } catch (e) {
    console.error('[projectStore] failed to load project icons', e)
  }
}

// True when any project is currently syncing - used by header/console
export const anySyncing = computed(() =>
  Object.values(projectRuntime.value).some(r => r.syncing)
)

// ---------------------------------------------------------------------------
// Per-project refresh state - ONE source of truth for "is this project's status
// being refreshed right now", shared by every trigger (background git/diff
// timers, the per-project Refresh button, the global Refresh button).
//
// It is a counter, not a boolean, because several independent checks (git
// status, remote diff, stack info) can be in flight for the same project at
// once and each must be able to say "I'm done" without cancelling the others.
// ---------------------------------------------------------------------------

// Also materializes `epoch` at 1 if this project has never had one, so that a captured epoch is
// always >= 1 and can never collide with the 0 that `currentEpoch` reports for a project whose
// runtime entry is gone (deleted mid-flight) - see currentEpoch below.
export function beginRefresh(id) {
  const current = projectRuntime.value[id]
  projectRuntime.value[id] = {
    ...current,
    epoch: current?.epoch ?? 1,
    refreshCount: (current?.refreshCount || 0) + 1,
  }
}

// A decrement must never CREATE a runtime entry: no entry means the project was removed while this
// check was in flight, and writing one back resurrects a project that no longer exists (§3.2 - the
// same shape as the result-write guard in useSyncStatus, fixed here so every caller inherits it).
export function endRefresh(id) {
  const current = projectRuntime.value[id]
  if (!current) return
  projectRuntime.value[id] = { ...current, refreshCount: Math.max(0, (current.refreshCount || 1) - 1) }
}

export function isRefreshing(id) {
  return (projectRuntime.value[id]?.refreshCount || 0) > 0
}

// Drives the header's global Refresh button, so it reports the exact same work
// the per-project buttons report - including work no human triggered (a
// background timer tick), which is the whole point of deriving it.
export const anyRefreshing = computed(() =>
  Object.values(projectRuntime.value).some(r => (r.refreshCount || 0) > 0)
)

// Generation token per project - the cancellation primitive for work that Tauri's `invoke()`
// itself cannot abort (git status, remote-diff checks: real network/subprocess round-trips with
// no cancel handle). Every check captures `currentEpoch(id)` before awaiting and re-checks it
// after; a stale result (epoch changed while it was in flight) is discarded silently - never
// written to `projectRuntime`, never counted as "finished" against the new generation.
//
// `bumpEpoch` ALSO force-resets `refreshCount` to 0 immediately - an instant UI cut (any spinning
// refresh icon stops right now) independent of whether the superseded call is still physically
// pending, because every event that calls this (a project's host/path edited, sync check turned
// off, projects reloaded from disk) means "whatever was in flight no longer applies," and the user
// should see that immediately rather than wait for the stale call to eventually resolve.
// This must never be used to cancel a real rsync push/pull - only read-only status checks.
export function bumpEpoch(id) {
  const current = projectRuntime.value[id]
  const epoch = (current?.epoch ?? 0) + 1
  projectRuntime.value[id] = { ...current, epoch, refreshCount: 0 }
  return epoch
}

// 0 means "this project has no runtime state" - it was removed. Since beginRefresh guarantees a
// live project's epoch is >= 1, a check that captured its epoch can detect removal with the same
// comparison it already uses for supersession, without a separate existence test.
export function currentEpoch(id) {
  return projectRuntime.value[id]?.epoch ?? 0
}
