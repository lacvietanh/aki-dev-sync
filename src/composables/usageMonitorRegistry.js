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

// Claude Code LOCAL monitor locked while proxy active: Anthropic account API/pricing inactive when traffic rerouted.
function lockedFor(agentId, host) {
  return computed(() => agentId === 'claudecode' && host === LOCAL_HOST && claudeMode.value === 'proxy');
}

// Multiton monitor (same args -> same instance): acquires hold (must pair with releaseMonitor); empty host gets disabled NO_HOST monitor.
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

// Releases one hold (CLAUDE.md Regression Guard: stops watching at 0 holders, preserves cached instance and reading).
export function releaseMonitor(monitor) {
  const entry = monitor && registry.get(monitor.id);
  if (!entry || entry.holders === 0) return;
  entry.holders--;
  if (entry.holders === 0) entry.monitor.stopWatching();
}
