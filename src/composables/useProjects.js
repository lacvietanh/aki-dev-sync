// Thin re-export facade — all state and logic live in split modules.
// Components import { useProjects } and destructure as before; useSsh.js
// imports { projects, Toast } directly — both patterns continue to work.
import { projects, projectRuntime, anySyncing, isReloading, Toast } from '../store/projectStore'
import {
  showGitModal, gitProject, gitStatusText, fetchGitStatus, openGitModal, closeGitModal,
  isGitLoading, runGitFetch, runGitPush, runGitCommit, projectChangelogText
} from './useGit'
import {
  showConfigModal, editingProject,
  loadData, saveProjectsList, openConfig, closeConfig, saveConfig, createNewProject, confirmRemove,
} from './useProjectConfig'
import {
  showSpecialModal, specialProject, specialFiles, specialSelected, specialLoading,
  startSync, openSpecialModal, closeSpecialModal, toggleSpecialSelection, selectAllSpecial, confirmPushSpecial,
} from './useSync'
import { checkProjectSyncStatus, checkAllSyncStatus } from './useSyncStatus'
import { startBackgroundRefresh } from './useBackgroundRefresh'

export {
  // store
  projects, projectRuntime, anySyncing, isReloading, Toast,
  // git
  showGitModal, gitProject, gitStatusText, fetchGitStatus, openGitModal, closeGitModal,
  isGitLoading, runGitFetch, runGitPush, runGitCommit, projectChangelogText,
  // config
  showConfigModal, editingProject,
  loadData, saveProjectsList, openConfig, closeConfig, saveConfig, createNewProject, confirmRemove,
  // sync
  showSpecialModal, specialProject, specialFiles, specialSelected, specialLoading,
  startSync, openSpecialModal, closeSpecialModal, toggleSpecialSelection, selectAllSpecial, confirmPushSpecial,
  // sync status + background refresh
  checkProjectSyncStatus, checkAllSyncStatus, startBackgroundRefresh,
}

// Factory shim — backward compat for the 8 components that call useProjects()
export function useProjects() {
  return {
    projects, projectRuntime, anySyncing, isReloading, Toast,
    showGitModal, gitProject, gitStatusText, fetchGitStatus, openGitModal, closeGitModal,
    isGitLoading, runGitFetch, runGitPush, runGitCommit, projectChangelogText,
    showConfigModal, editingProject,
    loadData, saveProjectsList, openConfig, closeConfig, saveConfig, createNewProject, confirmRemove,
    showSpecialModal, specialProject, specialFiles, specialSelected, specialLoading,
    startSync, openSpecialModal, closeSpecialModal, toggleSpecialSelection, selectAllSpecial, confirmPushSpecial,
    checkProjectSyncStatus, checkAllSyncStatus, startBackgroundRefresh,
  }
}
