import { ref } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";

const PIN_STORAGE_KEY = "aki-devsync-pin-all-spaces";

const isPinned = ref(localStorage.getItem(PIN_STORAGE_KEY) === "true");

export function useAppWindow() {
  const appWindow = getCurrentWindow();

  function minimize() {
    appWindow.minimize();
  }

  function closeWin() {
    appWindow.close();
  }

  function startDragging() {
    appWindow.startDragging();
  }

  function applyPinned(pinned) {
    appWindow.setAlwaysOnTop(pinned);
    appWindow.setVisibleOnAllWorkspaces(pinned);
  }

  function togglePin() {
    isPinned.value = !isPinned.value;
    localStorage.setItem(PIN_STORAGE_KEY, String(isPinned.value));
    applyPinned(isPinned.value);
  }

  function restorePin() {
    if (isPinned.value) applyPinned(true);
  }

  return {
    minimize,
    closeWin,
    startDragging,
    isPinned,
    togglePin,
    restorePin,
  };
}
