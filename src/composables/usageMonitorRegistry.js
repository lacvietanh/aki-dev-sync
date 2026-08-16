// @docs docs/plan/done/usage-monitor-entity-refactor.md §2
//
// Multiton owning monitor identity: one UsageMonitor per `agentId@host`, shared across slots to deduplicate SSH polling.
import { computed, effectScope, reactive } from 'vue';
import { createUsageMonitor } from './usageMonitor';
import { monitorId, isMonitorEnabled, setMonitorEnabled, LOCAL_HOST } from '../store/usageMonitorStore';
import { claudeMode } from '../store/claudeModeStore';

// Map<id, { monitor, holders }>: cached for session, watching only while holders > 0 to avoid orphan SSH loops.
const registry = new Map();

// Detached scope: keeps monitor watchers alive across individual caller component unmounts.
const monitorScope = effectScope(true);

/**
 * Claude Code's LOCAL monitor reads Anthropic's own account API and pricing table, which describe
 * nothing real once Proxy mode reroutes traffic elsewhere - so it is locked off while proxy is
 * active. Purely a function of identity + mode; no other monitor is ever locked.
 */
function lockedFor(agentId, host) {
  return computed(() => agentId === 'claudecode' && host === LOCAL_HOST && claudeMode.value === 'proxy');
}

/**
 * The monitor for one agent on one machine. Same arguments → same instance, always.
 *
 * ACQUIRES a hold: every call must be paired with `releaseMonitor()` when the caller stops
 * displaying it (its `onUnmounted`, or the moment it re-targets at another host). An unpaired call
 * leaves the monitor polling forever.
 *
 * An empty `host` (no SSH host configured yet) yields a real but permanently-disabled monitor rather
 * than null, so callers never branch on it - the card renders its ordinary "monitoring off" state.
 * It gets its own identity (`monitorId`'s `NO_HOST`), never the local monitor's: sharing one was
 * what made an un-targeted REMOTE tab display local numbers and switch the LOCAL monitor off.
 */
export function getMonitor(agentId, host) {
  const id = monitorId(agentId, host);
  const existing = registry.get(id);
  if (existing) {
    // Reactivates watching if returning from zero holders; instance and last reading are already intact.
    if (existing.holders === 0) existing.monitor.startWatching();
    existing.holders++;
    return existing.monitor;
  }

  const locked = lockedFor(agentId, host);
  // Derived from !locked.value (never stored in persisted preferences to prevent proxy mode from clobbering user setting).
  const enabled = computed(() => !!host && !locked.value && isMonitorEnabled(id));
  const toggle = () => {
    if (locked.value) return;
    // No host selected: enabled is pinned false, so toggle is a no-op.
    if (!host) return;
    setMonitorEnabled(id, !enabled.value);
  };

  // Reactive wrapper so templates read monitor fields directly without .value.
  const monitor = reactive(monitorScope.run(() => createUsageMonitor({ id, agentId, host, enabled, locked, toggle })));
  registry.set(id, { monitor, holders: 1 });
  return monitor;
}

/**
 * Give up one hold taken by `getMonitor`. At zero holders the monitor STOPS WATCHING - its poll
 * timer is cleared and its wake subscription dropped.
 *
 * Named for exactly that blast radius (CLAUDE.md, Regression Guard for multi-entity state): it
 * releases ONE monitor. It does not evict the instance, does not clear the registry, and does not
 * touch that monitor's stored reading - which is why re-targeting a slot back at this host paints
 * the last-known numbers at once instead of an empty card.
 */
export function releaseMonitor(monitor) {
  const entry = monitor && registry.get(monitor.id);
  if (!entry || entry.holders === 0) return;
  entry.holders--;
  if (entry.holders === 0) entry.monitor.stopWatching();
}
