import { ref } from 'vue'
import { invoke } from '../utils/tauri'
import { isHost } from '../services/bridge'
import { projects, projectRuntime, isReloading, Toast, ideAvailability, iconTimestamp } from '../store/projectStore'
import { useLogs } from './useLogs'
import { dropProjectLogs } from '../store/logStore'
import { refreshAllProjects, refreshProject, startBackgroundRefresh } from './useBackgroundRefresh'
import { hydrateProjectNotes, migrateLegacyProjectNotes } from './useProjectNotes'
import {
  setProjectConfigEntry,
  getProjectConfigEntry,
  bumpProjectConfigGeneration,
  isCurrentProjectConfigGeneration,
  markAllProjectConfigStatusesUnknown,
  queueConfigWrite,
} from '../store/projectConfigStore'
import { buildProjectListSavePayload, buildProjectConfigSeeds, DEFAULT_PULL_EXCLUDES, DEFAULT_PUSH_EXCLUDES, shouldUseDefaultExcludes } from './projectConfigPure'

export { resolveHostSwitch } from './projectConfigPure'

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

const PYCACHE_EXCLUDE_ENTRY = '__pycache__/'

/**
 * Seeds `<local_path>/.akidevsync/project.json` from each project's pre-1.32.0 registry fields, only where
 * the file does not already exist ("a file already in the repo wins", 1.22.0 precedent). Unreadable folders
 * and an all-empty legacy record are refused Rust-side (`NothingToSeed`/`Unavailable`) and retried on the
 * next launch. Returns the id → outcome map (`SeedOutcome`, snake_case) so the caller can act per project
 * instead of only counting.
 */
async function seedProjectConfigsIfMissing(loadedProjects) {
  const seeds = buildProjectConfigSeeds(loadedProjects)
  if (seeds.length === 0) return {}
  try {
    return await invoke('write_project_configs_if_missing', { seeds })
  } catch (e) {
    console.error('[migrate] project.json seeding failed', e)
    return {}
  }
}

// Cut item (docs/plan/done/settings-and-state-layout.md, Slice E): a persisted per-project "already considered"
// marker, local to this machine - once a project's id is in this set, the pycache migration never looks at
// it again, so a user who deliberately removes `__pycache__/` from their project.json does not get it
// silently written back on the next load/Refresh (the old content-check-only version re-added it forever).
const PYCACHE_MIGRATION_STORAGE_KEY = 'aki-pycache-exclude-migration-v1'

function readPycacheMigratedIds() {
  try {
    const raw = localStorage.getItem(PYCACHE_MIGRATION_STORAGE_KEY)
    const arr = raw ? JSON.parse(raw) : []
    return new Set(Array.isArray(arr) ? arr : [])
  } catch {
    return new Set()
  }
}

function markPycacheMigrated(ids) {
  if (!ids.length) return
  try {
    const existing = readPycacheMigratedIds()
    for (const id of ids) existing.add(id)
    localStorage.setItem(PYCACHE_MIGRATION_STORAGE_KEY, JSON.stringify([...existing]))
  } catch (e) {
    console.error('[migrate] could not persist pycache migration marker', e)
  }
}

/**
 * C5 (docs/plan/done/settings-and-state-layout.md § C): adds `__pycache__/` to a project.json that already owns
 * pull/push_excludes (`status === 'ok'`) through the same read-modify-write `write_project_config` path the
 * config dialog uses - never the registry copy, and through the same per-project write queue as the dialog
 * (`queueConfigWrite`) so this can never interleave with a concurrent Save. Once-only per project id (see
 * `PYCACHE_MIGRATION_STORAGE_KEY` above): a project already marked is skipped even if its excludes no
 * longer contain the entry, because that absence may be a deliberate later removal, not something left over
 * from before this migration ran.
 */
async function migrateProjectConfigPycacheExcludes(loadedProjects) {
  const migratedIds = readPycacheMigratedIds()
  const newlyConsidered = []
  let changedCount = 0
  for (const p of loadedProjects || []) {
    if (!p?.id || !p?.local_path) continue
    if (migratedIds.has(p.id)) continue
    if (getProjectConfigEntry(p.id).status !== 'ok') continue
    const nextPull = ensureEntry(p.pull_excludes, PYCACHE_EXCLUDE_ENTRY)
    const nextPush = ensureEntry(p.push_excludes, PYCACHE_EXCLUDE_ENTRY)
    if (nextPull === p.pull_excludes && nextPush === p.push_excludes) {
      // Already has the entry - nothing to write, but still considered done from now on.
      newlyConsidered.push(p.id)
      continue
    }
    try {
      await queueConfigWrite(p.id, async () => {
        const file = await invoke('write_project_config', {
          localPath: p.local_path,
          config: {
            name: p.name || '',
            production_url: p.production_url || '',
            pull_excludes: nextPull,
            push_excludes: nextPush,
            commands: { dev: p.dev_cmd_override || '', build: p.build_cmd_override || '', deploy: p.deploy_cmd || '' },
          },
        })
        // `p` was captured before this `await` - resolve the live entry by id now, so a project replaced/removed while the write was in flight is never mutated on a detached, stale reference.
        const live = projects.value.find((x) => x.id === p.id)
        if (live) {
          live.pull_excludes = [...file.pull_excludes]
          live.push_excludes = [...file.push_excludes]
        }
        setProjectConfigEntry(p.id, { ...getProjectConfigEntry(p.id), pull_excludes: file.pull_excludes, push_excludes: file.push_excludes })
        changedCount++
        newlyConsidered.push(p.id)
      })
    } catch (e) {
      console.error('[migrate] pycache exclude write failed', p.id, e)
      // Not marked considered - retried next launch.
    }
  }
  markPycacheMigrated(newlyConsidered)
  return changedCount
}

/**
 * B1 (docs/plan/done/settings-and-state-layout.md § B1): copies the project-owned fields from a read `project.json`
 * onto the matching in-memory registry object — "a file already in the repo wins". Only applied when the
 * read actually succeeded (`status === 'ok'`); a Missing/Unavailable/Corrupt read must never blank out
 * whatever the registry object already holds (a brand-new project, or a folder that just went unreadable).
 */
function applyConfigToProject(project, entry) {
  if (!project || !entry || entry.status !== 'ok') return
  project.name = entry.name
  project.production_url = entry.production_url
  project.pull_excludes = [...entry.pull_excludes]
  project.push_excludes = [...entry.push_excludes]
  project.dev_cmd_override = entry.commands?.dev || ''
  project.build_cmd_override = entry.commands?.build || ''
  // deploy plan: `commands.deploy` is project-owned the same way, but has no legacy registry field to shadow - `deploy_cmd` is an in-memory-only mirror on the project object, never persisted to projects.json.
  project.deploy_cmd = entry.commands?.deploy || ''
}

/**
 * B2 (docs/plan/done/settings-and-state-layout.md § B2, amended § Amendments for the 1.32.0 remote_path fix):
 * restores the active host's hooks/deploy from `targets.<remote_host>` onto the top-level fields — the ONE
 * persisted source for those (Rust never serializes the top-level copies, see projects.rs). Missing target
 * (never-used host) leaves the fields as loaded rather than inventing a value.
 *
 * `remote_path` is NOT restored from here — it is a project fact now, always read from its own top-level
 * field. The one exception is a genuinely empty top-level value: a registry written before the Rust boot
 * migration ran (or an edge case it missed) may still carry the old per-host copy, so this self-heals that
 * one case without ever overriding a real project-level value that already exists.
 */
export function hydrateRemoteFromTargets(loadedProjects) {
  for (const p of loadedProjects || []) {
    const target = p?.targets?.[p.remote_host]
    if (!target) continue
    if (!p.remote_path && target.remote_path) p.remote_path = target.remote_path
    if (target.hooks) p.hooks = target.hooks
  }
}

/**
 * B2 (docs/plan/done/settings-and-state-layout.md § B2): hydrates in-memory last_sync_action/time/status/host
 * from `state/<id>/*\/last_sync.json` - the ONE persisted source now (`useSync.js` writes there directly
 * via `write_last_sync` instead of round-tripping through the registry). "Which host did I last sync
 * with" = the newest last_sync.json across that project's hosts (`read_last_sync_all` already resolves
 * this per id); ProjectTable's last-action cell and `useSync.js`'s auto-approval read both consume the
 * fields this sets, so nothing downstream needs to change.
 */
export async function hydrateLastSyncFromState(loadedProjects) {
  const ids = (loadedProjects || []).filter((p) => p?.id).map((p) => p.id)
  if (ids.length === 0) return
  try {
    const map = await invoke('read_last_sync_all', { ids })
    const byId = new Map((loadedProjects || []).map((p) => [p.id, p]))
    for (const [id, entry] of Object.entries(map || {})) {
      const p = byId.get(id)
      if (!p || !entry) continue
      p.last_sync_action = entry.action
      p.last_sync_time = entry.time
      p.last_sync_status = entry.status
      p.last_sync_host = entry.host
    }
  } catch (e) {
    console.error('[sync_state] hydrate last_sync failed', e)
  }
}

/** D5: unreadable folder → registry no longer carries `name` (project-owned, project.json only) — fall
 * back to the folder's own basename so the row is never blank. */
export function projectDisplayName(project) {
  if (project?.name) return project.name
  const path = (project?.local_path || '').replace(/\/+$/, '')
  return path.split('/').pop() || project?.id || 'Unknown'
}

/** Flattens Rust `ProjectConfigRead` into store entry; non-ok status yields safe empty content. */
function toConfigEntry(read) {
  const status = read?.status || 'unavailable'
  const f = read?.file
  return {
    status,
    name: f?.name || '',
    production_url: f?.production_url || '',
    pull_excludes: Array.isArray(f?.pull_excludes) ? f.pull_excludes : [],
    push_excludes: Array.isArray(f?.push_excludes) ? f.push_excludes : [],
    commands: f?.commands || { dev: '', build: '', deploy: '' },
    error: read?.error || '',
  }
}

/** Boot hydrate: single read_project_config_map round-trip for all projects rather than N. */
export async function hydrateProjectConfig(projectList) {
  const targets = (projectList || [])
    .filter((p) => p?.id && p?.local_path)
    .map((p) => ({ id: p.id, local_path: p.local_path }))
  if (targets.length === 0) return
  // Same staleness guard as single read below, one token per id against in-flight race.
  const tokens = new Map(targets.map((t) => [t.id, bumpProjectConfigGeneration(t.id)]))
  try {
    const map = await invoke('read_project_config_map', { targets })
    for (const [id, read] of Object.entries(map || {})) {
      if (!isCurrentProjectConfigGeneration(id, tokens.get(id))) continue
      const entry = toConfigEntry(read)
      setProjectConfigEntry(id, entry)
      // Look up the entry currently in `projects.value` by id at apply time, never the `projectList`
      // snapshot this function was called with - a captured reference could be a stale object if anything
      // replaced `projects.value` while this batch's IPC round trip was in flight.
      applyConfigToProject(projects.value.find((p) => p.id === id), entry)
    }
  } catch (e) {
    // Failed batch does not mark projects unavailable to avoid false read-only state; entries were already
    // reset to 'unknown' by `markAllProjectConfigStatusesUnknown` before this call, so the sync gate stays
    // closed for these ids rather than trusting whatever status happened to be cached before the reload.
    console.error('[projectConfig] hydrate failed', e)
  }
}

/**
 * Re-read ONE project's project.json on config dialog open; scoped to id (multi-entity guard). A result
 * that is no longer the current generation is dropped, so a late read can never overwrite an edit already
 * in progress in the (separately-copied) `editingProject` form state — same idiom as `refreshProjectNotes`.
 */
export async function refreshProjectConfig(id, localPath) {
  if (!id || !localPath) return
  const token = bumpProjectConfigGeneration(id)
  try {
    const read = await invoke('read_project_config', { localPath })
    if (!isCurrentProjectConfigGeneration(id, token)) return
    const entry = toConfigEntry(read)
    setProjectConfigEntry(id, entry)
    applyConfigToProject(projects.value.find((p) => p.id === id), entry)
  } catch (e) {
    console.error('[projectConfig] refresh failed', e)
    if (!isCurrentProjectConfigGeneration(id, token)) return
    setProjectConfigEntry(id, { ...getProjectConfigEntry(id), status: 'unavailable', error: String(e?.message || e) })
  }
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
    // Restore remote_path/hooks from targets.<remote_host> before anything reads them - Rust no longer persists the top-level copies (projects.rs), so a fresh load has them empty until this runs.
    hydrateRemoteFromTargets(loaded)
    // Hydrate last_sync_action/time/status/host from state/ before this window renders anything - both boot load and titlebar Refresh share loadData, so this covers both call sites at once.
    await hydrateLastSyncFromState(loaded)
    // Settings/state (remote_host/remote_path/hooks -> targets, legacy baselines/last_sync -> state/)
    // migrates once in Rust setup() before this window even exists (docs/plan/done/settings-and-state-layout.md
    // § Migration, B3) - `loaded` here already reflects it, and its own summary line is in usage.log.
    // Each migration below still logs its own line on fire so usage.log identifies which step ran.
    let migrated = false
    if (migratePushOnlyPaths(loaded)) {
      migrated = true
      appendGlobalLog("MIGRATE", "Migrated sync_git toggle to push-only exclude-list semantics.")
    }
    if (migrateStripNotesExcludes(loaded)) {
      migrated = true
      appendGlobalLog("MIGRATE", "Removed .akidevsync/ from the default pull/push exclude lists.")
    }
    // Cut item (docs/plan/done/settings-and-state-layout.md, Slice E): seed only at boot, never on a titlebar
    // Refresh - `showToast` is already this function's only boot-vs-Refresh signal (App.vue's boot call
    // passes false, requestReloadConfig's Refresh call passes true). Once seeded (or explicitly refused,
    // NothingToSeed/Unavailable) a project has nothing left to retry mid-session; re-running the whole-folder
    // batch on every Refresh only adds an extra IPC round trip to the exact window T1 closes.
    const isBootLoad = !showToast
    let seededConfigCount = 0
    if (isBootLoad) {
      // Seeded before hydrate so a project.json created here already reflects every registry-field migration above, and so `hydrateProjectConfig` below reads it back as 'ok' straight away.
      const seedOutcomes = await seedProjectConfigsIfMissing(loaded)
      seededConfigCount = Object.values(seedOutcomes).filter((o) => o === 'written').length
      if (seededConfigCount > 0) {
        appendGlobalLog("MIGRATE", `Seeded ${seededConfigCount} project(s)' .akidevsync/project.json from existing settings.`)
      }
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
    // Reset every cached status to 'unknown' before the objects it describes are replaced - a stale
    // 'ok' must never outlive the fresh, freshly-stripped object during the window before
    // `hydrateProjectConfig` below completes (docs/plan/done/settings-and-state-layout.md).
    markAllProjectConfigStatusesUnknown()
    projects.value = loaded
    setupGlobalListener()

    // Hydrate per-repo notes and per-repo config first, then migrate legacy fields through applyTaskEdit after projects.value is assigned.
    await hydrateProjectNotes(loaded)
    await hydrateProjectConfig(loaded)
    if (await migrateLegacyProjectNotes(loaded)) {
      migrated = true
      appendGlobalLog("MIGRATE", "Moved project tasks/notes into each repo's .akidevsync/notes.json.")
    }
    // Runs after hydrate, against project.json directly (its own RMW write), for every project.json
    // that already owns pull/push_excludes - see migrateProjectConfigPycacheExcludes's doc comment for why
    // this cannot be a registry mutation like the migrations above.
    const pycacheMigratedCount = await migrateProjectConfigPycacheExcludes(loaded)
    if (pycacheMigratedCount > 0) {
      appendGlobalLog("MIGRATE", `Added __pycache__/ to ${pycacheMigratedCount} project(s)' .akidevsync/project.json exclude lists.`)
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

/**
 * B1/B2/T4: the ONE save funnel every mutation routes through - so it is also the one place that keeps the
 * registry from re-accumulating what now lives elsewhere. Delegates entirely to the pure
 * `buildProjectListSavePayload` (never mutates the live `projects.value` objects itself): a legacy copy is
 * dropped only once `project.json` already owns it (`status === 'ok'`) - otherwise it is the only copy left
 * and must round-trip unchanged.
 */
// PERSIST-1 invariant: Host-only persist of host projects.value; mutations must route via ID-based actions first.
export async function saveProjectsList() {
  try {
    const payload = buildProjectListSavePayload(projects.value, (id) => getProjectConfigEntry(id).status)
    await invoke("save_projects", { projects: payload })
    // Mirror the write-back onto the live objects too, so a later host-switch sees the just-saved path without waiting for a reload (multi-entity guard: scoped to each project's own remote_host key only).
    for (const p of projects.value) {
      if (!p.remote_host) continue
      p.targets = { ...(p.targets || {}), [p.remote_host]: { remote_path: p.remote_path, hooks: p.hooks || null } }
    }
  } catch (err) {
    appendGlobalLog("ERROR", `Failed to save projects: ${err}`)
  }
}

export async function setProjectDisabled(id, disabled) {
  const project = projects.value.find(p => p.id === id)
  if (!project || project.disabled === disabled) return
  project.disabled = disabled
  await saveProjectsList()
}

/**
 * B1 (§D): re-reads project.json before snapshotting into the dialog, so the dialog edits the hydrated
 * values rather than a possibly-stale in-memory copy (same reason as `requestProjectNotesRefresh` for
 * Tasks). Awaited (unlike the old fire-and-forget refresh) precisely so the snapshot below reflects it; a
 * local file read is fast, and the generation-counter guard still protects against a slow/late result.
 */
export async function openConfig(project) {
  if (project.id && project.local_path) {
    await refreshProjectConfig(project.id, project.local_path)
  }
  const hydrated = projects.value.find((p) => p.id === project.id) || project
  // A project.json that is Missing (fresh clone, or never seeded) has nothing to prefill from - offer
  // the same defaults `createNewProject` uses, rather than an empty list, but only when there is truly
  // nothing already in memory to keep (an unmigrated legacy value still wins).
  const configStatus = project.id ? getProjectConfigEntry(project.id).status : 'unknown'
  const useDefaultExcludes = shouldUseDefaultExcludes(configStatus, hydrated.pull_excludes, hydrated.push_excludes)
  const p = {
    ...hydrated,
    // Honor remote_host/remote_path/hooks the caller explicitly passed (e.g. `setRemoteHost`'s
    // switch-to-a-host-with-no-target case) instead of silently discarding them for the live project's
    // still-old values - a normal open (table gear icon) passes the live object itself, so this is a
    // no-op there.
    remote_host: project.remote_host,
    remote_path: project.remote_path,
    hooks: project.hooks
      ? { ...project.hooks }
      : hydrated.hooks
        ? { ...hydrated.hooks }
        : { pre_pull_cmd: null, post_pull_cmd: null, pre_push_cmd: null, post_push_cmd: null, run_hooks_on_remote: true },
    // deploy plan: same "caller override wins, else hydrated, else nothing configured" order as hooks.
    deploy: project.deploy
      ? { ...project.deploy }
      : hydrated.deploy
        ? { ...hydrated.deploy }
        : null,
    pull_excludes: useDefaultExcludes ? [...DEFAULT_PULL_EXCLUDES] : [...(hydrated.pull_excludes || [])],
    push_excludes: useDefaultExcludes ? [...DEFAULT_PUSH_EXCLUDES] : [...(hydrated.push_excludes || [])],
    production_url: hydrated.production_url ?? "",
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
    const result = await applyProjectConfig({ ...editingProject.value })
    // ApplyProjectConfig already changed nothing and showed its own error toast when the save was rejected (not-writable) or the write failed - no success toast, no log line, no closing the modal.
    if (isHost) {
      if (!result?.ok) return
      appendGlobalLog("CONFIG", `User ${isNew ? 'created new' : 'updated config for'} project "${projectDisplayName(editingProject.value)}".`)
      Toast.fire({ icon: 'success', title: isNew ? 'Project created' : 'Config saved' })
      closeConfig()
      return
    }
    // On a companion screen `applyProjectConfig` is a fire-and-forget call over
    // the wire (services/action.js's `actionStub` sends once and always resolves `undefined` - there is no
    // reply channel this function can read a real outcome from). Never claim "Config saved" or close the
    // dialog here without the host's acknowledgment: show a neutral, existing-element toast only and leave
    // the dialog open, so a failed or rejected save on the host is never misreported as done on the phone.
    Toast.fire({ icon: 'info', title: 'Sent to the Mac' })
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
      pull_excludes: [...DEFAULT_PULL_EXCLUDES],
      push_excludes: [...DEFAULT_PUSH_EXCLUDES],
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
  const projectName = projectDisplayName(editingProject.value)

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
