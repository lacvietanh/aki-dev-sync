<template>
  <!-- TOP: in-app terminal tabs in this project's group (cyan — terminal identity; red when one of
       them has exited). BOTTOM: external Terminal.app windows/tabs standing in that project's
       directory RIGHT NOW (slate — re-derived from the process table every 5s, so it falls as
       windows are closed).
       No wrapper element (sibling of tasks/TaskCountBadges.vue, same contract): the parent MUST be
       position:relative. Geometry comes from main.css's shared .cell-badge pattern. The colours are
       deliberately different from the task badges (amber/white) so a terminal badge can never be
       misread as a task count. -->
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
  background: var(--accent-red, #ef4444);
  color: #fff;
}

.term-badge-ext {
  background: #94a3b8;
  color: #0b1220;
}
</style>
