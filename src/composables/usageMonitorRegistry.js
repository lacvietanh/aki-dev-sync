// @docs docs/plan/done/usage-monitor-entity-refactor.md §2
//
// The multiton that owns monitor identity: one UsageMonitor per `agentId@host`, created on first
// request and kept for the session.
//
// WHY A REGISTRY RATHER THAN ONE MONITOR PER SLOT. Polling cost is per (agent, host), not per
// viewer: two slots both showing Claude Code on hostA must produce ONE SSH round trip per interval.
// The pre-refactor design got that property from four hand-built shared sources in
// AgentUsageSection.vue, which is also exactly what capped the app at a single remote host. Keying
// by identity keeps the property and removes the cap at the same time - sharing is now a
// consequence of two slots naming the same thing, not a rule anyone has to maintain.
import { computed, effectScope, reactive } from 'vue';
import { createUsageMonitor } from './usageMonitor';
import { monitorId, isMonitorEnabled, setMonitorEnabled, LOCAL_HOST } from '../store/usageMonitorStore';
import { claudeMode } from '../store/claudeModeStore';

// id -> { monitor, holders }. `holders` is how many mounted views are currently displaying it: a
// monitor is created on first request and CACHED for the session, but it only WATCHES (poll timer +
// wake subscription) while at least one holder exists. Without that count, merely opening the host
// dropdown and looking at ten machines left twenty 30s SSH poll loops running behind cards nobody
// renders - and their amber breaker control, the only way to stop one, was unreachable.
const registry = new Map();

// Detached scope: `getMonitor` is normally first called from inside some component's setup, and a
// monitor's watchers created there would be torn down when THAT component unmounts - stopping a
// monitor other slots are still displaying. Running construction in a scope owned by this module
// makes a monitor's lifetime the session's, which is what its identity implies.
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
    // Back from zero: the instance and its last reading survived, so the card repaints immediately and only the polling has to be re-armed.
    if (existing.holders === 0) existing.monitor.startWatching();
    existing.holders++;
    return existing.monitor;
  }

  const locked = lockedFor(agentId, host);
  // Locked means "this monitor cannot say anything true right now", so it does not poll - DERIVED
  // from the mode, never stored. Proxy mode used to force the flag off by writing `false` into the
  // enabled map instead; the map is the object `setMonitorEnabled` spreads into localStorage, so the
  // next toggle of ANY other monitor persisted the forced value, and leaving proxy mode (or even
  // restarting) could no longer bring the local Claude Code monitor back. A consequence of the
  // current mode must not be able to leak into the user's stored preference at all - the way to
  // guarantee that is to never write it down.
  const enabled = computed(() => !!host && !locked.value && isMonitorEnabled(id));
  const toggle = () => {
    if (locked.value) return;
    // No host chosen yet: `enabled` is pinned false, so a toggle could only persist a flag under the placeholder identity that nothing will ever read. Do nothing instead.
    if (!host) return;
    setMonitorEnabled(id, !enabled.value);
  };

  // `reactive` so templates read `monitor.data` / `monitor.enabled` directly instead of `.value` on every field - the shape AgentUsageSlot.vue already consumed before the refactor.
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

// There is deliberately no watcher on `claudeMode` here any more. Proxy mode ON stops the local
// Claude Code monitor through `locked` → `enabled` above (a pure derivation, re-evaluated the moment
// the mode changes), and proxy mode OFF simply lets the user's own stored preference apply again.
// The watcher this replaces wrote a forced `false` into the shared enabled map, which is exactly the
// multi-entity failure the project's Regression Guard is about: one monitor's transient, app-imposed
// state riding along in the object every other monitor's toggle persists.
