<template>
  <!-- TOP: in-app terminal tabs (cyan, red if exited). BOTTOM: external Terminal.app windows (slate). -->
  <span v-if="tabs > 0" class="cell-badge cell-badge-top term-badge-tabs" :class="{ 'is-exited': exited }">{{ tabs }}</span>
  <span v-if="external > 0" class="cell-badge cell-badge-bottom term-badge-ext">{{ external }}</span>
</template>

<script setup>
defineProps({
  tabs:     { type: Number,  default: 0 },
  external: { type: Number,  default: 0 },
  exited:   { type: Boolean, default: false },
})
</script>

<style scoped>
.term-badge-tabs {
  background: var(--accent-cyan);
  color: #04121a;
}

.term-badge-tabs.is-exited {
  background: var(--accent-red, var(--accent-red));
  color: var(--white);
}

.term-badge-ext {
  background: var(--slate-400);
  color: var(--surface-deep);
}
</style>
