<template>
  <svg v-if="intervalS > 0"
    :class="['refresh-ring', overlay ? 'refresh-ring--overlay' : 'refresh-ring--inline']"
    viewBox="0 0 36 36" aria-hidden="true"
    :title="`Auto-refresh every ${intervalS}s`">
    <circle class="rr-track" cx="18" cy="18" r="15" />
    <circle
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
