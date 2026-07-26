// Seam A entry points — the write side of remote control (docs/plan/done/remote-control.md §3, R-2).
//
// Why this module exists: the mirror (seam S) already carries host state to the phone READ-only.
// To let a companion *act*, its button must not mutate its own mirrored copy — it must ask the
// host to run the real thing. `action()` (services/action.js) does exactly that, but it can only
// discover functions EXPORTED FROM `src/store/*.js` (the glob both seams share). The real
// orchestration lives in composables (`useSync.startSync`, `useBackgroundRefresh.refreshProject`),
// which the glob never sees. This file is the thin store-level shim that exposes them as
// intent-able actions, one per user gesture.
//
// ID, never object (ACT-1 corollary): every action takes a project **id**, not the project
// object. A companion serializes its args to JSON over the wire, so passing the object would ship
// a DETACHED copy the host would then mutate — its writes would never land back in the reactive
// `projects` array and never mirror anywhere. Taking the id and re-resolving the LIVE project on
// the host is what keeps `startSync`'s in-place mutations (`last_sync_time`, …) reactive. On the
// host these wrappers are `action(fn) === fn` (zero overhead), so host behaviour is byte-identical
// to calling the composable directly — the id→object hop is the only change, and it resolves to
// the same live object the component was already passing.
import { action } from '../services/action'
import { invoke } from '../utils/tauri'
import { projects, projectRuntime, bumpEpoch, Toast } from './projectStore'
import { sshHosts, hasSshUndo, hasSshRedo } from './sshStore'
import { globalLogs } from './logStore'
import { askConfirm } from './dialogStore'
import { startSync, openSelectDialog } from '../composables/useSync'
import { refreshProject, refreshAllProjects } from '../composables/useBackgroundRefresh'
import { saveProjectsList } from '../composables/useProjectConfig'

function byId(id) {
  return projects.value.find((p) => p.id === id) || null
}

// Matches useLogs().appendGlobalLog's line format without dragging the (Tauri-listener-owning)
// useLogs composable into this eagerly-globbed store module — globalLogs is a leaf store ref.
function logSsh(message) {
  globalLogs.value.push(`[${new Date().toLocaleTimeString()}] [SSH] ${message}`)
}

/** PUSH / PULL a project. `startSync` owns every guard (sync-check off, already-syncing, the
 *  --delete confirm dialog) — those run on the HOST when a companion's intent is dispatched. The
 *  --delete confirm is mirrored via dialogStore (plan §3), so it now appears and is answerable on
 *  BOTH the Mac and the phone that triggered the sync, not just the Mac. */
export const requestSync = action('remoteActions.requestSync', (id, direction) => {
  const project = byId(id)
  if (!project) {
    console.warn('[remoteActions] requestSync: no project for id', id)
    return
  }
  return startSync(project, direction)
})

/** SELECT → push specific files. Same reason as requestSync: everything `openSelectDialog` does is
 *  host-work — the native file picker is the Mac's, the conflict stat runs against the Mac's disk,
 *  and its overwrite confirm is a mirrored dialog, which only the host can ask and resolve. Without
 *  this seam a phone's SELECT click ran the whole flow on the phone, where the popup blocked off the
 *  host's call stack and the sync could never be answered — the 1.20.0 spinner bug, same shape. */
export const requestSelectPush = action('remoteActions.requestSelectPush', (id) => {
  const project = byId(id)
  if (!project) {
    console.warn('[remoteActions] requestSelectPush: no project for id', id)
    return
  }
  return openSelectDialog(project)
})

/** Log out this Mac's Antigravity session (IDE / desktop / CLI). Host-only for the same reason:
 *  `logout_antigravity*` act on the Mac, and the confirm is a mirrored dialog. Returns whether the
 *  logout actually ran, so the clicking screen only reports success on a real one — on a companion
 *  the action stub resolves to `undefined`, which must not be read as "logged out". */
export const requestAgLogout = action('remoteActions.requestAgLogout', async (sourceType) => {
  const isIde = sourceType === 'ide'
  const isCli = sourceType === 'cli'
  const isDesktop = sourceType === 'desktop' || sourceType === 'desktop_cli'

  let title = 'Đăng xuất IDE?'
  let html = 'Ứng dụng sẽ tự đóng và xoá phiên đăng nhập hiện tại.<br>Settings, extension, rule và permission vẫn được giữ nguyên.'
  if (isDesktop) {
    title = 'Đăng xuất AG?'
    html = 'Ứng dụng sẽ dừng các tiến trình và xoá phiên đăng nhập hiện tại.<br>Lịch sử hội thoại và cấu hình vẫn được giữ nguyên.'
  } else if (isCli) {
    title = 'Đăng xuất CLI (Terminal)?'
    html = 'Ứng dụng sẽ dừng các tiến trình CLI và xoá phiên đăng nhập hiện tại.<br>Lịch sử hội thoại và cấu hình vẫn được giữ nguyên.'
  }

  const answer = await askConfirm({
    kind: 'confirm',
    title,
    html,
    icon: 'warning',
    confirmButtonColor: '#ef4444',
    cancelButtonColor: '#374151',
    confirmButtonText: 'Đăng xuất',
    cancelButtonText: 'Hủy',
  })
  if (!answer || !answer.confirmed) return false

  await invoke(isIde ? 'logout_antigravity' : 'logout_antigravity_cli')
  return true
})

/** Flip a project's DRY toggle and persist. Mutating the mirrored `projects` ref on the host is
 *  what pushes the new value back to every screen (incl. the phone that flipped it) through the
 *  mirror — the companion never writes its own copy. */
export const setDryRun = action('remoteActions.setDryRun', (id, value) => {
  const project = byId(id)
  if (!project) return
  project.dry_run = !!value
  saveProjectsList()
})

/** Refresh ONE project's status (git + remote diff + stack). */
export const requestRefresh = action('remoteActions.requestRefresh', (id) => {
  const project = byId(id)
  if (!project) return
  refreshProject(project)
})

/** Refresh every project — the header's global refresh. */
export const requestRefreshAll = action('remoteActions.requestRefreshAll', () => {
  refreshAllProjects()
})

/** Apply a task-list/notes edit to exactly ONE project by id (PERSIST-1,
 *  docs/plan/1.20.0-terminal-and-remote-sync.md §2 / CLAUDE.md multi-entity regression guard).
 *  Only the fields actually present in `patch` are assigned — this must NEVER become
 *  `projects.value = patch` or otherwise touch any project besides the one resolved by `id`.
 *
 *  This is the fix for the reported "task note reverts after a while" bug: before this, a
 *  companion's edit reached disk via a bare `invoke('save_projects', {projects: projects.value})`
 *  (the old call sites in useProjectTasks.js/ProjectTasksModal.vue) — that shipped the
 *  COMPANION's whole array to disk but never touched the HOST's reactive `projects` ref, so the
 *  next broadcastFull() (fired on every phone reconnect) replayed the host's stale copy straight
 *  back over the edit. Resolving the live project by id and assigning only `patch`'s fields keeps
 *  the mutation on the host's own reactive object, so it mirrors out AND persists correctly. */
export const applyTaskEdit = action('remoteActions.applyTaskEdit', (projectId, patch) => {
  const p = byId(projectId)
  if (!p || !patch) return
  if (patch.tasks !== undefined) p.tasks = patch.tasks
  if (patch.notes !== undefined) p.notes = patch.notes
  return saveProjectsList()
})

/** Reorder the WHOLE project list (drag-and-drop in ProjectTable.vue). This is the one
 *  legitimate whole-array write left: a reorder has no per-entity payload — the id list itself
 *  IS the new order, no project's own fields change. Ids not already present in `projects.value`
 *  are dropped rather than trusted from the wire; if the set doesn't match 1:1 (stale companion
 *  payload racing a project add/remove) the whole reorder is skipped rather than silently
 *  dropping a project — never call `projects.value = <a shorter list>`. */
export const reorderProjects = action('remoteActions.reorderProjects', (orderedIds) => {
  if (!Array.isArray(orderedIds)) return
  const byIdMap = new Map(projects.value.map((p) => [p.id, p]))
  const reordered = orderedIds.map((id) => byIdMap.get(id)).filter(Boolean)
  if (reordered.length !== projects.value.length) return
  projects.value = reordered
  return saveProjectsList()
})

/** Create-or-update a project's config from the modal's edited data (ACT-1 / matrix "Config save").
 *  Takes a PLAIN edited project (not a live ref): the companion serialises it over the intent wire,
 *  and the host re-applies it into its OWN reactive `projects` array — which is what makes the Mac
 *  UI update live and mirror the change back to every screen. Before this, `saveConfig` mutated the
 *  phone's copy and only `invoke('save_projects')`-persisted to disk, so the Mac stayed stale until
 *  a reload re-read disk (the reported bug). UI-only bits (Toast, closeModal, url-normalise) stay in
 *  `saveConfig` on the clicker. Returns the persist promise so the host caller can await it.
 *  On the host `action(fn) === fn`, so this is byte-identical to the old inline mutation. */
export const applyProjectConfig = action('remoteActions.applyProjectConfig', (plain) => {
  if (!plain || !plain.id) return
  const index = projects.value.findIndex((p) => p.id === plain.id)
  const isNew = index === -1

  if (!isNew) {
    const prev = projects.value[index]
    // Host or local-path change invalidates any in-flight status check for the OLD identity —
    // bump the epoch (discards stale results, clears the busy indicator) and blank the pending
    // push/pull counts, which were measured against the old host. Never touches a live rsync.
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
    }
  } else {
    projectRuntime.value[plain.id] = {
      git_status: '...',
      git_log: '',
      remote_url: '',
      syncing: false,
      epoch: 0,
      refreshCount: 0,
    }
    projects.value.push({ ...plain })
  }

  const persist = saveProjectsList()
  const saved = projects.value.find((p) => p.id === plain.id)
  if (saved) refreshProject(saved)
  return persist
})

/** Remove a project from the list (matrix "Remove project"). Same reason as applyProjectConfig:
 *  the deletion must happen on the host's reactive `projects`, not the phone's copy. The confirm
 *  dialog is mirrored separately, see requestRemoveProject below. */
export const removeProject = action('remoteActions.removeProject', (id) => {
  if (!id) return
  projects.value = projects.value.filter((p) => p.id !== id)
  // Dropping the runtime entry also cancels any in-flight status check for this id (currentEpoch
  // then reports 0, which never matches the >=1 epoch a check captured) — see projectStore.
  delete projectRuntime.value[id]
  return saveProjectsList()
})

/** Host-side confirm+remove for the Config modal's "Remove Project?" button (mirrored dialog,
 *  plan §3). `editingProject` (useProjectConfig.js) is a plain composable ref, not one of the
 *  mirrored `src/store/*.js` refs (R-1 in remote-control.md, still deferred) — it can hold a
 *  different value per screen, so the CLICKING screen resolves id+name from its OWN local
 *  `editingProject` first (still correct there, since it's the same screen that has the modal
 *  open) and ships them as plain args. The confirm dialog itself is then asked, answered, and
 *  (on Yes) executed entirely on the host, exactly like every other mirrored dialog — this is
 *  what makes the dialog visible/answerable from whichever screen is looking, not just the
 *  clicker. Returns whether it actually removed, so the caller (confirmRemove) only closes/logs
 *  on a real removal — never on a stale action-stub resolution (see confirmRemove's comment). */
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

/** Reconcile after a `~/.ssh/config` write (save / undo / redo) — the HOST side of the SSH modal
 *  (ACT-1 / matrix "SSH config → Save"). The RPCs that mutate the file (`save_ssh_config` etc.) ran
 *  on the clicker and already hit the Mac's disk; what MUST run on the host is the reactive fallout:
 *  re-reading the host list and the undo/redo availability, and migrating any project pinned to a
 *  now-missing host. Before this, a companion set `sshHosts`/`hasSshUndo` on its OWN mirrored copy
 *  (never travels back) and mutated its OWN `projects` copy — so the Mac UI kept the stale host list
 *  and the un-migrated projects until a reload. `oldHosts` is read from the LIVE host `sshHosts`, so
 *  nothing has to cross the intent wire. The interactive missing-host replacement dialog (the
 *  many-to-many case) is asked via the mirrored dialogStore (plan §3), so it is visible and
 *  answerable from whichever screen is looking, not just the Mac. On the host action(fn)===fn,
 *  so behaviour is unchanged there. */
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
      affected.forEach((p) => { p.remote_host = newHost })
      needsSave = true
      logSsh(`Auto-migrated ${affected.length} projects from '${missingHost}' to '${newHost}'.`)
      Toast.fire({ icon: 'info', title: `Auto-migrated projects to '${newHost}'` })
    }
  } else {
    for (const missingHost of missingHosts) {
      const affected = projects.value.filter((p) => p.remote_host === missingHost)
      if (affected.length === 0) continue

      const inputOptions = {}
      newHosts.forEach((h) => { inputOptions[h] = h })

      // DialogHost needs a 'select' kind for this one (docs/plan/1.20.0-terminal-and-remote-sync.md
      // §3) — this whole function already runs host-side (applySshHostsChange is itself an
      // action, always dispatched via useSsh.js), so askConfirm here behaves identically to the
      // old direct call to the popup library, just mirrored to any companion screen too.
      const answer = await askConfirm({
        kind: 'select',
        title: '⚠️ SSH Host Missing',
        html: `Host <b>${missingHost}</b> no longer exists in SSH config, but is used by <b>${affected.length} project(s)</b>.<br><br>Select a replacement host to update them automatically:`,
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
