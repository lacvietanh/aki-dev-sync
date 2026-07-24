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
import { computed, reactive, watch } from 'vue';
import AgentUsageSlot from './AgentUsageSlot.vue';
import { useSsh } from '../composables/useSsh';
import { useAgentUsage } from '../composables/useAgentUsage';
import { claudeMode } from '../store/claudeModeStore';
import { tierCount } from '../store/usageTierStore';
import { agEnabled, ccLocalEnabled, ccRemoteEnabled, setSourceEnabled } from '../store/usageSourcesStore';

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

// Three independent, toggleable usage sources shared by both display slots. Polling is
// driven purely by each source's own `enabled` flag (persisted), not by which slot (if
// any) currently has it selected for display - so a slot can show a source that's off
// (rendered as "Monitoring off" or last-known cached data by AgentUsage) without that
// implicitly turning it on, and turning a source on/off doesn't care who's looking at it.
// `lockedRef`, when provided, blocks manual toggle() calls (guarded again at the UI layer
// in AgentUsageSlot.vue) - used for Claude Code local monitoring, which reads straight from
// the native Anthropic account API/pricing and is meaningless once Proxy mode reroutes
// traffic elsewhere (see claudeModeStore.js).
function useToggleableSource(agentKey, resolveHost, enabledRef, srcKey, lockedRef = null) {
  // `enabledRef` is now a MIRRORED store ref (usageSourcesStore) and `toggle` routes through the
  // `setSourceEnabled` ACTION — so a companion's power-button click runs on the host and mirrors
  // back; the monitor on/off is one shared Mac setting, not a per-device flag. localStorage is
  // written only on the host (inside the action); the companion never writes its own copy.
  function toggle() {
    if (lockedRef?.value) return;
    setSourceEnabled(srcKey, !enabledRef.value);
  }
  const hostRef = computed(() => (enabledRef.value ? resolveHost() : null));
  const hook = useAgentUsage(agentKey, hostRef);
  return reactive({ enabled: enabledRef, toggle, locked: lockedRef || computed(() => false), ...hook });
}

// Local sources cost nothing (no SSH round trip) - on by default, each with its own independent
// power switch inside the LOCAL tab. Enabled flags live in usageSourcesStore (mirrored + actionable).
const ag = useToggleableSource('antigravity', () => 'local', agEnabled, 'ag');
const ccLocalLocked = computed(() => claudeMode.value === 'proxy');
const ccLocal = useToggleableSource('claudecode', () => 'local', ccLocalEnabled, 'ccLocal', ccLocalLocked);

// Proxy mode ON forces monitoring off (locked, can't be manually re-enabled - see toggle()
// above). Proxy mode OFF just unlocks the switch; it does NOT auto-restore a prior enabled state,
// by design. Set the mirrored ref directly (not persisted, matching the original): on the host this
// mirrors to companions; on a companion the host's own watch drives the same value, so both agree.
watch(claudeMode, (mode) => {
  if (mode === 'proxy') ccLocalEnabled.value = false;
});

// Remote costs an SSH round trip, so it gets its own switch like the two local sources
// (the power icon in the REMOTE tab) - independent of whether project sync/diff is on.
const ccRemote = useToggleableSource('claudecode', () => selectedSshHost.value, ccRemoteEnabled, 'ccRemote');
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
