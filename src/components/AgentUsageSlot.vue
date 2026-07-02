<template>
  <div class="usage-column">
    <div class="column-header">
      <!-- Left: which category — LOCAL or REMOTE. -->
      <div class="tab-group">
        <button class="tab" :class="{ 'is-active': topTab === 'local' }" @click="topTab = 'local'">
          <i class="fa-solid fa-laptop-code"></i> LOCAL
        </button>
        <button
          class="tab"
          :class="{ 'is-active': topTab === 'remote' }"
          :disabled="remoteLockedByPeer"
          :title="remoteLockedByPeer ? 'Already shown in the other panel' : ''"
          @click="topTab = 'remote'"
        >
          <i class="fa-solid fa-cloud"></i> REMOTE
        </button>
      </div>

      <!-- Right: LOCAL mode picks which local agent (each with its own power button,
           colored to double as the on/off status). REMOTE mode shows the global Remote Mode
           power icon (left of the host picker — only reachable/visible from this REMOTE tab,
           native contextual placement) followed by the SSH host it's monitoring. There's only
           one source in REMOTE, so the space freed by not needing a tab-per-agent goes here. -->
      <div class="tab-group">
        <template v-if="topTab === 'local'">
          <button
            v-for="src in localTabs"
            :key="src.key"
            class="tab src-tab"
            :class="{ 'is-active': localSub === src.key }"
            :disabled="peerView === `local-${src.key}`"
            :title="peerView === `local-${src.key}` ? 'Already shown in the other panel' : `${src.title} monitoring ${src.source.enabled ? 'ON — click to turn off' : 'OFF — click to turn on'}`"
            @click="localSub = src.key"
          >
            <i class="fa-solid fa-power-off src-power" :class="src.source.enabled ? 'is-on' : 'is-off'"
               @click.stop="src.source.toggle()"></i>
            <img :src="src.icon" class="src-icon" alt="" />
            <span>{{ src.label }}</span>
          </button>
        </template>
        <template v-else>
          <i
            class="fa-solid fa-power-off src-power remote-power"
            :class="remoteModeEnabled ? 'is-on' : 'is-off'"
            @click.stop="toggleRemoteMode"
            :title="remoteModeEnabled ? 'Remote Mode ON — click to disable all SSH/remote operations' : 'Remote Mode OFF — click to enable'"
          ></i>
          <select v-model="selectedSshHost" class="host-select-mini" :disabled="!ccRemote.enabled" title="Remote host to monitor">
            <option value="" disabled>Select Host</option>
            <option v-for="h in sshHosts" :key="h" :value="h">{{ h }}</option>
          </select>
        </template>
      </div>
    </div>

    <AgentUsage
      :agentId="activeAgentId"
      :agentName="activeAgentName"
      :data="activeSource.data"
      :loading="activeSource.loading"
      :error="activeSource.error"
      :stale="activeSource.stale"
      :isCached="activeSource.isCached"
      :cachedAt="activeSource.cachedAt"
      :showEmail="showEmail"
      :sourceOff="!activeSource.enabled"
      :accounts="activeSource.accounts"
      :viewing-email="activeSource.viewingEmail"
      :active-email="activeSource.activeEmail"
      @retry="activeSource.refresh"
      @force-sync="activeSource.forceSync"
      @select-account="activeSource.selectAccount"
      @toggle-email="toggleEmail"
    />
  </div>
</template>

<script setup>
import { ref, computed, watch } from 'vue';
import AgentUsage from './AgentUsage.vue';
import { useSsh } from '../composables/useSsh';
import { remoteModeEnabled, toggleRemoteMode } from '../store/remoteModeStore';
import { slotViews } from '../store/usageViewStore';

// Each slot owns its own display selection (which source it shows) and email-visibility
// preference, persisted per slot-id. The underlying sources (ag / ccLocal / ccRemote) are
// shared reactive bundles owned by the parent — both slots can point at the same source
// simultaneously without double-polling, since polling is driven by the source's own
// `enabled` flag, not by which slot currently displays it.
const props = defineProps({
  slotId: { type: String, required: true },
  defaultTopTab: { type: String, default: 'local' },   // 'local' | 'remote'
  defaultLocalSub: { type: String, default: 'ag' },     // 'ag' | 'cc'
  ag: { type: Object, required: true },
  ccLocal: { type: Object, required: true },
  ccRemote: { type: Object, required: true },
});

const { sshHosts, selectedSshHost } = useSsh();

const topTabKey = `aki-usage-slot-${props.slotId}-top`;
const localSubKey = `aki-usage-slot-${props.slotId}-sub`;
const showEmailKey = `aki-usage-slot-${props.slotId}-show-email`;

const topTab = ref(localStorage.getItem(topTabKey) || props.defaultTopTab);
const localSub = ref(localStorage.getItem(localSubKey) || props.defaultLocalSub);
const showEmail = ref(localStorage.getItem(showEmailKey) !== 'false');

// Same-view lock: the two slots share `slotViews` (A/B) so each can tell what the other is
// currently showing and disable picking that exact view here — the sole remaining Vue
// components run their setup() synchronously in template order, so by the time slot B's
// setup runs, slot A has already registered its view. If a restored (localStorage) selection
// would collide with what the other slot registered first, fall back to the next free view
// instead of leaving two panels stuck showing the same thing.
const peerId = props.slotId === 'A' ? 'B' : 'A';
const viewOrder = ['local-ag', 'local-cc', 'remote'];
function viewKeyOf(top, sub) { return top === 'local' ? `local-${sub}` : 'remote'; }

const initialView = viewKeyOf(topTab.value, localSub.value);
const resolvedInitial = initialView === slotViews[peerId]
  ? viewOrder.find(v => v !== slotViews[peerId])
  : initialView;
if (resolvedInitial !== initialView) {
  if (resolvedInitial === 'remote') { topTab.value = 'remote'; }
  else { topTab.value = 'local'; localSub.value = resolvedInitial.split('-')[1]; }
}
slotViews[props.slotId] = viewKeyOf(topTab.value, localSub.value);

const peerView = computed(() => slotViews[peerId]);
const remoteLockedByPeer = computed(() => topTab.value !== 'remote' && peerView.value === 'remote');

watch(topTab, (v) => { localStorage.setItem(topTabKey, v); slotViews[props.slotId] = viewKeyOf(v, localSub.value); });
watch(localSub, (v) => { localStorage.setItem(localSubKey, v); slotViews[props.slotId] = viewKeyOf(topTab.value, v); });

function toggleEmail() {
  showEmail.value = !showEmail.value;
  localStorage.setItem(showEmailKey, String(showEmail.value));
}

const localTabs = computed(() => [
  { key: 'ag', label: 'AG', title: 'Antigravity', icon: '/antigravity-icon.png', source: props.ag },
  { key: 'cc', label: 'CC', title: 'Claude Code (local)', icon: '/claude-icon.png', source: props.ccLocal },
]);

// useAgentUsage() returns the same shape for every agent (accounts/viewingEmail/etc. are
// always present, just no-ops for Claude Code), so a single computed can drive one
// <AgentUsage> binding instead of three near-identical blocks.
const activeAgentId = computed(() => (topTab.value === 'local' && localSub.value === 'ag') ? 'antigravity' : 'claudecode');
const activeAgentName = computed(() => (activeAgentId.value === 'antigravity') ? 'Antigravity' : 'Claude Code');
const activeSource = computed(() => {
  if (topTab.value === 'local') return localSub.value === 'ag' ? props.ag : props.ccLocal;
  return props.ccRemote;
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

.src-power {
  font-size: 9px;
  padding: 1px;
  border-radius: 3px;
  cursor: pointer;
  transition: color 0.15s ease, background 0.15s ease;
}
.src-power:hover {
  background: rgba(255, 255, 255, 0.08);
}
.src-power.is-on {
  color: #22c55e;
}
.src-power.is-off {
  color: #6b7280;
}
.remote-power {
  font-size: 10px;
  margin-right: 2px;
}

.tab:disabled,
.tab:disabled:hover {
  opacity: 0.3;
  cursor: not-allowed;
  background: transparent;
}

.src-icon {
  width: 13px;
  height: 13px;
  border-radius: 3px;
  display: block;
}

.host-select-mini {
  background-color: var(--bg-tertiary);
  color: var(--text-light);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 4px;
  padding: 2px 4px;
  height: 21px;
  max-width: 110px;
  font-size: 10px;
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
</style>
