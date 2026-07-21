import { invoke } from '@tauri-apps/api/core'
import { projects, projectRuntime, currentEpoch, beginRefresh, endRefresh } from '../store/projectStore'

// Third per-project status check, alongside fetchGitStatus and checkProjectSyncStatus. It reads
// the project's package.json/config to derive the DEV and BUILD commands shown in the OPEN popup.
//
// It lives here, as a peer of the other two, rather than inline in loadData() where it used to be:
// stack info is derived state that goes stale exactly like git status does (editing
// `scripts.dev` changes it), so "refresh this project" has to include it or the global Refresh
// button would quietly stop refreshing something it used to refresh.
export async function fetchProjectStack(projectId) {
  const project = projects.value.find(p => p.id === projectId)
  if (!project) return
  // beginRefresh first - see fetchGitStatus.
  beginRefresh(projectId)
  const epoch = currentEpoch(projectId)
  try {
    const stack = await invoke('check_project_stack', { localPath: project.local_path })
    if (currentEpoch(projectId) !== epoch) return // stale - superseded mid-flight, discard silently
    projectRuntime.value[projectId] = { ...projectRuntime.value[projectId], stack_info: stack }
  } catch (_) {
    // Not a recognized stack (or unreadable) - leave the previous value rather than blanking the
    // DEV/BUILD commands on a transient read failure.
  } finally {
    // Only the generation that started this counts its own completion - see fetchGitStatus.
    if (currentEpoch(projectId) === epoch) endRefresh(projectId)
  }
}
