// Thin re-export facade - all state and logic live in split modules.
import { projects, projectRuntime, anySyncing, anyRefreshing, isRefreshing, isReloading, Toast } from '../store/projectStore'
import {
  showGitModal, gitProject, gitStatusText, fetchGitStatus, openGitModal, closeGitModal,
  isGitLoading, runGitFetch, runGitPush, runGitPull, runGitCommit, projectChangelogText
} from './useGit'
import {
  showConfigModal, editingProject,
  loadData, saveProjectsList, openConfig, closeConfig, saveConfig, createNewProject, confirmRemove,
} from './useProjectConfig'
import {
  startSync, openSelectDialog,
} from './useSync'
import { checkProjectSyncStatus, checkAllSyncStatus } from './useSyncStatus'
import { startBackgroundRefresh, refreshProject, refreshAllProjects } from './useBackgroundRefresh'

export {
  // store
  projects, projectRuntime, anySyncing, anyRefreshing, isRefreshing, isReloading, Toast,
  // git
  showGitModal, gitProject, gitStatusText, fetchGitStatus, openGitModal, closeGitModal,
  isGitLoading, runGitFetch, runGitPush, runGitPull, runGitCommit, projectChangelogText,
  // config
  showConfigModal, editingProject,
  loadData, saveProjectsList, openConfig, closeConfig, saveConfig, createNewProject, confirmRemove,
  // sync
  startSync, openSelectDialog,
  // sync status + refresh controller
  checkProjectSyncStatus, checkAllSyncStatus,
  startBackgroundRefresh, refreshProject, refreshAllProjects,
}

// Factory shim - backward compat for components that call useProjects()
export function useProjects() {
  return {
    projects, projectRuntime, anySyncing, anyRefreshing, isRefreshing, isReloading, Toast,
    showGitModal, gitProject, gitStatusText, fetchGitStatus, openGitModal, closeGitModal,
    isGitLoading, runGitFetch, runGitPush, runGitPull, runGitCommit, projectChangelogText,
    showConfigModal, editingProject,
    loadData, saveProjectsList, openConfig, closeConfig, saveConfig, createNewProject, confirmRemove,
    startSync, openSelectDialog,
    checkProjectSyncStatus, checkAllSyncStatus,
    startBackgroundRefresh, refreshProject, refreshAllProjects,
  }
}
