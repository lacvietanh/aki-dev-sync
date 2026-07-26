<template>
  <!-- Two absolute overlay badges — amber pinned-count top-right, white normal-count bottom-right.
       Renders nothing when a count is 0. The parent element MUST be position:relative (this
       component has no wrapper element of its own, by design — Extreme Narrow: no new DOM node
       in the flow, just overlays on whatever button/icon already exists; .btn-cell-trigger and
       CountBadgeWrap both provide that anchor). Geometry comes from main.css's shared .cell-badge
       pattern; only the colours are this component's own. -->
  <span v-if="pinned > 0" class="cell-badge cell-badge-top task-badge-pin">{{ pinned }}</span>
  <span v-if="open > 0" class="cell-badge cell-badge-bottom task-badge-normal">{{ open }}</span>
</template>

<script setup>
defineProps({
  pinned: { type: Number, default: 0 },
  open: { type: Number, default: 0 },
})
</script>

<style scoped>
/* Pinned open tasks - amber (matches the pin colour) */
.task-badge-pin {
  background: var(--accent-amber);
  color: #1a1000;
}

/* Normal open tasks - white */
.task-badge-normal {
  background: #fff;
  color: #04121a;
}
</style>
