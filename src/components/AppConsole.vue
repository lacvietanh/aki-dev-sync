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
        <div class="terminal-actions" style="display: flex; gap: 6px;">
          <button class="btn-tech btn-tech-secondary" @click="isLogExpanded = !isLogExpanded" style="padding: 4px 8px; font-size: 9px;">
            <i class="fa-solid" :class="isLogExpanded ? 'fa-chevron-down' : 'fa-chevron-up'"></i>
            {{ isLogExpanded ? 'COLLAPSE' : 'EXPAND' }}
          </button>
          <button class="btn-tech btn-tech-secondary" @click="copyLogs" :disabled="displayedLogs.length === 0" style="padding: 4px 8px; font-size: 9px;">
            <i class="fa-solid fa-copy"></i> {{ copied ? 'COPIED' : 'COPY' }}
          </button>
          <button class="btn-tech btn-tech-secondary" @click="clearLog" :disabled="displayedLogs.length === 0" style="padding: 4px 8px; font-size: 9px;">
            <i class="fa-solid fa-trash"></i> CLEAR
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
import { useLogs } from '../composables/useLogs';
import { useProjects } from '../composables/useProjects';

const { activeLogProjectId, isLogExpanded, displayedLogs, consoleRef, copied, copyLogs, clearLog } = useLogs();
const { projects, anySyncing } = useProjects();

function getLogClass(line) {
  if (line.includes("[ERROR]") || line.includes("FAILED")) return "log-error";
  if (line.includes("SYNC COMPLETED")) return "log-success";
  if (line.startsWith("[REMOTE]") || line.startsWith("[MOCK]")) return "log-remote";
  if (line.startsWith("[")) return "log-global-date";
  return "log-normal";
}
</script>
