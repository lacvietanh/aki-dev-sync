import { invoke } from '@tauri-apps/api/core'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import Swal from 'sweetalert2'
import { projectRuntime, Toast } from '../store/projectStore'
import { syncCheckEnabled } from '../store/syncCheckStore'
import { useLogs } from './useLogs'
import { saveProjectsList } from './useProjectConfig'
import { fetchGitStatus } from './useGit'

const { appendGlobalLog, appendLog, projectLogs, activeLogProjectId, isLogExpanded } = useLogs()

// Artifacts this app itself produces/manages - routine sync churn on them is expected and must
// not raise a confirm dialog. See docs/plan/done/narrow-mode-and-ux-1.14.0.md §A1.
const FLOW_APP_ARTIFACTS = ['REPORT.html']

/**
 * Push-only dirs = dir-entries (`/`-suffixed) present in pull_excludes but absent
 * from push_excludes - e.g. `.git/` by default. Push carries them, pull ignores them.
 */
function pushOnlyDirs(project) {
  const pushSet = new Set((project.push_excludes || []).map(e => e.trim()))
  return (project.pull_excludes || [])
    .map(e => e.trim())
    .filter(e => e.endsWith('/') && !pushSet.has(e))
}

function matchesDirExclude(relPath, dirExcludes) {
  return dirExcludes.some(e => {
    const name = e.replace(/\/$/, '')
    return name && (relPath === name || relPath.startsWith(`${name}/`))
  })
}

function basename(relPath) {
  const parts = String(relPath).split('/')
  return parts[parts.length - 1]
}

export async function startSync(project, direction, specificPaths = []) {
  if (!syncCheckEnabled.value) {
    Toast.fire({ icon: 'warning', title: 'Sync check is off' })
    return
  }
  if (projectRuntime.value[project.id]?.syncing) {
    Toast.fire({ icon: 'warning', title: `${project.name} is syncing, please wait` })
    return
  }

  projectRuntime.value[project.id] = { ...projectRuntime.value[project.id], syncing: true }
  const isDryRun = project.dry_run

  // Save previous log state so cancel can restore it instead of forcing-close
  const prevLogProjectId = activeLogProjectId.value
  const prevLogExpanded = isLogExpanded.value

  activeLogProjectId.value = project.id
  isLogExpanded.value = true
  if (!projectLogs.value[project.id]) projectLogs.value[project.id] = []
  projectLogs.value[project.id] = []

  const isDeleteOp = !isDryRun && specificPaths.length === 0 &&
    ((direction === 'push' && project.delete_on_push) || (direction === 'pull' && project.delete_on_pull))

  const abortSync = () => {
    projectRuntime.value[project.id] = { ...projectRuntime.value[project.id], syncing: false }
    activeLogProjectId.value = prevLogProjectId
    isLogExpanded.value = prevLogExpanded
  }

  if (isDeleteOp) {
    appendLog(project.id, `>>> Checking files at risk for ${direction.toUpperCase()} --delete...`)
    let deleteList = []
    let previewFailed = false
    try {
      deleteList = await invoke('get_sync_delete_preview', { project, direction })
    } catch (e) {
      previewFailed = true
    }

    if (previewFailed) {
      const { isConfirmed: proceedAnyway } = await Swal.fire({
        title: 'Không thể kiểm tra file sẽ bị xóa',
        html: `Kết nối tới remote thất bại khi preview --delete.<br>Bạn có muốn tiếp tục <b>${direction.toUpperCase()} --delete</b> mà không biết file nào sẽ bị xóa không?`,
        icon: 'error',
        showCancelButton: true,
        confirmButtonColor: '#ef4444',
        cancelButtonColor: '#374151',
        confirmButtonText: 'Vẫn tiếp tục (nguy hiểm)',
        cancelButtonText: 'Hủy bỏ',
        background: '#131317',
        color: '#F3F4F6',
      })
      if (!proceedAnyway) {
        abortSync()
        return
      }
    }

    if (deleteList.length > 0) {
      // Flow-app artifacts (e.g. REPORT.html) are routine churn, so ordinary --delete sync (which
      // has no newer/older awareness at all, in either direction - it deletes anything missing
      // from the source side, full stop) gets an exception carved out just for them: only
      // auto-approve when the copy about to be deleted has NOT been touched since this project's
      // last completed sync. If it was modified more recently than that, someone likely
      // regenerated it deliberately on the other side since - ask instead of silently wiping it.
      // Applies to both push and pull.
      const artifactEntries = deleteList.filter(f => FLOW_APP_ARTIFACTS.includes(basename(f)))
      if (artifactEntries.length > 0) {
        let staleArtifacts = []
        try {
          const info = await invoke('get_file_conflict_info', {
            localPath: project.local_path,
            remoteHost: project.remote_host,
            remotePath: project.remote_path,
            relPaths: artifactEntries,
          })
          const destMtime = (rel) => {
            const entry = info.find(f => f.rel_path === rel)
            if (!entry) return Infinity // couldn't verify - treat as fresh, ask
            return direction === 'push' ? entry.remote_mtime : entry.local_mtime
          }
          const lastSync = project.last_sync_time || 0
          staleArtifacts = artifactEntries.filter(f => destMtime(f) <= lastSync)
        } catch (err) {
          console.error('Flow-app artifact mtime check failed, asking to be safe:', err)
        }
        if (staleArtifacts.length > 0) {
          appendLog(project.id, `>>> Auto-approved ${staleArtifacts.length} deletion(s) of flow-app artifact(s) unchanged since last sync (${FLOW_APP_ARTIFACTS.join(', ')})`)
        }
        // Fresh artifacts (modified since last sync) are left in deleteList - they fall through
        // to the normal confirm dialog below like any other at-risk file.
        deleteList = deleteList.filter(f => !staleArtifacts.includes(f))
      }
    }

    if (deleteList.length > 0 && direction === 'push') {
      // R3: deletions confined to a push-only dir (e.g. `.git/`) auto-approve  - 
      // the caller already opted out of pulling that dir, so its churn isn't "at risk" data.
      const pushOnly = pushOnlyDirs(project)
      const autoApproved = deleteList.filter(f => matchesDirExclude(f, pushOnly))
      const needsConfirm = deleteList.filter(f => !matchesDirExclude(f, pushOnly))
      if (autoApproved.length > 0) {
        appendLog(project.id, `>>> Auto-approved ${autoApproved.length} deletion(s) in push-only paths (${pushOnly.join(', ')})`)
      }
      deleteList = needsConfirm
    }

    if (deleteList.length > 0) {
      const dest = direction === 'push' ? 'Remote' : 'Local'
      const fullFileList = deleteList.map(f => `  ${f}`).join('\n')
      const { isConfirmed } = await Swal.fire({
        title: `XÁC NHẬN: ${deleteList.length} FILE SẼ BỊ XÓA`,
        width: '560px',
        html:
          `<b>${direction.toUpperCase()} --delete</b> sẽ xóa vĩnh viễn <b>${deleteList.length}</b> file(s) chỉ tồn tại trên <b>${dest}</b> (không có ở phía nguồn):<br>` +
          `<pre style="text-align:left;font-size:11px;line-height:1.5;background:#0a0f16;padding:10px;border-radius:6px;max-height:240px;overflow-y:auto;margin:10px 0;white-space:pre;word-break:break-all;border:1px solid #1f2937;color:#e5e7eb;">${fullFileList}</pre>` +
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
          return val
        }
      })
      if (!isConfirmed) {
        abortSync()
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
    })
    project.last_sync_action = actionName + (isDryRun ? " (Dry)" : "")
    project.last_sync_time = Math.floor(Date.now() / 1000)
    project.last_sync_host = project.remote_host
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
      title: `Select files to push - ${project.name}`,
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
      title: `${outsideProject.length} file(s) outside project path - skipped`,
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
      conflicts = info.filter(f => {
        if (!f.remote_exists) return false
        // Flow-app artifacts only deserve a prompt when the destination copy is newer than the
        // source - i.e. pushing would clobber a report regenerated on the remote in the meantime.
        if (FLOW_APP_ARTIFACTS.includes(basename(f.rel_path)) && f.remote_mtime <= f.local_mtime) {
          return false
        }
        return true
      })
    } catch (err) {
      console.error('Conflict check failed:', err)
      Toast.fire({ icon: 'error', title: 'Không thể kiểm tra conflict với remote - hủy push' })
      return
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
