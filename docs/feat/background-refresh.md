# Background Refresh

Automatic background polling that keeps three independent data types fresh without user interaction. Each type has a different cost profile and refresh interval.

> **Scope**: this doc covers *what* each refresh type fetches and what it costs. For *who triggers a
> refresh, where the busy indicator comes from, and how an in-flight check is cancelled*, see
> [docs/arch/refresh-controller.md](../arch/refresh-controller.md) (with flowcharts).

## The three refresh types

### 1. Git Status (`git_interval_s`)

**What it fetches:** Local git info — branch, dirty status, recent log. Runs entirely locally via `git` CLI.

**Cost:** Negligible. No SSH, no network. Completes in ~50ms.

**Trigger:** `git_interval_s` timer in `useBackgroundRefresh.js`, plus every `refreshProject()` call (per-project button, global button, `saveConfig`, app mount) and after a sync completes.

**Implementation:** `useGit.js` → `fetchGitStatus(projectId)` → Tauri command `get_git_info`.

**Interval:** 60s.

---

### 2. Remote Diff (`remote_diff_interval_s`)

**What it fetches:** Whether local and remote have diverged — serves the Push/Pull button highlight state. Runs `rsync --dry-run` in both directions (push and pull) via SSH.

**Cost:** Medium-high. Each check spawns two SSH+rsync processes. For N projects, `checkAllSyncStatus()` runs N×2 rsync processes sequentially.

**Trigger:** the `remote_diff_interval_s` timer in `useBackgroundRefresh.js` (60s), plus every `refreshProject()` call. Also triggered 3s after a real sync completes.

**Implementation:** `useSyncStatus.js` → `checkProjectSyncStatus(project)` → Tauri command `check_sync_status` → `count_rsync_changes()` in `sync.rs`.

**Gated by sync check:** `checkProjectSyncStatus()` early-returns if `syncCheckEnabled` (see [sync-check-and-usage-switches.md](sync-check-and-usage-switches.md)) is off — covers this interval poll and the manual Refresh path in one place, since both call the same function.

**Result:** `hasPendingPush` and `hasPendingPull` written into `projectRuntime`. On startup both are initialized to `null` (not `undefined`) — buttons render in a faint "checking" state (`.btn-sync-checking`) until the first check resolves. After that: `true` → fully lit, `false` → dim (`.btn-sync-clean`).

**Planned interval:** 60s (unchanged).

#### The `.git/` directory mtime problem

When `sync_git: true`, `.git/` is included in the rsync dry-run. This caused the Push button to be **permanently lit** even immediately after a clean push.

Root cause: `git status` — and any git-aware tool (IDE background check, git hooks) — **writes** to `.git/index` during normal operation. Git uses the index to cache `stat()` metadata of tracked files; when that cache is stale, git refreshes it and writes the updated entry back to disk. This is called an *index refresh*. The write changes the mtime of `.git/index`, which in turn changes the mtime of the `.git/` directory itself.

rsync sees `.git/` as modified and lists it in dry-run output → `count_rsync_changes` returned 1 → push button always lit.

**Fix (`sync.rs` — `count_rsync_changes`):** Filter out all directory entries (lines ending with `/`) from the rsync output count. Only actual file changes increment the count.

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

**What it fetches:** Claude Code and Antigravity quota/usage data — locally on this machine and/or from a selected remote host.

**Sources:** three independent, toggleable `useAgentUsage()` instances live in `AgentUsageSection.vue` — `ag` (Antigravity, always `host = 'local'`), `ccLocal` (Claude Code, always `host = 'local'`), `ccRemote` (Claude Code, `host` = selected SSH host). Each polls only while its own `enabled` flag is true; polling is entirely decoupled from which of the two `AgentUsageSlot` display panels (if any) currently shows it. All three now use the same `useToggleableSource()` pattern with their own independent, persisted power switch — `ccRemote`'s (`aki-src-ccremote-enabled`) is no longer tied to the sync-check switch (see [sync-check-and-usage-switches.md](sync-check-and-usage-switches.md)).

**Cost:** Local reads (`ag`, `ccLocal`) run a local shell/`zsh -lc node`, no network. Remote (`ccRemote`) is one SSH `cat`/probe per interval, only while Remote Mode is on and a host is selected.

**Trigger:** Local sources start immediately (default ON). `ccRemote` starts once a host is selected AND Remote Mode is on. Polls every 30s. Cleaned up on component unmount.

**Implementation:** `useAgentUsage.js` (composable) → Tauri command `get_agent_usage`, dispatched local-vs-SSH inside `agent_usage.rs::run_interpreter_timeout` (renamed from `run_remote_script_timeout` in 1.12.0) via `is_local_host(host)`.

**Planned interval:** 30s (current) — acceptable since it's a single lightweight read.

---

## Current state vs. planned

| Capability | Status |
|---|---|
| Remote diff polling (60s) | ✅ Implemented |
| Agent usage polling (30s) | ✅ Implemented |
| `.git/` directory filter fix | ✅ Implemented |
| `null` init → no false-active on startup | ✅ Implemented |
| Git status polling (60s) | ✅ Implemented |
| Unified `useBackgroundRefresh` singleton | ✅ Implemented — see [refresh-controller.md](../arch/refresh-controller.md) |
| Per-project busy indicator + check cancellation | ✅ Implemented — see [refresh-controller.md](../arch/refresh-controller.md) |
| Per-type configurable intervals | ⬜ Not yet |
| Settings modal in titlebar | ⬜ Not yet |
| Auto-refresh silent log mode | ✅ Implemented |

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

The `useBackgroundRefresh` module-singleton already manages the git and diff timers this way — a
`watch` per interval clears and restarts only the affected timer, and `0` disables that type. What
is still missing is a UI to edit these values (they currently come from `refreshStore` defaults)
and bringing the usage poll under the same roof.

A settings icon (gear) next to the Reload button in `AppHeader.vue` opens a `RefreshSettingsModal` with three numeric inputs and descriptions of what each type checks and how expensive it is.

---

## Log behavior

**User-triggered reload:** one summary line — `"Loaded N projects successfully."` No per-project git detail.

**Auto-refresh:** completely silent. Errors surface as `[ERROR]` only.

Current code still logs per-project GIT lines during `loadData`. Planned cleanup: remove `appendGlobalLog("GIT", ...)` calls from `fetchGitStatus`; let `loadData` emit a single summary after `Promise.all`.
