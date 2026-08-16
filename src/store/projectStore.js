import { ref, computed } from 'vue'
import Swal from 'sweetalert2'
import { invoke } from '../utils/tauri'
import { action } from '../services/action'

export const Toast = Swal.mixin({
  toast: true,
  position: 'top-end',
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

// Live Terminal.app count per project (docs/plan/done/terminal-ownership-model.md §5): atomic snapshot mirrored to companion.
export const externalTermCounts = ref({})

// Count of external Terminal.app sessions belonging to no listed project (global badge).
export const externalTermGlobalCount = ref(0)

// Funnel for external Terminal.app launch: runs on host via action(), schedules immediate rescan.
// Dynamic import of useExternalTerminals avoids circular dependency at store bootstrap.
export const registerExternalTerminalLaunch = action(
  'projectStore.registerExternalTerminalLaunch',
  async ({ owner = null, path = null } = {}) => {
    try {
      await invoke('open_local_terminal', { localPath: path, owner })
      const { scheduleExternalTermRescan } = await import('../composables/useExternalTerminals')
      scheduleExternalTermRescan()
    } catch (e) {
      console.error('[projectStore] external terminal launch failed', e)
    }
  }
)

export const isReloading = ref(false)

// Session-only removed project IDs preventing zombie resurrection (docs/plan/done/1.20.1-flow-audit-fixes.md §3.3).
export const removedProjectIds = ref(new Set())

// Records project removal for this id only; never clears existing set entries.
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

// ICON-1 (docs/plan/done/remote-control.md §7.0): complete map `{ [projectId]: dataUri | null }` mirrored to companions.
export const projectIcons = ref({})

// Host-side icon map loader; companions receive updates via state mirror.
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

// Per-project refresh counter tracking concurrent in-flight status checks (git, remote diff, stack info).
export function beginRefresh(id) {
  const current = projectRuntime.value[id]
  projectRuntime.value[id] = {
    ...current,
    epoch: current?.epoch ?? 1,
    refreshCount: (current?.refreshCount || 0) + 1,
  }
}

// Decrements refreshCount only if runtime entry exists (avoids resurrecting removed projects).
export function endRefresh(id) {
  const current = projectRuntime.value[id]
  if (!current) return
  projectRuntime.value[id] = { ...current, refreshCount: Math.max(0, (current.refreshCount || 1) - 1) }
}

export function isRefreshing(id) {
  return (projectRuntime.value[id]?.refreshCount || 0) > 0
}

// Drives global Refresh button; true if any background or manual status check is in flight.
export const anyRefreshing = computed(() =>
  Object.values(projectRuntime.value).some(r => (r.refreshCount || 0) > 0)
)

// Generation token cancelling async status checks; bumpEpoch invalidates in-flight runs and clears refreshCount.
// For read-only status checks only — never used for rsync push/pull cancellations.
export function bumpEpoch(id) {
  const current = projectRuntime.value[id]
  const epoch = (current?.epoch ?? 0) + 1
  projectRuntime.value[id] = { ...current, epoch, refreshCount: 0 }
  return epoch
}

// Returns project epoch (>= 1) or 0 if runtime state is gone (project removed).
export function currentEpoch(id) {
  return projectRuntime.value[id]?.epoch ?? 0
}
