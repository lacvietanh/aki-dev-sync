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

// PER-SCREEN, and therefore NOT in src/store/*.js — services/mirror.js auto-discovers every `isRef`
// export under src/store/ and would mirror both (the rule is documented in useTerminalPanel.js's
// file header). `consoleRef` holds a live DOM node, which the mirror's encoder cannot serialise at
// all; `copied` is this screen's own 2s COPIED flash, and mirroring it made a COPY tap on the phone
// flash the Mac's button. Module scope (not inside useLogs()) because every caller of useLogs()
// must see the SAME two refs: LogStack.vue binds the template ref, useSync.js scrolls it.
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

  // Both appenders go through logStore's capped funnel (LOG_CAP, contract C-2) rather than pushing
  // into the array directly - that is where the 2,000-line ceiling and the mirror's append cursor
  // are maintained together.
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

  // Deliberately NOT wrapped in action(): `activeLogProjectId` / `isLogExpanded` are per-screen and
  // excluded from the mirror (§3.12, see store/logStore.js). Each screen opens and closes its own
  // panel; routing this through the host would put the choice back on the wire, which is the very
  // bleed §3.12 removes. Nothing reverts it now, because nothing mirrors it.
  function toggleProjectLog(id) {
    if (activeLogProjectId.value === id) {
      activeLogProjectId.value = null;
    } else {
      activeLogProjectId.value = id;
      isLogExpanded.value = true;
    }
  }

  // utils/clipboard.js, not `navigator.clipboard` directly: the companion is a non-secure context
  // where that API does not exist, so COPY LOGS was silently dead on the phone. A total failure now
  // says so instead of flashing COPIED over a clipboard that was never written.
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
