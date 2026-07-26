<template>
  <div class="dashboard-bottom" :class="{ 'is-collapsed': !isLogExpanded }">
    <div class="terminal-panel">
      <div class="terminal-header">
        <div class="terminal-title">
          <!-- Panel tabs (plan §4.6 row 9: "a TERMINAL tab beside AppConsole.vue's existing
               GLOBAL/project log tabs" — reuses this existing header row, adds no new one). -->
          <button class="btn-tech btn-tech-secondary btn-icon-only panel-tab" :class="{ 'is-active': activePanel === 'log' }" @click="activePanel = 'log'" title="Event log">
            <i class="fa-solid fa-book-journal-whills"></i>
          </button>
          <button class="btn-tech btn-tech-secondary btn-icon-only panel-tab" :class="{ 'is-active': activePanel === 'terminal' }" @click="activePanel = 'terminal'" title="Terminal">
            <i class="fa-solid fa-terminal"></i>
          </button>
          <template v-if="activePanel === 'log'">
            <span v-if="activeLogProjectId === null" class="ml-2"><i class="fa-solid fa-book-journal-whills text-cyan mr-1"></i> GLOBAL EVENT LOG</span>
            <span v-else class="ml-2">
              <span class="badge-project">{{ projects.find(p => p.id === activeLogProjectId)?.name }}</span>
              <i class="fa-solid fa-terminal text-amber ml-1 mr-1"></i> RAW CONSOLE
            </span>
            <button v-if="activeLogProjectId !== null" class="btn-tech btn-tech-secondary btn-icon-only ml-2 text-red" @click="activeLogProjectId = null" title="Close Project Log & Return to Global Log">
              <i class="fa-solid fa-circle-xmark"></i>
            </button>
            <span class="status-indicator ml-2" v-if="anySyncing">
              <span class="status-dot"></span> SYNCING...
            </span>
          </template>
          <!-- Dead-shell state is carried by recolouring text that is already here, plus the
               RESTART button turning amber — no extra banner or status row (Extreme Narrow). -->
          <span v-else class="ml-2" :class="terminalRef?.alive === false ? 'text-red' : ''">
            <i class="fa-solid fa-terminal mr-1" :class="terminalRef?.alive === false ? 'text-red' : 'text-cyan'"></i>
            {{ terminalRef?.alive === false ? 'TERMINAL - EXITED' : 'TERMINAL' }}
          </span>
        </div>
        <div class="terminal-actions">
          <button class="btn-tech btn-tech-secondary btn-terminal-action" @click="isLogExpanded = !isLogExpanded" title="Collapse / Expand panel">
            <i class="fa-solid" :class="isLogExpanded ? 'fa-chevron-down' : 'fa-chevron-up'"></i>
            <span class="u-narrow-hide">{{ isLogExpanded ? 'COLLAPSE' : 'EXPAND' }}</span>
          </button>
          <template v-if="activePanel === 'log'">
            <button class="btn-tech btn-tech-secondary btn-terminal-action" @click="copyLogs" :disabled="displayedLogs.length === 0" title="Copy logs">
              <i class="fa-solid" :class="copied ? 'fa-check log-copied-icon' : 'fa-copy'"></i>
              <span class="u-narrow-hide">{{ copied ? 'COPIED' : 'COPY' }}</span>
            </button>
            <button class="btn-tech btn-tech-secondary btn-terminal-action" @click="clearLog" :disabled="displayedLogs.length === 0" title="Clear log">
              <i class="fa-solid fa-trash"></i>
              <span class="u-narrow-hide">CLEAR</span>
            </button>
          </template>
          <!-- Terminal management, in the header row that already exists rather than a new
               toolbar. CLEAR wipes the HOST's scrollback (so it stays cleared on every screen and
               after a reconnect), RESTART kills and respawns the shared shell, OPEN hands the
               shell's current directory to Terminal.app. -->
          <template v-else>
            <button class="btn-tech btn-tech-secondary btn-terminal-action" @click="terminalRef?.clear()" title="Clear terminal scrollback (all screens)">
              <i class="fa-solid fa-trash"></i>
              <span class="u-narrow-hide">CLEAR</span>
            </button>
            <button class="btn-tech btn-tech-secondary btn-terminal-action" :class="{ 'text-amber': terminalRef?.alive === false }" @click="terminalRef?.restart()" title="Kill and start a new shell">
              <i class="fa-solid fa-rotate-right"></i>
              <span class="u-narrow-hide">RESTART</span>
            </button>
            <button class="btn-tech btn-tech-secondary btn-terminal-action" :disabled="terminalRef?.alive === false" @click="terminalRef?.kill()" title="Kill the running shell">
              <i class="fa-solid fa-circle-stop"></i>
              <span class="u-narrow-hide">KILL</span>
            </button>
            <button class="btn-tech btn-tech-secondary btn-terminal-action" @click="terminalRef?.openExternal()" title="Open this shell's current directory in Terminal.app">
              <i class="fa-solid fa-arrow-up-right-from-square"></i>
              <span class="u-narrow-hide">OPEN</span>
            </button>
          </template>
        </div>
      </div>
      <!-- u-select-text (main.css): log output is the single most copy-worthy surface in the app,
           so it opts out of the app-wide no-selection default. -->
      <div v-if="activePanel === 'log'" class="console-output u-select-text" ref="consoleRef">
        <div v-if="displayedLogs.length === 0" class="empty-logs">
          <i class="fa-solid fa-ghost mb-2"></i><br>
          {{ activeLogProjectId ? "No raw logs yet. Trigger a sync action." : "No global events recorded yet." }}
        </div>
        <div v-for="(line, index) in displayedLogs" :key="index" class="log-line" :class="getLogClass(line)">{{ line }}</div>
      </div>
      <!-- Kept mounted only while active: a PTY is spawned lazily on first open (pty_spawn is
           idempotent — T-3), so switching to LOG and back does not respawn or lose the session;
           TerminalView itself keeps no local state that would need to survive an unmount here,
           since scrollback lives on the host (src-tauri/src/pty.rs) and rehydrates on remount. -->
      <div v-else class="terminal-mount-wrap">
        <TerminalView ref="terminalRef" />
      </div>
    </div>
  </div>
</template>

<script setup>
import { onMounted, onUnmounted, ref } from 'vue';
import { useLogs } from '../composables/useLogs';
import { useProjects } from '../composables/useProjects';
import { useTerminalPanel } from '../composables/useTerminalPanel';
import TerminalView from './TerminalView.vue';

const { activeLogProjectId, isLogExpanded, displayedLogs, consoleRef, copied, copyLogs, clearLog } = useLogs();
const { projects, anySyncing } = useProjects();

// Local, per-screen UI state — NOT mirrored (same reasoning as roadmap's Explorer View
// "navigation is an input event, local to each screen"): which screen is looking at LOG vs
// TERMINAL is independent per device, unlike the shared PTY session itself. It lives in a
// composable rather than here so ProjectTable's "In-App Terminal" item can switch to this tab.
const { activePanel } = useTerminalPanel();

// TerminalView's defineExpose — the panel-management buttons above live in this header row and
// drive the terminal through it. Null while the LOG tab is showing, hence every `?.`.
const terminalRef = ref(null);

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

.panel-tab {
  opacity: 0.5;
}

.panel-tab.is-active {
  opacity: 1;
  color: var(--accent-cyan);
  border-color: var(--accent-cyan);
}

.terminal-mount-wrap {
  flex: 1;
  min-height: 0;
  display: flex;
}
</style>
