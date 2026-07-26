import { computed, nextTick } from "vue";
import { listen } from "@tauri-apps/api/event";
import { onHostBoot } from "../utils/scheduler";
import {
  globalLogs, projectLogs, activeLogProjectId,
  isLogExpanded, consoleRef, copied,
  globalListener, setGlobalListener
} from "../store/logStore";

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
    } catch (err) {
      console.warn('Clipboard copy failed:', err);
    }
  }

  async function setupGlobalListener() {
    if (globalListener) return;
    // Companion has no __TAURI_INTERNALS__, so `listen()` would reject there - and the
    // companion never needs it anyway, since mirrored log lines already arrive via
    // src/store/logStore.js. Gate through the seam-P boundary module (utils/scheduler) instead
    // of importing `isHost` here, per ENV-1 (docs/plan/done/remote-control.md §9): on host this is a
    // plain `await listen(...)`, on companion `listen()` is never called at all.
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
    scrollConsole,
    appendLog,
    appendGlobalLog,
    clearLog,
    toggleProjectLog,
    copyLogs,
    setupGlobalListener,
  };
}
