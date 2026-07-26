<template>
  <div class="agent-usage-section" :style="{ height: sectionHeight }">
    <!-- The separators between slots and between rows are borders on the neighbour, not elements:
         a divider <div> is a DOM node whose only job is to be a line (CLAUDE.md, UI Extreme Narrow). -->
    <div v-for="(row, rIdx) in activeTierRows" :key="rIdx" class="tier-row-container">
      <div class="usage-split-layout">
        <AgentUsageSlot v-for="slot in row" :key="slot.id" :slot-id="slot.id" />
      </div>
    </div>
  </div>
</template>

<script setup>
// @docs docs/plan/done/usage-monitor-entity-refactor.md
//
// Pure tier layout. This component used to construct four shared usage sources and pass them down
// to every slot - which is precisely what limited the app to one remote host: `agRemote`/`ccRemote`
// were singletons resolving against the one global `selectedSshHost`. Slots now resolve their own
// monitor from `usageMonitorRegistry` (keyed `agentId@host`), so two slots can watch two different
// machines at once, and nothing here has to know which.
import { computed } from 'vue';
import AgentUsageSlot from './AgentUsageSlot.vue';
import { tierCount } from '../store/usageTierStore';

// Declarative N-Tier configuration schema:
// Row 0 (Tier 1): Slot A & Slot B
// Row 1 (Tier 2): Slot C & Slot D
// Standardized architecture: adding Tiers requires zero template code changes. Each slot's default
// target lives with the rest of its persisted target state, in `store/usageSlotStore.js`.
const ALL_TIER_ROWS = [
  [{ id: 'A' }, { id: 'B' }],
  [{ id: 'C' }, { id: 'D' }]
];

const activeTierRows = computed(() => {
  return ALL_TIER_ROWS.slice(0, tierCount.value);
});

// One tier row is a fixed-height band (header + card body); rows are separated by ROW_GAP_PX.
// There is no cap: the old `Math.min(…, 335)` could never bind - the tallest reachable value is
// 2 * ROW_HEIGHT_PX + ROW_GAP_PX = 332 with the two tiers ALL_TIER_ROWS defines - so it only
// hid what the real ceiling was.
const ROW_HEIGHT_PX = 161;
const ROW_GAP_PX = 10;

const sectionHeight = computed(() => {
  const rows = Math.max(tierCount.value, 1);
  return `${rows * ROW_HEIGHT_PX + (rows - 1) * ROW_GAP_PX}px`;
});

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

/* Row separator: a border on the row that follows one, not a node of its own. */
.tier-row-container + .tier-row-container {
  border-top: 1px dashed rgba(255, 255, 255, 0.08);
  padding-top: 6px;
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

/* Column separator: a border on every slot after the first (the scope id lands on the child
   component's root element), so two slots still read as two columns with no divider node. */
.usage-split-layout > * + * {
  border-left: 1px solid rgba(255, 255, 255, 0.05);
  padding-left: 8px;
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

  .usage-split-layout > * + * {
    padding-left: 2px;
  }
}
</style>
