// Thin re-export facade — all state and logic live in split modules.
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
  startSync, openSelectDialog,
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
  startSync, openSelectDialog,
  // sync status + background refresh
  checkProjectSyncStatus, checkAllSyncStatus, startBackgroundRefresh,
}

// Factory shim — backward compat for components that call useProjects()
export function useProjects() {
  return {
    projects, projectRuntime, anySyncing, isReloading, Toast,
    showGitModal, gitProject, gitStatusText, fetchGitStatus, openGitModal, closeGitModal,
    isGitLoading, runGitFetch, runGitPush, runGitCommit, projectChangelogText,
    showConfigModal, editingProject,
    loadData, saveProjectsList, openConfig, closeConfig, saveConfig, createNewProject, confirmRemove,
    startSync, openSelectDialog,
    checkProjectSyncStatus, checkAllSyncStatus, startBackgroundRefresh,
  }
}
