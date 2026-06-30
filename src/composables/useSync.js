import { invoke } from '@tauri-apps/api/core'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import Swal from 'sweetalert2'
import { projectRuntime, Toast } from '../store/projectStore'
import { useLogs } from './useLogs'
import { saveProjectsList } from './useProjectConfig'
import { fetchGitStatus } from './useGit'

const { appendGlobalLog, appendLog, projectLogs, activeLogProjectId, isLogExpanded } = useLogs()

export async function startSync(project, direction, specificPaths = []) {
  if (projectRuntime.value[project.id]?.syncing) {
    Toast.fire({ icon: 'warning', title: `${project.name} is syncing, please wait` })
    return
  }

  projectRuntime.value[project.id] = { ...projectRuntime.value[project.id], syncing: true }
  const isDryRun = project.dry_run

  activeLogProjectId.value = project.id
  isLogExpanded.value = true
  if (!projectLogs.value[project.id]) projectLogs.value[project.id] = []
  projectLogs.value[project.id] = []

  const isDeleteOp = !isDryRun && specificPaths.length === 0 &&
    ((direction === 'push' && project.delete_on_push) || (direction === 'pull' && project.delete_on_pull))

  if (isDeleteOp) {
    appendLog(project.id, `>>> Checking files at risk for ${direction.toUpperCase()} --delete...`)
    let deleteList = []
    try {
      deleteList = await invoke('get_sync_delete_preview', { project, direction })
    } catch (_) {}

    if (deleteList.length > 0) {
      const dest = direction === 'push' ? 'Remote' : 'Local'
      const sample = deleteList.slice(0, 8).map(f => `  ${f}`).join('\n')
      const moreNote = deleteList.length > 8 ? `\n  … and ${deleteList.length - 8} more` : ''
      const { isConfirmed, value: typed } = await Swal.fire({
        title: `XÁC NHẬN XÓA ${deleteList.length} FILE`,
        html:
          `<b>${direction.toUpperCase()} --delete</b> sẽ xóa vĩnh viễn <b>${deleteList.length}</b> file(s) trên <b>${dest}</b>:<br>` +
          `<pre style="text-align:left;font-size:11px;background:#0a0f16;padding:8px;border-radius:6px;max-height:140px;overflow:auto;margin:10px 0;">${sample}${moreNote}</pre>` +
          `Nhập tên project <b>${project.name}</b> để xác nhận:`,
        input: 'text',
        inputPlaceholder: project.name,
        icon: 'warning',
        showCancelButton: true,
        confirmButtonColor: '#ef4444',
        cancelButtonColor: '#374151',
        confirmButtonText: `Xác nhận ${direction.toUpperCase()} & Xóa`,
        cancelButtonText: 'Hủy bỏ',
        background: '#131317',
        color: '#F3F4F6',
        preConfirm: (val) => {
          if (val !== project.name) {
            Swal.showValidationMessage(`Nhập đúng "${project.name}" để xác nhận`)
            return false
          }
          return true
        }
      })
      if (!isConfirmed || typed !== project.name) {
        projectRuntime.value[project.id] = { ...projectRuntime.value[project.id], syncing: false }
        isLogExpanded.value = false
        activeLogProjectId.value = null
        return
      }
    }
    projectLogs.value[project.id] = []
  }

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

    if (!isDryRun && specificPaths.length === 0) {
      if (direction === 'push') {
        const isMirror = project.delete_on_push
        projectRuntime.value[project.id] = {
          ...projectRuntime.value[project.id],
          hasPendingPush: false, pushCount: 0,
          ...(isMirror ? { hasPendingPull: false, pullCount: 0 } : {}),
        }
      } else if (direction === 'pull') {
        const isMirror = project.delete_on_pull
        projectRuntime.value[project.id] = {
          ...projectRuntime.value[project.id],
          hasPendingPull: false, pullCount: 0,
          ...(isMirror ? { hasPendingPush: false, pushCount: 0 } : {}),
        }
      }
    }

    if (activeLogProjectId.value === project.id) {
      setTimeout(() => {
        isLogExpanded.value = false
        activeLogProjectId.value = null
      }, 1500)
    }

    Toast.fire({ icon: 'success', title: isDryRun ? 'Dry run complete' : 'Sync complete' })
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

/**
 * SELECT: opens native OS file dialog, checks for remote conflicts, then pushes selected files.
 */
export async function openSelectDialog(project) {
  let selected
  try {
    selected = await openDialog({
      title: `Select files to push — ${project.name}`,
      multiple: true,
      defaultPath: project.local_path,
    })
  } catch (err) {
    console.error('File dialog error:', err)
    return
  }

  if (!selected || (Array.isArray(selected) && selected.length === 0)) return

  const selectedArr = Array.isArray(selected) ? selected : [selected]

  // Convert absolute paths → relative paths (relative to local_path)
  const localBase = project.local_path.endsWith('/') ? project.local_path : project.local_path + '/'
  const relPaths = []
  const outsideProject = []
  for (const abs of selectedArr) {
    if (abs.startsWith(localBase)) {
      relPaths.push(abs.slice(localBase.length))
    } else {
      outsideProject.push(abs)
    }
  }

  if (outsideProject.length > 0) {
    Toast.fire({
      icon: 'warning',
      title: `${outsideProject.length} file(s) outside project path — skipped`,
    })
  }

  if (relPaths.length === 0) return

  // Check for remote conflicts
  let conflicts = []
  if (project.remote_host && project.remote_path) {
    try {
      const info = await invoke('get_file_conflict_info', {
        localPath: project.local_path,
        remoteHost: project.remote_host,
        remotePath: project.remote_path,
        relPaths,
      })
      conflicts = info.filter(f => f.remote_exists)
    } catch (err) {
      console.error('Conflict check failed:', err)
    }
  }

  if (conflicts.length > 0) {
    const rows = conflicts.map(f =>
      `<tr>
        <td style="text-align:left;padding:3px 8px;font-size:11px;font-family:monospace;word-break:break-all">${escHtml(f.rel_path)}</td>
        <td style="padding:3px 8px;font-size:11px;white-space:nowrap">${escHtml(f.local_mtime_fmt)}</td>
        <td style="padding:3px 8px;font-size:11px;white-space:nowrap;color:#f59e0b">${escHtml(f.remote_mtime_fmt)}</td>
      </tr>`
    ).join('')

    const { isConfirmed } = await Swal.fire({
      title: `${conflicts.length} file(s) đã tồn tại trên remote`,
      html:
        `<p style="font-size:12px;margin:0 0 10px">Push sẽ ghi đè các file sau:</p>` +
        `<div style="overflow-x:auto">` +
        `<table style="width:100%;border-collapse:collapse;font-size:12px">` +
        `<thead><tr>
          <th style="text-align:left;padding:4px 8px;border-bottom:1px solid #374151;font-size:10px;color:#9ca3af">FILE</th>
          <th style="padding:4px 8px;border-bottom:1px solid #374151;font-size:10px;color:#9ca3af">LOCAL</th>
          <th style="padding:4px 8px;border-bottom:1px solid #374151;font-size:10px;color:#f59e0b">REMOTE</th>
        </tr></thead>` +
        `<tbody>${rows}</tbody></table></div>`,
      icon: 'warning',
      showCancelButton: true,
      confirmButtonColor: '#f59e0b',
      cancelButtonColor: '#374151',
      confirmButtonText: 'Ghi đè & Push',
      cancelButtonText: 'Hủy',
      background: '#131317',
      color: '#F3F4F6',
    })
    if (!isConfirmed) return
  }

  startSync(project, 'push', relPaths)
}

function escHtml(s) {
  return String(s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
}
