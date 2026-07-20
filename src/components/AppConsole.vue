<template>
  <div class="dashboard-bottom" :class="{ 'is-collapsed': !isLogExpanded }">
    <div class="terminal-panel">
      <div class="terminal-header">
        <div class="terminal-title">
          <span v-if="activeLogProjectId === null"><i class="fa-solid fa-book-journal-whills text-cyan mr-1"></i> GLOBAL EVENT LOG</span>
          <span v-else>
            <span class="badge-project">{{ projects.find(p => p.id === activeLogProjectId)?.name }}</span>
            <i class="fa-solid fa-terminal text-amber ml-1 mr-1"></i> RAW CONSOLE
          </span>
          <button v-if="activeLogProjectId !== null" class="btn-tech btn-tech-secondary btn-icon-only ml-2 text-red" @click="activeLogProjectId = null" title="Close Project Log & Return to Global Log">
            <i class="fa-solid fa-circle-xmark"></i>
          </button>
          <span class="status-indicator ml-2" v-if="anySyncing">
            <span class="status-dot"></span> SYNCING...
          </span>
        </div>
        <div class="terminal-actions">
          <button class="btn-tech btn-tech-secondary btn-terminal-action" @click="isLogExpanded = !isLogExpanded" title="Collapse / Expand log panel">
            <i class="fa-solid" :class="isLogExpanded ? 'fa-chevron-down' : 'fa-chevron-up'"></i>
            <span class="u-narrow-hide">{{ isLogExpanded ? 'COLLAPSE' : 'EXPAND' }}</span>
          </button>
          <button class="btn-tech btn-tech-secondary btn-terminal-action" @click="copyLogs" :disabled="displayedLogs.length === 0" title="Copy logs">
            <i class="fa-solid" :class="copied ? 'fa-check log-copied-icon' : 'fa-copy'"></i>
            <span class="u-narrow-hide">{{ copied ? 'COPIED' : 'COPY' }}</span>
          </button>
          <button class="btn-tech btn-tech-secondary btn-terminal-action" @click="clearLog" :disabled="displayedLogs.length === 0" title="Clear log">
            <i class="fa-solid fa-trash"></i>
            <span class="u-narrow-hide">CLEAR</span>
          </button>
        </div>
      </div>
      <div class="console-output" ref="consoleRef">
        <div v-if="displayedLogs.length === 0" class="empty-logs">
          <i class="fa-solid fa-ghost mb-2"></i><br>
          {{ activeLogProjectId ? "No raw logs yet. Trigger a sync action." : "No global events recorded yet." }}
        </div>
        <div v-for="(line, index) in displayedLogs" :key="index" class="log-line" :class="getLogClass(line)">{{ line }}</div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { onMounted, onUnmounted } from 'vue';
import { useLogs } from '../composables/useLogs';
import { useProjects } from '../composables/useProjects';

const { activeLogProjectId, isLogExpanded, displayedLogs, consoleRef, copied, copyLogs, clearLog } = useLogs();
const { projects, anySyncing } = useProjects();

function handleEsc(e) {
  if (e.key === 'Escape' && isLogExpanded.value && !document.querySelector('.modal-overlay')) {
    activeLogProjectId.value = null;
    isLogExpanded.value = false;
  }
}

onMounted(() => window.addEventListener('keydown', handleEsc, true));
onUnmounted(() => window.removeEventListener('keydown', handleEsc, true));

function getLogClass(line) {
  if (line.includes("[ERROR]") || line.includes("FAILED")) return "log-error";
  if (line.includes("SYNC COMPLETED")) return "log-success";
  if (line.startsWith("[REMOTE]") || line.startsWith("[MOCK]")) return "log-remote";
  if (line.startsWith("[")) return "log-global-date";
  return "log-normal";
}
</script>

<style scoped>
.terminal-actions {
  display: flex;
  gap: 6px;
}

.btn-terminal-action {
  padding: 4px 8px;
  font-size: 9px;
}

.log-copied-icon {
  color: var(--accent-green);
}
</style>
