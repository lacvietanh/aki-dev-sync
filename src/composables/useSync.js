import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import Swal from 'sweetalert2'
import { projectRuntime, Toast } from '../store/projectStore'
import { useLogs } from './useLogs'
import { saveProjectsList } from './useProjectConfig'
import { fetchGitStatus } from './useGit'
import { checkProjectSyncStatus } from './useSyncStatus'

export const showSpecialModal = ref(false)
export const specialProject = ref(null)
export const specialFiles = ref([])
export const specialSelected = ref([])
export const specialLoading = ref(false)

const { appendGlobalLog, appendLog, projectLogs, activeLogProjectId, isLogExpanded } = useLogs()

export async function startSync(project, direction, specificPaths = []) {
  if (projectRuntime.value[project.id]?.syncing) {
    Toast.fire({ icon: 'warning', title: `${project.name} is syncing, please wait` })
    return
  }

  if (direction === 'push' && project.delete_on_push && specificPaths.length === 0) {
    if (projectRuntime.value[project.id]?.hasPendingPull === true) {
      const confirmed = await Swal.fire({
        title: 'CẢNH BÁO MẤT DỮ LIỆU!',
        html: `Remote đang có file mới hoặc thay đổi chưa được PULL về.<br><br>Vì bạn đang bật tùy chọn <b>PUSH with --delete</b>, lệnh PUSH lúc này sẽ XÓA SẠCH các thay đổi đó trên Remote để ép đồng bộ theo Local.<br><br>Bạn có chắc chắn muốn PUSH đè lên không?`,
        icon: 'warning',
        showCancelButton: true,
        confirmButtonColor: '#ef4444',
        cancelButtonColor: '#374151',
        confirmButtonText: 'Vẫn PUSH (Xóa Remote)',
        cancelButtonText: 'Hủy bỏ',
        background: '#131317',
        color: '#F3F4F6'
      }).then(result => result.isConfirmed)

      if (!confirmed) return
    }
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

    // Optimistic: mark the synced direction as clean, then recheck to confirm
    if (!isDryRun && specificPaths.length === 0) {
      if (direction === 'push') {
        projectRuntime.value[project.id] = { ...projectRuntime.value[project.id], hasPendingPush: false }
      } else if (direction === 'pull') {
        projectRuntime.value[project.id] = { ...projectRuntime.value[project.id], hasPendingPull: false }
      }
      setTimeout(() => checkProjectSyncStatus(project), 3000)
    }

    if (activeLogProjectId.value === project.id) {
      setTimeout(() => {
        isLogExpanded.value = false
        activeLogProjectId.value = null
      }, 1500)
    }

    Toast.fire({ icon: 'success', title: 'Sync complete' })
  } catch (err) {
    appendLog(project.id, `\n[ERROR] Sync failed: ${err}`)
    appendGlobalLog("ERROR", `Sync failed for "${project.name}": ${err}`)
    project.last_sync_status = "error"
    await saveProjectsList()
    Toast.fire({ icon: 'error', title: 'Sync failed' })
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
    specialFiles.value = await invoke("get_project_files", { localPath: project.local_path, syncGit: project.sync_git })
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
