// Thin re-export facade — all state and logic live in split modules.
// Components import { useProjects } and destructure as before; useSsh.js
// imports { projects, Toast } directly — both patterns continue to work.
import { projects, projectRuntime, anySyncing, isReloading, Toast } from '../store/projectStore'
import { showGitModal, gitProject, gitStatusText, fetchGitStatus, openGitModal, closeGitModal } from './useGit'
import {
  showConfigModal, editingProject,
  loadData, saveProjectsList, openConfig, closeConfig, saveConfig, createNewProject, confirmRemove,
} from './useProjectConfig'
import {
  showSpecialModal, specialProject, specialFiles, specialSelected, specialLoading,
  startSync, openSpecialModal, closeSpecialModal, toggleSpecialSelection, selectAllSpecial, confirmPushSpecial,
} from './useSync'

export {
  // store
  projects, projectRuntime, anySyncing, isReloading, Toast,
  // git
  showGitModal, gitProject, gitStatusText, fetchGitStatus, openGitModal, closeGitModal,
  // config
  showConfigModal, editingProject,
  loadData, saveProjectsList, openConfig, closeConfig, saveConfig, createNewProject, confirmRemove,
  // sync
  showSpecialModal, specialProject, specialFiles, specialSelected, specialLoading,
  startSync, openSpecialModal, closeSpecialModal, toggleSpecialSelection, selectAllSpecial, confirmPushSpecial,
}

// Factory shim — backward compat for the 8 components that call useProjects()
export function useProjects() {
  return {
    projects, projectRuntime, anySyncing, isReloading, Toast,
    showGitModal, gitProject, gitStatusText, fetchGitStatus, openGitModal, closeGitModal,
    showConfigModal, editingProject,
    loadData, saveProjectsList, openConfig, closeConfig, saveConfig, createNewProject, confirmRemove,
    showSpecialModal, specialProject, specialFiles, specialSelected, specialLoading,
    startSync, openSpecialModal, closeSpecialModal, toggleSpecialSelection, selectAllSpecial, confirmPushSpecial,
  }
}
