<template>
  <div class="agent-usage-section">
    <div class="usage-split-layout">
      <!-- Local Engine -->
      <div class="usage-column">
        <div class="column-header">
          <h3 class="column-title"><i class="fa-solid fa-laptop-code text-cyan mr-1"></i> LOCAL</h3>
          <button class="btn-eye-toggle" @click="toggleLocalEmail" :aria-label="showLocalEmail ? 'Hide email' : 'Show email'" :title="showLocalEmail ? 'Hide email' : 'Show email'">
            <i class="fa-regular" :class="showLocalEmail ? 'fa-eye' : 'fa-eye-slash'"></i>
          </button>
        </div>
        <AgentUsage
          agentId="antigravity"
          agentName="Antigravity"
          locationType="local"
          :data="antigravityData"
          :loading="antigravityLoading"
          :error="antigravityError"
          :stale="antigravityStale"
          :showEmail="showLocalEmail"
          @retry="antigravityRefresh"
          @force-sync="antigravityForceSync"
        />
      </div>

      <div class="column-divider"></div>

      <!-- Remote -->
      <div class="usage-column">
        <div class="column-header">
          <h3 class="column-title"><i class="fa-solid fa-cloud text-amber mr-1"></i> REMOTE</h3>
          <button class="btn-eye-toggle" @click="toggleRemoteEmail" :aria-label="showRemoteEmail ? 'Hide email' : 'Show email'" :title="showRemoteEmail ? 'Hide email' : 'Show email'">
            <i class="fa-regular" :class="showRemoteEmail ? 'fa-eye' : 'fa-eye-slash'"></i>
          </button>
        </div>
        <AgentUsage
          agentId="claudecode"
          agentName="Claude Code"
          locationType="remote"
          :hostName="selectedSshHost"
          :data="claudeData"
          :loading="claudeLoading"
          :error="claudeError"
          :stale="claudeStale"
          :showEmail="showRemoteEmail"
          @retry="claudeForceSync"
          @force-sync="claudeForceSync"
        />
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref } from 'vue';
import AgentUsage from './AgentUsage.vue';
import { useSsh } from '../composables/useSsh';
import { useAgentUsage } from '../composables/useAgentUsage';

const { selectedSshHost } = useSsh();

const showLocalEmail = ref(localStorage.getItem('aki-show-local-email') !== 'false');
const showRemoteEmail = ref(localStorage.getItem('aki-show-remote-email') !== 'false');

function toggleLocalEmail() {
  showLocalEmail.value = !showLocalEmail.value;
  localStorage.setItem('aki-show-local-email', String(showLocalEmail.value));
}
function toggleRemoteEmail() {
  showRemoteEmail.value = !showRemoteEmail.value;
  localStorage.setItem('aki-show-remote-email', String(showRemoteEmail.value));
}
// Setup Claude Code (remote) monitoring
const {
  data: claudeData,
  loading: claudeLoading,
  error: claudeError,
  stale: claudeStale,
  forceSync: claudeForceSync
} = useAgentUsage('claudecode', selectedSshHost);

// Setup Antigravity (local) monitoring - host is 'local'
const localHostRef = ref('local');
const { 
  data: antigravityData, 
  loading: antigravityLoading, 
  error: antigravityError, 
  stale: antigravityStale, 
  refresh: antigravityRefresh,
  forceSync: antigravityForceSync
} = useAgentUsage('antigravity', localHostRef);

</script>

<style scoped>
.agent-usage-section {
  background: rgba(22, 22, 26, 0.6);
  border-bottom: 1px solid var(--border-color);
  padding: 6px 12px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.usage-split-layout {
  display: flex;
  gap: 12px;
  align-items: stretch;
}

.usage-column {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.column-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding-bottom: 4px;
  border-bottom: 1px dashed rgba(255, 255, 255, 0.1);
}

.btn-eye-toggle {
  background: transparent;
  border: none;
  color: var(--text-darker);
  cursor: pointer;
  padding: 2px 4px;
  font-size: 10px;
  line-height: 1;
  opacity: 0.5;
  transition: opacity 0.15s ease, color 0.15s ease;
}
.btn-eye-toggle:hover {
  opacity: 1;
  color: var(--text-muted);
}

.column-title {
  margin: 0;
  font-size: 11px;
  font-weight: 800;
  color: #9CA3AF;
  letter-spacing: 0.5px;
}

.column-divider {
  width: 1px;
  background: rgba(255, 255, 255, 0.05);
  margin: 0 4px;
}

</style>
