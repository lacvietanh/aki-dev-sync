<!-- GLOBAL / project event log dock stack. Everything below used to live in AppConsole.vue's old panel-switch branches; moved here verbatim as part of splitting the dock into independent stacks (docs/arch/terminal-stack.md). Behaviour unchanged: same COPY/CLEAR buttons, same project-log badge + SYNCING indicator, same log line classing. -->
<template>
  <DockStack
    stack-key="log"
    :collapsed="collapsed"
    @update:collapsed="collapsed = $event"
  >
    <template #title>
      <span v-if="activeLogProjectId === null"><i class="fa-solid fa-book-journal-whills text-cyan mr-1"></i> GLOBAL EVENT LOG</span>
      <span v-else>
        <span class="badge-project">{{ activeProjectName }}</span>
        <i class="fa-solid fa-list-ul text-amber ml-1 mr-1"></i> PROJECT LOG
      </span>
      <button v-if="activeLogProjectId !== null" class="btn-cell-trigger ml-2 text-red" @click="activeLogProjectId = null" title="Close Project Log & Return to Global Log">
        <i class="fa-solid fa-circle-xmark"></i>
      </button>
      <span class="status-indicator ml-2" v-if="anySyncing">
        <span class="status-dot"></span> SYNCING...
      </span>
    </template>
    <template #actions>
      <button class="btn-tech btn-tech-secondary btn-terminal-action" @click="copyLogs" :disabled="displayedLogs.length === 0" title="Copy logs">
        <i class="fa-solid" :class="copied ? 'fa-check log-copied-icon' : 'fa-copy'"></i>
        <span class="u-narrow-hide">{{ copied ? 'COPIED' : 'COPY' }}</span>
      </button>
      <button class="btn-tech btn-tech-secondary btn-terminal-action" @click="clearLog" :disabled="displayedLogs.length === 0" title="Clear log">
        <i class="fa-solid fa-trash"></i>
        <span class="u-narrow-hide">CLEAR</span>
      </button>
    </template>
    <template #peek>
      <div class="log-peek log-line" :class="getLogClass(latestLogLine)">{{ latestLogLine }}</div>
    </template>
    <!-- u-select-text (main.css): log output is the single most copy-worthy surface in the app, so it opts out of the app-wide no-selection default. -->
    <div class="console-output u-select-text" ref="consoleRef">
      <div v-if="displayedLogs.length === 0" class="empty-logs">
        <i class="fa-solid fa-ghost mb-2"></i><br>
        {{ activeLogProjectId ? "No raw logs yet. Trigger a sync action." : "No global events recorded yet." }}
      </div>
      <div v-for="(line, index) in displayedLogs" :key="index" class="log-line" :class="getLogClass(line)">{{ line }}</div>
    </div>
  </DockStack>
</template>

<script setup>
import { computed } from 'vue';
import DockStack from '../DockStack.vue';
import { useLogs } from '../../composables/useLogs';
import { useProjects } from '../../composables/useProjects';
import { isLogExpanded } from '../../store/logStore';

const { activeLogProjectId, displayedLogs, consoleRef, copied, copyLogs, clearLog, latestLogLine } = useLogs();
const { projects, anySyncing } = useProjects();

const activeProjectName = computed(() => projects.value.find((p) => p.id === activeLogProjectId.value)?.name ?? '');

// Inverts logStore.isLogExpanded (per-screen mirrored) to DockStack's collapsed state; writable computed syncs back.
const collapsed = computed({
  get: () => !isLogExpanded.value,
  set: (v) => { isLogExpanded.value = !v; },
});

function getLogClass(line) {
  if (line.includes("[ERROR]") || line.includes("FAILED")) return "log-error";
  if (line.includes("SYNC COMPLETED")) return "log-success";
  if (line.startsWith("[")) return "log-global-date";
  return "log-normal";
}
</script>

<style scoped>
.log-copied-icon {
  color: var(--accent-green);
}
</style>
