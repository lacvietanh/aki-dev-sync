<template>
  <!-- Position relative anchor for .cell-badge overlay around action buttons. -->
  <div class="sync-btn-wrap" :style="anchorName ? `anchor-name: ${anchorName}` : ''">
    <!-- popovertarget only works on a button, so the count badge is one only when it has a popover to open. -->
    <component :is="popoverId ? 'button' : 'span'"
               v-if="count > 0"
               :type="popoverId ? 'button' : undefined"
               class="cell-badge cell-badge-top sync-count-badge"
               :class="popoverId ? 'sync-count-badge-clickable' : ''"
               :title="countTitle"
               :popovertarget="popoverId || undefined"
               @click.stop>{{ count }}</component>
    <!-- Corner delete indicator badge when --delete is armed. -->
    <i v-if="deleteArmed"
       class="fa-solid fa-trash cell-badge cell-badge-bottom sync-delete-badge"
       :class="deleteSide === 'left' ? 'cell-badge-left' : ''"
       :title="deleteTitle"></i>
    <!-- Conflict overlay (docs/plan/conflict-detection-and-agy-report.md §4): free top-left corner, opens the read-only breakdown popover via the same native Popover API + CSS anchor positioning as the OPEN popup, without triggering the sync button underneath. -->
    <button v-if="conflictCount > 0"
            type="button"
            class="cell-badge cell-badge-top cell-badge-left sync-conflict-badge"
            :title="conflictTitle"
            :popovertarget="popoverId"
            @click.stop>⚠ {{ conflictCount }}</button>
    <!-- Deploy/hook overlays (docs/plan/deploy-action.md § "Visibility without Settings"): bottom-right,
         stacked with the delete badge - tiny letter chips, tooltip-only (no click target), so PUSH never
         grows a row for them (Extreme Narrow). -->
    <span v-if="deployOnPush || hasHooks" class="cell-badge-cluster">
      <i v-if="deployOnPush" class="sync-deploy-badge" :title="deployTitle">D</i>
      <i v-if="hasHooks" class="sync-hooks-badge" :title="hooksTitle">H</i>
    </span>
    <slot />
  </div>
</template>

<script setup>
defineProps({
  count: { type: Number, default: 0 },
  // Popover shared by the count badge and the conflict badge, so any lit badge reaches the same read-only breakdown.
  countTitle: { type: String, default: '' },
  popoverId: { type: String, default: '' },
  // Anchor for the shared popover: on the wrapper, which exists in every row, never on a badge that only renders while its count is above zero.
  anchorName: { type: String, default: '' },
  deleteArmed: { type: Boolean, default: false },
  deleteTitle: { type: String, default: '' },
  deleteSide: { type: String, default: 'right' },
  conflictCount: { type: Number, default: 0 },
  conflictTitle: { type: String, default: '' },
  deployOnPush: { type: Boolean, default: false },
  deployTitle: { type: String, default: '' },
  hasHooks: { type: Boolean, default: false },
  hooksTitle: { type: String, default: '' },
});
</script>

<style scoped>
.sync-btn-wrap {
  position: relative;
  display: inline-flex;
}

/* Pending count badge styling (red). Pushed 3px further off the corner than the shared .cell-badge
   default so its hit target (pointer-events: auto below, when clickable) overlaps the PUSH/PULL button
   less - misclicks on the badge instead of the button were reported after it became clickable. */
.sync-count-badge {
  top: -9px;
  right: -9px;
  z-index: 1;
  border: 0;
  font-family: inherit;
  background: var(--accent-red);
  color: var(--white);
}

/* Clickable only when wired to a popover - otherwise stays a pass-through overlay so the underlying button keeps receiving the click. */
.sync-count-badge-clickable {
  pointer-events: auto;
  cursor: pointer;
}

/* Conflict overlay: clickable, so pointer-events is re-enabled unlike the plain count badge. Same extra
   offset as .sync-count-badge, for the same reason (less hit-target overlap with the button). */
.sync-conflict-badge {
  top: -9px;
  left: -9px;
  pointer-events: auto;
  cursor: pointer;
  z-index: 2;
  border: 0;
  font-family: inherit;
  background: var(--accent-amber);
  color: var(--white);
}

/* Small delete indicator badge on faint chip. */
.sync-delete-badge {
  /* Enable pointer-events for tooltip visibility on delete badge. */
  pointer-events: auto;
  z-index: 1;
  min-width: 0;
  height: auto;
  padding: 1px 2px;
  border-radius: 3px;
  background: rgba(255, 255, 255, 0.85);
  box-shadow: none;
  color: var(--accent-red);
  font-size: 7px;
  line-height: 1;
  opacity: 0.95;
}

/* D/H letter chips, bottom-right corner - stacked beside the delete badge (which defaults right too),
   never widening the PUSH button (Extreme Narrow). */
.cell-badge-cluster {
  position: absolute;
  bottom: -3px;
  right: -3px;
  display: flex;
  gap: 2px;
  pointer-events: none;
  z-index: 1;
}

.sync-deploy-badge,
.sync-hooks-badge {
  position: static;
  pointer-events: auto;
  min-width: 0;
  height: auto;
  padding: 1px 3px;
  border-radius: 3px;
  box-shadow: 0 0 0 1px var(--shade);
  font-size: 8px;
  font-weight: 800;
  font-style: normal;
  line-height: 1.2;
  opacity: 1;
  background: rgba(255, 255, 255, 0.95);
}

.sync-deploy-badge {
  color: var(--accent-cyan);
}

.sync-hooks-badge {
  color: var(--text-darker);
}
</style>
