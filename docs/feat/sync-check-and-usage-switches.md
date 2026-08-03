# Sync Check + Claude Code Remote Usage - Two Independent Switches

## Why

Originally a single flag, `remoteModeEnabled` (`src/store/remoteModeStore.js` - that file no longer exists, it was removed by this split; path kept here only as the historical record of where the flag lived, `aki-remote-mode-enabled`), gated everything remote-related: project sync (push/pull/select/open), background remote-diff checks, *and* Claude Code remote usage monitoring. That coupling was a bug in disguise - turning off Claude Code's remote usage monitor (which the user might do just to stop an unwanted SSH poll) silently disabled push/pull/sync for every project too, and vice versa. There was no way to turn one off without the other.

This was split into two independent switches, each with its own `localStorage` key:

- **`syncCheckEnabled`** (`src/store/syncCheckStore.js`, `aki-sync-check-enabled`, default ON) - gates project sync/diff only.
- **the remote usage monitor switch** - gates Claude Code remote usage monitoring only, built the same way as the two local usage sources, so all sources are symmetric instead of the remote one being a special case that borrowed someone else's flag.

Later changes extended the same shape rather than adding new mechanisms:

- **The flags moved out of the component and into a store (1.19.0).** They used to be component-local refs inside `AgentUsageSection.vue`, which the remote-control mirror (it globs `store/*.js`) could not see - a companion phone's power toggle only flipped the phone's own copy. Living in a store makes them mirror host→companion, and the setter is an `action()` so a phone's toggle runs on the Mac and comes back through the mirror. See `docs/feat/remote-control.md`.
- **Antigravity remote monitoring** got the same switch in 1.19.0. All sources default on, deliberately: polling follows a monitor's own `enabled` flag rather than what a slot displays, so this does start one `ssh <host> node` probe per refresh interval on upgrade for anyone with a host selected — the same cost the Claude Code remote monitor has always had. Defaulting it off would have avoided a one-time SSH round trip at the price of two identical features behaving differently forever. An unreachable host is handled by the consecutive-failure breaker in `src/composables/usageMonitor.js` (`haltPolling`), not by the default.
- **The four fixed flags became one keyed map (1.20.0).** `src/store/usageMonitorStore.js` holds a single `monitorEnabled` map under `aki-usage-monitor-enabled`, keyed by `monitorId(agentId, host)` (e.g. `claudecode@hostA`) - read with `isMonitorEnabled(id)`, written with the mirrored `setMonitorEnabled(id, value)`. A monitor with no entry defaults ON, which reproduces every pre-1.20.0 default. The four old keys (`aki-src-ag-enabled`, `aki-src-cclocal-enabled`, `aki-src-ccremote-enabled`, `aki-src-agremote-enabled`) are now only a one-time legacy seed (`usageMonitorStore.js`'s `seed()`); the two remote ones resolve against whichever host was selected at seed time - the only host they could ever have described. See `docs/plan/done/usage-monitor-entity-refactor.md`.

## What `syncCheckEnabled` gates

| Area | File | Mechanism |
|---|---|---|
| PUSH / PULL / SELECT buttons | `src/components/ProjectTable.vue` | `:disabled="... || !syncCheckEnabled"` |
| Open popup's **Upload (select files)** item only | `src/components/ProjectTable.vue` | `popup-disabled` + a tooltip naming the switch. The "☁️ REMOTE (SSH)" block itself renders on `v-if="p.remote_host && p.remote_path"` alone: SSH Terminal, the VSCode/Antigravity Remote entries and COPY are ways to *reach* the server, not rsync traffic, so the switch no longer hides them (it used to hide the whole block) |
| Manual + background remote-diff checks | `src/composables/useSyncStatus.js`, `src/composables/useBackgroundRefresh.js` | `checkProjectSyncStatus()` early-returns `if (!syncCheckEnabled.value) return` (covers the Refresh button and any direct call). The background diff timer goes further than a no-op: `restartDiffTimer()` doesn't create its `setInterval` at all while off, and a `watch(syncCheckEnabled, ...)` tears down/rebuilds the timer on every toggle (running one check immediately on re-enable). Turning off also calls `bumpEpoch()` (`src/store/projectStore.js`) for every project, discarding any diff check already in flight and clearing its busy indicator immediately rather than waiting for that now-irrelevant call to resolve - see "Per-project busy state and cancellation" below |
| Sync mutation boundary | `src/composables/useSync.js` | `startSync()` early-returns with a warning Toast - a defensive guard at the actual mutation boundary, in case some future caller bypasses the UI-level disables |

**UI**: a power icon in the SYNC column header (`src/components/ProjectTable.vue`), next to `RefreshRing`.

## What the two remote-usage switches gate

Each remote monitor's switch gates that monitor's polling - nothing else, and nothing about any other monitor. A remote monitor is switched exactly like a local one: one entry in `usageMonitorStore`'s map, toggled through `setMonitorEnabled(monitorId(agentId, host), value)`. The monitor object itself (its poll loop, breaker, cache) is `src/composables/usageMonitor.js`, handed out per identity by `src/composables/usageMonitorRegistry.js`'s `getMonitor(agentId, host)`, whose `toggle` closure is what the power icon calls.

Remote AG needed no Rust at all: `get_agent_usage('antigravity', host)` already routes a non-empty host through `run_remote_shell()` → `ssh <host> sh` (`src-tauri/src/remote_shell.rs`), and `provision_agent_usage` is a no-op for Antigravity.

**UI (1.19.0)**: the REMOTE tab now carries the same `AG | CC` tab pair as LOCAL, each tab with its own power icon doubling as that monitor's on/off state - one shared `v-for` in `src/components/AgentUsageSlot.vue`, not two hand-copied templates. The SSH host picker sits beside them, deliberately narrow.

**Host is per slot (1.20.0)**: the picker writes the *slot's* own host - `AgentUsageSlot.vue:40-41` binds `:value="target.host"` and `@change="setSlotTarget(slotId, { remoteHost: … })"` - so two slots can watch two different hosts on screen at the same time. The app-wide `selectedSshHost` (`sshStore`) is now only the fallback for a slot that has never picked a host of its own (`usageSlotStore.js`'s `slotTarget()`), plus its unrelated jobs (SSH-config modal, project sync/diff). The pre-1.20.0 statement "both remote sources watch the same `selectedSshHost`: one host choice, two monitors of it" describes the old design and is no longer true.

## What none of them gate

Local usage sources (Antigravity, Claude Code local) have their own independent per-source power switches (`antigravity@local`/`claudecode@local`) - no switch above touches them.

## The refresh controller (added post-split, same investigation)

> Full architecture, flowcharts and invariants: **[docs/arch/refresh-controller.md](../arch/refresh-controller.md)**.
> This section only records why the switch is what surfaced it.

Once the switch existed, real use surfaced a deeper problem that the switch had only made visible: **the global Refresh button and every other refresh path were not the same feature.**

`AppHeader.handleRefresh()` called `loadData()` - a full app reload (re-read `projects.json`, SSH hosts, IDE availability) - and the "all the buttons dim, then come back" effect users associated with refreshing came from `loadData`'s global `isReloading` flag. The background git/diff timers, meanwhile, ran their checks with no visible state at all, and the per-project Refresh button had whatever ad-hoc flag was most recently bolted onto it. Three mechanisms, no shared concept. That is why a project's own icon never reacted to its own refresh cycle, and why bolting yet another per-button flag on could not fix it.

The fix was to give the status layer exactly one unit of work and one scheduler:

- **One unit**: `refreshProject(p)` (`useBackgroundRefresh.js`) runs a project's three derived-state checks in parallel - `fetchGitStatus` (`useGit.js`), `checkProjectSyncStatus` (`useSyncStatus.js`), `fetchProjectStack` (`useProjectStack.js`). Everything that can cause a refresh is a caller of that unit or of its constituent checks: the two background timers, the per-project button, the global button (`refreshAllProjects()`), and `saveConfig()`.
- **Busy state lives on the checks, not their callers**: `beginRefresh`/`endRefresh` maintain a per-project counter (`projectRuntime[id].refreshCount`) read via `isRefreshing(id)`; the header's spinner derives from the same counters via `anyRefreshing`. A counter rather than a boolean because several checks are in flight for one project at once. This is what makes a background tick light up the per-project icons - no trigger gets special-cased.
- **`loadData()` is an app-load concern again**, called once on mount, not by the Refresh button. It re-reads config from disk; refreshing derived status is a different operation.

### Cancellation

`invoke()` has no abort handle, so cancellation uses a per-project generation token, `projectRuntime[id].epoch` (`bumpEpoch()`/`currentEpoch()` in `projectStore.js`). Each check captures the epoch after `beginRefresh` and re-checks it after its `await`; a mismatch means the result is stale and is discarded - never written, never decrementing the new generation's counter. `bumpEpoch()` also force-resets `refreshCount` to 0, so the indicator clears the instant the cause fires rather than whenever the superseded call resolves.

**This only ever cancels read-only status checks - an rsync push/pull in progress is never touched.**

Who bumps the epoch:

| Cause | Where |
|---|---|
| A project's `remote_host`/`local_path` changed | `saveConfig()` - also blanks `hasPendingPush`/`hasPendingPull` (measured against the old host) and immediately re-runs `refreshProject` against the new one |
| Sync check switched off | `toggleSyncCheck()` - bumps every project |
| Project list re-read from disk | `loadData()` - per project, as it rebuilds runtime state |
| Project removed | `confirmRemove()` - implicitly: dropping the runtime entry makes `currentEpoch()` report `0`, which by invariant can never equal a captured epoch (`beginRefresh` guarantees `>= 1` for a live project) |

## Migration

Both new keys are seeded from the old `aki-remote-mode-enabled` value on first run after the split (see `syncCheckStore.js`'s `initialEnabled()`; the usage-side seed started life in `AgentUsageSection.vue` and now lives in `usageMonitorStore.js`'s `seed()`, which folds those old `aki-src-*-enabled` keys into the keyed map), so an existing user's behavior is unchanged until they explicitly diverge the two switches. The legacy key is left in place (not deleted) so rolling back to a pre-split build doesn't lose the setting.

## History

An earlier design (this doc, pre-split) argued for exactly one master switch and explicitly rejected a two-tier system - see CHANGELOG 1.9.0. That reasoning didn't hold up in practice: users who wanted to mute one SSH poll (Claude Code remote usage) had no way to do so without also disabling sync for every remote project. The split described here restores independence between the two concerns while keeping each switch as simple as the single one used to be.

## Related source files

- `src/store/syncCheckStore.js` - the sync/diff switch
- `src/store/usageMonitorStore.js` - the keyed `monitorEnabled` map, `monitorId`, `isMonitorEnabled` + `setMonitorEnabled` (the mirrored action), and the legacy seed
- `src/store/usageSlotStore.js` - what each slot is pointed at, including its own `remoteHost`, via the mirrored `setSlotTarget`
- `src/composables/usageMonitor.js`, `src/composables/usageMonitorRegistry.js` - the monitor itself (poll loop, `haltPolling` breaker) and the one-instance-per-`agentId@host` registry
- `src/components/AgentUsageSlot.vue` - the LOCAL/REMOTE tab UI and the shared AG|CC tab loop
- `src/components/ProjectTable.vue`, `src/composables/useSync.js`, `src/composables/useSyncStatus.js` - the sync-check gates
- `src/store/projectStore.js` - refresh counter (`beginRefresh`/`endRefresh`/`isRefreshing`/`anyRefreshing`) and cancellation primitive (`bumpEpoch`/`currentEpoch`)
- `src/composables/useBackgroundRefresh.js` - the refresh controller: `refreshProject`, `refreshAllProjects`, the git/diff timers and ring keys
- `src/composables/useGit.js`, `src/composables/useSyncStatus.js`, `src/composables/useProjectStack.js` - the three per-project checks
- `src/composables/useProjectConfig.js`, `src/components/AppHeader.vue` - epoch call sites and the global Refresh button
