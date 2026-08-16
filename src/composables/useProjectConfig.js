import { ref } from 'vue'
import { invoke } from '../utils/tauri'
import { projects, projectRuntime, isReloading, Toast, ideAvailability, iconTimestamp } from '../store/projectStore'
import { useLogs } from './useLogs'
import { dropProjectLogs } from '../store/logStore'
import { refreshAllProjects, refreshProject, startBackgroundRefresh } from './useBackgroundRefresh'
import { hydrateProjectNotes, migrateLegacyProjectNotes } from './useProjectNotes'

export const showConfigModal = ref(false)
export const editingProject = ref(null)

const { appendGlobalLog, projectLogs, activeLogProjectId, setupGlobalListener } = useLogs()

/**
 * Migration off `sync_git` toggle to push-only exclude-list semantics (push-only = in pull_excludes, absent from push_excludes).
 * Idempotent: absent `sync_git` is a no-op; preserves legacy sync_git values on disk without rewriting other entries.
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

/**
 * Migration removing `.akidevsync/` from pull/push exclude lists so repo notes/metadata sync across devices.
 * Idempotent entry-scoped removal (multi-entity guard).
 */
function migrateStripNotesExcludes(loadedProjects) {
  const entry = '.akidevsync/'
  let changed = false
  for (const p of loadedProjects) {
    const nextPull = removeEntry(p.pull_excludes, entry)
    const nextPush = removeEntry(p.push_excludes, entry)
    if (nextPull === p.pull_excludes && nextPush === p.push_excludes) continue
    p.pull_excludes = nextPull
    p.push_excludes = nextPush
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
 * Single source of truth for "is this project safe to rsync at all" (docs/plan/done/1.20.1-flow-audit-fixes.md §2.1).
 * Guards against empty local_path resolving to filesystem root `/` with `--delete`.
 * Returns `{ field, message }` where empty strings indicate the configuration is valid.
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

/** Cache TTL for IDE availability checks (changes on app install/uninstall, not per interaction). */
const IDE_AVAILABILITY_TTL_MS = 60_000
let ideAvailabilityCheckedAt = 0
let ideAvailabilityInFlight = null

/**
 * Probes installed IDEs (VSCode, VSCode Insiders, Antigravity) with TTL caching and dirty-checking.
 * Caching avoids broadcasting redundant store deltas across mirrored companions during hover events.
 */
export async function refreshIdeAvailability({ force = false } = {}) {
  if (!force && ideAvailabilityInFlight) return ideAvailabilityInFlight
  if (!force && ideAvailability.value && Date.now() - ideAvailabilityCheckedAt < IDE_AVAILABILITY_TTL_MS) return
  ideAvailabilityInFlight = (async () => {
    let next
    try {
      next = await invoke('check_ide_availability')
    } catch (e) {
      console.error("Failed to check IDE availability:", e)
      next = { vscode: false, vscode_insiders: false, antigravity: false }
    }
    ideAvailabilityCheckedAt = Date.now()
    const prev = ideAvailability.value
    const unchanged = prev && Object.keys(next).every((k) => prev[k] === next[k])
      && Object.keys(prev).length === Object.keys(next).length
    if (!unchanged) ideAvailability.value = next
  })()
  try {
    await ideAvailabilityInFlight
  } finally {
    ideAvailabilityInFlight = null
  }
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
    // Each migration logs its own line on fire so usage.log identifies which step ran.
    let migrated = false
    if (migratePushOnlyPaths(loaded)) {
      migrated = true
      appendGlobalLog("MIGRATE", "Migrated sync_git toggle to push-only exclude-list semantics.")
    }
    if (migrateStripNotesExcludes(loaded)) {
      migrated = true
      appendGlobalLog("MIGRATE", "Removed .akidevsync/ from the default pull/push exclude lists.")
    }

    for (const p of loaded) {
      const prev = projectRuntime.value[p.id]
      projectRuntime.value[p.id] = {
        git_status: "...",
        git_log: "",
        remote_url: "",
        // Preserve in-flight syncing state and syncDirection so ProjectTable stop button targets the active operation.
        syncing: prev?.syncing ?? false,
        syncDirection: prev?.syncDirection ?? null,
        hasPendingPush: null,
        hasPendingPull: null,
        // Advance monotonic epoch to discard stale in-flight status checks from previous project definitions.
        epoch: (prev?.epoch ?? 0) + 1,
        refreshCount: 0,
      }
      if (!projectLogs.value[p.id]) projectLogs.value[p.id] = []
    }
    projects.value = loaded
    setupGlobalListener()

    // Hydrate per-repo notes first, then migrate legacy fields through applyTaskEdit after projects.value is assigned.
    await hydrateProjectNotes(loaded)
    if (await migrateLegacyProjectNotes(loaded)) {
      migrated = true
      appendGlobalLog("MIGRATE", "Moved project tasks/notes into each repo's .akidevsync/notes.json.")
    }

    if (migrated) await saveProjectsList()

    // force: a manual reload is the user explicitly asking for fresh state, TTL or not.
    await refreshIdeAvailability({ force: true })

    // Refresh icon timestamp to bust browser cache
    iconTimestamp.value = Date.now()

    appendGlobalLog("LOAD", `Loaded ${loaded.length} projects successfully.`)

    // Start background refresh cycles and trigger an immediate pass to populate stack_info in parallel.
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

// PERSIST-1 invariant: Host-only persist of host projects.value; mutations must route via ID-based actions first.
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

  // Backstop for the disabled Save button (§2.1) - saveConfig is also reachable by Enter/keyboard and from a companion screen, where the button state is not what decides.
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

  // Guard against recreating a project removed on another screen while this modal was open.
  if (isNew) {
    const { isProjectRemoved } = await import('../store/projectStore')
    if (isProjectRemoved(editingProject.value.id)) {
      Toast.fire({ icon: 'error', title: `"${editingProject.value.name}" was removed - not saved` })
      closeConfig()
      return
    }
  }

  try {
    // Host-side mutation/persist via dynamic applyProjectConfig to mirror updates across screens and break import cycles.
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
      disabled: false,
      // Tasks and notes live in <local_path>/.akidevsync/notes.json; absent keys indicate migrated projects.
    }
    openConfig(p)
  }
}

export async function confirmRemove() {
  if (!editingProject.value) return
  const id = editingProject.value.id
  const projectName = editingProject.value.name

  // Host-side confirmation and removal via dynamic requestRemoveProject; companion fire-and-forget resolves undefined.
  const { requestRemoveProject } = await import('../store/remoteActions')
  const removed = await requestRemoveProject(id, projectName)
  if (removed) {
    if (activeLogProjectId.value === id) activeLogProjectId.value = null
    // Scoped to target project ID only (multi-entity guard) to clean up log buffers and cursors.
    dropProjectLogs(id)
    closeConfig()
    appendGlobalLog("REMOVE", `Project "${projectName}" was removed from the local list.`)
  }
}
