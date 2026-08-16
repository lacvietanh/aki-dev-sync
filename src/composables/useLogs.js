import { computed, nextTick, ref } from "vue";
import { listen } from "@tauri-apps/api/event";
import { onHostBoot } from "../utils/scheduler";
import { Toast } from "../store/projectStore";
import { copyText } from "../utils/clipboard";
import {
  globalLogs, projectLogs, activeLogProjectId,
  isLogExpanded,
  globalListener, setGlobalListener,
  appendGlobalLogLines, appendProjectLogLines
} from "../store/logStore";

// Module-scoped per-screen refs (not in store/ so mirror.js won't sync live DOM node or local copy flash across devices).
const consoleRef = ref(null);
const copied = ref(false);

export function useLogs() {
  const displayedLogs = computed(() => {
    if (activeLogProjectId.value) {
      return projectLogs.value[activeLogProjectId.value] || [];
    }
    return globalLogs.value;
  });

  // Feeds the collapsed dock stack's one-line peek (dock/LogStack.vue #peek) — updates live as logs stream in since it derives from the same displayedLogs computed.
  const latestLogLine = computed(() => {
    const lines = displayedLogs.value;
    return lines.length ? lines[lines.length - 1] : '';
  });

  function scrollConsole() {
    nextTick(() => {
      if (consoleRef.value) {
        consoleRef.value.scrollTop = consoleRef.value.scrollHeight;
      }
    });
  }

  // Appenders route through logStore funnel (LOG_CAP ceiling & mirror append cursor).
  function appendLog(projectId, line) {
    appendProjectLogLines(projectId, [line]);
    if (activeLogProjectId.value === projectId) {
      scrollConsole();
    }
  }

  function appendGlobalLog(action, message) {
    appendGlobalLogLines([`[${new Date().toLocaleTimeString()}] [${action}] ${message}`]);
    if (!activeLogProjectId.value) {
      scrollConsole();
    }
  }

  function clearLog() {
    if (activeLogProjectId.value) {
      projectLogs.value[activeLogProjectId.value] = [];
    } else {
      globalLogs.value = [];
    }
  }

  // Not wrapped in action(): activeLogProjectId/isLogExpanded are per-screen and excluded from mirror (§3.12).
  function toggleProjectLog(id) {
    if (activeLogProjectId.value === id) {
      activeLogProjectId.value = null;
    } else {
      activeLogProjectId.value = id;
      isLogExpanded.value = true;
    }
  }

  // Uses utils/clipboard.js because companion non-secure context lacks navigator.clipboard.
  let copyTimer = null;
  async function copyLogs() {
    const logs = displayedLogs.value;
    if (logs.length === 0) return;
    if (await copyText(logs.join("\n"))) {
      copied.value = true;
      if (copyTimer) clearTimeout(copyTimer);
      copyTimer = setTimeout(() => {
        copied.value = false;
        copyTimer = null;
      }, 2000);
    } else {
      Toast.fire({ icon: 'error', title: 'Could not copy - select the log text and copy it by hand' });
    }
  }

  async function setupGlobalListener() {
    if (globalListener) return;
    // Gate listen() on host via onHostBoot (companion has no Tauri IPC and receives mirrored logs via logStore).
    await onHostBoot(async () => {
      setGlobalListener(await listen("sync-log", (event) => {
        const payload = event.payload;
        if (payload && payload.project_id && payload.line !== undefined) {
          appendLog(payload.project_id, payload.line);
        }
      }));
    });
  }

  return {
    globalLogs,
    projectLogs,
    activeLogProjectId,
    isLogExpanded,
    consoleRef,
    copied,
    displayedLogs,
    latestLogLine,
    scrollConsole,
    appendLog,
    appendGlobalLog,
    clearLog,
    toggleProjectLog,
    copyLogs,
    setupGlobalListener,
  };
}
