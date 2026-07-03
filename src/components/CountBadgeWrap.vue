<template>
  <div class="sync-btn-wrap">
    <span v-if="count > 0" class="sync-count-badge">{{ count }}</span>
    <slot />
  </div>
</template>

<script setup>
defineProps({
  count: { type: Number, default: 0 },
});
</script>

<style scoped>
/* Count badge — absolute overlay above the button, zero impact on layout */
.sync-btn-wrap {
  position: relative;
  display: inline-flex;
}

.sync-count-badge {
  position: absolute;
  top: -5px;
  right: -5px;
  z-index: 1;
  min-width: 14px;
  height: 14px;
  padding: 0 3px;
  background: #ef4444;
  border-radius: 4px;
  font-size: 9px;
  font-weight: 700;
  line-height: 14px;
  text-align: center;
  color: #fff;
  pointer-events: none;
  user-select: none;
}
</style>
