import { ref } from 'vue'
import { invoke } from '../utils/tauri'
import { projects, projectRuntime, isReloading, Toast, ideAvailability, iconTimestamp } from '../store/projectStore'
import { useLogs } from './useLogs'
import { refreshAllProjects, refreshProject, startBackgroundRefresh } from './useBackgroundRefresh'

export const showConfigModal = ref(false)
export const editingProject = ref(null)

const { appendGlobalLog, projectLogs, activeLogProjectId, setupGlobalListener } = useLogs()

/**
 * Migration off the `sync_git` toggle onto exclude-list semantics (push-only paths plan,
 * 2026-07-19): a push-only dir = present in pull_excludes, absent from push_excludes.
 *
 * Idempotent by construction - no localStorage flag needed (and none should be added: a
 * flag is volatile state guarding durable data, which was itself the root cause of the bug
 * this migration used to have - see push-only-paths plan for the incident). The backing
 * Rust struct also enforces this: `sync_git` is now `Option<bool>` with
 * `skip_serializing_if = "Option::is_none"`, so once this migration deletes the key, it is
 * never re-materialized on disk. A project with no `sync_git` property is therefore, by
 * definition, either already migrated or created after the migration shipped - nothing to
 * do for it, so that branch is a total no-op (no field on that project is touched).
 *
 * For a project that still HAS `sync_git`, preserves its prior effective behavior exactly:
 *   sync_git === true  → drop `.git/` from push_excludes if present (was pushed)
 *   sync_git === false → add `.git/` to push_excludes if missing (was not pushed)
 *   always → ensure `.git/` is in pull_excludes (matches the old hardcoded pull behavior)
 * Only ever adds/removes that one entry - never rewrites the rest of the list
 * (Regression Guard: multi-entity stores must not get a wider blast radius than the bug).
 */
function migratePushOnlyPaths(loadedProjects) {
  let changed = false
  for (const p of loadedProjects) {
    if (!Object.prototype.hasOwnProperty.call(p, 'sync_git')) {
      continue
    }
    if (p.sync_git === true) {
      p.push_excludes = removeEntry(p.push_excludes, '.git/')
    } else {
      p.push_excludes = ensureEntry(p.push_excludes, '.git/')
    }
    p.pull_excludes = ensureEntry(p.pull_excludes, '.git/')
    delete p.sync_git
    changed = true
  }
  return changed
}

function ensureEntry(list, entry) {
  const arr = list || []
  return arr.includes(entry) ? arr : [...arr, entry]
}

function removeEntry(list, entry) {
  const arr = list || []
  return arr.includes(entry) ? arr.filter(e => e !== entry) : arr
}

/**
 * Single source of truth for "is this project safe to rsync at all"
 * (docs/plan/1.20.1-flow-audit-fixes.md §2.1). An empty `local_path` makes the sync path build
 * `format!("{}/", "")` → `/`: PUSH uploads the whole filesystem root, and PULL — which is the worse
 * direction, since `delete_on_pull` defaults to true on a new project — mirrors the remote *into*
 * `/` with `--delete`. Neither is recoverable, so the same predicate gates the Save button
 * (ProjectConfigModal), `saveConfig` and `startSync`; Rust's `validate_project` is the backstop.
 *
 * Returns `{ field, message }` - `field` names the offending input so the modal can mark it
 * without re-implementing the rules; both are `''` when the project is fine.
 */
export function projectPathIssue(project) {
  const none = { field: '', message: '' }
  if (!project) return none
  const local = (project.local_path || '').trim()
  const remote = (project.remote_path || '').trim()
  if (!local) return { field: 'local_path', message: 'Local Path is required' }
  if (!local.startsWith('/')) return { field: 'local_path', message: 'Local Path must be absolute (start with /)' }
  if (!remote) return { field: 'remote_path', message: 'Remote Destination Directory is required' }
  return none
}

/** Message-only form of `projectPathIssue` - `''` means the project is safe to sync. */
export function projectPathError(project) {
  return projectPathIssue(project).message
}

export async function loadData(sshHosts, showToast = false) {
  if (isReloading.value) return
  isReloading.value = true
  try {
    if (showToast) appendGlobalLog("SYSTEM", "User triggered manual reload.")
    appendGlobalLog("LOAD", "Initializing workspace and scanning SSH hosts...")
    sshHosts.value = await invoke("get_ssh_hosts")
    appendGlobalLog("LOAD", `Found ${sshHosts.value.length} SSH hosts.`)
    const loaded = await invoke("load_projects")
    const migrated = migratePushOnlyPaths(loaded)

    for (const p of loaded) {
      const prev = projectRuntime.value[p.id]
      projectRuntime.value[p.id] = {
        git_status: "...",
        git_log: "",
        remote_url: "",
        // Preserve syncing flag if a sync is in progress during reload
        syncing: prev?.syncing ?? false,
        hasPendingPush: null,
        hasPendingPull: null,
        // The project list was just re-read from disk, so any status check still in flight
        // describes a project definition we no longer hold - advance the generation so those
        // results are discarded instead of landing on top of the fresh state, and start this
        // generation idle. (Advancing, not resetting to 0: epoch must stay monotonic per project
        // or an in-flight check could coincidentally match again. See bumpEpoch in projectStore.)
        epoch: (prev?.epoch ?? 0) + 1,
        refreshCount: 0,
      }
      if (!projectLogs.value[p.id]) projectLogs.value[p.id] = []
    }
    projects.value = loaded
    setupGlobalListener()
    if (migrated) {
      await saveProjectsList()
      appendGlobalLog("MIGRATE", "Migrated sync_git toggle to push-only exclude-list semantics.")
    }

    // Prefetch IDE availability status once
    try {
      ideAvailability.value = await invoke('check_ide_availability')
    } catch (e) {
      console.error("Failed to check IDE availability:", e)
      ideAvailability.value = { vscode: false, vscode_insiders: false, antigravity: false }
    }

    // Refresh icon timestamp to bust browser cache
    iconTimestamp.value = Date.now()

    appendGlobalLog("LOAD", `Loaded ${loaded.length} projects successfully.`)

    // Start the background cycles, then run one full pass immediately - this is also what
    // populates stack_info (DEV/BUILD commands), which used to be fetched here in a sequential
    // per-project await loop before it became one of the checks refreshProject runs in parallel.
    startBackgroundRefresh()
    refreshAllProjects()

    if (showToast) Toast.fire({ icon: 'success', title: 'Data Reloaded!' })
  } catch (err) {
    appendGlobalLog("ERROR", `Failed to load data: ${err}`)
    if (showToast) Toast.fire({ icon: 'error', title: 'Reload failed' })
  } finally {
    isReloading.value = false
  }
}

// PERSIST-1 invariant (docs/plan/1.20.0-terminal-and-remote-sync.md §2): this is a HOST-SIDE
// persist of the HOST's own `projects.value`. It must only ever be reached from code already
// running on the host — i.e. from inside an action() body, or a plain function only ever called
// from one. Reaching it from a companion-triggered path (a bare click handler, a v-model change
// handler not routed through an id-based action first) is exactly the class of bug that shipped
// the "task note reverts" regression: a companion would ship ITS OWN copy of `projects.value`
// here, which this function would then happily persist to disk while the host's reactive state —
// and the next broadcastFull() — stayed on the old value. Adding a guard inside this function
// cannot fix that (by the time it runs, the wrong array is already in hand); the fix is that
// every mutation site upstream goes through an id-based action first (see remoteActions.js's
// applyTaskEdit/reorderProjects and this file's own applyProjectConfig/removeProject usage below).
export async function saveProjectsList() {
  try {
    await invoke("save_projects", { projects: projects.value })
  } catch (err) {
    appendGlobalLog("ERROR", `Failed to save projects: ${err}`)
  }
}

export function openConfig(project) {
  const p = {
    ...project,
    hooks: project.hooks
      ? { ...project.hooks }
      : { pre_pull_cmd: null, post_pull_cmd: null, pre_push_cmd: null, post_push_cmd: null, run_hooks_on_remote: true },
    pull_excludes: [...(project.pull_excludes || [])],
    push_excludes: [...(project.push_excludes || [])],
    production_url: project.production_url ?? "",
  }
  editingProject.value = p
  showConfigModal.value = true
}

export function closeConfig() {
  showConfigModal.value = false
  editingProject.value = null
}

export async function saveConfig() {
  if (!editingProject.value) return

  // Backstop for the disabled Save button (§2.1) - saveConfig is also reachable by Enter/keyboard
  // and from a companion screen, where the button state is not what decides.
  const pathError = projectPathError(editingProject.value)
  if (pathError) {
    Toast.fire({ icon: 'error', title: pathError })
    return
  }

  if (editingProject.value.production_url) {
    const pUrl = editingProject.value.production_url.trim()
    if (!pUrl.startsWith('http://') && !pUrl.startsWith('https://') && pUrl !== "") {
      editingProject.value.production_url = 'https://' + pUrl
    } else {
      editingProject.value.production_url = pUrl
    }
  }

  const isNew = !projects.value.some(p => p.id === editingProject.value.id)

  // §3.3: this modal may have been open when the project was removed from the other screen. Without
  // this check the `isNew` branch below re-creates it, and on a companion the host's rejection is
  // invisible (applyProjectConfig is fire-and-forget there) so the phone would report "Project
  // created" for a project that no longer exists. `removedProjectIds` is mirrored, so this is true on
  // both screens.
  if (isNew) {
    const { isProjectRemoved } = await import('../store/projectStore')
    if (isProjectRemoved(editingProject.value.id)) {
      Toast.fire({ icon: 'error', title: `"${editingProject.value.name}" was removed - not saved` })
      closeConfig()
      return
    }
  }

  try {
    // The list mutation + persist + refresh must run on the HOST so its reactive `projects` updates
    // live and mirrors to every screen (ACT-1 / feat matrix "Config save"). Before this, saving on
    // a companion mutated only the phone's copy and `invoke('save_projects')`-persisted to disk, so
    // the Mac UI stayed stale until a reload re-read disk. On the host `applyProjectConfig` is the
    // real fn; from a companion it ships the edited data as an intent the host applies. Dynamic
    // import avoids the useProjectConfig ⇄ remoteActions static cycle (remoteActions imports
    // saveProjectsList from this module).
    const { applyProjectConfig } = await import('../store/remoteActions')
    await applyProjectConfig({ ...editingProject.value })
    appendGlobalLog("CONFIG", `User ${isNew ? 'created new' : 'updated config for'} project "${editingProject.value.name}".`)
    Toast.fire({ icon: 'success', title: isNew ? 'Project created' : 'Config saved' })
    closeConfig()
  } catch (err) {
    appendGlobalLog("ERROR", `Failed to save config: ${err}`)
    Toast.fire({ icon: 'error', title: 'Failed to save config' })
  }
}

export async function createNewProject(sshHosts) {
  const { open } = await import('@tauri-apps/plugin-dialog')
  const selectedPath = await open({
    directory: true,
    multiple: false,
    title: "Select Local Project Folder"
  })

  if (selectedPath) {
    const folderName = selectedPath.split('/').pop() || "New Project"
    const newId = "project-" + Date.now()

    let productionUrl = ""
    if (folderName.includes(".")) {
      productionUrl = "https://" + folderName
    }

    const p = {
      id: newId,
      name: folderName,
      local_path: selectedPath.endsWith('/') ? selectedPath : selectedPath + "/",
      remote_host: sshHosts.value[0] || "localhost",
      remote_path: "~/",
      production_url: productionUrl,
      pull_excludes: [".DS_Store", "*.log", ".git/", "node_modules/", ".nuxt/", ".output/", ".wrangler/", "dist/", ".claude/"],
      push_excludes: [".DS_Store", "*.log", "node_modules/", ".nuxt/", ".output/", ".wrangler/", "dist/", ".claude/"],
      hooks: { pre_pull_cmd: null, post_pull_cmd: null, pre_push_cmd: null, post_push_cmd: null, run_hooks_on_remote: true },
      last_sync_action: null,
      last_sync_time: null,
      last_sync_host: null,
      last_sync_status: null,
      dry_run: true,
      delete_on_pull: true,
      delete_on_push: false,
      tasks: [],
      notes: "",
    }
    openConfig(p)
  }
}

export async function confirmRemove() {
  if (!editingProject.value) return
  const id = editingProject.value.id
  const projectName = editingProject.value.name

  // The confirm dialog itself is mirrored state now (docs/plan/1.20.0-terminal-and-remote-sync.md
  // §3) — requestRemoveProject asks + (on Yes) removes entirely on the host, same ACT-1 reason as
  // saveConfig/removeProject above. Dynamic import avoids the useProjectConfig ⇄ remoteActions
  // static cycle (remoteActions imports saveProjectsList from this module).
  //
  // On the HOST this genuinely awaits the real confirm+remove, so `removed` reflects the real
  // outcome. On a COMPANION, action() is fire-and-forget (it never RPCs a result back — see
  // services/action.js) — `removed` resolves to `undefined` immediately, before the host has even
  // shown the dialog. Guarding on `removed` means a companion never closes this modal or logs
  // "removed" on a stale assumption; its config modal simply doesn't auto-close after a
  // phone-triggered removal (this modal's open/closed state isn't mirrored at all yet — a known,
  // separately-tracked gap, not something this fix introduces).
  const { requestRemoveProject } = await import('../store/remoteActions')
  const removed = await requestRemoveProject(id, projectName)
  if (removed) {
    if (activeLogProjectId.value === id) activeLogProjectId.value = null
    closeConfig()
    appendGlobalLog("REMOVE", `Project "${projectName}" was removed from the local list.`)
  }
}
