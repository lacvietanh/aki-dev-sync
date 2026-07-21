# Sync Check + Claude Code Remote Usage - Two Independent Switches

## Why

Originally a single flag, `remoteModeEnabled` (`src/store/remoteModeStore.js`,
`aki-remote-mode-enabled`), gated everything remote-related: project sync (push/pull/select/open),
background remote-diff checks, *and* Claude Code remote usage monitoring. That coupling was a bug
in disguise - turning off Claude Code's remote usage monitor (which the user might do just to stop
an unwanted SSH poll) silently disabled push/pull/sync for every project too, and vice versa. There
was no way to turn one off without the other.

This was split into two independent switches, each with its own `localStorage` key:

- **`syncCheckEnabled`** (`src/store/syncCheckStore.js`, `aki-sync-check-enabled`, default ON)  - 
  gates project sync/diff only.
- **`ccRemote`** (`src/components/AgentUsageSection.vue`, `aki-src-ccremote-enabled`, default ON)  - 
  gates Claude Code remote usage monitoring only, using the same `useToggleableSource()` pattern
  as the two local usage sources (`ag`, `ccLocal`), so all three sources are now symmetric instead
  of the remote source being a special case that borrowed someone else's flag.

## What `syncCheckEnabled` gates

| Area | File | Mechanism |
|---|---|---|
| PUSH / PULL / SELECT buttons | `src/components/ProjectTable.vue` | `:disabled="... || !syncCheckEnabled"` |
| Open popup's remote IDE section | `src/components/ProjectTable.vue` | `v-if="p.remote_host && p.remote_path && syncCheckEnabled"` - the whole "☁️ REMOTE (SSH)" block disappears, not just individual items |
| Manual + background remote-diff checks | `src/composables/useSyncStatus.js`, `src/composables/useBackgroundRefresh.js` | `checkProjectSyncStatus()` early-returns `if (!syncCheckEnabled.value) return` (covers the Refresh button and any direct call). The background diff timer goes further than a no-op: `restartDiffTimer()` doesn't create its `setInterval` at all while off, and a `watch(syncCheckEnabled, ...)` tears down/rebuilds the timer on every toggle (running one check immediately on re-enable). Turning off also calls `bumpEpoch()` (`src/store/projectStore.js`) for every project, discarding any diff check already in flight and clearing its busy indicator immediately rather than waiting for that now-irrelevant call to resolve - see "Per-project busy state and cancellation" below |
| Sync mutation boundary | `src/composables/useSync.js` | `startSync()` early-returns with a warning Toast - a defensive guard at the actual mutation boundary, in case some future caller bypasses the UI-level disables |

**UI**: a power icon in the SYNC column header (`src/components/ProjectTable.vue`), next to
`RefreshRing`.

## What `ccRemote`'s switch gates

Claude Code remote usage monitoring only - nothing else. Lives in `AgentUsageSection.vue` exactly
like the two local sources (`ag`, `ccLocal`); see `src/composables/useAgentUsage.js` and the
`useToggleableSource()` helper.

**UI**: the power icon in the usage widget's REMOTE tab (`src/components/AgentUsageSlot.vue`),
left of the SSH host-select dropdown - unchanged in position from before the split, just now wired
to its own dedicated flag instead of the old master switch.

## What neither gates

Local usage sources (Antigravity, Claude Code local) have their own independent per-source power
switches (`ag`/`ccLocal` in `AgentUsageSection.vue`) - neither switch above touches them.

## The refresh controller (added post-split, same investigation)

> Full architecture, flowcharts and invariants: **[docs/arch/refresh-controller.md](../arch/refresh-controller.md)**.
> This section only records why the switch is what surfaced it.

Once the switch existed, real use surfaced a deeper problem that the switch had only made
visible: **the global Refresh button and every other refresh path were not the same feature.**

`AppHeader.handleRefresh()` called `loadData()` - a full app reload (re-read `projects.json`, SSH
hosts, IDE availability) - and the "all the buttons dim, then come back" effect users associated
with refreshing came from `loadData`'s global `isReloading` flag. The background git/diff timers,
meanwhile, ran their checks with no visible state at all, and the per-project Refresh button had
whatever ad-hoc flag was most recently bolted onto it. Three mechanisms, no shared concept. That
is why a project's own icon never reacted to its own refresh cycle, and why bolting yet another
per-button flag on could not fix it.

The fix was to give the status layer exactly one unit of work and one scheduler:

- **One unit**: `refreshProject(p)` (`useBackgroundRefresh.js`) runs a project's three derived-state
  checks in parallel - `fetchGitStatus` (`useGit.js`), `checkProjectSyncStatus` (`useSyncStatus.js`),
  `fetchProjectStack` (`useProjectStack.js`). Everything that can cause a refresh is a caller of
  that unit or of its constituent checks: the two background timers, the per-project button, the
  global button (`refreshAllProjects()`), and `saveConfig()`.
- **Busy state lives on the checks, not their callers**: `beginRefresh`/`endRefresh` maintain a
  per-project counter (`projectRuntime[id].refreshCount`) read via `isRefreshing(id)`; the header's
  spinner derives from the same counters via `anyRefreshing`. A counter rather than a boolean
  because several checks are in flight for one project at once. This is what makes a background
  tick light up the per-project icons - no trigger gets special-cased.
- **`loadData()` is an app-load concern again**, called once on mount, not by the Refresh button.
  It re-reads config from disk; refreshing derived status is a different operation.

### Cancellation

`invoke()` has no abort handle, so cancellation uses a per-project generation token,
`projectRuntime[id].epoch` (`bumpEpoch()`/`currentEpoch()` in `projectStore.js`). Each check
captures the epoch after `beginRefresh` and re-checks it after its `await`; a mismatch means the
result is stale and is discarded - never written, never decrementing the new generation's counter.
`bumpEpoch()` also force-resets `refreshCount` to 0, so the indicator clears the instant the cause
fires rather than whenever the superseded call resolves.

**This only ever cancels read-only status checks - an rsync push/pull in progress is never touched.**

Who bumps the epoch:

| Cause | Where |
|---|---|
| A project's `remote_host`/`local_path` changed | `saveConfig()` - also blanks `hasPendingPush`/`hasPendingPull` (measured against the old host) and immediately re-runs `refreshProject` against the new one |
| Sync check switched off | `toggleSyncCheck()` - bumps every project |
| Project list re-read from disk | `loadData()` - per project, as it rebuilds runtime state |
| Project removed | `confirmRemove()` - implicitly: dropping the runtime entry makes `currentEpoch()` report `0`, which by invariant can never equal a captured epoch (`beginRefresh` guarantees `>= 1` for a live project) |

## Migration

Both new keys are seeded from the old `aki-remote-mode-enabled` value on first run after the split
(see `syncCheckStore.js`'s `initialEnabled()` and the seed loop at the top of
`AgentUsageSection.vue`'s `<script setup>`), so an existing user's behavior is unchanged until they
explicitly diverge the two switches. The legacy key is left in place (not deleted) so rolling back
to a pre-split build doesn't lose the setting.

## History

An earlier design (this doc, pre-split) argued for exactly one master switch and explicitly
rejected a two-tier system - see CHANGELOG 1.9.0. That reasoning didn't hold up in practice: users
who wanted to mute one SSH poll (Claude Code remote usage) had no way to do so without also
disabling sync for every remote project. The split described here restores independence between
the two concerns while keeping each switch as simple as the single one used to be.

## Related source files

- `src/store/syncCheckStore.js` - the sync/diff switch
- `src/components/AgentUsageSection.vue` - the ccRemote switch (`useToggleableSource` call) + migration seed
- `src/components/AgentUsageSlot.vue` - ccRemote switch UI
- `src/components/ProjectTable.vue`, `src/composables/useSync.js`, `src/composables/useSyncStatus.js` - the sync-check gates
- `src/store/projectStore.js` - refresh counter (`beginRefresh`/`endRefresh`/`isRefreshing`/`anyRefreshing`) and cancellation primitive (`bumpEpoch`/`currentEpoch`)
- `src/composables/useBackgroundRefresh.js` - the refresh controller: `refreshProject`, `refreshAllProjects`, the git/diff timers and ring keys
- `src/composables/useGit.js`, `src/composables/useSyncStatus.js`, `src/composables/useProjectStack.js` - the three per-project checks
- `src/composables/useProjectConfig.js`, `src/components/AppHeader.vue` - epoch call sites and the global Refresh button
