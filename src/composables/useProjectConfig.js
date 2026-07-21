import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import Swal from 'sweetalert2'
import { projects, projectRuntime, isReloading, Toast, ideAvailability, iconTimestamp, bumpEpoch } from '../store/projectStore'
import { useLogs } from './useLogs'
import { refreshAllProjects, refreshProject, startBackgroundRefresh } from './useBackgroundRefresh'

export const showConfigModal = ref(false)
export const editingProject = ref(null)

const { appendGlobalLog, projectLogs, activeLogProjectId, setupGlobalListener } = useLogs()

/**
 * Migration off the `sync_git` toggle onto exclude-list semantics (push-only paths plan,
 * 2026-07-19): a push-only dir = present in pull_excludes, absent from push_excludes.
 *
 * Idempotent by construction — no localStorage flag needed (and none should be added: a
 * flag is volatile state guarding durable data, which was itself the root cause of the bug
 * this migration used to have — see push-only-paths plan for the incident). The backing
 * Rust struct also enforces this: `sync_git` is now `Option<bool>` with
 * `skip_serializing_if = "Option::is_none"`, so once this migration deletes the key, it is
 * never re-materialized on disk. A project with no `sync_git` property is therefore, by
 * definition, either already migrated or created after the migration shipped — nothing to
 * do for it, so that branch is a total no-op (no field on that project is touched).
 *
 * For a project that still HAS `sync_git`, preserves its prior effective behavior exactly:
 *   sync_git === true  → drop `.git/` from push_excludes if present (was pushed)
 *   sync_git === false → add `.git/` to push_excludes if missing (was not pushed)
 *   always → ensure `.git/` is in pull_excludes (matches the old hardcoded pull behavior)
 * Only ever adds/removes that one entry — never rewrites the rest of the list
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
        // describes a project definition we no longer hold — advance the generation so those
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

    // Start the background cycles, then run one full pass immediately — this is also what
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

  if (editingProject.value.production_url) {
    const pUrl = editingProject.value.production_url.trim()
    if (!pUrl.startsWith('http://') && !pUrl.startsWith('https://') && pUrl !== "") {
      editingProject.value.production_url = 'https://' + pUrl
    } else {
      editingProject.value.production_url = pUrl
    }
  }

  const index = projects.value.findIndex(p => p.id === editingProject.value.id)
  const isNew = index === -1
  const prevProject = !isNew ? projects.value[index] : null
  const identityChanged = prevProject && (
    prevProject.remote_host !== editingProject.value.remote_host ||
    prevProject.local_path !== editingProject.value.local_path
  )

  try {
    if (!isNew) {
      projects.value[index] = { ...editingProject.value }
      if (identityChanged) {
        // Host or local path changed — any status check still in flight describes the OLD
        // identity and must not land here (see bumpEpoch in projectStore.js). This is the
        // "cancel the check, not a real rsync" boundary: bumping the epoch discards stale
        // results and clears the busy indicator immediately; it never touches a push/pull in
        // progress. The push/pull state is blanked to unknown for the same reason — it was
        // measured against the old host.
        bumpEpoch(editingProject.value.id)
        projectRuntime.value[editingProject.value.id] = {
          ...projectRuntime.value[editingProject.value.id],
          hasPendingPush: null,
          hasPendingPull: null,
        }
      }
      appendGlobalLog("CONFIG", `User updated config for project "${editingProject.value.name}".`)
    } else {
      projectRuntime.value[editingProject.value.id] = {
        git_status: "...",
        git_log: "",
        remote_url: "",
        syncing: false,
        epoch: 0,
        refreshCount: 0,
      }
      projects.value.push({ ...editingProject.value })
      appendGlobalLog("CONFIG", `User created new project "${editingProject.value.name}".`)
    }

    await saveProjectsList()
    // One refresh covers git status, remote diff and stack_info (DEV/BUILD commands) — the last
    // of which used to be fetched inline here with its own `check_project_stack` call.
    const savedProject = projects.value.find(p => p.id === editingProject.value.id)
    if (savedProject) refreshProject(savedProject)
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

export function confirmRemove() {
  if (!editingProject.value) return

  Swal.fire({
    title: 'Remove Project?',
    text: `Remove "${editingProject.value.name}" from the app list? Your actual code files will NOT be touched.`,
    icon: 'warning',
    showCancelButton: true,
    confirmButtonColor: '#ef4444',
    cancelButtonColor: '#374151',
    confirmButtonText: 'Yes, remove it',
    background: '#131317',
    color: '#F3F4F6'
  }).then((result) => {
    if (result.isConfirmed) {
      const id = editingProject.value.id
      const projectName = editingProject.value.name
      projects.value = projects.value.filter(p => p.id !== id)
      // Dropping the runtime entry is also what cancels any status check still in flight for this
      // project: currentEpoch() then reports 0, which can never equal the >= 1 epoch that check
      // captured, so its result is discarded instead of resurrecting an entry for a project that
      // no longer exists. Do not "optimize" this into keeping the entry around.
      delete projectRuntime.value[id]
      if (activeLogProjectId.value === id) activeLogProjectId.value = null
      saveProjectsList()
      closeConfig()
      appendGlobalLog("REMOVE", `Project "${projectName}" was removed from the local list.`)
    }
  })
}
