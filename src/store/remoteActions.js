// Seam A entry points: write side of remote control (docs/plan/done/remote-control.md §3, R-2).
// PERSIST-1: notes.json mutations serialize on host (docs/plan/done/1.22.0-notes-json-ssot.md & 1.20.0-terminal-and-remote-sync.md §2).
// Action dispatching resolves live host reactive objects by ID instead of detached copies (docs/plan/done/1.20.1-flow-audit-fixes.md §3.6).
import { action } from '../services/action'
import { invoke } from '../utils/tauri'
import {
  projects,
  projectRuntime,
  bumpEpoch,
  Toast,
  refreshProjectIcons,
  iconTimestamp,
  markProjectRemoved,
  isProjectRemoved,
} from './projectStore'
import {
  setProjectNotesEntry,
  getProjectNotesEntry,
  dropProjectNotesEntry,
  bumpProjectNotesGeneration,
} from './projectNotesStore'
import { isProjectNotesWritable, refreshProjectNotes } from '../composables/useProjectNotes'
import { sshHosts, hasSshUndo, hasSshRedo } from './sshStore'
import { appendGlobalLogLines } from './logStore'
import { askConfirm } from './dialogStore'
import { startSync, openSelectDialog } from '../composables/useSync'
import { refreshProject, refreshAllProjects } from '../composables/useBackgroundRefresh'
import { saveProjectsList } from '../composables/useProjectConfig'

function byId(id) {
  return projects.value.find((p) => p.id === id) || null
}

// Appends through logStore funnel (counted against LOG_CAP, uses delta mirror) without importing useLogs.
function logSsh(message) {
  appendGlobalLogLines([`[${new Date().toLocaleTimeString()}] [SSH] ${message}`])
}

// Escapes user-authored strings (SSH hosts, project names) for HTML dialog bodies.
function escHtml(s) {
  return String(s ?? '').replace(/[&<>"']/g, (c) => (
    { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]
  ))
}

// Dispatches PUSH/PULL on host where sync guards and mirrored delete confirmations execute.
export const requestSync = action('remoteActions.requestSync', (id, direction) => {
  const project = byId(id)
  if (!project) {
    console.warn('[remoteActions] requestSync: no project for id', id)
    return
  }
  rememberSyncDirection(id, direction)
  return startSync(project, direction)
})

// Records active sync direction before startSync so UI renders STOP on the active button; mirrors via runtime.
function rememberSyncDirection(id, direction) {
  projectRuntime.value[id] = { ...projectRuntime.value[id], syncDirection: direction }
}

// Cancels rsync process group on host; surfaces failure or already-finished state via Toast.
export const requestCancelSync = action('remoteActions.requestCancelSync', async (id) => {
  const project = byId(id)
  if (!project) return
  try {
    // False indicates transfer finished before click; informs user so state is unambiguous.
    const killed = await invoke('cancel_sync', { projectId: id })
    if (killed === false) Toast.fire({ icon: 'info', title: `Nothing left to stop for "${project.name}"` })
  } catch (e) {
    Toast.fire({ icon: 'error', title: `Could not stop sync: ${String(e).replace('Error: ', '')}` })
  }
})

// SELECT push funnel: file picking, stat diffing, and overwrite dialogs execute on host.
export const requestSelectPush = action('remoteActions.requestSelectPush', (id) => {
  const project = byId(id)
  if (!project) {
    console.warn('[remoteActions] requestSelectPush: no project for id', id)
    return
  }
  rememberSyncDirection(id, 'push')
  return openSelectDialog(project)
})

// Updates DRY toggle on host reactive projects ref and persists to disk.
export const setDryRun = action('remoteActions.setDryRun', (id, value) => {
  const project = byId(id)
  if (!project) return
  project.dry_run = !!value
  saveProjectsList()
})

// Updates remote host: resets pending counts, bumps epoch to invalidate old diffs, and triggers refresh.
export const setRemoteHost = action('remoteActions.setRemoteHost', (id, host) => {
  const project = byId(id)
  if (!project || project.remote_host === host) return
  project.remote_host = host
  bumpEpoch(id)
  projectRuntime.value[id] = { ...projectRuntime.value[id], hasPendingPush: null, hasPendingPull: null }
  saveProjectsList()
  refreshProject(project)
})

// Refreshes single project status (git + remote diff + stack).
export const requestRefresh = action('remoteActions.requestRefresh', (id) => {
  const project = byId(id)
  if (!project) return
  refreshProject(project)
})

// Refreshes all projects (global header refresh).
export const requestRefreshAll = action('remoteActions.requestRefreshAll', () => {
  refreshAllProjects()
})

// Applies task/notes patch to ONE project by id, writing to `<local_path>/.akidevsync/notes.json` on host.
// Mutates host reactive store directly so updates mirror cleanly; returns `{ file, clobbered }` or null.
export const applyTaskEdit = action('remoteActions.applyTaskEdit', async (projectId, patch) => {
  const p = byId(projectId)
  if (!p || !patch) return null

  // Must reject before store mutation to avoid showing edits that cannot be persisted.
  if (!isProjectNotesWritable(projectId)) {
    Toast.fire({ icon: 'error', title: getProjectNotesEntry(projectId).error || 'Notes are not writable right now' })
    return null
  }

  // Synchronous optimistic update; bumping generation invalidates any in-flight reads.
  const before = getProjectNotesEntry(projectId)
  bumpProjectNotesGeneration(projectId)
  setProjectNotesEntry(projectId, {
    ...before,
    ...(patch.tasks !== undefined ? { tasks: patch.tasks } : {}),
    ...(patch.notes !== undefined ? { notes: patch.notes } : {}),
  })

  return queueNotesWrite(projectId, async () => {
    try {
      const res = await invoke('write_project_notes', {
        localPath: p.local_path,
        // `undefined` -> `null` over IPC instructs Rust to preserve existing on-disk field.
        notes: patch.notes !== undefined ? patch.notes : null,
        tasks: patch.tasks !== undefined ? patch.tasks : null,
        // Read inside queue after prior write completes to avoid false clobber warnings on rapid edits.
        baseUpdatedAt: getProjectNotesEntry(projectId).updated_at || null,
      })
      bumpProjectNotesGeneration(projectId)
      // Re-seed from what is actually on disk, not from what we hoped we wrote.
      setProjectNotesEntry(projectId, {
        status: 'ok',
        notes: res.file.notes,
        tasks: res.file.tasks,
        updated_at: res.file.updated_at,
        error: '',
      })
      if (res.clobbered) {
        Toast.fire({
          icon: 'warning',
          title: 'notes.json had newer changes on disk — yours replaced them (recoverable via git)',
        })
      }
      return res
    } catch (e) {
      // Retains optimistic value on write failure to prevent losing user input.
      Toast.fire({ icon: 'error', title: `Could not save notes: ${e}` })
      return null
    }
  })
})

// Per-project promise queue serialising disk writes so consecutive edits observe correct baseUpdatedAt.
const notesWriteChains = new Map()
function queueNotesWrite(projectId, run) {
  const prev = notesWriteChains.get(projectId) || Promise.resolve()
  // `.then(run, run)` — failed write must not poison chain for subsequent edits.
  const next = prev.then(run, run)
  notesWriteChains.set(projectId, next.catch(() => {}))
  return next
}

// Re-reads project notes file on host and mirrors result to companions.
export const requestProjectNotesRefresh = action('remoteActions.requestProjectNotesRefresh', (projectId) => {
  const p = byId(projectId)
  if (!p) return
  return refreshProjectNotes(p.id, p.local_path)
})

// Reorders projects by id list; validates 1:1 match against current projects to prevent dropping items.
export const reorderProjects = action('remoteActions.reorderProjects', (orderedIds) => {
  if (!Array.isArray(orderedIds)) return
  const byIdMap = new Map(projects.value.map((p) => [p.id, p]))
  const reordered = orderedIds.map((id) => byIdMap.get(id)).filter(Boolean)
  if (reordered.length !== projects.value.length) return
  projects.value = reordered
  return saveProjectsList()
})

// Creates or updates project in host reactive `projects` array and persists changes to disk.
export const applyProjectConfig = action('remoteActions.applyProjectConfig', async (plain) => {
  if (!plain || !plain.id) return
  const index = projects.value.findIndex((p) => p.id === plain.id)
  const isNew = index === -1

  // Rejects save if project was removed while modal was open, preventing accidental resurrection.
  if (isNew && isProjectRemoved(plain.id)) {
    Toast.fire({ icon: 'error', title: `"${plain.name}" was removed - not saved` })
    return { rejected: 'removed' }
  }

  if (!isNew) {
    const prev = projects.value[index]
    // Identity change invalidates in-flight status checks; bump epoch and reset pending diff counts.
    const identityChanged =
      prev.remote_host !== plain.remote_host || prev.local_path !== plain.local_path
    projects.value[index] = { ...plain }
    if (identityChanged) {
      bumpEpoch(plain.id)
      projectRuntime.value[plain.id] = {
        ...projectRuntime.value[plain.id],
        hasPendingPush: null,
        hasPendingPull: null,
      }
      // Reset notes to 'unknown' synchronously so writes are locked until new path notes are loaded.
      setProjectNotesEntry(plain.id, { status: 'unknown' })
      refreshProjectNotes(plain.id, plain.local_path)
    }
  } else {
    projectRuntime.value[plain.id] = {
      git_status: '...',
      git_log: '',
      remote_url: '',
      syncing: false,
      // Live project epoch is always >= 1; increment ensures monotonic epoch even if id was previously used.
      epoch: (projectRuntime.value[plain.id]?.epoch ?? 0) + 1,
      refreshCount: 0,
    }
    projects.value.push({ ...plain })
    // Seed 'unknown' status first to lock writes until notes are initially read from disk.
    setProjectNotesEntry(plain.id, { status: 'unknown' })
    refreshProjectNotes(plain.id, plain.local_path)
  }

  await saveProjectsList()
  const saved = projects.value.find((p) => p.id === plain.id)
  if (saved) refreshProject(saved)
  // Refreshes icon cache after disk save and bumps timestamp to bust webview image cache.
  await refreshProjectIcons()
  iconTimestamp.value = Date.now()
})

// Removes a project from reactive list, marks id removed, cleans runtime/notes entries, and persists.
export const removeProject = action('remoteActions.removeProject', (id) => {
  if (!id) return
  projects.value = projects.value.filter((p) => p.id !== id)
  // Records removed id so any open config modal cannot resurrect it.
  markProjectRemoved(id)
  // Deleting runtime entry cancels in-flight status checks (currentEpoch reports 0).
  delete projectRuntime.value[id]
  // Drops memory notes entry; on-disk repo file is intentionally preserved.
  dropProjectNotesEntry(id)
  return saveProjectsList()
})

// Mirrored removal confirmation dialog executed on host; returns boolean removal status.
export const requestRemoveProject = action('remoteActions.requestRemoveProject', async (id, name) => {
  const answer = await askConfirm({
    kind: 'confirm',
    title: 'Remove Project?',
    text: `Remove "${name}" from the app list? Your actual code files will NOT be touched.`,
    icon: 'warning',
    confirmButtonColor: '#ef4444',
    cancelButtonColor: '#374151',
    confirmButtonText: 'Yes, remove it',
  })
  if (!answer || !answer.confirmed) return false
  await removeProject(id)
  return true
})

// Reconciles SSH hosts on host: updates undo/redo state, detects missing hosts, and prompts migration.
export const applySshHostsChange = action('remoteActions.applySshHostsChange', async () => {
  const oldHosts = [...sshHosts.value]

  try {
    const status = await invoke('get_ssh_history_status')
    hasSshUndo.value = status.can_undo
    hasSshRedo.value = status.can_redo
  } catch (_) {
    hasSshUndo.value = false
    hasSshRedo.value = false
  }

  const newHosts = await invoke('get_ssh_hosts')
  sshHosts.value = newHosts

  const missingHosts = oldHosts.filter((h) => !newHosts.includes(h))
  const addedHosts = newHosts.filter((h) => !oldHosts.includes(h))
  if (missingHosts.length === 0) return

  let needsSave = false

  if (missingHosts.length === 1 && addedHosts.length === 1) {
    const missingHost = missingHosts[0]
    const newHost = addedHosts[0]
    const affected = projects.value.filter((p) => p.remote_host === missingHost)
    if (affected.length > 0) {
      // Prompts confirmation before repointing projects to prevent unintended --delete pushes to wrong server.
      const list = affected.map((p) => `<li>${escHtml(p.name)}</li>`).join('')
      const answer = await askConfirm({
        kind: 'confirm',
        title: 'SSH host renamed?',
        html:
          `Host <b>${escHtml(missingHost)}</b> is gone and <b>${escHtml(newHost)}</b> is new. ` +
          `Repoint these <b>${affected.length} project(s)</b> to <b>${escHtml(newHost)}</b>?` +
          `<ul style="text-align:left; margin: 8px auto 0; max-width: 320px;">${list}</ul>` +
          `<br>If it was not a rename, keep them as they are - a wrong host makes the next ` +
          `<b>--delete</b> PUSH mirror onto the wrong server.`,
        icon: 'warning',
        confirmButtonColor: '#f59e0b',
        cancelButtonColor: '#374151',
        confirmButtonText: `Repoint ${affected.length} project(s)`,
        cancelButtonText: `Keep '${missingHost}'`,
      })
      if (answer && answer.confirmed) {
        affected.forEach((p) => { p.remote_host = newHost })
        needsSave = true
        logSsh(`Repointed ${affected.length} projects from '${missingHost}' to '${newHost}' (user confirmed).`)
        Toast.fire({ icon: 'info', title: `Repointed projects to '${newHost}'` })
      } else {
        logSsh(`Kept ${affected.length} projects on '${missingHost}' - repoint to '${newHost}' declined.`)
      }
    }
  } else {
    for (const missingHost of missingHosts) {
      const affected = projects.value.filter((p) => p.remote_host === missingHost)
      if (affected.length === 0) continue

      const inputOptions = {}
      newHosts.forEach((h) => { inputOptions[h] = h })

      // Prompts selection dialog for replacement host across mirrored screens.
      const answer = await askConfirm({
        kind: 'select',
        title: '⚠️ SSH Host Missing',
        html: `Host <b>${escHtml(missingHost)}</b> no longer exists in SSH config, but is used by <b>${affected.length} project(s)</b>.<br><br>Select a replacement host to update them automatically:`,
        icon: 'warning',
        inputOptions,
        inputPlaceholder: '--- Select replacement host ---',
        confirmButtonText: 'Update',
        cancelButtonText: 'Skip',
        color: '#e2e8f0',
      })
      const newHost = answer && answer.confirmed ? answer.value : null

      if (newHost) {
        affected.forEach((p) => { p.remote_host = newHost })
        needsSave = true
        logSsh(`Migrated ${affected.length} projects from ${missingHost} to ${newHost}.`)
      }
    }
  }

  if (needsSave) {
    await saveProjectsList()
    if (missingHosts.length !== 1 || addedHosts.length !== 1) {
      Toast.fire({ icon: 'success', title: 'Projects updated with new hosts' })
    }
  }
})
