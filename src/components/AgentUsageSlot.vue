<template>
  <div class="usage-column">
    <div class="column-header">
      <!-- Left: which category - LOCAL or REMOTE. -->
      <div class="tab-group">
        <button class="tab" :class="{ 'is-active': topTab === 'local' }" title="Local" @click="topTab = 'local'">
          <i class="fa-solid fa-laptop-code"></i> <span class="u-narrow-hide">LOCAL</span>
        </button>
        <button
          class="tab"
          :class="{ 'is-active': topTab === 'remote' }"
          title="Remote"
          @click="topTab = 'remote'"
        >
          <i class="fa-solid fa-cloud"></i> <span class="u-narrow-hide">REMOTE</span>
        </button>
      </div>

      <!-- Right: LOCAL mode picks which local agent (each with its own power button,
           colored to double as the on/off status). REMOTE mode shows the global Remote Mode
           power icon (left of the host picker - only reachable/visible from this REMOTE tab,
           native contextual placement) followed by the SSH host it's monitoring. There's only
           one source in REMOTE, so the space freed by not needing a tab-per-agent goes here. -->
      <div class="tab-group">
        <template v-if="topTab === 'local'">
          <button
            v-for="src in localTabs"
            :key="src.key"
            class="tab src-tab"
            :class="{ 'is-active': localSub === src.key }"
            :title="src.source.locked ? `${src.title} monitoring locked OFF - Proxy mode active, native usage data would be meaningless` : `${src.title} monitoring ${src.source.enabled ? 'ON - click to turn off' : 'OFF - click to turn on'}`"
            @click="localSub = src.key"
          >
            <i class="fa-solid fa-power-off src-power" :class="[src.source.enabled ? 'is-on' : 'is-off', { 'is-locked': src.source.locked }]"
               @click.stop="!src.source.locked && src.source.toggle()"></i>
            <img :src="src.icon" class="src-icon" alt="" />
            <span class="u-narrow-hide">{{ src.label }}</span>
          </button>
        </template>
        <template v-else>
          <i
            class="fa-solid fa-power-off src-power"
            :class="ccRemote.enabled ? 'is-on' : 'is-off'"
            @click.stop="ccRemote.toggle()"
            :title="ccRemote.enabled ? 'Claude Code remote monitoring ON - click to turn off' : 'Claude Code remote monitoring OFF - click to turn on'"
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
      :data="slotAccountInfo.data"
      :loading="activeSource.loading"
      :error="activeSource.error"
      :stale="activeSource.stale"
      :isCached="slotAccountInfo.isCached"
      :cachedAt="slotAccountInfo.cachedAt"
      :showEmail="showEmail"
      :sourceOff="!activeSource.enabled"
      :locked="!!activeSource.locked"
      :accounts="activeSource.accounts"
      :viewing-email="slotViewingEmail"
      :active-email="activeSource.activeEmail"
      :active-emails="activeSource.activeEmails"
      @retry="activeSource.refresh"
      @select-account="handleSelectAccount"
      @logout-success="activeSource.resetAccount"
      @toggle-email="toggleEmail"
    />
  </div>
</template>

<script setup>
import { ref, computed, watch } from 'vue';
import AgentUsage from './AgentUsage.vue';
import { useSsh } from '../composables/useSsh';

// Each slot owns its own display selection (which source it shows) and email-visibility
// preference, persisted per slot-id. The underlying sources (ag / ccLocal / ccRemote) are
// shared reactive bundles owned by the parent - both slots can point at the same source
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

watch(topTab, (v) => { localStorage.setItem(topTabKey, v); });
watch(localSub, (v) => { localStorage.setItem(localSubKey, v); });

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

// Per-slot viewing email/key state: lets Slot A and Slot B independently select and display
// different active or cached accounts from the shared `ag` data source.
const slotViewingEmail = ref(null);

function handleSelectAccount(keyOrEmail) {
  slotViewingEmail.value = slotViewingEmail.value === keyOrEmail ? null : keyOrEmail;
}

const slotAccountInfo = computed(() => {
  const src = activeSource.value;
  if (activeAgentId.value !== 'antigravity' || !src.data) {
    return { data: src.data, isCached: src.isCached, cachedAt: src.cachedAt };
  }

  const key = slotViewingEmail.value;
  if (!key) {
    return { data: src.data, isCached: src.isCached, cachedAt: src.cachedAt };
  }

  const emailPart = key.includes(':') ? key.split(':')[0] : key;

  // Check live match in allAccounts or src.data:
  if (src.data.allAccounts && Array.isArray(src.data.allAccounts)) {
    const liveMatch = src.data.allAccounts.find(a => {
      const aKey = a.sourceType ? `${a.email}:${a.sourceType}` : a.email;
      return aKey === key || (key.includes(':') ? aKey === key : a.email === emailPart);
    });
    if (liveMatch) {
      return { data: liveMatch, isCached: false, cachedAt: null };
    }
  } else if (src.data.email === emailPart) {
    return { data: src.data, isCached: false, cachedAt: null };
  }

  // Fallback to offline cache in localStorage:
  const rawCache = localStorage.getItem('aki-antigravity-usage-cache-v2');
  if (rawCache) {
    try {
      const parsed = JSON.parse(rawCache);
      const acc = parsed.accounts?.[key] || parsed.accounts?.[emailPart];
      if (acc) {
        return { data: acc.data, isCached: true, cachedAt: acc.fetchedAt };
      }
    } catch (_) {}
  }
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

.src-icon {
  width: 18px;
  height: 18px;
  border-radius: 4px;
  display: block;
  object-fit: contain;
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

/* Narrow mode (docs/plan/done/narrow-mode-and-ux-1.14.0.md §B2): labels are hidden via the global
   .u-narrow-hide utility (applied in the template); this block only tightens the layout that
   utility can't express - icon-only tabs no longer need the old label-sized horizontal padding. */
@media (max-width: 700px) {
  .tab {
    padding: 3px 5px;
    gap: 2px;
  }
}
</style>
