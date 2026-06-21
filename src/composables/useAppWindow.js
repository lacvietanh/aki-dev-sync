import { getCurrentWindow } from "@tauri-apps/api/window";

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

  return {
    minimize,
    closeWin,
    startDragging,
  };
}
