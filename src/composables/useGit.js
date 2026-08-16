import { ref } from 'vue'
import { invoke } from '../utils/tauri'
import { projects, projectRuntime, currentEpoch, beginRefresh, endRefresh } from '../store/projectStore'
import { useLogs } from './useLogs'

export const showGitModal = ref(false)
export const gitProject = ref(null)
export const gitStatusText = ref('')
export const projectChangelogText = ref(null)

const { appendGlobalLog } = useLogs()

// updateModalLog=false protects active modal output during background badge refresh; epoch check drops stale responses.
export async function fetchGitStatus(projectId, silent = false, updateModalLog = true) {
  const project = projects.value.find(p => p.id === projectId)
  if (!project) return
  // beginRefresh first guarantees epoch exists so captured value is never 0 ("project removed").
  beginRefresh(projectId)
  const epoch = currentEpoch(projectId)
  try {
    if (!silent) appendGlobalLog("GIT", `Checking status for "${project.name}"...`)
    const info = await invoke("get_git_info", { localPath: project.local_path })
    if (currentEpoch(projectId) !== epoch) return // stale - superseded mid-flight, discard silently
    projectRuntime.value[projectId] = {
      ...projectRuntime.value[projectId],
      git_status: info.status,
      git_log: info.log,
      remote_url: info.remote_url || "",
      git_changed_count: info.changed_count || 0,
      // Distinct from "No Git": directory is missing (e.g. unmounted volume), requiring separate badge state in ProjectTable.
      local_path_missing: !!info.local_path_missing,
    }
    if (!silent) appendGlobalLog("GIT", `Status for "${project.name}": ${info.status}`)
    if (updateModalLog && gitProject.value && gitProject.value.id === projectId) {
      gitStatusText.value = info.log || 'No Git history available.'
    }
  } catch (err) {
    if (currentEpoch(projectId) !== epoch) return // stale - superseded mid-flight, discard silently
    const errorLog = `Failed to load Git status:\n${err}`
    projectRuntime.value[projectId] = {
      ...projectRuntime.value[projectId],
      git_status: "Git Error",
      git_log: errorLog,
      git_changed_count: 0,
    }
    appendGlobalLog("ERROR", `Failed git status for "${project.name}": ${err}`)
    if (updateModalLog && gitProject.value && gitProject.value.id === projectId) {
      gitStatusText.value = errorLog
    }
  } finally {
    // Only original epoch decrements refresh counter; stale runs were reset to 0 by bumpEpoch.
    if (currentEpoch(projectId) === epoch) endRefresh(projectId)
  }
}

export async function checkProjectChangelog(project) {
  if (!project) return
  try {
    const text = await invoke("read_project_changelog", { localPath: project.local_path })
    projectChangelogText.value = text
  } catch (e) {
    projectChangelogText.value = null
  }
}

export async function openGitModal(project) {
  gitProject.value = project
  gitStatusText.value = 'Loading...'
  showGitModal.value = true
  projectChangelogText.value = null
  isGitLoading.value = true
  try {
    checkProjectChangelog(project)
    await fetchGitStatus(project.id)
  } finally {
    isGitLoading.value = false
  }
}

export function closeGitModal() {
  showGitModal.value = false
  gitProject.value = null
  gitStatusText.value = ''
}

export const isGitLoading = ref(false)

// -c color.ui=always forces ANSI color codes in subprocess output for terminal-like display in Git modal.
const COLOR_ARGS = ["-c", "color.ui=always"]

// Runs git step sequence, displays combined output in Git modal status pane, then triggers silent badge refresh.
async function runGitAction(project, verb, steps) {
  if (!project) return
  isGitLoading.value = true
  gitStatusText.value = `Running git ${verb}...`
  appendGlobalLog("GIT", `Running git ${verb} for "${project.name}"...`)
  try {
    let combined = ''
    for (const args of steps) {
      const res = await invoke("run_git_command", { localPath: project.local_path, args })
      combined += (combined ? '\n' : '') + res
    }
    appendGlobalLog("GIT", `Git ${verb} result for "${project.name}": ${combined}`)
    gitStatusText.value = combined || `(git ${verb}: no output)`
    await fetchGitStatus(project.id, true, false)
  } catch (err) {
    appendGlobalLog("ERROR", `Git ${verb} failed for "${project.name}": ${err}`)
    gitStatusText.value = `Git ${verb} failed:\n${err}`
    throw err
  } finally {
    isGitLoading.value = false
  }
}

export function runGitFetch(project) {
  return runGitAction(project, 'fetch', [[...COLOR_ARGS, "fetch"]])
}

export function runGitPush(project) {
  return runGitAction(project, 'push', [[...COLOR_ARGS, "push"]])
}

export function runGitPull(project) {
  return runGitAction(project, 'pull', [[...COLOR_ARGS, "pull"]])
}

export function runGitCommit(project, message) {
  if (!project || !message.trim()) return
  return runGitAction(project, 'commit', [["add", "-A"], [...COLOR_ARGS, "commit", "-m", message]])
}
