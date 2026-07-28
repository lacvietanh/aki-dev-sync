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

// Matches useLogs().appendGlobalLog's line format without dragging the (Tauri-listener-owning)
// useLogs composable into this eagerly-globbed store module. Goes through logStore's append funnel
// rather than pushing into the ref directly, so these lines are counted against LOG_CAP and ride the
// mirror's append-delta path instead of forcing a full-map resend.
function logSsh(message) {
  appendGlobalLogLines([`[${new Date().toLocaleTimeString()}] [SSH] ${message}`])
}

// Dialog bodies below are rendered as HTML, and host/project names are user-authored free text
// (`~/.ssh/config` entries, the project name field) - escape before interpolating.
function escHtml(s) {
  return String(s ?? '').replace(/[&<>"']/g, (c) => (
    { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]
  ))
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
  rememberSyncDirection(id, direction)
  return startSync(project, direction)
})

/** Which direction the row's running sync is going, recorded BEFORE `startSync` flips `syncing`
 *  (docs/plan/done/1.20.1-flow-audit-fixes.md §3.6). The row turns exactly that one button into STOP, so
 *  the control sits where the user's eyes already are; without this the UI cannot tell which of
 *  PUSH/PULL is the live one. Written at the two funnels every sync passes through (here and
 *  `requestSelectPush`), on the host, so it mirrors to the phone with the rest of the runtime.
 *  Touches ONLY this project's runtime entry. */
function rememberSyncDirection(id, direction) {
  projectRuntime.value[id] = { ...projectRuntime.value[id], syncDirection: direction }
}

/** Stop the rsync/ssh process group of ONE running sync (§3.6). Host-side: the process lives on the
 *  Mac, so a phone's STOP is an intent like every other button here.
 *
 *  Failure is LOUD by design — a user who hit STOP on a `--delete` mirror must never be left
 *  believing it stopped when it did not, so an unavailable/failing command surfaces through the
 *  existing Toast (UI Extreme Narrow: no new element) instead of a silent console line. */
export const requestCancelSync = action('remoteActions.requestCancelSync', async (id) => {
  const project = byId(id)
  if (!project) return
  try {
    // `cancel_sync` returns whether it actually killed anything: false means the transfer finished
    // between the render and the click. Say so - a silent STOP on a row that keeps spinning would
    // leave the user unsure whether a `--delete` mirror is still running.
    const killed = await invoke('cancel_sync', { projectId: id })
    if (killed === false) Toast.fire({ icon: 'info', title: `Nothing left to stop for "${project.name}"` })
  } catch (e) {
    Toast.fire({ icon: 'error', title: `Could not stop sync: ${String(e).replace('Error: ', '')}` })
  }
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
  rememberSyncDirection(id, 'push')
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
 *  docs/plan/done/1.20.0-terminal-and-remote-sync.md §2 / CLAUDE.md multi-entity regression guard).
 *  Only the fields actually present in `patch` are assigned — this must NEVER become
 *  `projects.value = patch` or otherwise touch any project besides the one resolved by `id`.
 *
 *  This is the fix for the reported "task note reverts after a while" bug: before this, a
 *  companion's edit reached disk via a bare `invoke('save_projects', {projects: projects.value})`
 *  (the old call sites in useProjectTasks.js/ProjectTasksModal.vue) — that shipped the
 *  COMPANION's whole array to disk but never touched the HOST's reactive `projects` ref, so the
 *  next broadcastFull() (fired on every phone reconnect) replayed the host's stale copy straight
 *  back over the edit. Resolving the live project by id and assigning only `patch`'s fields keeps
 *  the mutation on the host's own reactive object, so it mirrors out AND persists correctly.
 *
 *  SINCE 1.22.0 the data no longer lives in `projects.json` at all — it lives in the project's own
 *  repo at `<local_path>/.akidevsync/notes.json` (docs/plan/done/1.22.0-notes-json-ssot.md). PERSIST-1
 *  itself is unchanged and this is still the ONLY funnel: it remains an `action()`, so a phone's
 *  edit is dispatched as an intent and this whole body runs on the Mac, against the Mac's live
 *  project and the Mac's filesystem. That is the entire companion write story — no new command
 *  enters COMPANION_ALLOWED_COMMANDS.
 *
 *  Returns the write result (`{ file, clobbered }`) on success, or `null` when the write was refused
 *  or failed — `migrateLegacyProjectNotes` needs to know that before it deletes the legacy copy. */
export const applyTaskEdit = action('remoteActions.applyTaskEdit', async (projectId, patch) => {
  const p = byId(projectId)
  if (!p || !patch) return null

  // REFUSED BEFORE THE STORE IS TOUCHED, not after. An unmounted volume / corrupt file cannot be
  // written, and mutating the store first would show the user a change that never reached disk —
  // which the next broadcastFull() would then silently revert. That is the exact shape of the
  // 1.20.0 "task note reverts" bug, and the ordering of these two statements is what prevents it.
  if (!isProjectNotesWritable(projectId)) {
    Toast.fire({ icon: 'error', title: getProjectNotesEntry(projectId).error || 'Notes are not writable right now' })
    return null
  }

  // Optimistic, and SYNCHRONOUS — the field must feel instant. Bumping the generation here is what
  // makes any read already in flight for this id discard itself instead of landing on top of the
  // edit a moment later (see projectNotesStore.bumpProjectNotesGeneration).
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
        // `undefined` → `null` over IPC = "leave this field on disk alone", never "clear it" (the
        // multi-entity guard applied to the file's own fields). A notes-only edit must not blank the
        // task list a `git pull` just brought in.
        notes: patch.notes !== undefined ? patch.notes : null,
        tasks: patch.tasks !== undefined ? patch.tasks : null,
        // Read HERE, not from the `before` snapshot above: inside the queue the previous write for
        // this id has already re-seeded the entry, so this is the `updated_at` we genuinely last
        // observed on disk. Using the pre-queue snapshot instead would make every second rapid edit
        // report `clobbered` against our OWN previous write — a false "someone else changed this
        // file" alarm, which is worse than none: it is the one signal telling the user to go look
        // at git, and it stops being believed the moment it cries wolf.
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
      // The on-screen text is the user's only remaining copy at this moment, so the store entry keeps
      // the optimistic value rather than snapping back — losing what they just typed on top of a
      // failed save is strictly worse than showing an unsaved edit next to an explicit error.
      Toast.fire({ icon: 'error', title: `Could not save notes: ${e}` })
      return null
    }
  })
})

/** Serialises writes PER PROJECT ID, so two quick edits cannot interleave between their read and
 *  their rename — and so the second one reads a `baseUpdatedAt` that already reflects the first.
 *
 *  Per id, not global: one project's slow network mount must not stall another project's save. The
 *  Rust side has its own global mutex around the read-modify-write, which is what makes the FILE
 *  safe; this queue is what makes the CALLER's view of `updated_at` truthful. The two solve
 *  different halves and neither replaces the other. */
const notesWriteChains = new Map()
function queueNotesWrite(projectId, run) {
  const prev = notesWriteChains.get(projectId) || Promise.resolve()
  // `.then(run, run)` — a failed write must not poison the chain and silently swallow every
  // subsequent edit for that project.
  const next = prev.then(run, run)
  notesWriteChains.set(projectId, next.catch(() => {}))
  return next
}

/** Re-read ONE project's notes file on the HOST (modal open, `local_path` change).
 *
 *  Exists as an `action()` for the same reason `applyTaskEdit` is one: `read_project_notes` is
 *  deliberately absent from COMPANION_ALLOWED_COMMANDS (a phone has no filesystem), so a companion
 *  asks the Mac to re-read and receives the result through the mirrored `projectNotes` store. */
export const requestProjectNotesRefresh = action('remoteActions.requestProjectNotesRefresh', (projectId) => {
  const p = byId(projectId)
  if (!p) return
  return refreshProjectNotes(p.id, p.local_path)
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
export const applyProjectConfig = action('remoteActions.applyProjectConfig', async (plain) => {
  if (!plain || !plain.id) return
  const index = projects.value.findIndex((p) => p.id === plain.id)
  const isNew = index === -1

  // §3.3 — the project this modal is editing was REMOVED while the modal stayed open (the removal
  // does not close another screen's modal). Saving would land in the "new project" branch below and
  // resurrect it. Reject instead, and say which of the two things happened: someone who just
  // deleted a project and sees it come back cannot tell whether the delete failed or the save
  // worked. Nothing is written and no other project is touched.
  if (isNew && isProjectRemoved(plain.id)) {
    Toast.fire({ icon: 'error', title: `"${plain.name}" was removed - not saved` })
    return { rejected: 'removed' }
  }

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
      // THE NOTES FOLLOW THE DIRECTORY (1.22.0). `local_path` may have just changed, and the store
      // is still holding the OLD directory's tasks — showing those against the new path, and then
      // saving them into it, is the same empty-then-overwrite class of bug the status model exists
      // to prevent.
      //
      // The invalidation is SYNCHRONOUS and the re-read is not. Kicking off `refreshProjectNotes`
      // alone would only narrow the window, not close it: until it resolves,
      // `getProjectNotesEntry(id)` still answers with the OLD directory's `{status:'ok', tasks}`,
      // `isProjectNotesWritable` still says yes, and `applyTaskEdit` already uses the NEW
      // `local_path` — so an edit in that gap writes the old directory's task list into the new
      // directory. `'unknown'` is not writable, so the surface is correctly read-only until the real
      // read lands.
      setProjectNotesEntry(plain.id, { status: 'unknown' })
      refreshProjectNotes(plain.id, plain.local_path)
    }
  } else {
    projectRuntime.value[plain.id] = {
      git_status: '...',
      git_log: '',
      remote_url: '',
      syncing: false,
      // §3.2 — a LIVE project's epoch is always >= 1 (projectStore.beginRefresh/currentEpoch): 0 is
      // reserved for "this project has no runtime state, it was removed". Starting a new project at
      // 0 broke that invariant, so a status check that captured epoch 0 and finished after the
      // project was deleted still matched, and re-created `projectRuntime[id]` for a project that
      // no longer exists. Advancing from whatever is there (rather than assigning 1) matches
      // `loadData` and keeps the epoch monotonic per id, so a check in flight from an earlier
      // incarnation of the same id can never coincidentally match either.
      epoch: (projectRuntime.value[plain.id]?.epoch ?? 0) + 1,
      refreshCount: 0,
    }
    projects.value.push({ ...plain })
    // A brand-new project hydrates the same way an existing one does — normally `missing`, which is
    // writable, so its first note creates `.akidevsync/notes.json`. Seeded `'unknown'` first for the
    // same reason as the identity-change branch above: this id may be a REUSED one (created, removed,
    // re-created), and a leftover entry would be writable against a directory nothing has read yet.
    setProjectNotesEntry(plain.id, { status: 'unknown' })
    refreshProjectNotes(plain.id, plain.local_path)
  }

  await saveProjectsList()
  const saved = projects.value.find((p) => p.id === plain.id)
  if (saved) refreshProject(saved)
  // §3.1 — the Rust icon cache is (re)built only while `load_projects` runs, which `save_projects`
  // does not do: without this a project added mid-session never gets an icon until the next app
  // start, and one whose `local_path` was edited keeps the OLD path's icon under the same id.
  // `get_project_icons_map` goes through `load_projects`, so it rescans — hence after the persist,
  // never before it, or it would rescan the pre-save list. The timestamp bump is what makes the
  // host's `aki-devsync-icon://<id>?t=` URL change, so an edited project's stale icon is actually
  // re-fetched instead of served from the webview cache.
  await refreshProjectIcons()
  iconTimestamp.value = Date.now()
})

/** Remove a project from the list (matrix "Remove project"). Same reason as applyProjectConfig:
 *  the deletion must happen on the host's reactive `projects`, not the phone's copy. The confirm
 *  dialog is mirrored separately, see requestRemoveProject below. */
export const removeProject = action('remoteActions.removeProject', (id) => {
  if (!id) return
  projects.value = projects.value.filter((p) => p.id !== id)
  // Records THIS id only (§3.3) - a config modal still open for it on any screen must not be able
  // to save it back into existence. See projectStore.markProjectRemoved.
  markProjectRemoved(id)
  // Dropping the runtime entry also cancels any in-flight status check for this id (currentEpoch
  // then reports 0, which never matches the >=1 epoch a check captured) — see projectStore.
  delete projectRuntime.value[id]
  // THIS id's notes entry only (multi-entity guard). The FILE on disk is deliberately not deleted:
  // removing a project from the app list has never touched the user's files, and since 1.22.0
  // `.akidevsync/notes.json` is one of the user's files, living in their repo.
  dropProjectNotesEntry(id)
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
      // §3.4 — "1 host gone + 1 host new" is only a GUESS that the host was renamed; it matches
      // "deleted host A and added an unrelated host B in the same edit" exactly as well. Acting on
      // that guess silently repointed every affected project, and the next `--delete` PUSH then
      // mirrored onto the wrong server with no undo. So: ask, list the projects by name, and do
      // nothing unless the user explicitly confirms. Editing an SSH config is a config-file task -
      // the user is not thinking about projects at that moment - and repointing by hand afterwards
      // costs a minute, while discovering it by mirroring onto the wrong server costs the tree.
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

      // DialogHost needs a 'select' kind for this one (docs/plan/done/1.20.0-terminal-and-remote-sync.md
      // §3) — this whole function already runs host-side (applySshHostsChange is itself an
      // action, always dispatched via useSsh.js), so askConfirm here behaves identically to the
      // old direct call to the popup library, just mirrored to any companion screen too.
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
