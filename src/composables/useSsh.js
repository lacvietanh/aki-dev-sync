import { invoke } from "../utils/tauri";
import { useLogs } from "./useLogs";
import { Toast } from "./useProjects";
import {
  sshHosts, selectedSshHost, setSelectedSshHost, showSshModal,
  sshConfigText, hasSshUndo, hasSshRedo
} from "../store/sshStore";

export function useSsh() {
  const { appendGlobalLog } = useLogs();

  async function updateSshHistoryStatus() {
    try {
      const status = await invoke("get_ssh_history_status");
      hasSshUndo.value = status.can_undo;
      hasSshRedo.value = status.can_redo;
    } catch(e) {
      hasSshUndo.value = false;
      hasSshRedo.value = false;
    }
  }

  async function openSshConfig() {
    try {
      sshConfigText.value = await invoke("read_ssh_config");
      await updateSshHistoryStatus();
      showSshModal.value = true;
    } catch (err) {
      appendGlobalLog("ERROR", `Failed to read SSH config: ${err}`);
    }
  }

  function closeSshModal() {
    showSshModal.value = false;
    sshConfigText.value = "";
  }

  function handleEditorTab(e) {
    const start = e.target.selectionStart;
    const end = e.target.selectionEnd;
    const val = sshConfigText.value;
    sshConfigText.value = val.substring(0, start) + "  " + val.substring(end);
    setTimeout(() => {
      e.target.selectionStart = e.target.selectionEnd = start + 2;
    }, 0);
  }

  // Reconcile host-side reactive state (sshHosts, undo/redo, host migration); dynamic import avoids useSsh <-> remoteActions cycle.
  async function reconcileSshHostsOnHost() {
    const { applySshHostsChange } = await import("../store/remoteActions");
    await applySshHostsChange();
  }

  async function saveSshConfig() {
    try {
      await invoke("save_ssh_config", { content: sshConfigText.value });
      appendGlobalLog("SSH", "User manually updated ~/.ssh/config. Undo state created.");
      closeSshModal();
      Toast.fire({ icon: 'success', title: 'SSH config saved' });
      await reconcileSshHostsOnHost();
    } catch (err) {
      appendGlobalLog("ERROR", `Failed to save SSH config: ${err}`);
      Toast.fire({ icon: 'error', title: 'Failed to save SSH config' });
    }
  }

  async function undoSshConfig() {
    if (!hasSshUndo.value) return;
    try {
      sshConfigText.value = await invoke("undo_ssh_config");
      appendGlobalLog("SSH", "Successfully UNDONE changes to ~/.ssh/config.");
      Toast.fire({ icon: 'success', title: 'SSH config undone' });
      await reconcileSshHostsOnHost();
    } catch (err) {
      appendGlobalLog("ERROR", `Failed to undo SSH config: ${err}`);
      Toast.fire({ icon: 'error', title: 'Undo failed' });
    }
  }

  async function redoSshConfig() {
    if (!hasSshRedo.value) return;
    try {
      sshConfigText.value = await invoke("redo_ssh_config");
      appendGlobalLog("SSH", "Successfully REDONE changes to ~/.ssh/config.");
      Toast.fire({ icon: 'success', title: 'SSH config redone' });
      await reconcileSshHostsOnHost();
    } catch (err) {
      appendGlobalLog("ERROR", `Failed to redo SSH config: ${err}`);
      Toast.fire({ icon: 'error', title: 'Redo failed' });
    }
  }

  return {
    sshHosts,
    selectedSshHost,
    setSelectedSshHost,
    showSshModal,
    sshConfigText,
    hasSshUndo,
    hasSshRedo,
    openSshConfig,
    closeSshModal,
    handleEditorTab,
    saveSshConfig,
    undoSshConfig,
    redoSshConfig
  };
}
