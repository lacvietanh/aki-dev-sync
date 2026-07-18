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

**Gated by Remote Mode:** `checkProjectSyncStatus()` early-returns if the global `remoteModeEnabled` switch (see [remote-mode.md](remote-mode.md)) is off — covers this interval poll and the manual Refresh path in one place, since both call the same function.

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

**Sources (v1.9.0):** three independent, toggleable `useAgentUsage()` instances live in `AgentUsageSection.vue` — `ag` (Antigravity, always `host = 'local'`), `ccLocal` (Claude Code, always `host = 'local'`), `ccRemote` (Claude Code, `host` = selected SSH host). Each polls only while its own `enabled` flag is true; polling is entirely decoupled from which of the two `AgentUsageSlot` display panels (if any) currently shows it. `ag`/`ccLocal` have their own per-source power switch (persisted in `localStorage`); `ccRemote.enabled` is not independent — it mirrors the global `remoteModeEnabled` switch (see [remote-mode.md](remote-mode.md)), so remote usage polling stops the instant Remote Mode is turned off.

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
| Git status polling | ⬜ Not yet — on-demand only |
| Unified `useBackgroundRefresh` singleton | ⬜ Not yet |
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

A single `useBackgroundRefresh` module-singleton manages all three timers. When a setting changes, only the affected timer is cleared and restarted. Setting an interval to `0` disables that type.

A settings icon (gear) next to the Reload button in `AppHeader.vue` opens a `RefreshSettingsModal` with three numeric inputs and descriptions of what each type checks and how expensive it is.

---

## Log behavior

**User-triggered reload:** one summary line — `"Loaded N projects successfully."` No per-project git detail.

**Auto-refresh:** completely silent. Errors surface as `[ERROR]` only.

Current code still logs per-project GIT lines during `loadData`. Planned cleanup: remove `appendGlobalLog("GIT", ...)` calls from `fetchGitStatus`; let `loadData` emit a single summary after `Promise.all`.
