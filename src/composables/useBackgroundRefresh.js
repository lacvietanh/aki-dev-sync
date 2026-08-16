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
// The app's single wake/self-heal mechanism lives with the usage monitor (it was built there first); this file subscribes rather than running a second heartbeat. See §3.21 of the flow-audit plan.
import { subscribeWake } from './usageMonitor'
import { startExternalTerminalWatch } from './useExternalTerminals'

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

// When each cycle last actually did its work. Read only by the wake self-heal below - a timer that has been suspended is indistinguishable from a healthy one except by how long ago it last ran.
let lastGitTickAt = Date.now()
let lastDiffTickAt = Date.now()

export const gitRefreshKey = ref(0)
export const diffRefreshKey = ref(0)

function runGitTick() {
  lastGitTickAt = Date.now()
  // Per-project skip (PROJ-TOGGLE) - manual git checks go through fetchGitStatus directly and are unaffected; see docs/feat/background-refresh.md.
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

// Gated on syncCheckEnabled, not just left running to no-op inside checkProjectSyncStatus: with
// sync check off there is nothing this cycle should be doing at all, so it should not exist as a
// live timer either - matches the "off means off" behavior already expected of the PUSH/PULL
// fieldset in ProjectTable.vue, at the controller level instead of only inside the leaf function.
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

// ---------------------------------------------------------------------------
// Wake self-heal.
//
// WKWebView suspends/throttles setInterval when the window is occluded, minimized, or the Mac
// sleeps - the two timers above simply stop, and nothing downstream notices: project status
// freezes on whatever it last showed, which is the app quietly lying about the state of a repo.
// This is the SAME failure the usage monitor already recovers from, and deliberately the same
// two-listener shape (docs/arch/usage-claudecode.md §4): visibility/focus for the moment the user
// looks back, plus a heartbeat for suspends that fire neither DOM event.
//
// It is NOT a refresh-on-focus feature: every path gap-checks first, so alt-tabbing through the app costs nothing. A cycle is only re-run if it has actually missed ticks.
//
// There is exactly ONE wake mechanism in the app - `usageMonitor.subscribeWake` - and this file is a
// subscriber, not a second implementation. Two heartbeats would mean two places to reason about when
// recovery misbehaves, which is the opposite of what a self-heal is for.
// ---------------------------------------------------------------------------

// 2x the configured interval: one missed tick is scheduler jitter, two is a suspend. A cycle whose timer is off reports 0, and the shared watchdog then skips it.
function gitGapThresholdMs() {
  return gitTimer ? 2 * refreshSettings.value.git_interval_s * 1000 : 0
}
function diffGapThresholdMs() {
  return diffTimer ? 2 * refreshSettings.value.remote_diff_interval_s * 1000 : 0
}

let wakeInstalled = false

function installWakeSelfHeal() {
  // Host-only: the companion produces nothing (utils/scheduler.js, seam P) - its status data arrives over the mirror, so a wake there must not start firing checks of its own.
  if (wakeInstalled || !isHost) return
  wakeInstalled = true

  // One subscription per cycle, so a stalled git timer is restarted without also restarting a diff timer that was ticking fine.
  //
  // The gap check is repeated inside onWake and is NOT redundant: the shared watchdog applies the
  // threshold before calling, but visibilitychange/focus do not - they wake every subscriber
  // unconditionally. Without this, alt-tabbing back into the app would re-run a full git scan across
  // every project each time. Status refresh is not a refresh-on-focus feature: it re-runs only when
  // ticks were genuinely missed.
  const stalled = (lastAt, thresholdMs) => thresholdMs > 0 && Date.now() - lastAt > thresholdMs
  subscribeWake({
    onWake: () => { if (stalled(lastGitTickAt, gitGapThresholdMs())) restartGitTimer({ runNow: true }) },
    lastTickAt: () => lastGitTickAt,
    gapThresholdMs: gitGapThresholdMs,
  })
  subscribeWake({
    onWake: (reason) => {
      if (stalled(lastDiffTickAt, diffGapThresholdMs())) restartDiffTimer({ runNow: true })
      // Cheap local read, and deliberately NOT gap-gated: it is the one piece of derived state that
      // can change while the app is asleep with no event reaching it (someone editing
      // ~/.claude/settings.json, or another tool switching the profile), so the UI would otherwise
      // keep claiming the old mode. Skipped on 'watchdog' - that fires on a timing gap, not on the
      // user actually coming back to look.
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
  // Its own fixed 5s cycle, not one of the two configurable ones: it costs three tiny local
  // subprocesses and its whole point is being fresh enough that closing a Terminal window is
  // reflected before you look back at the app. Host-only and idempotent inside.
  startExternalTerminalWatch()
  installWakeSelfHeal()
  if (!watching) {
    watch(() => refreshSettings.value.git_interval_s, () => restartGitTimer())
    watch(() => refreshSettings.value.remote_diff_interval_s, () => restartDiffTimer())
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
