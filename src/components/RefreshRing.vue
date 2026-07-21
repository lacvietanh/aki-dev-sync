<template>
  <svg
    :class="['refresh-ring', overlay ? 'refresh-ring--overlay' : 'refresh-ring--inline', intervalS <= 0 && 'refresh-ring--off']"
    viewBox="0 0 36 36" aria-hidden="true"
    :title="intervalS > 0 ? `Auto-refresh every ${intervalS}s` : 'Auto-refresh paused'">
    <circle class="rr-track" cx="18" cy="18" r="15" />
    <circle
      v-if="intervalS > 0"
      class="rr-fill"
      :key="refreshKey"
      cx="18" cy="18" r="15"
      :style="{ animationDuration: intervalS + 's', stroke: strokeColor }"
    />
  </svg>
</template>

<script setup>
defineProps({
  intervalS: { type: Number, required: true },
  refreshKey: { type: Number, required: true },
  strokeColor: { type: String, default: 'rgba(6, 182, 212, 0.55)' },
  overlay: { type: Boolean, default: false },
});
</script>

<style scoped>
.refresh-ring {
  transform: rotate(-90deg);
  overflow: visible;
  pointer-events: none;
  flex-shrink: 0;
}
/* Inline mode: fixed size, sits in flex row */
.refresh-ring--inline {
  width: 16px;
  height: 16px;
}
/* Overlay mode: fills parent button with padding outset */
.refresh-ring--overlay {
  position: absolute;
  top: -4px;
  left: -4px;
  width: calc(100% + 8px);
  height: calc(100% + 8px);
}
/* Paused (interval 0 / source off): stay in the flex flow at the same size so layout never
   jumps — just dim the static track instead of unmounting the element. */
.refresh-ring--off {
  opacity: 0.35;
}
.rr-track {
  fill: none;
  stroke: rgba(255, 255, 255, 0.06);
  stroke-width: 2;
}
.rr-fill {
  fill: none;
  stroke-width: 2;
  stroke-linecap: round;
  stroke-dasharray: 94.25;
  stroke-dashoffset: 94.25;
  animation: rrFill linear forwards;
}
@keyframes rrFill {
  from { stroke-dashoffset: 94.25; }
  to   { stroke-dashoffset: 0; }
}
</style>
