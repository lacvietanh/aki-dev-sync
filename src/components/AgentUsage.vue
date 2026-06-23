<template>
  <div class="agent-usage-card">
    <div class="agent-header">
      <div class="agent-title-group">
        <div class="agent-icon-wrapper">
          <img v-if="agentId === 'claudecode'" src="/claude-icon.png" class="agent-img-icon" alt="Claude Code" />
          <img v-else-if="agentId === 'antigravity'" src="/antigravity-icon.png" class="agent-img-icon" alt="Antigravity" @click="handleIconClick" style="cursor: pointer;" title="Open Antigravity App" />
          <div v-else class="agent-icon" :class="agentId">
            <i :class="iconClass"></i>
          </div>
        </div>
        <div class="agent-info">
          <div class="agent-name">
            {{ agentName }}
            <span v-if="agentId === 'antigravity' && data && data.email" class="agent-account" style="margin-left: 6px;">
              - {{ data.email.split('@')[0] }}
            </span>
          </div>
          <div class="agent-location">
            <template v-if="agentId !== 'antigravity'">
              <i :class="locationIcon"></i> {{ locationName }}
            </template>
            <span v-if="agentId === 'claudecode' && data && claudeTierDisplay" class="agent-plan-badge claude">
              Claude {{ claudeTierDisplay }}
            </span>
            <span v-if="agentId === 'claudecode' && data && data.cost" class="agent-plan-badge claude" title="Cost of last session">
              Session: ${{ data.cost.total_cost_usd.toFixed(2) }}
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
      <div v-else-if="loading && !data" class="usage-bars-container">
        <div v-for="i in 2" :key="i" style="display: flex; flex-direction: column; gap: 8px; padding-bottom: 2px;">
          <div style="display: flex; justify-content: space-between; align-items: center;">
            <div class="skeleton-box" :style="{ width: i === 1 ? '40%' : '55%', height: '10px' }"></div>
            <div class="skeleton-box" style="width: 15%; height: 10px;"></div>
          </div>
          <div class="skeleton-box" style="width: 100%; height: 4px; border-radius: 2px;"></div>
        </div>
      </div>
      <div v-else-if="!data && !loading" class="usage-empty">
        <i class="fa-solid" :class="agentId === 'antigravity' ? 'fa-circle-info mb-1' : 'fa-hourglass-empty mb-1'"></i><br>
        {{ agentId === 'antigravity' ? 'IDE not running (Open Antigravity to monitor)' : 'No data — waiting for next session' }}
      </div>
      
      <div v-else-if="data" class="usage-bars-container">
        <!-- Render Claude Code specific bars -->
        <template v-if="agentId === 'claudecode' && data.rate_limits">
          <UsageProgressBar 
            v-if="data.rate_limits.five_hour"
            label="5-Hour Quota" 
            :percentage="data.rate_limits.five_hour.used_percentage" 
            :resetsAt="data.rate_limits.five_hour.resets_at" 
            @timeout="$emit('retry')"
            @force-sync="$emit('force-sync')"
          />
          <UsageProgressBar 
            v-if="data.rate_limits.seven_day"
            label="7-Day Quota" 
            :percentage="data.rate_limits.seven_day.used_percentage" 
            :resetsAt="data.rate_limits.seven_day.resets_at" 
            @timeout="$emit('retry')"
            @force-sync="$emit('force-sync')"
          />
        </template>
        
        <!-- Render Antigravity specific bars -->
        <template v-else-if="agentId === 'antigravity' && data.models">
          <UsageProgressBar 
            v-if="geminiPool"
            label="Gemini Quota Pool" 
            :percentage="geminiPool.remainingPercentage !== undefined ? Math.round((1 - geminiPool.remainingPercentage) * 100) : null" 
            :resetsAt="Math.floor(new Date(geminiPool.resetTime).getTime() / 1000)" 
            @timeout="$emit('retry')"
            @force-sync="$emit('force-sync')"
          />
          <UsageProgressBar 
            v-if="claudeOssPool"
            label="Claude / OSS Pool" 
            :percentage="claudeOssPool.remainingPercentage !== undefined ? Math.round((1 - claudeOssPool.remainingPercentage) * 100) : null" 
            :resetsAt="Math.floor(new Date(claudeOssPool.resetTime).getTime() / 1000)" 
            @timeout="$emit('retry')"
            @force-sync="$emit('force-sync')"
          />
        </template>
      </div>
    </div>
  </div>
</template>

<script setup>
import { computed } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import UsageProgressBar from './UsageProgressBar.vue';

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

const geminiPool = computed(() => {
  if (props.agentId !== 'antigravity' || !props.data || !props.data.models) return null;
  return props.data.models.find(m => m.label.toLowerCase().includes('gemini'));
});

const claudeOssPool = computed(() => {
  if (props.agentId !== 'antigravity' || !props.data || !props.data.models) return null;
  return props.data.models.find(m => !m.label.toLowerCase().includes('gemini')) || null;
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
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  padding: 8px 12px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 200px;
  flex: 1;
}

.agent-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
}

.agent-title-group {
  display: flex;
  align-items: center;
  gap: 10px;
}

.agent-icon-wrapper {
  display: flex;
  align-items: center;
  justify-content: center;
}

.agent-img-icon {
  width: 32px;
  height: 32px;
  border-radius: 8px;
  object-fit: contain;
  filter: drop-shadow(0 2px 4px rgba(0,0,0,0.4));
}

.agent-icon {
  width: 32px;
  height: 32px;
  border-radius: 6px;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 16px;
  background: var(--bg-tertiary);
  color: var(--text-muted);
}

.agent-icon.claudecode {
  background: rgba(217, 119, 87, 0.15);
  color: #D97757; /* Claude color approximation */
}

.agent-icon.antigravity {
  background: rgba(56, 189, 248, 0.15);
  color: #38bdf8; /* Antigravity color */
}

.agent-info {
  display: flex;
  flex-direction: column;
}

.agent-name {
  font-size: 13px;
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

.agent-location {
  font-size: 11px;
  color: var(--text-darker);
  display: flex;
  align-items: center;
  gap: 4px;
}

.agent-plan-badge {
  background: rgba(6, 182, 212, 0.1);
  color: #a5f3fc;
  padding: 1px 5px;
  border-radius: 4px;
  font-size: 9px;
  margin-left: 4px;
  font-weight: 600;
  letter-spacing: 0.5px;
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

.btn-retry {
  background: transparent;
  border: none;
  color: var(--text-muted);
  cursor: pointer;
  padding: 4px;
  border-radius: 4px;
  transition: all 0.2s ease;
}
.btn-retry:hover {
  background: var(--bg-tertiary);
  color: var(--text-light);
}

.loading-spinner {
  color: var(--text-muted);
  font-size: 12px;
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
  gap: 12px;
}

.usage-error {
  font-size: 11px;
  color: var(--color-danger);
  background: rgba(239, 68, 68, 0.1);
  padding: 8px;
  border-radius: 4px;
}


</style>
