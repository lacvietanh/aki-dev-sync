<template>
  <div class="agent-usage-card">
    <!-- Claude Code Custom Header -->
    <div v-if="agentId === 'claudecode'" class="agent-header claudecode-custom-header">
      <div class="agent-title-group">
        <div class="agent-icon-wrapper">
          <img src="/claude-icon.png" class="agent-img-icon" alt="Claude Code" />
        </div>
        <div class="agent-name-row">
          <span class="agent-name">{{ agentName }}</span>
          <span v-if="data && claudeTierDisplay" class="agent-plan-badge claude">
            Claude {{ claudeTierDisplay }}
          </span>
          <span v-if="data && data.cost" class="agent-plan-badge claude" title="Cost of last session">
            Session: ${{ data.cost.total_cost_usd.toFixed(2) }}
          </span>
        </div>
      </div>
      <div class="agent-status-badges">
        <span v-if="stale" class="badge-stale" title="Data is older than 10 minutes">Stale</span>
        <button class="btn-ui-action" :class="{ 'error-state': error, 'is-loading': loading }" @click="!loading && $emit('retry')" :disabled="loading" :title="loading ? 'Loading data' : 'Refresh Data'">
          <i class="fa-solid" :class="loading ? 'fa-circle-notch fa-spin' : 'fa-rotate-right'"></i>
        </button>
      </div>
    </div>

    <!-- Antigravity Header (Keep tiny logo + email) -->
    <div v-else class="agent-header">
      <div class="agent-title-group">
        <div class="agent-icon-wrapper">
          <img src="/antigravity-icon.png" class="agent-img-icon" alt="Antigravity" @click="handleIconClick" style="cursor: pointer;" title="Open Antigravity App" />
        </div>
        <div class="agent-info">
          <div class="agent-name">
            {{ agentName }}
            <span v-if="data && data.email" class="agent-account">
              - {{ data.email.split('@')[0] }}
            </span>
          </div>
        </div>
      </div>
      
      <div class="agent-status-badges">
        <span v-if="stale" class="badge-stale" title="Data is older than 10 minutes">Stale</span>
        <button class="btn-ui-action" :class="{ 'error-state': error, 'is-loading': loading }" @click="!loading && $emit('retry')" :disabled="loading" :title="loading ? 'Loading data' : 'Refresh Data'">
          <i class="fa-solid" :class="loading ? 'fa-circle-notch fa-spin' : 'fa-rotate-right'"></i>
        </button>
      </div>
    </div>

    <div class="agent-body">
      <div v-if="error" class="usage-error">
        <span><i class="fa-solid fa-triangle-exclamation mr-1"></i> {{ error }}</span>
      </div>
      
      <!-- Skeleton circles with fieldset wrapper for AG -->
      <div v-else-if="loading && !data" class="usage-circles-skeleton">
        <div v-if="agentId === 'claudecode'" class="circles-row">
          <div v-for="i in 2" :key="i" class="skeleton-circle-wrapper">
            <div class="skeleton-circle"></div>
            <div class="skeleton-text" style="width: 20px;"></div>
            <div class="skeleton-text" style="width: 30px; height: 6px;"></div>
          </div>
        </div>
        <div v-else class="circles-row">
          <fieldset class="zone-fieldset skeleton-zone">
            <legend class="zone-legend">Gemini</legend>
            <div class="zone-content">
              <div v-for="i in 2" :key="i" class="skeleton-circle-wrapper">
                <div class="skeleton-circle"></div>
                <div class="skeleton-text" style="width: 15px;"></div>
                <div class="skeleton-text" style="width: 25px; height: 6px;"></div>
              </div>
            </div>
          </fieldset>
          <fieldset class="zone-fieldset skeleton-zone">
            <legend class="zone-legend">Claude/OSS</legend>
            <div class="zone-content">
              <div v-for="i in 2" :key="i" class="skeleton-circle-wrapper">
                <div class="skeleton-circle"></div>
                <div class="skeleton-text" style="width: 15px;"></div>
                <div class="skeleton-text" style="width: 25px; height: 6px;"></div>
              </div>
            </div>
          </fieldset>
        </div>
      </div>
      
      <div v-else-if="!data && !loading" class="usage-empty">
        <i class="fa-solid" :class="agentId === 'antigravity' ? 'fa-circle-info mb-1' : 'fa-hourglass-empty mb-1'"></i><br>
        <span>{{ agentId === 'antigravity' ? 'IDE not running (Open Antigravity to monitor)' : 'No data — waiting for next session' }}</span>
        <button v-if="agentId === 'claudecode'" @click="$emit('force-sync')" class="btn-ui-action btn-sync-now" style="margin-top: 8px; padding: 4px 10px; display: inline-flex; align-items: center; gap: 6px;" title="Force Sync Quota">
          <i class="fa-solid fa-arrows-rotate"></i>
          <span>Force Sync</span>
        </button>
      </div>
      
      <div v-else-if="data" class="usage-bars-container">
        <!-- Render Claude Code specific circular progress (2 circles) -->
        <template v-if="agentId === 'claudecode'">
          <div class="circles-row">
            <UsageCircle 
              label="Claude 5-Hour Limit" 
              subLabel="5H" 
              :percentage="data.rate_limits?.five_hour?.used_percentage ?? null" 
              :resetsAt="data.rate_limits?.five_hour?.resets_at ?? null" 
              @timeout="$emit('force-sync')"
            />
            <UsageCircle 
              label="Claude 7-Day Limit" 
              subLabel="7D" 
              :percentage="data.rate_limits?.seven_day?.used_percentage ?? null" 
              :resetsAt="data.rate_limits?.seven_day?.resets_at ?? null" 
              @timeout="$emit('force-sync')"
            />
          </div>
        </template>
        
        <!-- Render Antigravity specific circular progress (4 circles bo trong 2 fieldset) -->
        <template v-else-if="agentId === 'antigravity'">
          <div class="circles-row">
            <fieldset class="zone-fieldset">
              <legend class="zone-legend">Gemini</legend>
              <div class="zone-content">
                <UsageCircle 
                  label="Gemini 5-Hour Limit" 
                  subLabel="5H" 
                  :percentage="gemini5hData ? gemini5hData.percentage : null" 
                  :resetsAt="gemini5hData ? gemini5hData.resetsAt : null" 
                  @timeout="$emit('retry')"
                />
                <UsageCircle 
                  label="Gemini Weekly Limit" 
                  subLabel="7D" 
                  :percentage="geminiWeeklyBucket?.remainingFraction !== undefined ? Math.round((1 - geminiWeeklyBucket.remainingFraction) * 100) : null" 
                  :resetsAt="geminiWeeklyBucket?.resetTime ? Math.floor(new Date(geminiWeeklyBucket.resetTime).getTime() / 1000) : null" 
                  @timeout="$emit('retry')"
                />
              </div>
            </fieldset>

            <fieldset class="zone-fieldset">
              <legend class="zone-legend">Claude/OSS</legend>
              <div class="zone-content">
                <UsageCircle 
                  label="Claude & GPT 5-Hour Limit" 
                  subLabel="5H" 
                  :percentage="claude5hData ? claude5hData.percentage : null" 
                  :resetsAt="claude5hData ? claude5hData.resetsAt : null" 
                  @timeout="$emit('retry')"
                />
                <UsageCircle 
                  label="Claude & GPT Weekly Limit" 
                  subLabel="7D" 
                  :percentage="claudeWeeklyBucket?.remainingFraction !== undefined ? Math.round((1 - claudeWeeklyBucket.remainingFraction) * 100) : null" 
                  :resetsAt="claudeWeeklyBucket?.resetTime ? Math.floor(new Date(claudeWeeklyBucket.resetTime).getTime() / 1000) : null" 
                  @timeout="$emit('retry')"
                />
              </div>
            </fieldset>
          </div>
        </template>
      </div>
    </div>
  </div>
</template>

<script setup>
// @docs docs/arch/usage-claudecode.md
// @docs docs/arch/usage-antigravity.md
import { computed } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import UsageCircle from './UsageCircle.vue';

const props = defineProps({
  agentId: String,     // 'claudecode' or 'antigravity'
  agentName: String,   // 'Claude Code' or 'Antigravity'
  locationType: String, // 'remote' or 'local'
  hostName: String,     // e.g., 'bien'
  data: Object,
  loading: Boolean,
  error: String,
  stale: Boolean
});

const emit = defineEmits(['retry', 'force-sync']);

// Antigravity 2.1.1+ Groups & Buckets detection
const quotaSummaryGroups = computed(() => {
  if (props.agentId !== 'antigravity' || !props.data || !props.data.quotaSummary) return null;
  return props.data.quotaSummary.groups || null;
});

const geminiGroup = computed(() => {
  if (!quotaSummaryGroups.value) return null;
  return quotaSummaryGroups.value.find(g => g.displayName.toLowerCase().includes('gemini')) || null;
});

const claudeGroup = computed(() => {
  if (!quotaSummaryGroups.value) return null;
  return quotaSummaryGroups.value.find(g => g.displayName.toLowerCase().includes('claude') || g.displayName.toLowerCase().includes('3p')) || null;
});

const gemini5hBucket = computed(() => {
  if (!geminiGroup.value?.buckets) return null;
  return geminiGroup.value.buckets.find(b => b.window === '5h' || b.bucketId.includes('5h')) || null;
});

const geminiWeeklyBucket = computed(() => {
  if (!geminiGroup.value?.buckets) return null;
  return geminiGroup.value.buckets.find(b => b.window === 'weekly' || b.bucketId.includes('weekly')) || null;
});

const claude5hBucket = computed(() => {
  if (!claudeGroup.value?.buckets) return null;
  return claudeGroup.value.buckets.find(b => b.window === '5h' || b.bucketId.includes('5h')) || null;
});

const claudeWeeklyBucket = computed(() => {
  if (!claudeGroup.value?.buckets) return null;
  return claudeGroup.value.buckets.find(b => b.window === 'weekly' || b.bucketId.includes('weekly')) || null;
});

// Backward compatibility fallbacks
const geminiPool = computed(() => {
  if (props.agentId !== 'antigravity' || !props.data || !props.data.models) return null;
  return props.data.models.find(m => m.label.toLowerCase().includes('gemini'));
});

const claudeOssPool = computed(() => {
  if (props.agentId !== 'antigravity' || !props.data || !props.data.models) return null;
  return props.data.models.find(m => !m.label.toLowerCase().includes('gemini')) || null;
});

const gemini5hData = computed(() => {
  const bucket = gemini5hBucket.value;
  if (bucket) {
    return {
      percentage: bucket.remainingFraction !== undefined ? Math.round((1 - bucket.remainingFraction) * 100) : null,
      resetsAt: bucket.resetTime ? Math.floor(new Date(bucket.resetTime).getTime() / 1000) : null
    };
  }
  const oldPool = geminiPool.value;
  if (oldPool) {
    return {
      percentage: oldPool.remainingPercentage !== undefined ? Math.round((1 - oldPool.remainingPercentage) * 100) : null,
      resetsAt: oldPool.resetTime ? Math.floor(new Date(oldPool.resetTime).getTime() / 1000) : null
    };
  }
  return null;
});

const claude5hData = computed(() => {
  const bucket = claude5hBucket.value;
  if (bucket) {
    return {
      percentage: bucket.remainingFraction !== undefined ? Math.round((1 - bucket.remainingFraction) * 100) : null,
      resetsAt: bucket.resetTime ? Math.floor(new Date(bucket.resetTime).getTime() / 1000) : null
    };
  }
  const oldPool = claudeOssPool.value;
  if (oldPool) {
    return {
      percentage: oldPool.remainingPercentage !== undefined ? Math.round((1 - oldPool.remainingPercentage) * 100) : null,
      resetsAt: oldPool.resetTime ? Math.floor(new Date(oldPool.resetTime).getTime() / 1000) : null
    };
  }
  return null;
});

const iconClass = computed(() => {
  if (props.agentId === 'claudecode') return 'fa-solid fa-robot';
  if (props.agentId === 'antigravity') return 'fa-solid fa-satellite';
  return 'fa-solid fa-microchip';
});

const locationIcon = computed(() => {
  return props.locationType === 'remote' ? 'fa-solid fa-cloud' : 'fa-solid fa-laptop-code';
});

const locationName = computed(() => {
  if (props.locationType === 'remote') {
    return props.hostName ? `Remote: ${props.hostName}` : 'Remote';
  }
  return 'Local';
});

const claudeTierDisplay = computed(() => {
  if (!props.data) return '';
  
  if (props.data.rateLimitTier && props.data.rateLimitTier !== 'Unknown') {
    let tier = props.data.rateLimitTier;
    let cleaned = tier.replace(/^(default_)?claude_/, '').replace(/_/g, ' ');
    
    return cleaned.split(' ').map(word => {
      if (word.toLowerCase() === 'max') return 'Max';
      if (word.toLowerCase() === 'pro') return 'Pro';
      if (/^\d+x$/i.test(word)) return word.toLowerCase();
      return word.charAt(0).toUpperCase() + word.slice(1);
    }).join(' ');
  }
  
  if (props.data.subscriptionType && props.data.subscriptionType !== 'Unknown') {
    return props.data.subscriptionType.charAt(0).toUpperCase() + props.data.subscriptionType.slice(1);
  }
  
  return '';
});

async function handleIconClick() {
  if (props.agentId === 'antigravity') {
    try {
      await invoke("macos_open", { args: ["-a", "Antigravity"] });
    } catch (e) {
      console.error("Failed to open Antigravity:", e);
    }
  }
}
</script>

<style scoped>
.agent-usage-card {
  background: transparent;
  border: none;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 200px;
  flex: 1;
  box-shadow: none;
}

.agent-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  width: 100%;
  margin-bottom: 2px;
}

.claudecode-custom-header {
  margin-bottom: 4px;
}

.agent-name-row {
  display: flex;
  align-items: center;
  gap: 6px;
}

.agent-title-group {
  display: flex;
  align-items: center;
  gap: 8px;
}

.agent-icon-wrapper {
  display: flex;
  align-items: center;
  justify-content: center;
}

.agent-img-icon {
  width: 18px; /* Tiny logo for AG */
  height: 18px;
  border-radius: 4px;
  object-fit: contain;
}

.agent-info {
  display: flex;
  align-items: center;
}

.agent-name {
  font-size: 12px;
  font-weight: 700;
  color: var(--text-light);
  display: flex;
  align-items: center;
  gap: 4px;
}

.agent-account {
  font-size: 11px;
  color: var(--text-darker);
  font-weight: 500;
}

.agent-plan-badge {
  background: rgba(6, 182, 212, 0.1);
  color: #a5f3fc;
  padding: 1px 5px;
  border-radius: 4px;
  font-size: 9px;
  font-weight: 600;
  letter-spacing: 0.5px;
  line-height: 1.2;
}

.agent-plan-badge.claude {
  background: rgba(217, 119, 87, 0.1);
  color: #D97757;
}

.agent-status-badges {
  display: flex;
  align-items: center;
  gap: 6px;
}

.badge-stale {
  font-size: 9px;
  font-weight: 600;
  text-transform: uppercase;
  background: var(--bg-tertiary);
  color: var(--text-darker);
  padding: 2px 6px;
  border-radius: 4px;
}

.btn-ui-action {
  background: transparent;
  border: none;
  color: var(--text-muted);
  cursor: pointer;
  padding: 4px;
  border-radius: 4px;
  transition: all 0.2s ease;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}
.btn-ui-action:hover {
  background: var(--bg-tertiary);
  color: var(--text-light);
}

.agent-body {
  flex: 1;
  display: flex;
  flex-direction: column;
  justify-content: center;
}

.usage-bars-container {
  display: flex;
  flex-direction: column;
  justify-content: center;
  align-items: center;
  width: 100%;
}

.circles-row {
  display: flex;
  gap: 4px; /* Tight gap */
  justify-content: space-between;
  align-items: stretch;
  width: 100%;
  padding: 2px 0;
}

/* fieldset for grouping Antigravity */
.zone-fieldset {
  flex: 1;
  border: 1px dashed rgba(255, 255, 255, 0.18); /* Brighter dashed line */
  border-radius: 6px;
  padding: 4px 2px 4px 2px; /* Super compact padding */
  margin: 0;
  min-width: 0;
  box-sizing: border-box;
  transition: border-color 0.2s ease;
}

.zone-fieldset:hover {
  border-color: rgba(255, 255, 255, 0.25);
}

.zone-legend {
  font-size: 8px;
  font-weight: 800;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.5px;
  padding: 0 4px;
  line-height: 1;
  margin-left: 6px;
}

.zone-content {
  display: flex;
  justify-content: space-around;
  align-items: flex-start;
  gap: 2px;
}

/* Skeleton loader for circles */
.usage-circles-skeleton {
  display: flex;
  flex-direction: column;
  gap: 6px;
  width: 100%;
}

.skeleton-zone {
  border-color: rgba(255, 255, 255, 0.04) !important;
}

.skeleton-circle-wrapper {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
}

.skeleton-circle {
  width: 40px;
  height: 40px;
  border-radius: 50%;
  background: rgba(255, 255, 255, 0.05);
  animation: pulse 1.5s infinite ease-in-out;
}

.skeleton-text {
  border-radius: 2px;
  background: rgba(255, 255, 255, 0.05);
  animation: pulse 1.5s infinite ease-in-out;
}

@keyframes pulse {
  0% { opacity: 0.6; }
  50% { opacity: 0.3; }
  100% { opacity: 0.6; }
}

.usage-error {
  font-size: 11px;
  color: var(--accent-red);
  background: rgba(239, 68, 68, 0.1);
  padding: 8px;
  border-radius: 4px;
  width: 100%;
}

.usage-empty {
  text-align: center;
  font-size: 11px;
  color: var(--text-darker);
  padding: 12px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  width: 100%;
}

.btn-sync-now {
  border-radius: 4px;
  color: var(--text-light);
  border: 1px solid rgba(255, 255, 255, 0.15);
}

.btn-sync-now:hover {
  background-color: var(--bg-tertiary);
  border-color: var(--accent-amber) !important;
  color: var(--accent-amber) !important;
}
</style>
