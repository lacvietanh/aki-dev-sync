<template>
  <div class="agent-usage-section">
    <div class="usage-split-layout">
      <!-- Local Engine -->
      <div class="usage-column">
        <div class="column-header">
          <h3 class="column-title"><i class="fa-solid fa-laptop-code text-cyan mr-1"></i> LOCAL</h3>
        </div>
        <AgentUsage
          agentId="antigravity"
          agentName="Antigravity"
          locationType="local"
          :data="antigravityData"
          :loading="antigravityLoading"
          :error="antigravityError"
          :stale="antigravityStale"
          @retry="antigravityRefresh"
          @force-sync="antigravityForceSync"
        />
      </div>

      <div class="column-divider"></div>

      <!-- Remote -->
      <div class="usage-column">
        <div class="column-header">
          <h3 class="column-title"><i class="fa-solid fa-cloud text-amber mr-1"></i> REMOTE</h3>
          <div class="host-selector">
            <span class="selector-label">Host:</span>
            <select v-model="selectedHost" class="host-select">
              <option value="" disabled>Select Host</option>
              <option v-for="host in sshHosts" :key="host" :value="host">{{ host }}</option>
            </select>
          </div>
        </div>
        <AgentUsage
          agentId="claudecode"
          agentName="Claude Code"
          locationType="remote"
          :hostName="selectedHost"
          :data="claudeData"
          :loading="claudeLoading"
          :error="claudeError"
          :stale="claudeStale"
          @retry="claudeForceSync"
          @force-sync="claudeForceSync"
        />
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted, watch } from 'vue';
import AgentUsage from './AgentUsage.vue';
import { useSsh } from '../composables/useSsh';
import { useAgentUsage } from '../composables/useAgentUsage';
import { useProjects } from '../composables/useProjects';

const { sshHosts } = useSsh();
const { projects } = useProjects();

const selectedHost = ref('');

// Sync selectedHost with the first available sshHost if not set, or prefer the active project's host
watch(sshHosts, (newHosts) => {
  if (newHosts.length > 0 && !selectedHost.value) {
    selectedHost.value = newHosts[0];
  }
}, { immediate: true });

// We can also watch projects to default to the first project's remote host if available
watch(projects, (newProjects) => {
  if (newProjects.length > 0 && sshHosts.value.includes(newProjects[0].remote_host) && !selectedHost.value) {
    selectedHost.value = newProjects[0].remote_host;
  }
}, { immediate: true });

// Setup Claude Code (remote) monitoring
const { 
  data: claudeData, 
  loading: claudeLoading, 
  error: claudeError, 
  stale: claudeStale, 
  refresh: claudeRefresh,
  forceSync: claudeForceSync
} = useAgentUsage('claudecode', selectedHost);

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
  padding: 10px 16px;
  display: flex;
  flex-direction: column;
  gap: 8px;
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
  gap: 12px;
}

.column-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding-bottom: 6px;
  border-bottom: 1px dashed rgba(255, 255, 255, 0.1);
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

.host-selector {
  display: flex;
  align-items: center;
  gap: 8px;
}

.selector-label {
  font-size: 10px;
  color: var(--text-muted);
  text-transform: uppercase;
  font-weight: 700;
}

.host-select {
  background-color: var(--bg-tertiary);
  color: var(--text-light);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 4px;
  padding: 2px 6px;
  height: 24px;
  font-size: 11px;
  font-family: inherit;
  outline: none;
  cursor: pointer;
  transition: all 0.2s ease;
  box-shadow: 0 1px 2px rgba(0,0,0,0.1);
}
.host-select:hover {
  background-color: rgba(255, 255, 255, 0.08);
  border-color: rgba(255, 255, 255, 0.2);
}

.host-select:focus {
  border-color: var(--accent-cyan);
  box-shadow: 0 0 0 2px rgba(0, 210, 255, 0.2);
}
</style>
