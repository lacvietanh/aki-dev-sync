// Pure, Vue/Tauri-free logic behind the registry save funnel, host-switch resolution and the config-seed
// batch (docs/plan/done/settings-and-state-layout.md). Extracted so `node --test` can exercise the exact
// functions `useProjectConfig.js` calls, without dragging Vue reactivity or the Tauri IPC layer into the
// test runner (same reasoning as `src/services/replayHydration.js`).

/**
 * T2 (docs/plan/done/settings-and-state-layout.md): the one default-exclude list, shared by `createNewProject`
 * (a brand-new project) and the config dialog's own missing-`project.json` prefill (an existing project
 * whose file was never seeded, or was deleted) - so the two "nothing to read from yet" paths never drift
 * apart into two different defaults.
 */
export const DEFAULT_PULL_EXCLUDES = Object.freeze([
  '.DS_Store', '*.log', '.git/', 'node_modules/', '__pycache__/', '.nuxt/', '.output/', '.wrangler/', 'dist/', '.claude/',
])
export const DEFAULT_PUSH_EXCLUDES = Object.freeze([
  '.DS_Store', '*.log', 'node_modules/', '__pycache__/', '.nuxt/', '.output/', '.wrangler/', 'dist/', '.claude/',
])

/**
 * The save decision itself, pure and atomic - `applyProjectConfig` (the writer)
 * and the config dialog's Save button both call this exact function, so neither can drift from the other.
 * A brand-new project (`isNew`) is always allowed - its folder was just verified by the OS directory picker,
 * before any project.json read has landed for it. An existing project may only save when its own last read
 * of project.json came back `'ok'` (file exists and parses) or `'missing'` (file absent, safe to create) -
 * `'unknown'` (mid-Refresh, or never read yet), `'unavailable'` and `'corrupt'` must never let a save
 * proceed, because the writer cannot tell whether it would silently clobber a file it never actually read.
 */
const SAVE_ALLOWED_STATUSES = new Set(['ok', 'missing'])
export function canSaveProjectConfig(isNew, status) {
  return !!isNew || SAVE_ALLOWED_STATUSES.has(status)
}

/**
 * One project's `save_projects` payload (docs/plan/done/settings-and-state-layout.md § B1/B2, amended § Amendments
 * for the 1.32.0 remote_path fix). Project-owned fields (name, production_url, excludes, dev/build commands)
 * are stripped ONLY once `project.json` already owns them (`configStatus === 'ok'`) - any other status
 * (missing/unavailable/corrupt/unknown) means the registry copy is still the only one and must round-trip
 * unchanged: a legacy copy is removed only once its new home holds it.
 *
 * `remote_path` is a PROJECT fact (one remote directory regardless of host, 1.32.0) and always round-trips
 * at the top level - it is never written into `targets[remote_host]` and never stripped. `deploy` is a
 * project fact that names its own host (2026-09-28) and round-trips at the top level too. `hooks` stay
 * genuinely host-specific and are written into `targets[remote_host]`, cleared from the top level ONLY
 * when `remote_host` is truthy, so a `targets['']` key is never created (N-d) and a half-configured project
 * (empty host) keeps its only copy of hooks at the top level (S-a).
 *
 * A stripped/moved field is set to `undefined`, never a hand-copied "empty shape" (e.g. a duplicated
 * `SyncHooks::default()` literal) - `undefined` is omitted by `JSON.stringify` (what the Tauri IPC layer
 * sends), so the key is simply absent from the wire payload and Rust's own `#[serde(default)]` on
 * `SyncProject` decides the value. A hand copy drifts silently the moment the real Rust default gains or
 * changes a field; omitting the key cannot drift, because there is nothing here to keep in sync with it.
 */
export function buildProjectSavePayload(project, configStatus) {
  const targets = { ...(project.targets || {}) }
  const targetHoldsRemote = !!project.remote_host
  if (targetHoldsRemote) {
    targets[project.remote_host] = { hooks: project.hooks || null }
  }
  const stripProjectOwned = configStatus === 'ok'
  return {
    ...project,
    targets,
    name: stripProjectOwned ? undefined : project.name,
    production_url: stripProjectOwned ? undefined : (project.production_url ?? undefined),
    pull_excludes: stripProjectOwned ? undefined : (project.pull_excludes || []),
    push_excludes: stripProjectOwned ? undefined : (project.push_excludes || []),
    dev_cmd_override: stripProjectOwned ? undefined : (project.dev_cmd_override ?? undefined),
    build_cmd_override: stripProjectOwned ? undefined : (project.build_cmd_override ?? undefined),
    // 1.32.0: remote_path always round-trips at the top level, regardless of remote_host/targets.
    remote_path: project.remote_path,
    hooks: targetHoldsRemote ? undefined : project.hooks,
    deploy: project.deploy || undefined,
    last_sync_action: undefined,
    last_sync_time: undefined,
    last_sync_host: undefined,
    last_sync_status: undefined,
  }
}

/**
 * Deploy plan pure predicates (docs/plan/done/deploy-action.md). `getDeployCmd` is the pure resolver; its only
 * production caller is `useDeploy.js::resolveDeployCmd`, which supplies the detected fallback read from the
 * `projectRuntime` store (`stack_info` never lives on the project object itself, so a truly
 * zero-arg resolver here would silently always return the saved value with no fallback). Every other
 * caller - the button, its tooltip, the D badge, the post-push offer, the confirm dialog - goes through
 * `resolveDeployCmd`, never this function directly, outside tests. Kept here (not in `useDeploy.js`)
 * because this file is the one Tauri/Vue-free module `node --test` can import.
 */
export function getDeployCmd(project, detectedDeployCmd) {
  return (project?.deploy_cmd ?? '').trim() || (detectedDeployCmd ?? '').trim()
}

export function deployRunOn(project) {
  return project?.deploy?.run_on === 'remote' ? 'remote' : 'local'
}

export function deployOnPush(project) {
  return !!project?.deploy?.on_push
}

/**
 *  (docs/plan/done/deploy-action.md): the ONE place the post-push `on_push` offer is decided - the
 * caller (`useSync.js`, only reachable from `run_sync`'s try-block success path, so a failed sync never
 * gets here) resolves the deploy command once via `useDeploy.js::resolveDeployCmd` and passes it in as
 * `deployCmd`, rather than this function re-deriving it (that duplication was a real gap: two
 * copies of the resolver each dropped the detected-command fallback). `node --test` exercises every other
 * branch (dry run, specific-paths push, on_push off/on, local/remote, detected-only vs no command at all).
 */
export function shouldOfferDeployAfterPush({ direction, isDryRun, specificPaths, project, deployCmd }) {
  if (isDryRun) return false
  if (direction !== 'push') return false
  if ((specificPaths || []).length > 0) return false
  if (deployBlockedReason(project)) return false
  // A push to a staging box must never offer deploying the production one.
  if (deployRunOn(project) === 'remote' && deployRemoteTarget(project).host !== project.remote_host) return false
  return deployOnPush(project) && !!deployCmd
}

/** Where a remote deploy runs: its own `host`, never the active sync host (docs/research/sync-host-safety.md); `path` falls back to the project's remote_path. */
export function deployRemoteTarget(project) {
  return { host: (project?.deploy?.host ?? '').trim(), path: (project?.deploy?.path ?? '').trim() || project?.remote_path || '' }
}

/** Why deploy cannot run as configured, or '' when it can. */
export function deployBlockedReason(project) {
  if (deployRunOn(project) !== 'remote') return ''
  const { host, path } = deployRemoteTarget(project)
  if (!host) return 'Remote deploy has no host - pick one in Project Settings'
  if (!path) return 'Remote deploy has no path - set one in Project Settings'
  return ''
}

export function deployTargetLabel(project) {
  if (deployRunOn(project) !== 'remote') return 'local'
  const { host, path } = deployRemoteTarget(project)
  return `${host || '(no host)'}:${path}`
}

/**
 * The ONE mutation site every post-await writer resolves by id through, never
 * by an index/reference captured before the `await`. Returns a NEW array plus `found` (never a silent no-op
 * on a vanished id).
 */
export function replaceProjectById(list, id, next) {
  const index = (list || []).findIndex((p) => p.id === id)
  if (index === -1) return { list: list || [], found: false }
  const copy = [...list]
  copy[index] = next
  return { list: copy, found: true }
}

/**
 * The ONE whole-list payload builder `saveProjectsList` calls before every `save_projects` invoke -
 * delegates to `buildProjectSavePayload` per project so the whole registry is stripped/moved consistently
 * in one pass.
 */
export function buildProjectListSavePayload(projectList, getConfigStatus) {
  return (projectList || []).map((p) => buildProjectSavePayload(p, getConfigStatus(p.id)))
}

/**
 * Pure host-switch resolution shared by the table dropdown (`remoteActions.setRemoteHost`) and the config
 * dialog's own Remote Host select (S3) - having this logic in two places is exactly the bug S3 describes.
 *
 * 1.32.0 fix: `remote_path` is a project fact (one remote directory regardless of host - see
 * `docs/plan/done/settings-and-state-layout.md` § Amendments), so a host switch never touches it; the caller
 * keeps `project.remote_path` unchanged, and `deploy` names its own host so it never follows. Only `hooks` are genuinely host-specific: the outgoing
 * host's current values are recorded into `targets` (so switching back restores them), then the incoming
 * host's saved target is resolved if one exists. Never mutates `project`.
 */
export function resolveHostSwitch(project, newHost) {
  const nextTargets = { ...(project.targets || {}) }
  // An empty remote_host has no target key to record itself under - never create targets[''].
  if (project.remote_host) {
    nextTargets[project.remote_host] = { hooks: project.hooks || null }
  }
  const incoming = nextTargets[newHost]
  let hooks
  if (incoming?.hooks) {
    hooks = { ...incoming.hooks }
  } else if (incoming) {
    // A saved target with no recorded hooks yet: nothing to reset to, keep current.
    hooks = project.hooks
  } else {
    // No saved target at all (F5): never reuse the old host's hooks, start clean.
    hooks = { pre_pull_cmd: null, post_pull_cmd: null, pre_push_cmd: null, post_push_cmd: null, run_hooks_on_remote: true, ignore_hook_errors: false }
  }
  return {
    targets: nextTargets,
    remote_host: newHost,
    hooks,
  }
}

/**
 * T2 (docs/plan/done/settings-and-state-layout.md): whether the config dialog should prefill the default
 * exclude lists instead of an empty one - only when project.json is genuinely `missing` (fresh clone, or
 * never seeded) AND nothing is already sitting in memory to keep (an unmigrated legacy value still wins).
 */
export function shouldUseDefaultExcludes(configStatus, pullExcludes, pushExcludes) {
  const hasNoExcludesYet = !(pullExcludes || []).length && !(pushExcludes || []).length
  return configStatus === 'missing' && hasNoExcludesYet
}

/**
 * `write_project_configs_if_missing` seed batch (docs/plan/done/settings-and-state-layout.md § C) - pure so the
 * exact shape the Rust command receives can be asserted without a live invoke.
 */
export function buildProjectConfigSeeds(loadedProjects) {
  return (loadedProjects || [])
    .filter((p) => p?.id && p?.local_path)
    .map((p) => ({
      id: p.id,
      local_path: p.local_path,
      file: {
        name: p.name || '',
        production_url: p.production_url || '',
        pull_excludes: p.pull_excludes || [],
        push_excludes: p.push_excludes || [],
        commands: { dev: p.dev_cmd_override || '', build: p.build_cmd_override || '' },
      },
    }))
}
