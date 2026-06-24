<template>
  <div class="usage-circle-wrapper">
    <!-- Premium Tooltip Container -->
    <div class="usage-circle-container" @mouseenter="updateTime">
      <div class="circle-svg-wrapper">
        <svg class="radial-progress" viewBox="0 0 36 36">
          <!-- Background Track -->
          <circle 
            class="circle-bg" 
            cx="18" 
            cy="18" 
            r="15" 
            stroke-width="3"
          />
          <!-- Active Progress Circle -->
          <circle 
            v-if="hasPercentage && percentage > 0"
            class="circle-fill" 
            :class="colorClass"
            cx="18" 
            cy="18" 
            r="15" 
            stroke-width="3"
            :stroke-dasharray="94.25"
            :stroke-dashoffset="strokeDashOffset"
          />
        </svg>
        <div class="percentage-inner" :class="[colorClass, { 'is-na': !hasPercentage }]">
          {{ displayPercentage }}
        </div>
      </div>
      <div class="circle-sub-label">{{ subLabel }}</div>
      <div class="circle-reset-time" :class="{ 'is-na': !resetsAt }">{{ directResetTime }}</div>

      <!-- CSS Premium Tooltip Popup -->
      <div class="premium-tooltip">
        <div class="tooltip-header">{{ label }}</div>
        <div class="tooltip-content">
          <div class="tooltip-row">
            <span class="tooltip-label">Usage:</span>
            <span class="tooltip-val" :class="colorClass">{{ displayPercentageText }}</span>
          </div>
          <div class="tooltip-row" v-if="resetsAt && !formattedResetTime.isPast">
            <span class="tooltip-label">Resets:</span>
            <span class="tooltip-val highlight">in {{ formattedResetTime.relativeTime }}</span>
          </div>
          <div class="tooltip-row time-abs" v-if="resetsAt">
            <span class="tooltip-label">Date:</span>
            <span class="tooltip-val-dim">({{ formattedResetTime.absoluteTime }})</span>
          </div>
          <div class="tooltip-row" v-if="!resetsAt && hasPercentage">
            <span class="tooltip-label">Reset:</span>
            <span class="tooltip-val-dim">Dynamic Window</span>
          </div>
          <div class="tooltip-row" v-if="!hasPercentage">
            <span class="tooltip-label">Status:</span>
            <span class="tooltip-val-dim">Not Available (N/A)</span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { computed, ref, onMounted, onUnmounted } from 'vue';

const props = defineProps({
  label: String,         // e.g. "Gemini 5-Hour Limit"
  subLabel: String,      // e.g. "5H", "WK", "7D"
  percentage: {
    type: Number,
    default: null
  },
  resetsAt: {
    type: Number,
    default: null
  }
});

const emit = defineEmits(['timeout']);

const currentTime = ref(Math.floor(Date.now() / 1000));
let timer = null;

const updateTime = () => {
  currentTime.value = Math.floor(Date.now() / 1000);
};

onMounted(() => {
  let wasPast = props.resetsAt > 0 && Math.floor(Date.now() / 1000) > props.resetsAt;
  timer = setInterval(() => {
    currentTime.value = Math.floor(Date.now() / 1000);
    const nowPast = props.resetsAt > 0 && currentTime.value > props.resetsAt;
    if (nowPast && !wasPast) {
      emit('timeout');
    }
    wasPast = nowPast;
  }, 10000);
});

onUnmounted(() => {
  if (timer) clearInterval(timer);
});

const hasPercentage = computed(() => props.percentage !== null && !isNaN(props.percentage));

const displayPercentage = computed(() => {
  if (hasPercentage.value) {
    return `${Math.round(props.percentage)}`;
  }
  return 'N/A';
});

const displayPercentageText = computed(() => {
  if (hasPercentage.value) {
    return `${Math.round(props.percentage)}% Used`;
  }
  return 'N/A';
});

const strokeDashOffset = computed(() => {
  if (!hasPercentage.value) return 94.25;
  const pct = Math.min(Math.max(props.percentage, 0), 100);
  return 94.25 - (94.25 * pct) / 100;
});

const colorClass = computed(() => {
  if (!hasPercentage.value) return 'color-na';
  if (props.percentage <= 70) return 'color-safe';
  if (props.percentage <= 90) return 'color-warning';
  return 'color-danger';
});

const formattedResetTime = computed(() => {
  if (!props.resetsAt) return { relativeTime: '', absoluteTime: '', isPast: false };
  
  const d = new Date(props.resetsAt * 1000);
  const absoluteTime = new Intl.DateTimeFormat('en-GB', {
    hour: '2-digit',
    minute: '2-digit',
    month: 'short',
    day: 'numeric',
    hour12: false
  }).format(d);

  const diffSeconds = props.resetsAt - currentTime.value;
  const isPast = diffSeconds <= 0;
  const absDiff = Math.abs(diffSeconds);
  
  let relativeTime = '';
  const days = Math.floor(absDiff / 86400);
  const hours = Math.floor((absDiff % 86400) / 3600);
  const minutes = Math.floor((absDiff % 3600) / 60);
  
  if (days > 0) {
    relativeTime = `${days}d ${hours}h`;
  } else if (hours > 0) {
    relativeTime = `${hours}h ${minutes}m`;
  } else {
    relativeTime = minutes > 0 ? `${minutes}m` : `<1m`;
  }

  if (isPast) {
    relativeTime = `${relativeTime} ago`;
  }

  return { relativeTime, absoluteTime, isPast };
});

const directResetTime = computed(() => {
  if (!props.resetsAt) return 'N/A';
  const diff = props.resetsAt - currentTime.value;
  if (diff <= 0) return 'ready';
  
  const days = Math.floor(diff / 86400);
  const hours = Math.floor((diff % 86400) / 3600);
  const minutes = Math.floor((diff % 3600) / 60);
  
  if (days > 0) {
    return `${days}d${hours}h`;
  }
  if (hours > 0) {
    return `${hours}h${minutes}m`;
  }
  return minutes > 0 ? `${minutes}m` : `<1m`;
});
</script>

<script>
// Prevent multiple component declaration issues if nested
export default {
  name: 'UsageCircle'
}
</script>

<style scoped>
.usage-circle-wrapper {
  display: inline-flex;
}

.usage-circle-container {
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
  cursor: pointer;
}

.circle-svg-wrapper {
  position: relative;
  width: 40px;
  height: 40px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.radial-progress {
  width: 100%;
  height: 100%;
  transform: rotate(-90deg); /* Start progress from the top (12 o'clock) */
}

.circle-bg {
  fill: none;
  stroke: rgba(255, 255, 255, 0.08);
}

.circle-fill {
  fill: none;
  stroke-linecap: round;
  transition: stroke-dashoffset 0.5s ease-in-out;
}

.percentage-inner {
  position: absolute;
  font-family: inherit;
  font-size: 10px;
  font-weight: 700;
  letter-spacing: -0.5px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.percentage-inner.is-na {
  font-size: 9px;
  font-weight: 600;
}

.circle-sub-label {
  font-size: 9px;
  font-weight: 700;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.3px;
  line-height: 1;
}

.circle-reset-time {
  font-size: 8px;
  font-weight: 600;
  color: var(--text-muted);
  margin-top: 1px;
  text-align: center;
  line-height: 1;
}

.circle-reset-time.is-na {
  color: var(--text-darker);
}

/* Colors matching your palette */
.color-safe {
  stroke: var(--accent-green);
  color: var(--accent-green);
}
.color-warning {
  stroke: var(--accent-amber);
  color: var(--accent-amber);
}
.color-danger {
  stroke: var(--accent-red);
  color: var(--accent-red);
}
.color-na {
  stroke: rgba(255, 255, 255, 0.15);
  color: var(--text-darker);
}

/* Tooltip implementation */
.premium-tooltip {
  visibility: hidden;
  opacity: 0;
  position: absolute;
  bottom: 125%; /* Position tooltip above the circle */
  left: 50%;
  transform: translateX(-50%) translateY(4px);
  background: rgba(18, 18, 22, 0.95);
  backdrop-filter: blur(8px);
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 6px;
  padding: 8px 10px;
  width: 170px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.5);
  z-index: 50;
  pointer-events: none;
  transition: opacity 0.15s ease, transform 0.15s ease, visibility 0.15s ease;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

/* Tooltip triangle indicator */
.premium-tooltip::after {
  content: "";
  position: absolute;
  top: 100%;
  left: 50%;
  transform: translateX(-50%);
  border-width: 5px;
  border-style: solid;
  border-color: rgba(18, 18, 22, 0.95) transparent transparent transparent;
}

.usage-circle-container:hover .premium-tooltip {
  visibility: visible;
  opacity: 1;
  transform: translateX(-50%) translateY(0);
}

.tooltip-header {
  font-size: 10px;
  font-weight: 800;
  color: var(--text-light);
  text-transform: uppercase;
  letter-spacing: 0.5px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  padding-bottom: 4px;
}

.tooltip-content {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.tooltip-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: 10px;
}

.tooltip-label {
  color: var(--text-muted);
}

.tooltip-val {
  font-weight: 700;
}

.tooltip-val.highlight {
  color: #a5f3fc; /* Bright cyan accent for emphasis */
  font-weight: 600;
}

.tooltip-val-dim {
  color: var(--text-darker);
  font-size: 9px;
  font-weight: 500;
}

.tooltip-row.time-abs {
  margin-top: 1px;
}
</style>
