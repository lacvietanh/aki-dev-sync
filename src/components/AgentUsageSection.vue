<template>
  <div class="agent-usage-section" :style="{ height: sectionHeight }">
    <!-- Separators between slots and rows are CSS borders on neighbours (UI Extreme Narrow). -->
    <div v-for="(row, rIdx) in activeTierRows" :key="rIdx" class="tier-row-container">
      <div class="usage-split-layout">
        <AgentUsageSlot v-for="slotId in row" :key="slotId" :slot-id="slotId" />
      </div>
    </div>
  </div>
</template>

<script setup>
// Tier layout: slots resolve individual monitors from usageMonitorRegistry (docs/plan/done/usage-monitor-entity-refactor.md).
import { computed } from 'vue';
import AgentUsageSlot from './AgentUsageSlot.vue';
import { tierCount, rowSlotIds } from '../store/usageTierStore';
import { slotTarget } from '../store/usageSlotStore';

// Dynamically generate slot IDs per tier row based on tierCount.
const activeTierRows = computed(() =>
  Array.from({ length: tierCount.value }, (_, r) => rowSlotIds(r))
);

// A row reserves its tallest agent's data card, measured from the styles below and AgentUsage.vue, so data arriving never shifts the project list; a taller state scrolls.
const SLOT_HEIGHT_PX = { claudecode: 150, antigravity: 122 };
const SECTION_CHROME_PX = 13; // padding 6 + 6, border-bottom 1
const ROW_SEPARATOR_PX = 13; // gap 6, padding-top 6, border-top 1

const sectionHeight = computed(() => {
  const rows = activeTierRows.value.map(ids => Math.max(...ids.map(id => SLOT_HEIGHT_PX[slotTarget(id).agentId])));
  return `${rows.reduce((sum, h) => sum + h, 0) + (rows.length - 1) * ROW_SEPARATOR_PX + SECTION_CHROME_PX}px`;
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
  border-top: 1px dashed var(--border-color);
  padding-top: 6px;
}

/* Narrower, low-contrast scrollbar than the app-wide 6px rule (main.css) - this element only. */
.agent-usage-section::-webkit-scrollbar {
  width: 4px;
}
.agent-usage-section::-webkit-scrollbar-thumb {
  background: var(--surface-hover);
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

/* Column separator: border on sibling slots without divider nodes. */
.usage-split-layout > * + * {
  border-left: 1px solid var(--border-color);
  padding-left: 8px;
}

/* Tighten horizontal spacing in narrow container mode. */
@container main-view (max-width: 700px) {
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
