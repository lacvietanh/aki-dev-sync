import { invoke } from '../utils/tauri'
import { projects, projectRuntime, currentEpoch, beginRefresh, endRefresh } from '../store/projectStore'

// Derived DEV/BUILD commands: kept as a per-project peer to fetchGitStatus so project refresh updates stale stack info.
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
    // Keep previous stack info on transient read failure instead of blanking DEV/BUILD commands.
  } finally {
    // Only the generation that started this counts its own completion - see fetchGitStatus.
    if (currentEpoch(projectId) === epoch) endRefresh(projectId)
  }
}
