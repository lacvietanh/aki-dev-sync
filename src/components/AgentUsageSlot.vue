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

      <!-- Right: agent selection and remote host picker. -->
      <div class="tab-group">
        <button
          v-for="src in srcTabs"
          :key="src.key"
          class="tab"
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

// A slot is a view onto a UsageMonitor entity resolved via usageMonitorRegistry.
const props = defineProps({
  slotId: { type: String, required: true },
});

const { sshHosts } = useSsh();

const target = computed(() => slotTarget(props.slotId));

// Acquire monitor hold pair for local/remote targets; release on host switch or unmount.
const monitors = shallowRef(monitorsFor(target.value));
function monitorsFor(t) {
  return { antigravity: getMonitor('antigravity', t.host), claudecode: getMonitor('claudecode', t.host) };
}
function releaseMonitors(pair) {
  releaseMonitor(pair.antigravity);
  releaseMonitor(pair.claudecode);
}
watch(() => target.value.host, () => {
  // Acquire new pair before releasing old to keep shared poll alive.
  const previous = monitors.value;
  monitors.value = monitorsFor(target.value);
  releaseMonitors(previous);
});
onUnmounted(() => releaseMonitors(monitors.value));

const monitor = computed(() => monitors.value[target.value.agentId]);

// Active agent tab key (AG/CC) preserved separately across local and remote scopes.
const activeAgentKey = computed(() => (target.value.scope === 'remote' ? target.value.remoteAgent : target.value.localAgent));
function setAgent(key) {
  setSlotTarget(props.slotId, target.value.scope === 'remote' ? { remoteAgent: key } : { localAgent: key });
}

// Source tabs for AG and CC with live monitors for power toggle state.
const srcTabs = computed(() => {
  const remote = target.value.scope === 'remote';
  return [
    { key: 'ag', label: 'AG', title: remote ? 'Antigravity (remote)' : 'Antigravity', icon: '/antigravity-icon.png', monitor: monitors.value.antigravity },
    { key: 'cc', label: 'CC', title: remote ? 'Claude Code (remote)' : 'Claude Code (local)', icon: '/claude-icon.png', monitor: monitors.value.claudecode },
  ];
});

// Power button tooltip: ON, OFF, locked proxy, or circuit breaker halted (Contract C-3).
function powerTitle(src) {
  const m = src.monitor;
  if (m.pollHalted) return `${src.title} - ${m.error || `host "${target.value.host}" unreachable`} - click to retry now`;
  if (m.locked) return `${src.title} monitoring locked OFF - Proxy mode active, native usage data would be meaningless`;
  return `${src.title} monitoring ${m.enabled ? 'ON - click to turn off' : 'OFF - click to turn on'}`;
}

// Toggle monitor or retry when halted by circuit breaker.
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
// Popup orientation derived from slot grid index (top/bottom, left/right).
const popupPosition = computed(() => {
  const idx = Math.max(slotIndexOf(props.slotId), 0);
  const col = idx % SLOTS_PER_ROW === 0 ? 'l' : 'r';
  const row = Math.floor(idx / SLOTS_PER_ROW);
  const vert = row < tierCount.value / 2 ? 't' : 'b';
  return `popup-pos-${vert}${col}`;
});
// Per-slot viewing email state persisted to localStorage.
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

  // Pin match against (email, sourceType) entity handle.
  const emailPart = key.includes(':') ? key.slice(0, key.indexOf(':')) : key;
  const typePart = key.includes(':') ? key.slice(key.indexOf(':') + 1) : null;
  const isPinned = (a) => !!a && a.email === emailPart && (typePart === null || (a.sourceType || 'ide') === typePart);

  // Check live match in allAccounts or single src.data.
  if (Array.isArray(src.data.allAccounts)) {
    const liveMatches = src.data.allAccounts.filter(isPinned);
    if (liveMatches.length === 1) {
      return { data: liveMatches[0], isCached: false, cachedAt: null };
    }
  } else if (isPinned(src.data)) {
    return { data: src.data, isCached: false, cachedAt: null };
  }

  // Offline cache fallback scoped to host.
  const acc = loadAgAccount(key, target.value.host);
  if (acc) {
    return { data: acc.data, isCached: true, cachedAt: acc.fetchedAt };
  }

  // Fallback to active live data without mutating or deleting slot preference.
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

/* Contract C-3: circuit breaker halted power icon indicator (amber). */
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

/* Tighten icon-only tab padding in narrow mode (labels hidden by .u-narrow-hide). */
@container main-view (max-width: 700px) {
  .tab {
    padding: 3px 5px;
    gap: 2px;
  }
}
</style>
