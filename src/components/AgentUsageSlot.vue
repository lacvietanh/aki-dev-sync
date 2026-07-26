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
          :title="src.monitor.locked ? `${src.title} monitoring locked OFF - Proxy mode active, native usage data would be meaningless` : `${src.title} monitoring ${src.monitor.enabled ? 'ON - click to turn off' : 'OFF - click to turn on'}`"
          @click="setAgent(src.key)"
        >
          <i class="fa-solid fa-power-off src-power" :class="[src.monitor.enabled ? 'is-on' : 'is-off', { 'is-locked': src.monitor.locked }]"
             @click.stop="src.monitor.toggle()"></i>
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
      :stale="monitor.stale"
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
      @logout-success="monitor.recheckAfterLogout"
      @toggle-email="toggleEmail"
    />
  </div>
</template>

<script setup>
import { ref, shallowRef, computed, watch } from 'vue';
import AgentUsage from './AgentUsage.vue';
import { useSsh } from '../composables/useSsh';
import { getMonitor } from '../composables/usageMonitorRegistry';
import { loadAgAccount } from '../composables/agUsageCache';
import { slotTarget, setSlotTarget } from '../store/usageSlotStore';

// A slot is a VIEW onto a UsageMonitor, never an owner of one. It holds only which monitor to look
// at - scope (LOCAL/REMOTE), agent (AG/CC) and, since the entity refactor, WHICH REMOTE HOST - plus
// its own display preferences. The monitors themselves are session-lived and shared through
// `usageMonitorRegistry`, so two slots naming the same (agent, host) get one instance and one poll.
//
// The per-slot host is the whole point: every slot's REMOTE tab used to read the one global
// `sshStore.selectedSshHost`, so two hosts could never be on screen together. See
// docs/plan/usage-monitor-entity-refactor.md.
const props = defineProps({
  slotId: { type: String, required: true },
});

const { sshHosts } = useSsh();

const target = computed(() => slotTarget(props.slotId));

// Resolved in a watcher, not a computed. `getMonitor` CREATES a monitor on first request - it
// starts a poll and installs watchers - and a computed getter is not a legal place for that; it is
// supposed to be a pure function of its dependencies. The watcher is the side-effect site.
const monitors = shallowRef(monitorsFor(target.value));
function monitorsFor(t) {
  return { antigravity: getMonitor('antigravity', t.host), claudecode: getMonitor('claudecode', t.host) };
}
watch(() => target.value.host, () => { monitors.value = monitorsFor(target.value); });

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

const activeAgentId = computed(() => target.value.agentId);
const activeAgentName = computed(() => (activeAgentId.value === 'antigravity') ? 'Antigravity' : 'Claude Code');

const showEmailKey = `aki-usage-slot-${props.slotId}-show-email`;
const showEmail = ref(localStorage.getItem(showEmailKey) !== 'false');
function toggleEmail() {
  showEmail.value = !showEmail.value;
  localStorage.setItem(showEmailKey, String(showEmail.value));
}
const popupPosition = computed(() => {
  switch (props.slotId) {
    case 'A': return 'popup-pos-tl';
    case 'B': return 'popup-pos-tr';
    case 'C': return 'popup-pos-bl';
    case 'D': return 'popup-pos-br';
    default: return 'popup-pos-tl';
  }
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
    return { data: src.data, isCached: src.isCached, cachedAt: src.cachedAt, isMissing: false };
  }

  const key = slotViewingEmail.value;
  if (!key) {
    return { data: src.data, isCached: src.isCached, cachedAt: src.cachedAt, isMissing: false };
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
      return { data: liveMatches[0], isCached: false, cachedAt: null, isMissing: false };
    }
  } else if (isPinned(src.data)) {
    return { data: src.data, isCached: false, cachedAt: null, isMissing: false };
  }

  // Fallback to the offline cache, scoped to the machine THIS slot is watching. Asking the cache
  // module rather than parsing localStorage here is what makes that scoping possible at all: the
  // old inline read matched on email alone, so a slot on host B could render host A's reading.
  const acc = loadAgAccount(key, target.value.host);
  if (acc) {
    return { data: acc.data, isCached: true, cachedAt: acc.fetchedAt, isMissing: false };
  }

  return { data: src.data, isCached: src.isCached, cachedAt: src.cachedAt, isMissing: true };
});

// Defensive fallback watcher: if selected account is missing from live & offline cache once loaded, clear state
watch(slotAccountInfo, (info) => {
  if (info.isMissing && slotViewingEmail.value && !monitor.value.loading) {
    slotViewingEmail.value = null;
    localStorage.removeItem(slotViewingEmailKey);
  }
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
.host-select-mini {
  background-color: var(--bg-tertiary);
  color: var(--text-light);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 4px;
  padding: 1px 2px;
  height: 19px;
  max-width: 70px;
  font-size: 9px;
  font-family: inherit;
  outline: none;
  cursor: pointer;
}
.host-select-mini:hover {
  background-color: rgba(255, 255, 255, 0.08);
  border-color: rgba(255, 255, 255, 0.2);
}
.host-select-mini:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

/* Narrow mode (docs/plan/done/narrow-mode-and-ux-1.14.0.md §B2): labels are hidden via the global
   .u-narrow-hide utility (applied in the template); this block only tightens the layout that
   utility can't express - icon-only tabs no longer need the old label-sized horizontal padding. */
@media (max-width: 700px) {
  .tab {
    padding: 3px 5px;
    gap: 2px;
  }

  /* Icon-only tabs still need their share of the row here, so the picker shrinks again. */
  .host-select-mini {
    max-width: 46px;
  }
}
</style>
