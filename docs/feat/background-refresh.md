# Background Refresh

Automatic background polling that keeps three independent data types fresh without user interaction. Each type has a different cost profile and refresh interval.

> **Scope**: this doc covers *what* each refresh type fetches and what it costs. For *who triggers a
> refresh, where the busy indicator comes from, and how an in-flight check is cancelled*, see
> [docs/arch/refresh-controller.md](../arch/refresh-controller.md) (with flowcharts).

## The three refresh types

### 1. Git Status (`git_interval_s`)

**What it fetches:** Local git info - branch, dirty status, recent log. Runs entirely locally via `git` CLI.

**Cost:** Negligible. No SSH, no network. Completes in ~50ms.

**Trigger:** `git_interval_s` timer in `useBackgroundRefresh.js`, plus every `refreshProject()` call (per-project button, global button, `saveConfig`, app mount) and after a sync completes.

**Implementation:** `useGit.js` → `fetchGitStatus(projectId)` → Tauri command `get_git_info`.

**Interval:** 60s.

---

### 2. Remote Diff (`remote_diff_interval_s`)

**What it fetches:** Whether local and remote have diverged - serves the Push/Pull button highlight state. Runs `rsync --dry-run` in both directions (push and pull) via SSH.

**Cost:** Medium-high. Each check spawns two SSH+rsync processes. For N projects, `checkAllSyncStatus()` runs N×2 rsync processes sequentially.

**Trigger:** the `remote_diff_interval_s` timer in `useBackgroundRefresh.js` (60s), plus every `refreshProject()` call. Also triggered 3s after a real sync completes.

**Implementation:** `useSyncStatus.js` → `checkProjectSyncStatus(project)` → Tauri command `check_sync_status` → `count_rsync_changes()` in `sync.rs`.

**Gated by sync check:** `checkProjectSyncStatus()` early-returns if `syncCheckEnabled` (see [sync-check-and-usage-switches.md](sync-check-and-usage-switches.md)) is off - covers this interval poll and the manual Refresh path in one place, since both call the same function.

**Result:** `hasPendingPush` and `hasPendingPull` written into `projectRuntime`. On startup both are initialized to `null` (not `undefined`) - buttons render in a faint "checking" state (`.btn-sync-checking`) until the first check resolves. After that: `true` → fully lit, `false` → dim (`.btn-sync-clean`).

**Planned interval:** 60s (unchanged).

#### The `.git/` directory mtime problem

When `sync_git: true`, `.git/` is included in the rsync dry-run. This caused the Push button to be **permanently lit** even immediately after a clean push.

Root cause: `git status` - and any git-aware tool (IDE background check, git hooks) - **writes** to `.git/index` during normal operation. Git uses the index to cache `stat()` metadata of tracked files; when that cache is stale, git refreshes it and writes the updated entry back to disk. This is called an *index refresh*. The write changes the mtime of `.git/index`, which in turn changes the mtime of the `.git/` directory itself.

rsync sees `.git/` as modified and lists it in dry-run output → `count_rsync_changes` returned 1 → push button always lit.

**Fix (`sync.rs` - `count_rsync_changes`):** Filter out all directory entries (lines ending with `/`) from the rsync output count. Only actual file changes increment the count.

This is safe because rsync always lists both the directory AND the changed files inside it when real file changes exist. If a commit adds `.git/COMMIT_EDITMSG` and updates `.git/index`, the output contains:

```
.git/              ← directory entry, filtered out
.git/COMMIT_EDITMSG   ← file, counted ✓
.git/index            ← file, counted ✓
```

When only directory mtime changes (index refresh with no content change):
```
.git/              ← directory entry, filtered out → count = 0 ✓
```

This gives accurate signal: Push button lights up for real commits and file changes, not for background git housekeeping.

---

### 3. Agent Usage (`usage_interval_s`)

**What it fetches:** Claude Code and Antigravity quota/usage data - locally on this machine and/or from a selected remote host.

**Sources:** one **UsageMonitor** per `(agent, machine)` pair, created on demand by `usageMonitorRegistry.getMonitor()` and keyed `agentId@host` (`antigravity@local`, `claudecode@devbox`, …). There is no fixed set: pointing a display slot at a second SSH host creates a second monitor for that host, so several remote machines can be watched at once, each with its own account. A monitor's machine is immutable - it is half of its identity - and only its `enabled` flag varies. Polling is decoupled from which `AgentUsageSlot` (if any) currently displays it, and two slots naming the same pair share the one monitor, so the display never doubles the poll rate. Each monitor's switch is persisted per id in `store/usageMonitorStore.js`, independent of the sync-check switch (see [sync-check-and-usage-switches.md](sync-check-and-usage-switches.md)).

**Cost:** Local monitors (`host = 'local'`) run a local shell/`zsh -lc node`, no network. A remote monitor is one SSH `cat`/probe per interval per host, only while it is switched on and its slot has a host selected.

**Trigger:** A monitor starts when it is switched on (default ON) and stops when switched off, keeping its last reading on screen as *Cached*. Polls every 30s. Monitors are session-lived - deliberately not torn down on component unmount, since they outlive whichever slot first asked for them.

**Implementation:** `usageMonitor.js` (the entity) + `usageMonitorRegistry.js` (identity) → Tauri command `get_agent_usage`, dispatched local-vs-SSH inside `agent_usage.rs::run_interpreter_timeout` (renamed from `run_remote_script_timeout` in 1.12.0) via `is_local_host(host)`. Design rationale: [docs/plan/usage-monitor-entity-refactor.md](../plan/usage-monitor-entity-refactor.md).

**Planned interval:** 30s (current) - acceptable since it's a single lightweight read.

---

## Current state vs. planned

| Capability | Status |
|---|---|
| Remote diff polling (60s) | ✅ Implemented |
| Agent usage polling (30s) | ✅ Implemented |
| `.git/` directory filter fix | ✅ Implemented |
| `null` init → no false-active on startup | ✅ Implemented |
| Git status polling (60s) | ✅ Implemented |
| Unified `useBackgroundRefresh` singleton | ✅ Implemented - see [refresh-controller.md](../arch/refresh-controller.md) |
| Per-project busy indicator + check cancellation | ✅ Implemented - see [refresh-controller.md](../arch/refresh-controller.md) |
| Per-type configurable intervals | ⬜ Not yet |
| Settings modal in titlebar | ⬜ Not yet |
| Auto-refresh silent log mode | ✅ Implemented |
| Sleep/wake self-heal | ✅ Implemented - see below |

---

## Sleep/wake self-heal

WKWebView suspends and throttles `setInterval` when the window is occluded, minimised, or the Mac
sleeps. The timers simply stop, and nothing downstream notices: project status freezes on whatever it
last showed, which is the app quietly lying about the state of a repo. A suspended interval does not
reliably resume ticking on its own once the window is visible again either.

There is **one** wake mechanism in the app, and it lives with the usage monitor
(`usageMonitor.js`, `subscribeWake`) because that is where it was built first. `useBackgroundRefresh`
subscribes to it rather than running a second heartbeat — two watchdogs would double the wake-up cost
and mean two places to reason about when recovery misbehaves. It drives recovery from two signals:

1. `visibilitychange` / `focus` — the moment the user looks back at the app.
2. A 7 s watchdog heartbeat — catches a suspend that flips neither DOM event.

**This is not refresh-on-focus.** Every path gap-checks first: a cycle re-runs only if it has actually
missed ticks, measured at 2× its own configured interval (one missed tick is scheduler jitter, two is
a suspend). Alt-tabbing through the app therefore costs nothing. The threshold is per-subscriber, so
a cycle that has legitimately backed off is not mistaken for a suspend, and a cycle whose timer is
switched off reports `0` and is skipped rather than silencing the whole heartbeat.

Host-only (`utils/scheduler.js`, seam P): a companion produces nothing of its own — its status data
arrives over the mirror, so a wake there must not start firing checks.

One thing is deliberately **not** gap-gated: the Claude Code profile is re-read on every
visibility/focus wake. It is a cheap local file read and the one piece of derived state that can
change while the app is asleep with no event reaching it — someone editing `~/.claude/settings.json`,
or another tool switching the profile — so without this the UI keeps claiming the old mode.

---

## Planned: unified settings

Settings shape (to be persisted in localStorage or Tauri store):

```js
refreshSettings = {
  git_interval_s: 60,
  remote_diff_interval_s: 60,
  usage_interval_s: 30,
}
```

The `useBackgroundRefresh` module-singleton already manages the git and diff timers this way - a
`watch` per interval clears and restarts only the affected timer, and `0` disables that type. What
is still missing is a UI to edit these values (they currently come from `refreshStore` defaults)
and bringing the usage poll under the same roof.

A settings icon (gear) next to the Reload button in `AppHeader.vue` opens a `RefreshSettingsModal` with three numeric inputs and descriptions of what each type checks and how expensive it is.

---

## Log behavior

**User-triggered reload:** one summary line - `"Loaded N projects successfully."` No per-project git detail.

**Auto-refresh:** completely silent. Errors surface as `[ERROR]` only.

Current code still logs per-project GIT lines during `loadData`. Planned cleanup: remove `appendGlobalLog("GIT", ...)` calls from `fetchGitStatus`; let `loadData` emit a single summary after `Promise.all`.
