import { ref, computed, nextTick } from "vue";
import { listen } from "@tauri-apps/api/event";

const globalLogs = ref([]);
const projectLogs = ref({});
const activeLogProjectId = ref(null);
const isLogExpanded = ref(false);
const consoleRef = ref(null);
const copied = ref(false);
let globalListener = null;

export function useLogs() {
  const displayedLogs = computed(() => {
    if (activeLogProjectId.value) {
      return projectLogs.value[activeLogProjectId.value] || [];
    }
    return globalLogs.value;
  });

  function scrollConsole() {
    nextTick(() => {
      if (consoleRef.value) {
        consoleRef.value.scrollTop = consoleRef.value.scrollHeight;
      }
    });
  }

  function appendLog(projectId, line) {
    if (!projectLogs.value[projectId]) projectLogs.value[projectId] = [];
    projectLogs.value[projectId].push(line);
    if (activeLogProjectId.value === projectId) {
      scrollConsole();
    }
  }

  function appendGlobalLog(action, message) {
    const line = `[${new Date().toLocaleTimeString()}] [${action}] ${message}`;
    globalLogs.value.push(line);
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

  function toggleProjectLog(id) {
    if (activeLogProjectId.value === id) {
      activeLogProjectId.value = null;
    } else {
      activeLogProjectId.value = id;
      isLogExpanded.value = true;
    }
  }

  async function copyLogs() {
    const logs = displayedLogs.value;
    if (logs.length === 0) return;
    try {
      await navigator.clipboard.writeText(logs.join("\n"));
      copied.value = true;
      setTimeout(() => (copied.value = false), 2000);
    } catch (err) {}
  }

  async function setupGlobalListener() {
    if (globalListener) return;
    globalListener = await listen("sync-log", (event) => {
      const payload = event.payload;
      if (payload && payload.project_id && payload.line !== undefined) {
        appendLog(payload.project_id, payload.line);
      }
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
    scrollConsole,
    appendLog,
    appendGlobalLog,
    clearLog,
    toggleProjectLog,
    copyLogs,
    setupGlobalListener,
  };
}
