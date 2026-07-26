<template>
  <div class="agent-usage-section" :style="{ height: sectionHeight }">
    <div v-for="(row, rIdx) in activeTierRows" :key="rIdx" class="tier-row-container">
      <div class="usage-split-layout">
        <template v-for="(slot, sIdx) in row" :key="slot.id">
          <AgentUsageSlot :slot-id="slot.id" />
          <div v-if="sIdx < row.length - 1" class="column-divider"></div>
        </template>
      </div>
      <div v-if="rIdx < activeTierRows.length - 1" class="row-divider"></div>
    </div>
  </div>
</template>

<script setup>
// @docs docs/plan/usage-monitor-entity-refactor.md
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

const sectionHeight = computed(() => {
  const count = tierCount.value;
  if (count <= 1) return '161px';
  return `${Math.min(count * 161 + (count - 1) * 10, 335)}px`;
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
