<template>
  <div class="agent-usage-section" :style="{ height: sectionHeight }">
    <div v-for="(row, rIdx) in activeTierRows" :key="rIdx" class="tier-row-container">
      <div class="usage-split-layout">
        <template v-for="(slot, sIdx) in row" :key="slot.id">
          <AgentUsageSlot
            :slot-id="slot.id"
            :default-top-tab="slot.defaultTop"
            :default-local-sub="slot.defaultSub"
            :ag="ag"
            :cc-local="ccLocal"
            :cc-remote="ccRemote"
          />
          <div v-if="sIdx < row.length - 1" class="column-divider"></div>
        </template>
      </div>
      <div v-if="rIdx < activeTierRows.length - 1" class="row-divider"></div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, reactive, watch } from 'vue';
import AgentUsageSlot from './AgentUsageSlot.vue';
import { useSsh } from '../composables/useSsh';
import { useAgentUsage } from '../composables/useAgentUsage';
import { claudeMode } from '../store/claudeModeStore';
import { tierCount } from '../store/usageTierStore';

const { selectedSshHost } = useSsh();

// Declarative N-Tier configuration schema:
// Row 0 (Tier 1): Slot A & Slot B
// Row 1 (Tier 2): Slot C & Slot D
// Standardized architecture: adding Tiers requires zero template code changes.
const ALL_TIER_ROWS = [
  [
    { id: 'A', defaultTop: 'local', defaultSub: 'ag' },
    { id: 'B', defaultTop: 'local', defaultSub: 'ag' }
  ],
  [
    { id: 'C', defaultTop: 'local', defaultSub: 'cc' },
    { id: 'D', defaultTop: 'remote', defaultSub: 'cc' }
  ]
];

const activeTierRows = computed(() => {
  return ALL_TIER_ROWS.slice(0, tierCount.value);
});

const sectionHeight = computed(() => {
  const count = tierCount.value;
  if (count <= 1) return '161px';
  return `${Math.min(count * 161 + (count - 1) * 10, 335)}px`;
});

// One-time seed: the ccRemote monitor used to piggyback on the single `aki-remote-mode-enabled`
// flag. Now it has its own key - copy the old value across on first run after the split so a user
// who had remote mode OFF doesn't get the monitor silently re-enabled.
for (const [newKey, legacy] of [['aki-src-ccremote-enabled', 'aki-remote-mode-enabled']]) {
  if (localStorage.getItem(newKey) === null) {
    const old = localStorage.getItem(legacy);
    if (old !== null) localStorage.setItem(newKey, old);
  }
}

// Three independent, toggleable usage sources shared by both display slots. Polling is
// driven purely by each source's own `enabled` flag (persisted), not by which slot (if
// any) currently has it selected for display - so a slot can show a source that's off
// (rendered as "Monitoring off" or last-known cached data by AgentUsage) without that
// implicitly turning it on, and turning a source on/off doesn't care who's looking at it.
// `lockedRef`, when provided, blocks manual toggle() calls (guarded again at the UI layer
// in AgentUsageSlot.vue) - used for Claude Code local monitoring, which reads straight from
// the native Anthropic account API/pricing and is meaningless once Proxy mode reroutes
// traffic elsewhere (see claudeModeStore.js).
function useToggleableSource(agentKey, resolveHost, storageKey, defaultEnabled, lockedRef = null) {
  const enabled = ref(
    localStorage.getItem(storageKey) !== null
      ? localStorage.getItem(storageKey) === 'true'
      : defaultEnabled
  );
  function toggle() {
    if (lockedRef?.value) return;
    enabled.value = !enabled.value;
    localStorage.setItem(storageKey, String(enabled.value));
  }
  const hostRef = computed(() => (enabled.value ? resolveHost() : null));
  const hook = useAgentUsage(agentKey, hostRef);
  return reactive({ enabled, toggle, locked: lockedRef || computed(() => false), ...hook });
}

// Local sources cost nothing (no SSH round trip) - on by default, each with its own
// independent power switch inside the LOCAL tab.
const ag = useToggleableSource('antigravity', () => 'local', 'aki-src-ag-enabled', true);
const ccLocalLocked = computed(() => claudeMode.value === 'proxy');
const ccLocal = useToggleableSource('claudecode', () => 'local', 'aki-src-cclocal-enabled', true, ccLocalLocked);

// Proxy mode ON forces monitoring off (locked, can't be manually re-enabled - see toggle()
// above). Proxy mode OFF just unlocks the switch; it does NOT auto-restore a prior enabled
// state, by design, to keep this behavior simple and predictable - the user turns it back
// on themselves, same as any other fresh "off" state.
watch(claudeMode, (mode) => {
  if (mode === 'proxy') ccLocal.enabled = false;
});

// Remote costs an SSH round trip, so it gets its own switch like the two local sources
// (the power icon in the REMOTE tab) - independent of whether project sync/diff is on.
const ccRemote = useToggleableSource(
  'claudecode',
  () => selectedSshHost.value,
  'aki-src-ccremote-enabled',
  true
);
</script>

<style scoped>
.agent-usage-section {
  background: rgba(22, 22, 26, 0.6);
  border-bottom: 1px solid var(--border-color);
  padding: 6px 12px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  overflow-y: auto;
  overflow-x: hidden;
  box-sizing: border-box;
  transition: height 0.25s cubic-bezier(0.16, 1, 0.3, 1);
}

.tier-row-container {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.row-divider {
  height: 1px;
  background: rgba(255, 255, 255, 0.08);
  margin: 3px 0;
  border-bottom: 1px dashed rgba(255, 255, 255, 0.06);
}

/* Narrower, low-contrast scrollbar than the app-wide 6px rule (main.css) - this element only. */
.agent-usage-section::-webkit-scrollbar {
  width: 4px;
}
.agent-usage-section::-webkit-scrollbar-thumb {
  background: rgba(255, 255, 255, 0.1);
  border-radius: 4px;
}
.agent-usage-section::-webkit-scrollbar-thumb:hover {
  background: rgba(255, 255, 255, 0.2);
}

.usage-split-layout {
  display: flex;
  gap: 12px;
  align-items: stretch;
}

.column-divider {
  width: 1px;
  background: rgba(255, 255, 255, 0.05);
  margin: 0 4px;
}

/* Horizontal padding/gaps here were sized for the wide layout - tighten them at narrow so the
   LOCAL/REMOTE columns get more of the scarce width instead of losing it to whitespace. */
@media (max-width: 700px) {
  .agent-usage-section {
    padding: 6px 2px;
  }

  .usage-split-layout {
    gap: 2px;
  }

  .column-divider {
    margin: 0;
  }
}
</style>
