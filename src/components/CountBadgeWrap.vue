<template>
  <!-- Adds the position:relative anchor that main.css's .cell-badge overlay needs, around a button
       that does not have one of its own (the git / PUSH / PULL buttons). Zero impact on layout. -->
  <div class="sync-btn-wrap">
    <span v-if="count > 0" class="cell-badge cell-badge-top sync-count-badge">{{ count }}</span>
    <slot />
  </div>
</template>

<script setup>
defineProps({
  count: { type: Number, default: 0 },
});
</script>

<style scoped>
.sync-btn-wrap {
  position: relative;
  display: inline-flex;
}

/* Same geometry as every other cell badge (main.css .cell-badge) — only the colour is local. Red
   is the "pending work" reading, distinct from the cyan/amber/white count badges elsewhere. */
.sync-count-badge {
  z-index: 1;
  background: #ef4444;
  color: #fff;
}
</style>
