import { invoke } from "@tauri-apps/api/core";
import Swal from "sweetalert2";
import { useLogs } from "./useLogs";
import { projects, Toast } from "./useProjects";
import {
  sshHosts, selectedSshHost, showSshModal,
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

  async function checkAndFixAffectedProjects(oldHosts, newHosts, saveProjectsListFn) {
    const missingHosts = oldHosts.filter(h => !newHosts.includes(h));
    const addedHosts = newHosts.filter(h => !oldHosts.includes(h));
    if (missingHosts.length === 0) return;

    let needsSave = false;

    if (missingHosts.length === 1 && addedHosts.length === 1) {
      const missingHost = missingHosts[0];
      const newHost = addedHosts[0];
      
      const affected = projects.value.filter(p => p.remote_host === missingHost);
      if (affected.length > 0) {
        affected.forEach(p => { p.remote_host = newHost; });
        needsSave = true;
        appendGlobalLog("SSH", `Auto-migrated ${affected.length} projects from '${missingHost}' to '${newHost}'.`);
        Toast.fire({ icon: 'info', title: `Auto-migrated projects to '${newHost}'` });
      }
    } else {
      for (const missingHost of missingHosts) {
        const affected = projects.value.filter(p => p.remote_host === missingHost);
        if (affected.length === 0) continue;

        const inputOptions = {};
        newHosts.forEach(h => { inputOptions[h] = h; });

        const { value: newHost } = await Swal.fire({
          title: '⚠️ SSH Host Missing',
          html: `Host <b>${missingHost}</b> no longer exists in SSH config, but is used by <b>${affected.length} project(s)</b>.<br><br>Select a replacement host to update them automatically:`,
          icon: 'warning',
          input: 'select',
          inputOptions,
          inputPlaceholder: '--- Select replacement host ---',
          showCancelButton: true,
          confirmButtonText: 'Update',
          cancelButtonText: 'Skip',
          allowOutsideClick: false,
          background: '#131317',
          color: '#e2e8f0'
        });

        if (newHost) {
          affected.forEach(p => { p.remote_host = newHost; });
          needsSave = true;
          appendGlobalLog("SSH", `Migrated ${affected.length} projects from ${missingHost} to ${newHost}.`);
        }
      }
    }
    
    if (needsSave) {
      await saveProjectsListFn();
      if (missingHosts.length !== 1 || addedHosts.length !== 1) {
        Toast.fire({ icon: 'success', title: 'Projects updated with new hosts' });
      }
    }
  }

  async function saveSshConfig(saveProjectsListFn) {
    try {
      const oldHosts = [...sshHosts.value];
      await invoke("save_ssh_config", { content: sshConfigText.value });
      await updateSshHistoryStatus();
      const newHosts = await invoke("get_ssh_hosts");
      sshHosts.value = newHosts;
      appendGlobalLog("SSH", "User manually updated ~/.ssh/config. Undo state created.");
      closeSshModal();
      Toast.fire({ icon: 'success', title: 'SSH config saved' });
      await checkAndFixAffectedProjects(oldHosts, newHosts, saveProjectsListFn);
    } catch (err) {
      appendGlobalLog("ERROR", `Failed to save SSH config: ${err}`);
      Toast.fire({ icon: 'error', title: 'Failed to save SSH config' });
    }
  }

  async function undoSshConfig(saveProjectsListFn) {
    if (!hasSshUndo.value) return;
    try {
      const oldHosts = [...sshHosts.value];
      sshConfigText.value = await invoke("undo_ssh_config");
      await updateSshHistoryStatus();
      const newHosts = await invoke("get_ssh_hosts");
      sshHosts.value = newHosts;
      appendGlobalLog("SSH", "Successfully UNDONE changes to ~/.ssh/config.");
      Toast.fire({ icon: 'success', title: 'SSH config undone' });
      await checkAndFixAffectedProjects(oldHosts, newHosts, saveProjectsListFn);
    } catch (err) {
      appendGlobalLog("ERROR", `Failed to undo SSH config: ${err}`);
      Toast.fire({ icon: 'error', title: 'Undo failed' });
    }
  }

  async function redoSshConfig(saveProjectsListFn) {
    if (!hasSshRedo.value) return;
    try {
      const oldHosts = [...sshHosts.value];
      sshConfigText.value = await invoke("redo_ssh_config");
      await updateSshHistoryStatus();
      const newHosts = await invoke("get_ssh_hosts");
      sshHosts.value = newHosts;
      appendGlobalLog("SSH", "Successfully REDONE changes to ~/.ssh/config.");
      Toast.fire({ icon: 'success', title: 'SSH config redone' });
      await checkAndFixAffectedProjects(oldHosts, newHosts, saveProjectsListFn);
    } catch (err) {
      appendGlobalLog("ERROR", `Failed to redo SSH config: ${err}`);
      Toast.fire({ icon: 'error', title: 'Redo failed' });
    }
  }

  return {
    sshHosts,
    selectedSshHost,
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
