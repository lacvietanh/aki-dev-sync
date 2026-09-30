// Pure-function coverage for src/composables/projectConfigPure.js (docs/plan/settings-and-state-layout.md,
// akiflow council slice D2, item 6 / S-c). Zero Vue/Tauri imports, same runner convention as
// replay-hydration.test.mjs.

import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  buildProjectSavePayload,
  buildProjectListSavePayload,
  resolveHostSwitch,
  buildProjectConfigSeeds,
  canSaveProjectConfig,
  shouldUseDefaultExcludes,
  replaceProjectById,
  getDeployCmd,
  deployRunOn,
  deployOnPush,
  deployTargetLabel,
  deployBlockedReason,
  shouldOfferDeployAfterPush,
} from '../../src/composables/projectConfigPure.js'

function baseProject(overrides = {}) {
  return {
    id: 'p1',
    local_path: '/Volumes/DEV/p1',
    remote_host: 'host-a',
    remote_path: '/srv/p1',
    hooks: { pre_pull_cmd: 'echo pre', post_pull_cmd: null, pre_push_cmd: null, post_push_cmd: null, run_hooks_on_remote: true, ignore_hook_errors: false },
    targets: {},
    name: 'P1',
    production_url: 'https://p1.example.com',
    pull_excludes: ['node_modules/'],
    push_excludes: ['dist/'],
    dev_cmd_override: 'npm run dev',
    build_cmd_override: 'npm run build',
    ...overrides,
  }
}

test('unreachable project keeps its registry copy through a save', () => {
  const p = baseProject()
  const payload = buildProjectSavePayload(p, 'unavailable')
  assert.equal(payload.name, 'P1')
  assert.equal(payload.production_url, 'https://p1.example.com')
  assert.deepEqual(payload.pull_excludes, ['node_modules/'])
  assert.deepEqual(payload.push_excludes, ['dist/'])
  assert.equal(payload.dev_cmd_override, 'npm run dev')
  assert.equal(payload.build_cmd_override, 'npm run build')
})

test('ok-status project has project-owned fields omitted (undefined), not hand-set to an empty shape', () => {
  const p = baseProject()
  const payload = buildProjectSavePayload(p, 'ok')
  assert.equal(payload.name, undefined)
  assert.equal(payload.production_url, undefined)
  assert.equal(payload.pull_excludes, undefined)
  assert.equal(payload.push_excludes, undefined)
  assert.equal(payload.dev_cmd_override, undefined)
  assert.equal(payload.build_cmd_override, undefined)
  // undefined-valued keys vanish on the wire (JSON.stringify, what the Tauri IPC layer sends) -
  // Rust's own #[serde(default)] then decides the value, never a hand-copied duplicate of it here.
  assert.equal(JSON.stringify(payload).includes('"name"'), false)
})

test('remote_path always round-trips at the top level, and a target holding hooks omits the top-level copy (undefined)', () => {
  const p = baseProject()
  const payload = buildProjectSavePayload(p, 'ok')
  assert.equal(payload.remote_path, '/srv/p1', 'remote_path is a project fact - never stripped')
  assert.equal(payload.hooks, undefined)
  assert.deepEqual(payload.targets['host-a'], { hooks: p.hooks })
})

test('empty remote_host project keeps hooks at the top level, never creates targets[""]', () => {
  const p = baseProject({ remote_host: '', remote_path: '/still/here', hooks: { pre_pull_cmd: 'x' } })
  const payload = buildProjectSavePayload(p, 'ok')
  assert.equal(payload.remote_path, '/still/here')
  assert.deepEqual(payload.hooks, { pre_pull_cmd: 'x' })
  assert.equal(Object.prototype.hasOwnProperty.call(payload.targets, ''), false)
})

// 1.32.0 fix: remote_path is a project fact, fixed across every host - resolveHostSwitch no longer returns or touches it at all; the caller keeps the project's existing remote_path unchanged on any host switch.
test('resolveHostSwitch to an unknown host yields empty hooks and leaves other hosts untouched', () => {
  const p = baseProject({
    remote_host: 'host-a',
    remote_path: '/srv/a',
    hooks: { pre_pull_cmd: 'a-hook' },
    targets: { 'host-b': { hooks: { pre_pull_cmd: 'b-hook' } } },
  })
  const result = resolveHostSwitch(p, 'host-c')
  assert.equal(result.remote_host, 'host-c')
  assert.equal(result.remote_path, undefined, 'remote_path is not part of the switch result at all')
  assert.equal(result.hooks.pre_pull_cmd, null)
  // host-a's outgoing state got recorded, host-b's saved target survives untouched.
  assert.deepEqual(result.targets['host-a'], { hooks: { pre_pull_cmd: 'a-hook' } })
  assert.deepEqual(result.targets['host-b'], { hooks: { pre_pull_cmd: 'b-hook' } })
  assert.equal('deploy' in result, false, 'deploy names its own host - a sync host switch never touches it')
})

test('N-d: resolveHostSwitch from an empty remote_host never creates targets[""]', () => {
  const p = baseProject({ remote_host: '', remote_path: '', hooks: null, targets: {} })
  const result = resolveHostSwitch(p, 'host-a')
  assert.equal(Object.prototype.hasOwnProperty.call(result.targets, ''), false)
})

test('buildProjectConfigSeeds skips projects without id/local_path and maps the rest', () => {
  const seeds = buildProjectConfigSeeds([
    baseProject({ id: 'a' }),
    { id: '', local_path: '/x' },
    { id: 'b', local_path: '' },
  ])
  assert.equal(seeds.length, 1)
  assert.equal(seeds[0].id, 'a')
  assert.equal(seeds[0].file.name, 'P1')
  assert.deepEqual(seeds[0].file.commands, { dev: 'npm run dev', build: 'npm run build' })
})

// CanSaveProjectConfig is the one function both the writer (applyProjectConfig)
// and the dialog's Save button (ProjectConfigModal.vue) call - a save is only allowed for a brand-new
// project, or an existing one whose project.json read already came back 'ok' or 'missing'.
test('canSaveProjectConfig: a brand-new project is always allowed regardless of status', () => {
  assert.equal(canSaveProjectConfig(true, 'unknown'), true)
  assert.equal(canSaveProjectConfig(true, 'unavailable'), true)
  assert.equal(canSaveProjectConfig(true, 'corrupt'), true)
})

test('canSaveProjectConfig: an existing project is allowed only for ok/missing, never unknown/unavailable/corrupt', () => {
  assert.equal(canSaveProjectConfig(false, 'ok'), true)
  assert.equal(canSaveProjectConfig(false, 'missing'), true)
  assert.equal(canSaveProjectConfig(false, 'unknown'), false, 'unknown (e.g. mid-Refresh) must block the save, not allow it')
  assert.equal(canSaveProjectConfig(false, 'unavailable'), false)
  assert.equal(canSaveProjectConfig(false, 'corrupt'), false)
})

// Fixture for replaceProjectById (see its doc comment). 2 projects x 2 hosts each.
function twoProjectsTwoHosts() {
  const a = baseProject({
    id: 'proj-a',
    remote_host: 'host-1',
    targets: {
      'host-1': { remote_path: '/srv/a1', hooks: null },
      'host-2': { remote_path: '/srv/a2', hooks: null },
    },
  })
  const b = baseProject({
    id: 'proj-b',
    remote_host: 'host-1',
    targets: {
      'host-1': { remote_path: '/srv/b1', hooks: null },
      'host-2': { remote_path: '/srv/b2', hooks: null },
    },
  })
  return [a, b]
}

test('replaceProjectById: replaces the target after the list was reordered, other entry is byte-identical', () => {
  const [a, b] = twoProjectsTwoHosts()
  const bBefore = JSON.stringify(b)
  const reordered = [b, a] // simulates a reorder that ran during the await window
  const updatedA = { ...a, remote_host: 'host-2', remote_path: '/srv/a2' }

  const { list, found } = replaceProjectById(reordered, 'proj-a', updatedA)

  assert.equal(found, true)
  assert.equal(list[0], b, 'proj-b entry is untouched (same reference)')
  assert.equal(JSON.stringify(list[0]), bBefore, 'proj-b, both hosts targets included, is byte-identical')
  assert.equal(list[1], updatedA)
  assert.equal(list.length, 2)
  // The input array itself is never mutated - a stale caller-held reference to `reordered` stays [b, a].
  assert.equal(reordered[1], a)
})

test('replaceProjectById: a vanished (removed) id changes nothing, other entry stays byte-identical', () => {
  const [a, b] = twoProjectsTwoHosts()
  const bBefore = JSON.stringify(b)
  const listWithoutA = [b] // simulates proj-a having been removed during the await window
  const updatedA = { ...a, remote_host: 'host-2' }

  const { list, found } = replaceProjectById(listWithoutA, 'proj-a', updatedA)

  assert.equal(found, false, 'a vanished id must be reported, never silently resurrected')
  assert.equal(list.length, 1)
  assert.equal(JSON.stringify(list[0]), bBefore, 'the one remaining project is byte-identical')
  assert.equal(list[0], b)
})

// The config dialog's default-exclude prefill decision.
test('shouldUseDefaultExcludes: only fires for a genuinely missing file with nothing already in memory', () => {
  assert.equal(shouldUseDefaultExcludes('missing', [], []), true)
  assert.equal(shouldUseDefaultExcludes('missing', ['node_modules/'], []), false, 'an unmigrated legacy value already in memory still wins')
  assert.equal(shouldUseDefaultExcludes('ok', [], []), false, 'an ok read with genuinely empty excludes is the user\'s own choice, not "nothing to read from yet"')
  assert.equal(shouldUseDefaultExcludes('unavailable', [], []), false)
  assert.equal(shouldUseDefaultExcludes('corrupt', [], []), false)
  assert.equal(shouldUseDefaultExcludes('unknown', [], []), false)
})

// Deploy plan pure predicates.
test('getDeployCmd trims, defaults to empty string, and falls back to the detected stack command', () => {
  assert.equal(getDeployCmd({ deploy_cmd: '  npm run deploy  ' }), 'npm run deploy')
  assert.equal(getDeployCmd({}), '')
  assert.equal(getDeployCmd(null), '')
  // Same fallback DEV/BUILD already have (ProjectTable.vue getDevCmd/getBuildCmd) - a saved commands.deploy always wins over the detected default, and an unset one falls back to it.
  assert.equal(getDeployCmd({}, 'npm run deploy'), 'npm run deploy')
  assert.equal(getDeployCmd({ deploy_cmd: 'wrangler deploy' }, 'npm run deploy'), 'wrangler deploy')
  assert.equal(getDeployCmd({ deploy_cmd: '  ' }, '  npm run deploy  '), 'npm run deploy')
  assert.equal(getDeployCmd({}, ''), '')
})

test('deployRunOn defaults to local unless the target says remote', () => {
  assert.equal(deployRunOn({}), 'local')
  assert.equal(deployRunOn({ deploy: { run_on: 'remote' } }), 'remote')
  assert.equal(deployRunOn({ deploy: { run_on: 'bogus' } }), 'local')
})

test('deployOnPush follows on_push regardless of run_on', () => {
  assert.equal(deployOnPush({ remote_host: 'prod', remote_path: '~/app', deploy: { run_on: 'remote', on_push: true, host: 'prod' } }), true)
  assert.equal(deployOnPush({ deploy: { run_on: 'local', on_push: true } }), true, 'on_push offers the confirm dialog whether the deploy runs local or remote')
  assert.equal(deployOnPush({ deploy: { run_on: 'remote', on_push: false } }), false)
  assert.equal(deployOnPush({}), false)
})

test('deployTargetLabel is "local" or "host:path"', () => {
  assert.equal(deployTargetLabel({ deploy: { run_on: 'local' } }), 'local')
  assert.equal(deployTargetLabel({ deploy: { run_on: 'remote', host: 'prod' }, remote_host: 'bien', remote_path: '~/app' }), 'prod:~/app', 'the deploy host, never the sync host; path falls back to remote_path')
  assert.equal(deployTargetLabel({ deploy: { run_on: 'remote', host: 'prod', path: '~/live' }, remote_host: 'bien', remote_path: '~/app' }), 'prod:~/live')
  assert.equal(deployTargetLabel({ deploy: { run_on: 'remote' }, remote_host: 'bien', remote_path: '~/app' }), '(no host):~/app')
})

// The pure decision behind the post-push `on_push` offer (useSync.js) - every branch named in
// docs/plan/deploy-action.md's own test list. `deployCmd` is the already-resolved command the caller
// (`resolveDeployCmd`) hands in; `syncSucceeded` is gone - a failed sync never reaches this call site at
// all (it lives inside `run_sync`'s try-block success branch), so there is nothing left here to test for it.
test('shouldOfferDeployAfterPush never fires on a dry run', () => {
  const project = { remote_host: 'prod', remote_path: '~/app', deploy: { run_on: 'remote', on_push: true, host: 'prod' } }
  assert.equal(shouldOfferDeployAfterPush({ direction: 'push', isDryRun: true, specificPaths: [], project, deployCmd: 'npm run deploy' }), false)
})

test('shouldOfferDeployAfterPush never fires on a specific-paths push', () => {
  const project = { remote_host: 'prod', remote_path: '~/app', deploy: { run_on: 'remote', on_push: true, host: 'prod' } }
  assert.equal(shouldOfferDeployAfterPush({ direction: 'push', isDryRun: false, specificPaths: ['a.txt'], project, deployCmd: 'npm run deploy' }), false)
})

test('shouldOfferDeployAfterPush never fires when on_push is off', () => {
  const project = { deploy: { run_on: 'remote', on_push: false } }
  assert.equal(shouldOfferDeployAfterPush({ direction: 'push', isDryRun: false, specificPaths: [], project, deployCmd: 'npm run deploy' }), false)
})

test('shouldOfferDeployAfterPush never fires on a pull, even with on_push on', () => {
  const project = { remote_host: 'prod', remote_path: '~/app', deploy: { run_on: 'remote', on_push: true, host: 'prod' } }
  assert.equal(shouldOfferDeployAfterPush({ direction: 'pull', isDryRun: false, specificPaths: [], project, deployCmd: 'npm run deploy' }), false)
})

test('shouldOfferDeployAfterPush offers after a real push with on_push on, local or remote', () => {
  const remote = { remote_host: 'prod', remote_path: '~/app', deploy: { run_on: 'remote', on_push: true, host: 'prod' } }
  const local = { deploy: { run_on: 'local', on_push: true } }
  assert.equal(shouldOfferDeployAfterPush({ direction: 'push', isDryRun: false, specificPaths: [], project: remote, deployCmd: 'npm run deploy' }), true)
  assert.equal(shouldOfferDeployAfterPush({ direction: 'push', isDryRun: false, specificPaths: [], project: local, deployCmd: 'npm run deploy' }), true)
})

// The resolved `deployCmd` is what decides this, not whether `commands.deploy` was saved - so a detected-only command (nothing saved, `getDeployCmd`'s fallback resolved it) still offers.
test('shouldOfferDeployAfterPush offers on a detected-only command (nothing saved)', () => {
  const project = { remote_host: 'prod', remote_path: '~/app', deploy: { run_on: 'remote', on_push: true, host: 'prod' } }
  assert.equal(shouldOfferDeployAfterPush({ direction: 'push', isDryRun: false, specificPaths: [], project, deployCmd: 'npm run deploy' }), true)
})

test('shouldOfferDeployAfterPush never fires without any deploy command, detected or saved', () => {
  const project = { remote_host: 'prod', remote_path: '~/app', deploy: { run_on: 'remote', on_push: true, host: 'prod' } }
  assert.equal(shouldOfferDeployAfterPush({ direction: 'push', isDryRun: false, specificPaths: [], project, deployCmd: '' }), false)
})

test('deployBlockedReason: a remote deploy with no host of its own is not runnable', () => {
  assert.equal(deployBlockedReason({ deploy: { run_on: 'local' } }), '')
  assert.match(deployBlockedReason({ remote_host: 'bien', remote_path: '~/app', deploy: { run_on: 'remote' } }), /no host/)
  assert.equal(deployBlockedReason({ remote_path: '~/app', deploy: { run_on: 'remote', host: 'prod' } }), '')
})

test('shouldOfferDeployAfterPush: a push to a different host than the deploy host never offers it', () => {
  const project = { remote_host: 'staging', remote_path: '~/app', deploy: { run_on: 'remote', on_push: true, host: 'prod' } }
  assert.equal(shouldOfferDeployAfterPush({ direction: 'push', isDryRun: false, specificPaths: [], project, deployCmd: 'npm run deploy' }), false)
  const noHost = { remote_host: 'prod', remote_path: '~/app', deploy: { run_on: 'remote', on_push: true } }
  assert.equal(shouldOfferDeployAfterPush({ direction: 'push', isDryRun: false, specificPaths: [], project: noHost, deployCmd: 'npm run deploy' }), false, 'no implicit sync host')
})

test('buildProjectSavePayload keeps deploy at the top level, never inside targets', () => {
  const p = baseProject({ deploy: { run_on: 'remote', on_push: true, host: 'prod' } })
  const payload = buildProjectSavePayload(p, 'ok')
  assert.deepEqual(payload.deploy, { run_on: 'remote', on_push: true, host: 'prod' })
  assert.equal('deploy' in payload.targets['host-a'], false)
})

test('resolveHostSwitch leaves deploy alone on any sync host switch', () => {
  const p = baseProject({ deploy: { run_on: 'remote', on_push: true, host: 'prod' } })
  const result = resolveHostSwitch(p, 'host-b')
  assert.equal('deploy' in result, false)
  assert.equal('deploy' in result.targets['host-a'], false)
})

// CLAUDE.md multi-entity guard: editing one project's deploy leaves the other project, and every host target of both, JSON.stringify-identical.
test('buildProjectListSavePayload: editing one project deploy leaves every other project and target untouched', () => {
  const projA = baseProject({
    id: 'proj-a',
    remote_host: 'host-a',
    deploy: { run_on: 'local', on_push: false },
    targets: { 'host-b': { hooks: { pre_pull_cmd: 'b' } } },
  })
  const projB = baseProject({ id: 'proj-b', name: 'P2', remote_host: 'host-p', deploy: { run_on: 'remote', on_push: true, host: 'prod' } })
  const before = buildProjectListSavePayload([projA, projB], () => 'unavailable')
  const projAEdited = { ...projA, deploy: { run_on: 'remote', on_push: true, host: 'prod' } }
  const after = buildProjectListSavePayload([projAEdited, projB], () => 'unavailable')
  assert.deepEqual(after[0].deploy, { run_on: 'remote', on_push: true, host: 'prod' })
  assert.equal(JSON.stringify(after[0].targets), JSON.stringify(before[0].targets), 'proj-a targets changed')
  assert.equal(JSON.stringify(after[1]), JSON.stringify(before[1]), 'proj-b (a different project entirely) changed')
})
