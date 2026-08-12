<template>
  <div class="usage-column">
    <div class="column-header">
      <!-- Left: which category - LOCAL or REMOTE. -->
      <div class="tab-group">
        <button class="tab" :class="{ 'is-active': target.scope === 'local' }" title="Local" @click="setSlotTarget(slotId, { scope: 'local' })">
          <i class="fa-solid fa-laptop-code"></i> <span class="u-narrow-hide">LOCAL</span>
        </button>
        <button
          class="tab"
          :class="{ 'is-active': target.scope === 'remote' }"
          title="Remote"
          @click="setSlotTarget(slotId, { scope: 'remote' })"
        >
          <i class="fa-solid fa-cloud"></i> <span class="u-narrow-hide">REMOTE</span>
        </button>
      </div>

      <!-- Right: which agent within the selected category. LOCAL and REMOTE both offer the same
           AG | CC pair, so they share ONE loop over `srcTabs` (the local/remote list is picked by
           the computed) instead of two hand-copied templates - each tab carries its own power icon,
           colored to double as that source's on/off status. REMOTE additionally carries the SSH
           host picker, kept deliberately narrow so the two tabs fit beside it. -->
      <div class="tab-group">
        <button
          v-for="src in srcTabs"
          :key="src.key"
          class="tab src-tab"
          :class="{ 'is-active': activeAgentKey === src.key }"
          :title="powerTitle(src)"
          @click="setAgent(src.key)"
        >
          <i class="fa-solid fa-power-off src-power" :class="[src.monitor.pollHalted ? 'is-halted' : (src.monitor.enabled ? 'is-on' : 'is-off'), { 'is-locked': src.monitor.locked }]"
             @click.stop="onPowerClick(src.monitor)"></i>
          <img :src="src.icon" class="src-icon" alt="" />
          <span class="u-narrow-hide">{{ src.label }}</span>
        </button>
        <select
          v-if="target.scope === 'remote'"
          :value="target.host"
          @change="setSlotTarget(slotId, { remoteHost: $event.target.value })"
          class="host-select-mini"
          :title="target.host ? `Remote host this slot is monitoring: ${target.host}` : 'Pick the remote host to monitor'"
        >
          <option value="" disabled>Select Host</option>
          <option v-for="h in sshHosts" :key="h" :value="h">{{ h }}</option>
        </select>
      </div>
    </div>

    <AgentUsage
      :agentId="activeAgentId"
      :agentName="activeAgentName"
      :data="slotAccountInfo.data"
      :loading="monitor.loading"
      :error="monitor.error"
      :dataAt="monitor.dataAt"
      :isCached="slotAccountInfo.isCached"
      :cachedAt="slotAccountInfo.cachedAt"
      :showEmail="showEmail"
      :remote="target.scope === 'remote'"
      :sourceOff="!monitor.enabled"
      :locked="!!monitor.locked"
      :accounts="monitor.accounts"
      :viewing-email="slotViewingEmail"
      :active-email="monitor.activeEmail"
      :active-emails="monitor.activeEmails"
      :popup-position="popupPosition"
      @retry="monitor.refresh"
      @select-account="handleSelectAccount"
      @toggle-email="toggleEmail"
    />
  </div>
</template>

<script setup>
import { ref, shallowRef, computed, watch, onUnmounted } from 'vue';
import AgentUsage from './AgentUsage.vue';
import { useSsh } from '../composables/useSsh';
import { getMonitor, releaseMonitor } from '../composables/usageMonitorRegistry';
import { loadAgAccount } from '../composables/agUsageCache';
import { slotTarget, setSlotTarget } from '../store/usageSlotStore';
import { tierCount, slotIndexOf, SLOTS_PER_ROW } from '../store/usageTierStore';

// A slot is a VIEW onto a UsageMonitor, never an owner of one. It holds only which monitor to look
// at - scope (LOCAL/REMOTE), agent (AG/CC) and, since the entity refactor, WHICH REMOTE HOST - plus
// its own display preferences. The monitors themselves are session-lived and shared through
// `usageMonitorRegistry`, so two slots naming the same (agent, host) get one instance and one poll.
//
// The per-slot host is the whole point: every slot's REMOTE tab used to read the one global
// `sshStore.selectedSshHost`, so two hosts could never be on screen together. See
// docs/plan/done/usage-monitor-entity-refactor.md.
const props = defineProps({
  slotId: { type: String, required: true },
});

const { sshHosts } = useSsh();

const target = computed(() => slotTarget(props.slotId));

// Resolved in a watcher, not a computed. `getMonitor` CREATES a monitor on first request - it
// starts a poll and installs watchers - and a computed getter is not a legal place for that; it is
// supposed to be a pure function of its dependencies. The watcher is the side-effect site.
//
// Every `getMonitor` is a HOLD that this slot owns and must give back, or the monitor for a host the
// user merely glanced at keeps polling it over SSH for the rest of the session. Both agents are held
// while the slot is mounted, not just the visible one: each tab renders its own power icon off its
// monitor's live state.
const monitors = shallowRef(monitorsFor(target.value));
function monitorsFor(t) {
  return { antigravity: getMonitor('antigravity', t.host), claudecode: getMonitor('claudecode', t.host) };
}
function releaseMonitors(pair) {
  releaseMonitor(pair.antigravity);
  releaseMonitor(pair.claudecode);
}
watch(() => target.value.host, () => {
  // Acquire the new pair BEFORE releasing the old one: another slot may be watching the same host,
  // and dropping to zero holders in between would stop and restart a poll that never needed to stop.
  const previous = monitors.value;
  monitors.value = monitorsFor(target.value);
  releaseMonitors(previous);
});
onUnmounted(() => releaseMonitors(monitors.value));

const monitor = computed(() => monitors.value[target.value.agentId]);

// Which of the AG|CC pair this slot is showing, within whichever scope is active. The two are
// remembered separately ("which agent do I watch on this machine" vs "...on the remote host") so
// picking AG under REMOTE does not silently re-point the LOCAL tab too.
const activeAgentKey = computed(() => (target.value.scope === 'remote' ? target.value.remoteAgent : target.value.localAgent));
function setAgent(key) {
  setSlotTarget(props.slotId, target.value.scope === 'remote' ? { remoteAgent: key } : { localAgent: key });
}

// LOCAL and REMOTE offer the same AG|CC pair, so they are two lists of one shape rendered by one
// loop (see the template). Each tab carries the monitor for ITS agent on the scope's machine, which
// is what lets its power icon show and toggle that monitor specifically.
const srcTabs = computed(() => {
  const remote = target.value.scope === 'remote';
  return [
    { key: 'ag', label: 'AG', title: remote ? 'Antigravity (remote)' : 'Antigravity', icon: '/antigravity-icon.png', monitor: monitors.value.antigravity },
    { key: 'cc', label: 'CC', title: remote ? 'Claude Code (remote)' : 'Claude Code (local)', icon: '/claude-icon.png', monitor: monitors.value.claudecode },
  ];
});

// Contract C-3 (docs/plan/done/1.20.1-flow-audit-fixes.md §1.1). When the circuit breaker halts a monitor
// the existing power icon turns AMBER - a third state on the control that is already there, never a
// new row or banner (CLAUDE.md, UI Extreme Narrow). It matters because `is-on` while nothing is
// polling is the app lying about the one thing the user opened this card to judge: whether the
// number in front of them is current.
function powerTitle(src) {
  const m = src.monitor;
  if (m.pollHalted) return `${src.title} - ${m.error || `host "${target.value.host}" unreachable`} - click to retry now`;
  if (m.locked) return `${src.title} monitoring locked OFF - Proxy mode active, native usage data would be meaningless`;
  return `${src.title} monitoring ${m.enabled ? 'ON - click to turn off' : 'OFF - click to turn on'}`;
}

// A halted monitor is still switched ON, so the ordinary toggle would read the click as "turn this
// off" - the opposite of what someone clicking an amber warning wants. While halted the icon is a
// retry button; every other time it is the toggle it has always been.
function onPowerClick(m) {
  if (m.pollHalted) { m.retryAfterHalt(); return; }
  m.toggle();
}

const activeAgentId = computed(() => target.value.agentId);
const activeAgentName = computed(() => (activeAgentId.value === 'antigravity') ? 'Antigravity' : 'Claude Code');

const showEmailKey = `aki-usage-slot-${props.slotId}-show-email`;
const showEmail = ref(localStorage.getItem(showEmailKey) !== 'false');
function toggleEmail() {
  showEmail.value = !showEmail.value;
  localStorage.setItem(showEmailKey, String(showEmail.value));
}
// Which corner the account popup unfolds from, derived from where the slot sits in the grid rather
// than named per slot. The old A/B/C/D switch had a `default` that every row-3+ slot fell into, so
// the bottom-most row would have opened its menu downwards, straight into the section's
// `overflow-y: auto` clip. Column comes from the slot's position in its row; vertical side from
// whether the row is in the panel's top half - which reproduces the old mapping exactly at 1 row
// (A opens down) and at 2 rows (A/B down, C/D up).
const popupPosition = computed(() => {
  const idx = Math.max(slotIndexOf(props.slotId), 0);
  const col = idx % SLOTS_PER_ROW === 0 ? 'l' : 'r';
  const row = Math.floor(idx / SLOTS_PER_ROW);
  const vert = row < Math.max(tierCount.value, 1) / 2 ? 't' : 'b';
  return `popup-pos-${vert}${col}`;
});
// Per-slot viewing email/key state: lets Slot A and Slot B independently select and display
// different active or cached accounts from the same monitor's data, persisted per slot.
const slotViewingEmailKey = `aki-usage-slot-${props.slotId}-viewing-account`;
const slotViewingEmail = ref(localStorage.getItem(slotViewingEmailKey) || null);

function handleSelectAccount(keyOrEmail) {
  const nextVal = slotViewingEmail.value === keyOrEmail ? null : keyOrEmail;
  slotViewingEmail.value = nextVal;
  if (nextVal) {
    localStorage.setItem(slotViewingEmailKey, nextVal);
  } else {
    localStorage.removeItem(slotViewingEmailKey);
  }
}

const slotAccountInfo = computed(() => {
  const src = monitor.value;
  if (activeAgentId.value !== 'antigravity' || !src.data) {
    return { data: src.data, isCached: src.isCached, cachedAt: src.cachedAt };
  }

  const key = slotViewingEmail.value;
  if (!key) {
    return { data: src.data, isCached: src.isCached, cachedAt: src.cachedAt };
  }

  // A pin names an ENTITY - `email:sourceType` - because one Google account can be signed into the
  // Antigravity IDE and the desktop/CLI pair at the same time, with two separate quotas. Matching a
  // pin on its email alone is what let this card render the CLI session's numbers under the IDE
  // label (agUsageCache: the entity is `(host, email, sourceType)`).
  const emailPart = key.includes(':') ? key.slice(0, key.indexOf(':')) : key;
  const typePart = key.includes(':') ? key.slice(key.indexOf(':') + 1) : null;
  const isPinned = (a) => !!a && a.email === emailPart && (typePart === null || (a.sourceType || 'ide') === typePart);

  // Check live match in allAccounts or src.data:
  if (src.data.allAccounts && Array.isArray(src.data.allAccounts)) {
    // A legacy pin carrying no session type can match two live accounts. Two candidates is not a
    // reason to pick one: show nothing rather than the wrong session's quota, and let the fallthrough
    // below re-derive an honest state.
    const liveMatches = src.data.allAccounts.filter(isPinned);
    if (liveMatches.length === 1) {
      return { data: liveMatches[0], isCached: false, cachedAt: null };
    }
  } else if (isPinned(src.data)) {
    return { data: src.data, isCached: false, cachedAt: null };
  }

  // Fallback to the offline cache, scoped to the machine THIS slot is watching. Asking the cache
  // module rather than parsing localStorage here is what makes that scoping possible at all: the
  // old inline read matched on email alone, so a slot on host B could render host A's reading.
  const acc = loadAgAccount(key, target.value.host);
  if (acc) {
    return { data: acc.data, isCached: true, cachedAt: acc.fetchedAt };
  }

  // The pin does not resolve on the machine this slot is currently watching. That is the CORRECT
  // answer, not an error: a pin is a host-free `email:sourceType` handle by design, so pointing the
  // slot at another host is expected to miss - the account genuinely is not signed in over there.
  // The card falls back to whatever IS live here, which labels itself with its own email, so nothing
  // is shown under the wrong name.
  //
  // What used to happen instead: a watcher read this state as "stale preference" and deleted BOTH
  // the in-memory selection and the persisted `aki-usage-slot-<id>-viewing-account` key - so merely
  // looking at another host destroyed the user's pinned account, and pointing the slot back found
  // nothing to restore. The preference is the user's, expressed by an explicit click; only another
  // explicit click (handleSelectAccount) may change it. Nothing here writes to localStorage at all,
  // and nothing here can touch any other slot's key (project Regression Guard - Multi-entity State).
  return { data: src.data, isCached: src.isCached, cachedAt: src.cachedAt };
});
</script>

<style scoped>
.usage-column {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.column-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding-bottom: 4px;
  border-bottom: 1px dashed rgba(255, 255, 255, 0.1);
}

.tab-group {
  display: flex;
  align-items: center;
  gap: 2px;
  min-width: 0;
}

.tab {
  display: flex;
  align-items: center;
  gap: 4px;
  background: transparent;
  border: 1px solid transparent;
  border-radius: 4px;
  padding: 3px 7px;
  cursor: pointer;
  font-size: 10px;
  font-weight: 800;
  letter-spacing: 0.4px;
  color: var(--text-darker);
  opacity: 0.6;
  transition: opacity 0.15s ease, background 0.15s ease, color 0.15s ease;
}
.tab:hover {
  opacity: 0.9;
}
.tab.is-active {
  opacity: 1;
  background: rgba(96, 165, 250, 0.16);
  color: #e5e7eb;
  border-color: rgba(96, 165, 250, 0.35);
}

.tab:disabled,
.tab:disabled:hover {
  opacity: 0.3;
  cursor: not-allowed;
  background: transparent;
}

/* Contract C-3: the third state of the power icon - polling halted by the circuit breaker. Amber,
   between the green of "on" and the grey of "off", because that is exactly what it means: switched
   on, but not actually polling. Defined here rather than beside .src-power in main.css because the
   breaker only exists for usage monitors; the other users of that global class have no such state.
   No new element is introduced (CLAUDE.md, UI Extreme Narrow) - the colour and the tooltip carry it. */
.src-power.is-halted {
  color: #f59e0b;
}

.src-icon {
  width: 18px;
  height: 18px;
  border-radius: 4px;
  display: block;
  object-fit: contain;
}

/* Deliberately small: since 1.19.0 the REMOTE tab carries the same AG | CC pair as LOCAL, so the
   host picker no longer owns the whole right-hand group - it gives up the width those two tabs
   need. The full host name still shows in the open dropdown and in the title tooltip. */
/* Narrow mode (docs/plan/done/narrow-mode-and-ux-1.14.0.md §B2): labels are hidden via the global
   .u-narrow-hide utility (applied in the template); this block only tightens the layout that
   utility can't express - icon-only tabs no longer need the old label-sized horizontal padding.
   .host-select-mini's own narrow-width step lives in main.css beside its base definition. */
@media (max-width: 700px) {
  .tab {
    padding: 3px 5px;
    gap: 2px;
  }
}
</style>
