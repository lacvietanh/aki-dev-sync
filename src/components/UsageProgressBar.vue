<template>
  <div class="usage-progress">
    <div class="usage-header">
      <span class="usage-label">{{ label }}</span>
      <span class="usage-value" :class="colorClass">{{ displayPercentage }}</span>
    </div>
    <div class="progress-track" v-if="hasPercentage">
      <div class="progress-fill" :class="colorClass" :style="{ width: Math.min(percentage, 100) + '%' }"></div>
    </div>
    <div class="usage-footer" v-if="resetsAt">
      <span class="reset-time" title="Reset time is based on your local timezone">
        <span class="reset-label">{{ formattedResetTime.isPast ? 'Reset' : 'Resets' }}</span>
        <span v-if="!formattedResetTime.isPast" class="reset-label">in</span>
        <span class="reset-relative">{{ formattedResetTime.relativeTime }}</span>
        <span class="reset-absolute">({{ formattedResetTime.absoluteTime }})</span>
        <button v-if="formattedResetTime.isPast" @click="!loading && $emit('force-sync')" class="btn-ui-action" style="margin-left: 6px;" :disabled="loading" :title="loading ? 'Loading data' : 'Force Sync Quota'">
          <i class="fa-solid" :class="loading ? 'fa-circle-notch fa-spin' : 'fa-rotate-right'"></i>
        </button>
      </span>
    </div>
  </div>
</template>

<script setup>
// @docs docs/research/claude-usage-1.2.7-analyze.md
import { computed, ref, onMounted, onUnmounted, watch } from 'vue';

const props = defineProps({
  label: String,
  percentage: {
    type: Number,
    default: null
  },
  resetsAt: {
    type: Number,
    default: null
  },
  loading: {
    type: Boolean,
    default: false
  }
});

const emit = defineEmits(['timeout', 'force-sync']);

const currentTime = ref(Math.floor(Date.now() / 1000));
let timer = null;

onMounted(() => {
  // Update time every 10 seconds; emit 'timeout' once when resetsAt is crossed
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
    return `${Math.round(props.percentage)}%`;
  }
  return 'N/A';
});

const colorClass = computed(() => {
  if (!hasPercentage.value) return 'color-safe';
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
</script>

<style scoped>
.usage-progress {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.usage-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: 11px;
  font-weight: 600;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.5px;
}
.usage-value {
  font-weight: 700;
}
.progress-track {
  height: 6px;
  background-color: rgba(255, 255, 255, 0.05);
  border-radius: 3px;
  overflow: hidden;
}
.progress-fill {
  height: 100%;
  border-radius: 3px;
  transition: width 0.5s ease-in-out;
}
.usage-footer {
  font-size: 10px;
  text-align: right;
}
.reset-time {
  cursor: help;
  display: inline-flex;
  gap: 4px;
  align-items: center;
}
.reset-label {
  color: rgba(255, 255, 255, 0.35);
}
.reset-relative {
  color: #ffffff;
  font-weight: 600;
}
.reset-absolute {
  color: rgba(255, 255, 255, 0.25);
}

/* Colors using existing CSS variables */
.color-safe { color: var(--accent-green); }
.color-safe.progress-fill { background-color: var(--accent-green); }

.color-warning { color: var(--accent-amber); }
.color-warning.progress-fill { background-color: var(--accent-amber); }

.color-danger { color: var(--accent-red); }
.color-danger.progress-fill { background-color: var(--accent-red); }

.btn-ui-action {
  display: flex;
  align-items: center;
  justify-content: center;
  background-color: var(--bg-tertiary);
  color: var(--text-muted);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 4px;
  padding: 4px;
  min-width: 24px;
  min-height: 24px;
  font-size: 11px;
  cursor: pointer;
  transition: all 0.2s ease;
  box-shadow: 0 1px 2px rgba(0,0,0,0.1);
}

.btn-ui-action:hover {
  background-color: rgba(255, 255, 255, 0.08);
  border-color: rgba(255, 255, 255, 0.2);
  color: var(--text-light);
  transform: translateY(-1px);
  box-shadow: 0 2px 4px rgba(0,0,0,0.2);
}

.btn-ui-action:active {
  transform: translateY(0);
  box-shadow: 0 1px 2px rgba(0,0,0,0.1);
}
</style>
