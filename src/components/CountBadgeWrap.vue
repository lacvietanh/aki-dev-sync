<template>
  <!-- Adds the position:relative anchor that main.css's .cell-badge overlay needs, around a button
       that does not have one of its own (the git / PUSH / PULL buttons). Zero impact on layout. -->
  <div class="sync-btn-wrap">
    <span v-if="count > 0" class="cell-badge cell-badge-top sync-count-badge">{{ count }}</span>
    <!-- WS-F: --delete corner indicator. Bottom is free since the count badge above owns top;
         deleteSide picks the horizontal corner so push and pull don't both point at the shared DRY
         toggle between them. Rendered only when armed; hidden entirely (not dimmed) otherwise, and
         the caller is responsible for passing deleteArmed=false while the button is in its STOP state. -->
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

/* Same geometry as every other cell badge (main.css .cell-badge) — only the colour is local. Red
   is the "pending work" reading, distinct from the cyan/amber/white count badges elsewhere. */
.sync-count-badge {
  z-index: 1;
  background: #ef4444;
  color: #fff;
}

/* Small glyph on a faint white chip — per user feedback, fully transparent read as too hard to spot
   at a glance; a light background+radius restores visibility while staying far below the count
   badge's prominence (no box-shadow, no border, minimal padding). */
.sync-delete-badge {
  /* `.cell-badge` sets pointer-events:none, which is right for the count badge (it has no title) but would silently kill this one's tooltip — the only place the consequence sentence is ever shown. Re-enabled here. */
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
