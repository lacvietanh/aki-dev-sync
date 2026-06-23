import { invoke } from '@tauri-apps/api/core'
import { projects, projectRuntime } from '../store/projectStore'
import { useLogs } from './useLogs'

export async function checkProjectSyncStatus(project) {
  if (projectRuntime.value[project.id]?.syncing) return
  try {
    const result = await invoke('check_sync_status', { project })
    const current = projectRuntime.value[project.id]

    if (current) {
      const wasPushNull = current.hasPendingPush === null
      const wasPullNull = current.hasPendingPull === null
      
      const pushChanged = current.hasPendingPush === false && result.has_local_changes === true
      const pullChanged = current.hasPendingPull === false && result.has_remote_changes === true

      const { appendLog } = useLogs()
      const time = new Date().toLocaleTimeString()

      // 1. Initial check log
      if (wasPushNull || wasPullNull) {
        appendLog(project.id, `[${time}] [Background] Initial state - Push: ${result.has_local_changes ? 'Yes' : 'No'} | Pull: ${result.has_remote_changes ? 'Yes' : 'No'}`)
      } else {
        // 2. Change detected log (False -> True)
        if (pushChanged && pullChanged) {
          appendLog(project.id, `[${time}] [Background] Local & Remote changes detected!`)
        } else if (pushChanged) {
          appendLog(project.id, `[${time}] [Background] Local changes detected! Ready to PUSH.`)
        } else if (pullChanged) {
          appendLog(project.id, `[${time}] [Background] Remote changes detected! Ready to PULL.`)
        }
      }
    }

    projectRuntime.value[project.id] = {
      ...current,
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
