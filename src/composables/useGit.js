import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { projects, projectRuntime } from '../store/projectStore'
import { useLogs } from './useLogs'

export const showGitModal = ref(false)
export const gitProject = ref(null)
export const gitStatusText = ref('')
export const projectChangelogText = ref(null)

const { appendGlobalLog } = useLogs()

export async function fetchGitStatus(projectId, silent = false) {
  const project = projects.value.find(p => p.id === projectId)
  if (!project) return
  try {
    if (!silent) appendGlobalLog("GIT", `Checking status for "${project.name}"...`)
    const info = await invoke("get_git_info", { localPath: project.local_path })
    projectRuntime.value[projectId] = {
      ...projectRuntime.value[projectId],
      git_status: info.status,
      git_log: info.log,
      remote_url: info.remote_url || "",
    }
    if (!silent) appendGlobalLog("GIT", `Status for "${project.name}": ${info.status}`)
    if (gitProject.value && gitProject.value.id === projectId) {
      gitStatusText.value = info.log || 'No Git history available.'
    }
  } catch (err) {
    const errorLog = `Failed to load Git status:\n${err}`
    projectRuntime.value[projectId] = {
      ...projectRuntime.value[projectId],
      git_status: "Git Error",
      git_log: errorLog,
    }
    appendGlobalLog("ERROR", `Failed git status for "${project.name}": ${err}`)
    if (gitProject.value && gitProject.value.id === projectId) {
      gitStatusText.value = errorLog
    }
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
  checkProjectChangelog(project)
  await fetchGitStatus(project.id)
}

export function closeGitModal() {
  showGitModal.value = false
  gitProject.value = null
  gitStatusText.value = ''
}

export const isGitLoading = ref(false)

export async function runGitFetch(project) {
  if (!project) return
  isGitLoading.value = true
  gitStatusText.value = 'Running git fetch...'
  appendGlobalLog("GIT", `Running git fetch for "${project.name}"...`)
  try {
    const res = await invoke("run_git_command", { localPath: project.local_path, args: ["fetch"] })
    appendGlobalLog("GIT", `Git Fetch result for "${project.name}": ${res}`)
    await fetchGitStatus(project.id)
  } catch (err) {
    appendGlobalLog("ERROR", `Git Fetch failed for "${project.name}": ${err}`)
    gitStatusText.value = `Git Fetch failed:\n${err}`
    throw err
  } finally {
    isGitLoading.value = false
  }
}

export async function runGitPush(project) {
  if (!project) return
  isGitLoading.value = true
  gitStatusText.value = 'Running git push...'
  appendGlobalLog("GIT", `Running git push for "${project.name}"...`)
  try {
    const res = await invoke("run_git_command", { localPath: project.local_path, args: ["push"] })
    appendGlobalLog("GIT", `Git Push result for "${project.name}": ${res}`)
    await fetchGitStatus(project.id)
  } catch (err) {
    appendGlobalLog("ERROR", `Git Push failed for "${project.name}": ${err}`)
    gitStatusText.value = `Git Push failed:\n${err}`
    throw err
  } finally {
    isGitLoading.value = false
  }
}

export async function runGitCommit(project, message) {
  if (!project || !message.trim()) return
  isGitLoading.value = true
  gitStatusText.value = 'Running git commit...'
  appendGlobalLog("GIT", `Running git commit for "${project.name}"...`)
  try {
    // Stage all changes first
    await invoke("run_git_command", { localPath: project.local_path, args: ["add", "-A"] })
    // Commit staged changes
    const res = await invoke("run_git_command", { localPath: project.local_path, args: ["commit", "-m", message] })
    appendGlobalLog("GIT", `Git Commit result for "${project.name}": ${res}`)
    await fetchGitStatus(project.id)
  } catch (err) {
    appendGlobalLog("ERROR", `Git Commit failed for "${project.name}": ${err}`)
    gitStatusText.value = `Git Commit failed:\n${err}`
    throw err
  } finally {
    isGitLoading.value = false
  }
}
