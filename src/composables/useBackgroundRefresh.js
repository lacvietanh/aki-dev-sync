import { ref, watch } from 'vue'
import { hostInterval } from '../utils/scheduler'
import { isHost } from '../services/bridge'
import { projects } from '../store/projectStore'
import { refreshSettings, triggerManualRefresh } from '../store/refreshStore'
import { syncCheckEnabled } from '../store/syncCheckStore'
import { refreshClaudeMode } from '../store/claudeModeStore'
import { checkAllSyncStatus, checkProjectSyncStatus } from './useSyncStatus'
import { fetchGitStatus } from './useGit'
import { fetchProjectStack } from './useProjectStack'
// Subscribes to the usage monitor's wake/self-heal mechanism rather than running a duplicate heartbeat.
import { subscribeWake } from './usageMonitor'
import { startExternalTerminalWatch } from './useExternalTerminals'

// Refresh controller: git/diff timers and UI triggers share the unified `refreshProject` unit.

let gitTimer = null
let diffTimer = null

// Last work timestamps, used by wake self-heal to detect suspended timers.
let lastGitTickAt = Date.now()
let lastDiffTickAt = Date.now()

export const gitRefreshKey = ref(0)
export const diffRefreshKey = ref(0)

function runGitTick() {
  lastGitTickAt = Date.now()
  // Skip disabled projects for background ticks (manual checks bypass this filter).
  projects.value.filter(p => !p.disabled).forEach(p => fetchGitStatus(p.id, true))
  gitRefreshKey.value++
}

function restartGitTimer({ runNow = false } = {}) {
  if (gitTimer) clearInterval(gitTimer)
  gitTimer = null
  const s = refreshSettings.value.git_interval_s
  if (s > 0) {
    gitTimer = hostInterval(runGitTick, s * 1000)
    if (runNow) runGitTick()          // bumps the ring key itself
    else gitRefreshKey.value++        // restart ring animation with new duration
  }
}

// Gated on syncCheckEnabled to avoid running a live timer when sync checks are disabled.
function runDiffTick() {
  lastDiffTickAt = Date.now()
  checkAllSyncStatus()
  diffRefreshKey.value++
}

function restartDiffTimer({ runNow = false } = {}) {
  if (diffTimer) clearInterval(diffTimer)
  diffTimer = null
  const s = refreshSettings.value.remote_diff_interval_s
  if (s > 0 && syncCheckEnabled.value) {
    diffTimer = hostInterval(runDiffTick, s * 1000)
    if (runNow) runDiffTick()          // bumps the ring key itself
    else diffRefreshKey.value++        // restart ring animation with new duration
  }
}

// Parallel check of all derived state for one project; each check tracks its own busy state.
export function refreshProject(project, { silent = true } = {}) {
  return Promise.all([
    fetchGitStatus(project.id, silent),
    checkProjectSyncStatus(project),
    fetchProjectStack(project.id),
  ])
}

// Fans out refreshProject to all projects in parallel and restarts timer countdown rings.
export function refreshAllProjects() {
  restartGitTimer()
  restartDiffTimer()
  const all = Promise.all(projects.value.map(p => refreshProject(p)))
  triggerManualRefresh()
  return all
}

// Wake self-heal: recovers from WKWebView timer throttling after sleep/occlusion via gap checks.

// Gap threshold: 2x interval treats two missed ticks as a suspend; returns 0 if timer is off.
function gitGapThresholdMs() {
  return gitTimer ? 2 * refreshSettings.value.git_interval_s * 1000 : 0
}
function diffGapThresholdMs() {
  return diffTimer ? 2 * refreshSettings.value.remote_diff_interval_s * 1000 : 0
}

let wakeInstalled = false

function installWakeSelfHeal() {
  // Host-only: companion receives status via mirror and must not run independent checks.
  if (wakeInstalled || !isHost) return
  wakeInstalled = true

  // Gap check guards onWake: prevents unconditional focus/visibility events from re-running un-stalled timers.
  const stalled = (lastAt, thresholdMs) => thresholdMs > 0 && Date.now() - lastAt > thresholdMs
  subscribeWake({
    onWake: () => { if (stalled(lastGitTickAt, gitGapThresholdMs())) restartGitTimer({ runNow: true }) },
    lastTickAt: () => lastGitTickAt,
    gapThresholdMs: gitGapThresholdMs,
  })
  subscribeWake({
    onWake: (reason) => {
      if (stalled(lastDiffTickAt, diffGapThresholdMs())) restartDiffTimer({ runNow: true })
      // Non-gap-gated local read on user return (visibility/focus): syncs external settings edits.
      if (reason !== 'watchdog') refreshClaudeMode()
    },
    lastTickAt: () => lastDiffTickAt,
    gapThresholdMs: diffGapThresholdMs,
  })
}

let watching = false

export function startBackgroundRefresh() {
  restartGitTimer()
  restartDiffTimer()
  // Fixed 5s polling for active external terminals (host-only, idempotent).
  startExternalTerminalWatch()
  installWakeSelfHeal()
  if (!watching) {
    watch(() => refreshSettings.value.git_interval_s, () => restartGitTimer())
    watch(() => refreshSettings.value.remote_diff_interval_s, () => restartDiffTimer())
    // Toggling sync check tears down/rebuilds the diff timer and triggers an immediate check if enabled.
    watch(syncCheckEnabled, (enabled) => {
      restartDiffTimer()
      if (enabled) checkAllSyncStatus()
    })
    watching = true
  }
}
