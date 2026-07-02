# Remote Mode — Single Global Master Switch

## Why

Not every user of this app works with a remote host. Before this feature, "off" states for
remote-related functionality were scattered and per-feature (a toggle on the usage widget here,
disabled buttons there) — no single place stopped the app from touching SSH at all. `remoteModeEnabled`
(`src/store/remoteModeStore.js`) is a single flag, persisted to `localStorage` (`aki-remote-mode-enabled`,
default ON), that gates every remote-touching code path in the app.

## What it gates

| Area | File | Mechanism |
|---|---|---|
| PUSH / PULL / SELECT buttons | `src/components/ProjectTable.vue` | `:disabled="... || !remoteModeEnabled"` |
| Open popup's remote IDE section | `src/components/ProjectTable.vue` | `v-if="p.remote_host && p.remote_path && remoteModeEnabled"` — the whole "☁️ REMOTE (SSH)" block disappears, not just individual items |
| Manual + background remote-diff checks | `src/composables/useSyncStatus.js` | `checkProjectSyncStatus()` early-returns `if (!remoteModeEnabled.value) return` — this single choke point covers both the Refresh button and the background timer (`checkAllSyncStatus()` just maps over it) |
| Sync mutation boundary | `src/composables/useSync.js` | `startSync()` early-returns with a warning Toast — a defensive guard at the actual mutation boundary, in case some future caller bypasses the UI-level disables |
| Claude Code remote usage monitoring | `src/components/AgentUsageSection.vue` | `ccRemote`'s `enabled` field is the `remoteModeEnabled` ref itself (`reactive({ enabled: remoteModeEnabled, ...useAgentUsage(...) })`) — not a separate toggle, so it can never drift out of sync with the master switch |

## What it does NOT gate

Local sources (Antigravity, Claude Code local) have their own independent per-source power
switches (`ag`/`ccLocal` in `AgentUsageSection.vue`) — turning Remote Mode off has no effect on them.

## UI

A single power icon inside the usage widget, left of the SSH host-select dropdown, visible only
when a panel's REMOTE tab is active (`src/components/AgentUsageSlot.vue`) — contextual placement,
not a separate always-visible header control. An earlier iteration placed this switch in
`AppHeader.vue` (a standalone "REMOTE" label + switch next to the SSH config button); that was
removed once the widget-level control existed, since having the same switch in two places was
redundant. There is exactly one switch; `AgentUsageSlot.vue` is just a second place to reach it,
sharing the same `remoteModeEnabled` store — never a duplicate state.

## History

An earlier design had a *second*, independent toggle scoped only to Claude Code remote usage
monitoring (a "two-tier" system: master switch + per-feature sub-switch). This was explicitly
rejected in favor of the single master switch described here — see git history / CHANGELOG 1.9.0.

## Related source files

- `src/store/remoteModeStore.js` — the store itself
- `src/components/AgentUsageSlot.vue` — the switch's UI
- `src/components/ProjectTable.vue`, `src/composables/useSync.js`, `src/composables/useSyncStatus.js` — the gates
- `src/components/AgentUsageSection.vue` — `ccRemote.enabled` aliasing
