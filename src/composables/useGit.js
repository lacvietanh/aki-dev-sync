import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { projects, projectRuntime } from '../store/projectStore'
import { useLogs } from './useLogs'

export const showGitModal = ref(false)
export const gitProject = ref(null)
export const gitStatusText = ref('')

const { appendGlobalLog } = useLogs()

export async function fetchGitStatus(projectId) {
  const project = projects.value.find(p => p.id === projectId)
  if (!project) return
  try {
    appendGlobalLog("GIT", `Checking status for "${project.name}"...`)
    const info = await invoke("get_git_info", { localPath: project.local_path })
    projectRuntime.value[projectId] = {
      ...projectRuntime.value[projectId],
      git_status: info.status,
      git_log: info.log,
      remote_url: info.remote_url || "",
    }
    appendGlobalLog("GIT", `Status for "${project.name}": ${info.status}`)
  } catch (err) {
    projectRuntime.value[projectId] = {
      ...projectRuntime.value[projectId],
      git_status: "Git Error",
      git_log: `Failed to load Git status:\n${err}`,
    }
    appendGlobalLog("ERROR", `Failed git status for "${project.name}": ${err}`)
  }
}

export function openGitModal(project) {
  gitProject.value = project
  showGitModal.value = true
  const rt = projectRuntime.value[project.id]
  gitStatusText.value = rt?.git_log || 'No Git history available.'
}

export function closeGitModal() {
  showGitModal.value = false
  gitProject.value = null
  gitStatusText.value = ''
}
