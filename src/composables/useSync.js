import { invoke } from '../utils/tauri'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { projectRuntime, Toast } from '../store/projectStore'
import { syncCheckEnabled } from '../store/syncCheckStore'
import { askConfirm } from '../store/dialogStore'
import { useLogs } from './useLogs'
import { projectPathError, projectDisplayName } from './useProjectConfig'
import { getProjectConfigEntry } from '../store/projectConfigStore'
import { fetchGitStatus } from './useGit'
import { offerDeployAfterPush, resolveDeployCmd } from './useDeploy'
import { shouldOfferDeployAfterPush } from './projectConfigPure'

const { appendGlobalLog, appendLog, projectLogs, activeLogProjectId, isLogExpanded } = useLogs()

// App-managed artifacts: routine sync churn on these is expected and skips confirm dialogs.
const FLOW_APP_ARTIFACTS = ['REPORT.html']

// Auto-close timer handle to collapse the log panel after sync completion.
let logCloseTimer = null

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

/** N-e: the (project, host) pair's own last sync, never the newest-across-hosts figure `project.last_sync_time` carries (see call site). */
async function activeHostLastSyncTime(project) {
  if (!project.remote_host) return 0
  try {
    const entry = await invoke('read_last_sync_for_host', { projectId: project.id, host: project.remote_host })
    return entry?.time || 0
  } catch (e) {
    console.error('[sync] read_last_sync_for_host failed', e)
    return 0
  }
}

/** A dry run changes nothing on either side, so it must not overwrite the record of the last real transfer (the in-memory fields still show it until the next Refresh). */
function persistLastSync(project) {
  invoke('write_last_sync', {
    projectId: project.id,
    host: project.remote_host,
    action: project.last_sync_action,
    time: project.last_sync_time,
    status: project.last_sync_status,
  }).catch((e) => console.error('write_last_sync failed:', e))
}

/** Has a real transfer to this (project, host) pair ever succeeded? A dry run moves nothing and a failed run proves nothing, so neither counts (a dry run of a wrong host must not switch the first-transfer confirm off). `null` = unknown (read failed) - callers treat it as "no". */
async function hostHasSyncHistory(project) {
  try {
    const entry = await invoke('read_last_sync_for_host', { projectId: project.id, host: project.remote_host })
    return !!entry && entry.status === 'success' && !entry.action.includes('(Dry)')
  } catch (e) {
    console.error('[sync] read_last_sync_for_host failed', e)
    return null
  }
}

export async function startSync(project, direction, specificPaths = []) {
  if (!syncCheckEnabled.value) {
    Toast.fire({ icon: 'warning', title: 'Sync check is off' })
    return
  }
  if (projectRuntime.value[project.id]?.syncing) {
    Toast.fire({ icon: 'warning', title: `${projectDisplayName(project)} is syncing, please wait` })
    return
  }

  // Boundary guard (item 2, docs/plan/done/settings-and-state-layout.md): sync must never run against a
  // project whose project.json is not the authoritative 'ok' - a missing/unavailable/corrupt read means
  // the excludes on `project` may be an empty or stale registry leftover, which must never reach rsync
  // (the plan's own "never a defaulted struct" contract). This is the actual funnel both PUSH/PULL button
  // clicks and a phone-mirrored dispatch go through, so it holds even when the UI-level disable (button
  // `:disabled`) is bypassed.
  if (getProjectConfigEntry(project.id).status !== 'ok') {
    Toast.fire({ icon: 'error', title: `"${projectDisplayName(project)}" settings (.akidevsync/project.json) are not ready - cannot sync yet` })
    return
  }

  // Guard against empty local/remote paths turning rsync into an operation on root.
  const pathError = projectPathError(project)
  if (pathError) {
    Toast.fire({ icon: 'error', title: pathError })
    return
  }

  projectRuntime.value[project.id] = { ...projectRuntime.value[project.id], syncing: true }
  // Specific path sync (e.g. SELECT push) bypasses dry-run setting.
  const isDryRun = specificPaths.length > 0 ? false : !!project.dry_run

  // Save previous log state so abortSync can restore it rather than force-closing.
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

  // First real transfer between this project and this host: name the destination and ask. Both previews
  // below are blind here - an empty or missing destination yields empty lists - and the host is a table
  // dropdown one click from any other box (docs/research/sync-host-safety.md).
  if (!isDryRun && !(await hostHasSyncHistory(project))) {
    const from = direction === 'push' ? 'this Mac' : `${project.remote_host}:${project.remote_path}`
    const to = direction === 'push' ? `${project.remote_host}:${project.remote_path}` : project.local_path
    const answer = await askConfirm({
      kind: 'confirm',
      title: `First ${direction.toUpperCase()} to ${project.remote_host}?`,
      html: `"${escHtml(projectDisplayName(project))}" has never synced with <b>${escHtml(project.remote_host)}</b>.<br>` +
        `<b>${direction.toUpperCase()}</b> ${escHtml(from)} → <b class="u-select-text">${escHtml(to)}</b>` +
        ((direction === 'push' ? project.delete_on_push : project.delete_on_pull) && specificPaths.length === 0 ? '<br>Mirror mode: destination files absent from the source will be deleted.' : ''),
      icon: 'warning',
      confirmButtonText: `${direction.toUpperCase()} to ${project.remote_host}`,
      cancelButtonText: 'Cancel',
    })
    if (!answer?.confirmed) {
      abortSync()
      return
    }
  }

  if (isDeleteOp) {
    appendLog(project.id, `>>> Checking files at risk for ${direction.toUpperCase()} --delete...`)
    let deleteList = []
    let overwriteList = []
    let previewFailed = false
    try {
      deleteList = await invoke('get_sync_delete_preview', { project, direction })
    } catch (e) {
      previewFailed = true
    }
    try {
      // Mirror mode drops `-u` (build_rsync_args), so it also silently overwrites any file the
      // destination currently holds a newer/equal copy of - a second destructive effect `--delete`
      // preview never sees (that one only catches destination-only files, not destination-newer ones).
      overwriteList = await invoke('get_sync_overwrite_preview', { project, direction })
    } catch (e) {
      previewFailed = true
    }

    // When either preview fails, fall through to require confirmation since at-risk files are unknown.

    if (deleteList.length > 0) {
      // Auto-approve deletion of flow-app artifacts only if untouched on destination since last sync.
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
          // The ACTIVE host's own last sync, never `project.last_sync_time` - that field is hydrated
          // as the newest last_sync.json across every host the project has ever used (correct for the
          // table's last-action cell, wrong here: a more recent sync to a DIFFERENT host must not suppress
          // a delete confirmation for the host we are about to sync with now).
          const lastSync = await activeHostLastSyncTime(project)
          staleArtifacts = artifactEntries.filter(f => destMtime(f) <= lastSync)
        } catch (err) {
          console.error('Flow-app artifact mtime check failed, asking to be safe:', err)
        }
        if (staleArtifacts.length > 0) {
          appendLog(project.id, `>>> Auto-approved ${staleArtifacts.length} deletion(s) of flow-app artifact(s) unchanged since last sync (${FLOW_APP_ARTIFACTS.join(', ')})`)
        }
        // Fresh artifacts modified after last sync remain in deleteList for manual confirmation.
        deleteList = deleteList.filter(f => !staleArtifacts.includes(f))
      }
    }

    if ((deleteList.length > 0 || overwriteList.length > 0) && direction === 'push') {
      // Auto-approve deletions AND overwrites in push-only paths (e.g. .git/) since destination isn't
      // pulled back - the same "local stays authoritative there" assumption PULL's own exclude already
      // makes, applied symmetrically instead of only to the deletion half.
      const pushOnly = pushOnlyDirs(project)
      const autoApprovedDeletes = deleteList.filter(f => matchesDirExclude(f, pushOnly))
      const autoApprovedOverwrites = overwriteList.filter(f => matchesDirExclude(f, pushOnly))
      deleteList = deleteList.filter(f => !matchesDirExclude(f, pushOnly))
      overwriteList = overwriteList.filter(f => !matchesDirExclude(f, pushOnly))
      const autoApprovedCount = autoApprovedDeletes.length + autoApprovedOverwrites.length
      if (autoApprovedCount > 0) {
        appendLog(project.id, `>>> Auto-approved ${autoApprovedCount} change(s) in push-only paths (${pushOnly.join(', ')})`)
      }
    }

    if (previewFailed || deleteList.length > 0 || overwriteList.length > 0) {
      const dest = direction === 'push' ? 'Remote' : 'Local'
      // HTML-escape filenames and project name to prevent markup injection in dialog.
      // The display name, never the raw registry `.name` (empty for an 'ok'-status project once
      // project-owned fields live in project.json) - an empty requireText would trivially match an empty
      // typed answer, defeating the confirmation entirely.
      const confirmName = projectDisplayName(project)
      const renderList = (list) => list.map(f => `  ${escHtml(f)}`).join('\n')
      const listBlock = (list) =>
        `<pre style="text-align:left;font-size:11px;line-height:1.5;background:#0a0f16;padding:10px;border-radius:6px;max-height:240px;overflow-y:auto;margin:10px 0;white-space:pre;word-break:break-all;border:1px solid #1f2937;color:#e5e7eb;">${renderList(list)}</pre>`
      const safeName = escHtml(confirmName)
      const totalAtRisk = deleteList.length + overwriteList.length
      const body = previewFailed
        ? `The remote could not be checked, so it is <b>unknown</b> which files are at risk.<br>` +
          `<b>${direction.toUpperCase()} --delete</b> will still permanently delete every file that exists only on <b>${dest}</b>, and silently overwrite any file <b>${dest}</b> currently holds a newer copy of.<br>`
        : (deleteList.length > 0
            ? `<b>${direction.toUpperCase()} --delete</b> will permanently delete <b>${deleteList.length}</b> file(s) that exist only on <b>${dest}</b> (absent from the source side):<br>` + listBlock(deleteList)
            : '') +
          (overwriteList.length > 0
            ? `<b>${direction.toUpperCase()}</b> will silently <b>overwrite ${overwriteList.length}</b> file(s) with older content - <b>${dest}</b> currently holds a newer/different copy that would be lost:<br>` + listBlock(overwriteList)
            : '')
      // Mirrored confirmation via dialogStore allows either host or companion to confirm.
      const answer = await askConfirm({
        kind: 'typed',
        title: previewFailed
          ? 'CONFIRM: IT IS UNKNOWN WHICH FILES ARE AT RISK'
          : `CONFIRM: ${totalAtRisk} FILE(S) AT RISK (${deleteList.length} to delete, ${overwriteList.length} to overwrite)`,
        width: '560px',
        // u-select-text allows copying the project name despite global user-select: none.
        html: body + `Type the project name <b class="u-select-text">${safeName}</b> to confirm:`,
        icon: previewFailed ? 'error' : 'warning',
        confirmButtonColor: '#ef4444',
        cancelButtonColor: '#374151',
        confirmButtonText: `Confirm ${direction.toUpperCase()}`,
        cancelButtonText: 'Cancel',
        inputPlaceholder: confirmName,
        // requireText validates exact project name match regardless of UI label translation.
        requireText: confirmName,
        mismatchText: `Type "${confirmName}" exactly to confirm`,
      })
      // Authoritative host validation of confirmation response.
      const typedOk = !!answer && answer.confirmed && answer.typed === confirmName
      if (!typedOk) {
        abortSync()
        return
      }
    }
    projectLogs.value[project.id] = []
  }

  let actionName = direction.toUpperCase()
  if (specificPaths.length === 1 && specificPaths[0] === ".git/") actionName = "SYNC GIT"
  else if (specificPaths.length > 0) actionName = "PUSH SPECIAL"

  appendLog(project.id, `>>> START SYNC [${actionName}] - ${projectDisplayName(project)}`)
  if (specificPaths.length > 0) {
    appendLog(project.id, `>>> TARGET: Partial Sync on ${specificPaths.length} specific item(s)`)
  }

  const dryText = isDryRun ? " (Dry Run)" : ""
  appendGlobalLog("SYNC", `Started ${actionName} for "${projectDisplayName(project)}"${dryText}`)

  try {
    await invoke("run_sync", {
      project,
      direction,
      dryRun: isDryRun,
      specificPaths,
    })
    // B2 (docs/plan/done/settings-and-state-layout.md § B2): persisted straight into
    // state/<id>/<host>/last_sync.json - the ONE persisted source now (sync_state.rs) - rather than onto
    // the registry object for save_projects to carry. The in-memory fields are still set for immediate
    // UI feedback (ProjectTable's last-action cell, the auto-approval read below); the save funnel strips
    // them before every save so projects.json never re-accumulates them (1.13.0 sync_git lesson).
    project.last_sync_action = actionName + (isDryRun ? " (Dry)" : "")
    project.last_sync_time = Math.floor(Date.now() / 1000)
    project.last_sync_host = project.remote_host
    project.last_sync_status = "success"
    if (!isDryRun) persistLastSync(project)
    fetchGitStatus(project.id)

    if (!isDryRun && specificPaths.length === 0) {
      if (direction === 'push') {
        const isMirror = project.delete_on_push
        projectRuntime.value[project.id] = {
          ...projectRuntime.value[project.id],
          hasPendingPush: false, pushCount: 0,
          ...(isMirror ? { hasPendingPull: false, pullCount: 0 } : {}),
        }
        // Deploy plan § "Always ask first": a non-dry push success is the ONE moment `on_push` offers the
        // confirm dialog - `shouldOfferDeployAfterPush` (projectConfigPure.js) is the pure, tested decision.
        // The command is resolved once, here, via `resolveDeployCmd` (the one impure caller of the pure
        // `getDeployCmd`) and handed to both the decision and the offer - neither re-derives it.
        const deployCmd = resolveDeployCmd(project)
        if (shouldOfferDeployAfterPush({ direction, isDryRun, specificPaths, project, deployCmd })) {
          offerDeployAfterPush(project, deployCmd).catch((e) => console.error('[useDeploy] offerDeployAfterPush failed', e))
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
      // 1.5s auto-close timer; verifies activeLogProjectId hasn't switched before collapsing.
      const closingProjectId = project.id
      if (logCloseTimer) clearTimeout(logCloseTimer)
      logCloseTimer = setTimeout(() => {
        logCloseTimer = null
        if (activeLogProjectId.value !== closingProjectId) return
        isLogExpanded.value = false
        activeLogProjectId.value = null
      }, 1500)
    }

    Toast.fire({ icon: 'success', title: isDryRun ? 'Dry run complete' : 'Sync complete' })
  } catch (err) {
    // Keep log panel open on error so user can inspect failure output.
    appendLog(project.id, `\n[ERROR] Sync failed: ${err}`)
    appendGlobalLog("ERROR", `Sync failed for "${projectDisplayName(project)}": ${err}`)
    project.last_sync_action = actionName + (isDryRun ? " (Dry)" : "")
    project.last_sync_time = Math.floor(Date.now() / 1000)
    project.last_sync_host = project.remote_host
    project.last_sync_status = "error"
    if (!isDryRun) persistLastSync(project)
    Toast.fire({ icon: 'error', title: 'Sync failed' })
  } finally {
    projectRuntime.value[project.id] = { ...projectRuntime.value[project.id], syncing: false }
  }
}

/**
 * SELECT: opens native OS file dialog, checks for remote conflicts, then pushes selected files.
 *
 * HOST-ONLY, like `startSync`: the native file picker is the Mac's, and the overwrite confirm below
 * is a mirrored dialog (`askConfirm` writes host state and only the host can resolve its waiter).
 * A companion reaches it through `remoteActions.requestSelectPush`, never by calling this directly.
 */
export async function openSelectDialog(project) {
  // Guard against empty paths before resolving relative paths and opening dialog.
  const pathError = projectPathError(project)
  if (pathError) {
    Toast.fire({ icon: 'error', title: pathError })
    return
  }

  let selected
  try {
    selected = await openDialog({
      title: `Select files to push - ${projectDisplayName(project)}`,
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
        // Flow-app artifacts prompt only when destination is newer than source.
        if (FLOW_APP_ARTIFACTS.includes(basename(f.rel_path)) && f.remote_mtime <= f.local_mtime) {
          return false
        }
        return true
      })
    } catch (err) {
      console.error('Conflict check failed:', err)
      Toast.fire({ icon: 'error', title: 'Could not check the remote for conflicts - push cancelled' })
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

    // Mirrored overwrite confirmation dialog via dialogStore.
    const answer = await askConfirm({
      kind: 'confirm',
      title: `${conflicts.length} file(s) already exist on the remote`,
      html:
        `<p style="font-size:12px;margin:0 0 10px">Push will overwrite these files:</p>` +
        `<div style="overflow-x:auto">` +
        `<table style="width:100%;border-collapse:collapse;font-size:12px">` +
        `<thead><tr>
          <th style="text-align:left;padding:4px 8px;border-bottom:1px solid #374151;font-size:10px;color:#9ca3af">FILE</th>
          <th style="padding:4px 8px;border-bottom:1px solid #374151;font-size:10px;color:#9ca3af">LOCAL</th>
          <th style="padding:4px 8px;border-bottom:1px solid #374151;font-size:10px;color:#f59e0b">REMOTE</th>
        </tr></thead>` +
        `<tbody>${rows}</tbody></table></div>`,
      icon: 'warning',
      confirmButtonColor: '#f59e0b',
      cancelButtonColor: '#374151',
      confirmButtonText: 'Overwrite & Push',
      cancelButtonText: 'Cancel',
    })
    if (!answer || !answer.confirmed) return
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
