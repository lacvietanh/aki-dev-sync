import { ref, watch } from 'vue'
import { projects } from '../store/projectStore'
import { refreshSettings, triggerManualRefresh } from '../store/refreshStore'
import { syncCheckEnabled } from '../store/syncCheckStore'
import { checkAllSyncStatus, checkProjectSyncStatus } from './useSyncStatus'
import { fetchGitStatus } from './useGit'
import { fetchProjectStack } from './useProjectStack'

// ---------------------------------------------------------------------------
// Refresh controller.
//
// There is exactly ONE unit of work in this app's status layer - "refresh
// project X" - and exactly one place that schedules it. Everything that can
// cause a refresh is a caller of the same unit:
//
//   background git timer   → fetchGitStatus            for every project
//   background diff timer  → checkProjectSyncStatus    for every project
//   per-project button     → refreshProject(p)         for one project
//   global header button   → refreshAllProjects()      = refreshProject per project
//   saving a project's config → refreshProject(p)
//
// Because each check owns its own busy state (beginRefresh/endRefresh in
// projectStore.js), all of the above light up the same per-project indicator
// with no special-casing per trigger. The header button's own spinner reads
// `anyRefreshing`, derived from those same counters - so it spins on a
// background tick too, not only when a human clicked it.
//
// This replaced an arrangement where the global button called loadData() (a
// full app reload: projects.json, SSH hosts, IDE availability) and got its
// "everything dims" effect from loadData's global `isReloading` flag, while
// the background timers had no visible state at all. Those were two unrelated
// mechanisms that merely looked like one feature.
// ---------------------------------------------------------------------------

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

// Gated on syncCheckEnabled, not just left running to no-op inside checkProjectSyncStatus: with
// sync check off there is nothing this cycle should be doing at all, so it should not exist as a
// live timer either - matches the "off means off" behavior already expected of the PUSH/PULL
// fieldset in ProjectTable.vue, at the controller level instead of only inside the leaf function.
function restartDiffTimer() {
  if (diffTimer) clearInterval(diffTimer)
  diffTimer = null
  const s = refreshSettings.value.remote_diff_interval_s
  if (s > 0 && syncCheckEnabled.value) {
    diffRefreshKey.value++ // restart ring animation with new duration
    diffTimer = setInterval(() => {
      checkAllSyncStatus()
      diffRefreshKey.value++
    }, s * 1000)
  }
}

// The unit of work: everything that is derived state for one project. Runs its checks in
// parallel; each reports its own busy state, so a slow remote diff keeps the indicator lit after
// the fast local git check has already landed.
export function refreshProject(project, { silent = true } = {}) {
  return Promise.all([
    fetchGitStatus(project.id, silent),
    checkProjectSyncStatus(project),
    fetchProjectStack(project.id),
  ])
}

// The global Refresh button. Fans out the same unit to every project in parallel and restarts the
// ring cycles so the countdown reflects the refresh that just happened. Also pokes the usage
// monitors, which are a separate subsystem with its own polling.
export function refreshAllProjects() {
  restartGitTimer()
  restartDiffTimer()
  const all = Promise.all(projects.value.map(p => refreshProject(p)))
  triggerManualRefresh()
  return all
}

let watching = false

export function startBackgroundRefresh() {
  restartGitTimer()
  restartDiffTimer()
  if (!watching) {
    watch(() => refreshSettings.value.git_interval_s, restartGitTimer)
    watch(() => refreshSettings.value.remote_diff_interval_s, restartDiffTimer)
    // Toggling sync check on/off must cleanly tear down and rebuild the diff cycle, not leave a
    // stale timer running and rely on the leaf function to silently no-op. Turning back on also
    // runs one check immediately instead of waiting out the rest of the interval - the same
    // "resume now, don't wait" behavior the ring's own restart already gives on an interval change.
    watch(syncCheckEnabled, (enabled) => {
      restartDiffTimer()
      if (enabled) checkAllSyncStatus()
    })
    watching = true
  }
}
