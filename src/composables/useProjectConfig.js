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

/** The directory this app stores a project's tasks & notes in, relative to `local_path`. Trailing
 *  slash is required: `is_under_dir_exclude` (Rust) only treats `/`-suffixed entries as directory
 *  entries, and `--exclude` needs it to match the directory rather than a same-named file. */
export const NOTES_DIR_EXCLUDE = '.akidevsync/'

/**
 * One-time migration adding `.akidevsync/` to BOTH exclude lists of every existing project
 * (docs/plan/done/1.22.0-notes-json-ssot.md §7).
 *
 * THE PULL SIDE IS WHY THIS EXISTS AND WHY IT IS NOT NEW-PROJECTS-ONLY. `delete_on_pull` defaults
 * to `true`, and a mirror PULL passes `--delete`. The remote does not have `.akidevsync/`. So a
 * single PULL on a project whose `pull_excludes` lack this entry DELETES the user's task list and
 * notes, silently, with no undo. Leaving that to a config screen the user has no reason to visit is
 * not an option for a destructive default.
 *
 * The push side is excluded too: the host runs code, it does not read the task list, and the notes
 * field's own placeholder invites credentials — pushing it would copy those onto a shared server on
 * the user's behalf. `.claude/` (agent-local metadata inside the repo) is already excluded in both
 * directions and is the same class of thing.
 *
 * Additive and entry-scoped, like `migratePushOnlyPaths` beside it: adds exactly one entry per list
 * and never rewrites the rest (Regression Guard — a migration's blast radius must match its bug).
 * Idempotent by construction: `ensureEntry` is a no-op once the entry is present, so no flag is
 * needed (and none may be added — see migratePushOnlyPaths' own note on why).
 */
function migrateNotesExcludes(loadedProjects) {
  let changed = false
  for (const p of loadedProjects) {
    const nextPull = ensureEntry(p.pull_excludes, NOTES_DIR_EXCLUDE)
    const nextPush = ensureEntry(p.push_excludes, NOTES_DIR_EXCLUDE)
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
 * Single source of truth for "is this project safe to rsync at all"
 * (docs/plan/done/1.20.1-flow-audit-fixes.md §2.1). An empty `local_path` makes the sync path build
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

/** How long a probe result stays good enough to reuse. IDE availability changes when someone
 *  installs or deletes an application - minutes-to-never, not per interaction. */
const IDE_AVAILABILITY_TTL_MS = 60_000
let ideAvailabilityCheckedAt = 0
let ideAvailabilityInFlight = null

/**
 * Which of the three IDEs are installed on the Mac (`check_ide_availability` = three
 * `Path::exists()` probes). Called on load AND every time the OPEN popup opens (ProjectTable.vue) -
 * fetching it once per `loadData` meant an IDE installed or removed while the app ran stayed
 * misreported until the next full reload. A failure writes all-false rather than leaving `null`:
 * `null` means "not asked yet", which the popup gates read as unavailable, and an unanswerable probe
 * deserves the same treatment.
 *
 * TTL-CACHED, and this is not just an invoke-count optimisation: the popup opens on HOVER, and
 * `ideAvailability` is a MIRRORED store ref, so a fresh object per hover meant a deep-watcher delta
 * broadcast to every companion per row swept. Within the TTL this is a no-op; `{ force: true }`
 * (loadData) always re-probes. The result is also compared before it is written, so an unchanged
 * answer never dirties the ref at all.
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
    // Each migration logs its OWN line when it fires: three of them share this one persist, and a
    // single generic "migrated" line is useless in usage.log when one of them is the thing that
    // went wrong.
    let migrated = false
    if (migratePushOnlyPaths(loaded)) {
      migrated = true
      appendGlobalLog("MIGRATE", "Migrated sync_git toggle to push-only exclude-list semantics.")
    }
    // Excludes BEFORE anything can be written into `.akidevsync/` (plan §5 WP-2): the failure this
    // prevents (a mirror PULL deleting a directory the remote does not have) destroys the notes
    // file rather than degrading it, so the guard must never trail the data.
    if (migrateNotesExcludes(loaded)) {
      migrated = true
      appendGlobalLog("MIGRATE", "Added .akidevsync/ to the default pull/push exclude lists.")
    }

    for (const p of loaded) {
      const prev = projectRuntime.value[p.id]
      projectRuntime.value[p.id] = {
        git_status: "...",
        git_log: "",
        remote_url: "",
        // Preserve the in-flight sync's flag AND its direction across a reload: ProjectTable's
        // `isStop` reads `syncDirection` to decide WHICH of PUSH/PULL turns into STOP, so dropping
        // it here would show STOP on the push button while a pull is running.
        syncing: prev?.syncing ?? false,
        syncDirection: prev?.syncDirection ?? null,
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

    // Tasks & notes now live in each project's own repo. Hydrate first, THEN migrate: the migration
    // decides per project by comparing the legacy fields against what is actually on disk, and it
    // must never write against a directory it could not read. Both run after `projects.value` is
    // assigned, because the migration writes through `applyTaskEdit`, which resolves the live
    // project by id (PERSIST-1's single funnel — see useProjectNotes.js).
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

// PERSIST-1 invariant (docs/plan/done/1.20.0-terminal-and-remote-sync.md §2): this is a HOST-SIDE
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
      // `.akidevsync/` in BOTH lists — see migrateNotesExcludes above for why the pull side is
      // load-bearing rather than tidy (a mirror PULL would otherwise delete the project's notes).
      pull_excludes: [".DS_Store", "*.log", ".git/", NOTES_DIR_EXCLUDE, "node_modules/", ".nuxt/", ".output/", ".wrangler/", "dist/", ".claude/"],
      push_excludes: [".DS_Store", "*.log", NOTES_DIR_EXCLUDE, "node_modules/", ".nuxt/", ".output/", ".wrangler/", "dist/", ".claude/"],
      hooks: { pre_pull_cmd: null, post_pull_cmd: null, pre_push_cmd: null, post_push_cmd: null, run_hooks_on_remote: true },
      last_sync_action: null,
      last_sync_time: null,
      last_sync_host: null,
      last_sync_status: null,
      dry_run: true,
      delete_on_pull: true,
      delete_on_push: false,
      // No `tasks` / `notes` here since 1.22.0: they live in <local_path>/.akidevsync/notes.json,
      // and an absent key on the project record is exactly what marks it as "already migrated"
      // (migrateLegacyProjectNotes' flagless idempotence — see useProjectNotes.js).
    }
    openConfig(p)
  }
}

export async function confirmRemove() {
  if (!editingProject.value) return
  const id = editingProject.value.id
  const projectName = editingProject.value.name

  // The confirm dialog itself is mirrored state now (docs/plan/done/1.20.0-terminal-and-remote-sync.md
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
    // Scoped to THIS id only (Regression Guard - Multi-entity State, CLAUDE.md): the log map and
    // its append cursor keep a per-project entry that nothing else ever removes.
    dropProjectLogs(id)
    closeConfig()
    appendGlobalLog("REMOVE", `Project "${projectName}" was removed from the local list.`)
  }
}
