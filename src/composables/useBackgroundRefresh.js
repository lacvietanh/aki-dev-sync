import { ref, watch } from 'vue'
import { projects } from '../store/projectStore'
import { refreshSettings, triggerManualRefresh } from '../store/refreshStore'
import { checkAllSyncStatus } from './useSyncStatus'
import { fetchGitStatus } from './useGit'

let gitTimer = null
let diffTimer = null

export const gitRefreshKey = ref(0)
export const diffRefreshKey = ref(0)

function restartGitTimer() {
  if (gitTimer) clearInterval(gitTimer)
  gitTimer = null
  const s = refreshSettings.value.git_interval_s
  if (s > 0) {
    gitRefreshKey.value++ // restart ring animation with new duration
    gitTimer = setInterval(() => {
      projects.value.forEach(p => fetchGitStatus(p.id, true))
      gitRefreshKey.value++
    }, s * 1000)
  }
}

function restartDiffTimer() {
  if (diffTimer) clearInterval(diffTimer)
  diffTimer = null
  const s = refreshSettings.value.remote_diff_interval_s
  if (s > 0) {
    diffRefreshKey.value++ // restart ring animation with new duration
    diffTimer = setInterval(() => {
      checkAllSyncStatus()
      diffRefreshKey.value++
    }, s * 1000)
  }
}

let watching = false

// Trigger all 3 refresh types immediately — called by the manual REFRESH button
export function refreshAll() {
  projects.value.forEach(p => fetchGitStatus(p.id, true))
  checkAllSyncStatus()
  triggerManualRefresh()
}

export function startBackgroundRefresh() {
  restartGitTimer()
  restartDiffTimer()
  if (!watching) {
    watch(() => refreshSettings.value.git_interval_s, restartGitTimer)
    watch(() => refreshSettings.value.remote_diff_interval_s, restartDiffTimer)
    watching = true
  }
}
