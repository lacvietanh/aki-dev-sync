import { invoke } from '../utils/tauri'
import { projects, projectRuntime, currentEpoch, beginRefresh, endRefresh } from '../store/projectStore'
import { syncCheckEnabled } from '../store/syncCheckStore'
import { useLogs } from './useLogs'

// One of the three per-project status checks. Like fetchGitStatus, it reports its own busy state
// through the shared beginRefresh/endRefresh counter (projectStore.js), so every trigger - the
// background diff timer, the per-project Refresh button, the global Refresh - drives the same
// visible indicator. The epoch check discards a result for a project whose host/path changed, or
// whose sync check was turned off, mid-flight - see bumpEpoch in projectStore.js.
export async function checkProjectSyncStatus(project) {
  if (!syncCheckEnabled.value) return
  if (projectRuntime.value[project.id]?.syncing) return
  // beginRefresh first - see fetchGitStatus.
  beginRefresh(project.id)
  const epoch = currentEpoch(project.id)
  try {
    const result = await invoke('check_sync_status', { project })
    if (currentEpoch(project.id) !== epoch) return // stale - superseded mid-flight, discard silently
    const current = projectRuntime.value[project.id]
    // The runtime entry IS the project's liveness record (projectStore: a missing entry reports
    // epoch 0 = removed). Writing one back for a project that no longer has one resurrects it -
    // §3.2's bug, whose other half was new projects starting at epoch 0. beginRefresh guarantees
    // this entry exists for a live project, so `undefined` here means exactly one thing.
    if (!current) return

    { // (block kept so the logging below stays where it was - `current` is now always defined here)
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
          appendLog(project.id, `[${time}] [Background] ⚠ DIVERGED - Local (${result.push_count}) & Remote (${result.pull_count}) both have changes. Resolve before syncing.`)
        } else if (pushChanged) {
          appendLog(project.id, `[${time}] [Background] Local changes detected (${result.push_count} file(s)). Ready to PUSH.`)
        } else if (pullChanged) {
          appendLog(project.id, `[${time}] [Background] Remote changes detected (${result.pull_count} file(s)). Ready to PULL.`)
        }
      }
    }

    projectRuntime.value[project.id] = {
      ...current,
      hasPendingPush: result.has_local_changes,
      hasPendingPull: result.has_remote_changes,
      pushCount: result.push_count ?? 0,
      pullCount: result.pull_count ?? 0,
    }
  } catch (_) {
    // SSH/network error - leave hasPendingPush/Pull unchanged so buttons don't flicker.
  } finally {
    // Only the generation that started this counts its own completion - see fetchGitStatus.
    if (currentEpoch(project.id) === epoch) endRefresh(project.id)
  }
}

// Background-driven fan-out only, with per-project skip (PROJ-TOGGLE) - manual actions go through checkProjectSyncStatus directly; see docs/feat/background-refresh.md.
export async function checkAllSyncStatus() {
  await Promise.all(projects.value.filter(p => !p.disabled).map(p => checkProjectSyncStatus(p)))
}
