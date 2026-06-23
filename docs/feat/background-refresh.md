# Background Refresh

Automatic background polling that keeps three independent data types fresh without user interaction. Each type has a different cost profile and refresh interval.

## The three refresh types

### 1. Git Status (`git_interval_s`)

**What it fetches:** Local git info — branch, dirty status, recent log. Runs entirely locally via `git` CLI.

**Cost:** Negligible. No SSH, no network. Completes in ~50ms.

**Trigger:** Currently on-demand only — called during `loadData` and after a sync completes. No interval polling yet.

**Implementation:** `useGit.js` → `fetchGitStatus(projectId)` → Tauri command `get_git_info`.

**Planned interval:** 60s.

---

### 2. Remote Diff (`remote_diff_interval_s`)

**What it fetches:** Whether local and remote have diverged — serves the Push/Pull button highlight state. Runs `rsync --dry-run` in both directions (push and pull) via SSH.

**Cost:** Medium-high. Each check spawns two SSH+rsync processes. For N projects, `checkAllSyncStatus()` runs N×2 rsync processes sequentially.

**Trigger:** Called during `loadData` (background, non-blocking), then on a 60s interval via `startSyncStatusPolling()`. Also triggered 3s after a real sync completes.

**Implementation:** `useSyncStatus.js` → `checkProjectSyncStatus(project)` → Tauri command `check_sync_status` → `count_rsync_changes()` in `sync.rs`.

**Result:** `hasPendingPush` and `hasPendingPull` written into `projectRuntime`. Buttons light up when value is `true` or `undefined`; dim (`.btn-sync-clean`) only when explicitly `false`.

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

**What it fetches:** Claude Code and Antigravity quota/usage data from the remote host. Reads a cached JSON file via SSH.

**Cost:** Low-medium. One SSH `cat` per agent per interval. Two agents = two SSH calls per tick.

**Trigger:** Starts when a remote host is selected (watch with `immediate: true`). Polls every 30s. Cleaned up on component unmount.

**Implementation:** `useAgentUsage.js` (composable, component-lifecycle bound) → Tauri command `get_agent_usage`. Two instances in `AgentUsageSection.vue`: one for `claudecode`, one for `antigravity`.

**Planned interval:** 30s (current) — acceptable since it's a single lightweight SSH read.

---

## Current state vs. planned

| Capability | Status |
|---|---|
| Remote diff polling (60s) | ✅ Implemented |
| Agent usage polling (30s) | ✅ Implemented |
| `.git/` directory filter fix | ✅ Implemented |
| Git status polling | ⬜ Not yet — on-demand only |
| Unified `useBackgroundRefresh` singleton | ⬜ Not yet |
| Per-type configurable intervals | ⬜ Not yet |
| Settings modal in titlebar | ⬜ Not yet |
| Auto-refresh silent log mode | ⬜ Not yet |

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

A single `useBackgroundRefresh` module-singleton manages all three timers. When a setting changes, only the affected timer is cleared and restarted. Setting an interval to `0` disables that type.

A settings icon (gear) next to the Reload button in `AppHeader.vue` opens a `RefreshSettingsModal` with three numeric inputs and descriptions of what each type checks and how expensive it is.

---

## Log behavior

**User-triggered reload:** one summary line — `"Loaded N projects successfully."` No per-project git detail.

**Auto-refresh:** completely silent. Errors surface as `[ERROR]` only.

Current code still logs per-project GIT lines during `loadData`. Planned cleanup: remove `appendGlobalLog("GIT", ...)` calls from `fetchGitStatus`; let `loadData` emit a single summary after `Promise.all`.
