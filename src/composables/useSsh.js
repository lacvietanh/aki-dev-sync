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

  // The reactive fallout of an SSH-config write — refreshing the mirrored `sshHosts` list, the
  // undo/redo availability flags, and migrating any project pinned to a now-missing host — MUST run
  // on the HOST so it lands in the Mac's reactive state and mirrors to every screen (ACT-1 / feat
  // matrix "SSH config → Save"). Before this it ran on the clicker: a companion set its own copy of
  // `sshHosts` and mutated its own `projects`, none of which reached the Mac. The RPC file writes
  // (`save/undo/redo_ssh_config`) already hit the Mac's disk from the clicker; only the reactive
  // reconcile is routed host-side, through `applySshHostsChange`. UI-only bits (Toast, close modal,
  // logging, showing the reverted text in the editor) stay on the clicker. Dynamic import avoids the
  // useSsh ⇄ remoteActions eager cycle (remoteActions imports composables).
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
