import { invoke } from '@tauri-apps/api/core'
import { projects, projectRuntime } from '../store/projectStore'

const POLL_INTERVAL_MS = 60_000

let pollTimer = null

export async function checkProjectSyncStatus(project) {
  if (projectRuntime.value[project.id]?.syncing) return
  try {
    const result = await invoke('check_sync_status', { project })
    projectRuntime.value[project.id] = {
      ...projectRuntime.value[project.id],
      hasPendingPush: result.has_local_changes,
      hasPendingPull: result.has_remote_changes,
    }
  } catch (_) {
    // SSH/network error — leave state unchanged so buttons don't flicker
  }
}

export async function checkAllSyncStatus() {
  await Promise.all(projects.value.map(p => checkProjectSyncStatus(p)))
}

export function startSyncStatusPolling() {
  if (pollTimer) return
  pollTimer = setInterval(checkAllSyncStatus, POLL_INTERVAL_MS)
}
