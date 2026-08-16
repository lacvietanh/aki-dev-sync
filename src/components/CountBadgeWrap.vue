<template>
  <!-- Position relative anchor for .cell-badge overlay around action buttons. -->
  <div class="sync-btn-wrap">
    <span v-if="count > 0" class="cell-badge cell-badge-top sync-count-badge">{{ count }}</span>
    <!-- Corner delete indicator badge when --delete is armed. -->
    <i v-if="deleteArmed"
       class="fa-solid fa-trash cell-badge cell-badge-bottom sync-delete-badge"
       :class="deleteSide === 'left' ? 'cell-badge-left' : ''"
       :title="deleteTitle"></i>
    <slot />
  </div>
</template>

<script setup>
defineProps({
  count: { type: Number, default: 0 },
  deleteArmed: { type: Boolean, default: false },
  deleteTitle: { type: String, default: '' },
  deleteSide: { type: String, default: 'right' },
});
</script>

<style scoped>
.sync-btn-wrap {
  position: relative;
  display: inline-flex;
}

/* Pending count badge styling (red). */
.sync-count-badge {
  z-index: 1;
  background: #ef4444;
  color: #fff;
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
  color: #ef4444;
  font-size: 7px;
  line-height: 1;
  opacity: 0.95;
}
</style>
