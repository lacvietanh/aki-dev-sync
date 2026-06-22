import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { projectRuntime, Toast } from '../store/projectStore'
import { useLogs } from './useLogs'
import { saveProjectsList } from './useProjectConfig'
import { fetchGitStatus } from './useGit'

export const showSpecialModal = ref(false)
export const specialProject = ref(null)
export const specialFiles = ref([])
export const specialSelected = ref([])
export const specialLoading = ref(false)

const { appendGlobalLog, appendLog, projectLogs, activeLogProjectId, isLogExpanded } = useLogs()

export async function startSync(project, direction, specificPaths = []) {
  if (projectRuntime.value[project.id]?.syncing) {
    Toast.fire({ icon: 'warning', title: `${project.name} đang sync, vui lòng chờ` })
    return
  }

  const isDryRun = project.dry_run
  projectRuntime.value[project.id] = { ...projectRuntime.value[project.id], syncing: true }
  activeLogProjectId.value = project.id
  isLogExpanded.value = true

  if (!projectLogs.value[project.id]) projectLogs.value[project.id] = []
  projectLogs.value[project.id] = []

  let actionName = direction.toUpperCase()
  if (specificPaths.length === 1 && specificPaths[0] === ".git/") actionName = "SYNC GIT"
  else if (specificPaths.length > 0) actionName = "PUSH SPECIAL"

  appendLog(project.id, `>>> START SYNC [${actionName}] - ${project.name}`)
  if (specificPaths.length > 0) {
    appendLog(project.id, `>>> TARGET: Partial Sync on ${specificPaths.length} specific item(s)`)
  }

  const dryText = isDryRun ? " (Dry Run)" : ""
  appendGlobalLog("SYNC", `Started ${actionName} for "${project.name}"${dryText}`)

  try {
    await invoke("run_sync", {
      project,
      direction,
      dryRun: isDryRun,
      specificPaths,
      syncGit: project.sync_git,
    })
    project.last_sync_action = actionName + (isDryRun ? " (Dry)" : "")
    project.last_sync_time = Math.floor(Date.now() / 1000)
    project.last_sync_status = "success"
    await saveProjectsList()
    fetchGitStatus(project.id)

    if (activeLogProjectId.value === project.id) {
      setTimeout(() => {
        isLogExpanded.value = false
        activeLogProjectId.value = null
      }, 1500)
    }

    Toast.fire({ icon: 'success', title: 'Đồng bộ hoàn tất!' })
  } catch (err) {
    appendLog(project.id, `\n[ERROR] Sync failed: ${err}`)
    appendGlobalLog("ERROR", `Sync failed for "${project.name}": ${err}`)
    project.last_sync_status = "error"
    await saveProjectsList()
    Toast.fire({ icon: 'error', title: 'Đồng bộ thất bại' })
  } finally {
    projectRuntime.value[project.id] = { ...projectRuntime.value[project.id], syncing: false }
  }
}

export async function openSpecialModal(project) {
  specialProject.value = project
  showSpecialModal.value = true
  specialFiles.value = []
  specialSelected.value = []
  specialLoading.value = true
  try {
    specialFiles.value = await invoke("get_project_files", { localPath: project.local_path })
  } catch (err) {
    appendGlobalLog("ERROR", `Failed to load files: ${err}`)
  }
  specialLoading.value = false
}

export function closeSpecialModal() {
  showSpecialModal.value = false
  specialProject.value = null
  specialFiles.value = []
  specialSelected.value = []
}

export function toggleSpecialSelection(file) {
  const idx = specialSelected.value.indexOf(file)
  if (idx === -1) specialSelected.value.push(file)
  else specialSelected.value.splice(idx, 1)
}

export function selectAllSpecial(selected) {
  if (selected) specialSelected.value = [...specialFiles.value]
  else specialSelected.value = []
}

export function confirmPushSpecial() {
  if (specialSelected.value.length === 0) return
  const p = specialProject.value
  const selected = [...specialSelected.value]
  closeSpecialModal()
  startSync(p, "push", selected)
}
