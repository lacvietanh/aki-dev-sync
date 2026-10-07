#!/usr/bin/env node
// Mac-only, one-shot: moves the three real post_push deploy hooks (docs/plan/done/deploy-action.md's last
// execution step, held UNTICKED pending the Mac F3 console check) from sync hooks into
// commands.deploy and the project-level `deploy`, each on its own named production host (docs/research/sync-host-safety.md). Never run against ~/.aki/devsync from this dev box - test only
// against the fixtures this script builds itself (see runSelfTest below).
//
// Usage:
//   node scripts/migrate-deploy-hooks.mjs                 # dry run (default) - prints before/after, writes nothing
//   node scripts/migrate-deploy-hooks.mjs --apply          # writes projects.json + each project.json
//   node scripts/migrate-deploy-hooks.mjs --self-test      # runs against throwaway fixtures only, then exits
//
// Optional overrides (for the self-test / a non-default install):
//   --projects-json <path>   default: $HOME/.aki/devsync/projects.json
//   --force-not-running       skip the "is the app running?" check when it cannot be verified

import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { execSync } from 'node:child_process'

// Each move's `host` is where that project's hook lives in `targets` AND where production actually runs
// (measured 2026-09-28: aki-gegrok-bot's process runs on grokvm, not bien) - named, never "the sync host".
const deployFor = (move) => ({ run_on: 'remote', on_push: true, host: move.host })

// The three moves this step performs, and nothing else (docs/plan/done/deploy-action.md's own quoted text - aborts rather than guessing if the live hook text has drifted from this).
const MOVES = [
  {
    slug: 'api.akitao.com',
    host: 'bien',
    expectedHook: 'cd ~/aki/web/api.akitao.com && npm run deploy',
    deployCmd: 'npm run deploy',
  },
  {
    slug: 'aki-gegrok-bot',
    host: 'grokvm',
    expectedHook: 'cd ~/aki/run/aki-gegrok-bot && bash scripts/deploy-restart.sh',
    deployCmd: 'bash scripts/deploy-restart.sh',
  },
  {
    slug: 'akidevrule',
    host: 'bien',
    expectedHook: "bash -lc 'cd ~/aki/AkiDevRule && ./install.sh'",
    deployCmd: './install.sh',
  },
]

function parseArgs(argv) {
  const apply = argv.includes('--apply')
  const selfTest = argv.includes('--self-test')
  const forceNotRunning = argv.includes('--force-not-running')
  const idx = argv.indexOf('--projects-json')
  const projectsJsonPath = idx !== -1 ? argv[idx + 1] : path.join(os.homedir(), '.aki', 'devsync', 'projects.json')
  return { apply, selfTest, forceNotRunning, projectsJsonPath }
}

/** Both processes that can hold projects.json's write lock: the shipped `.app` bundle, and a `tauri dev`
 * run (`npm run tauri dev` -> cargo builds `target/debug/aki-dev-sync`, the package name in
 * src-tauri/Cargo.toml - confirmed by an actual `cargo build` producing that exact binary path). Exported
 * so runSelfTest can assert this list never silently drops one of the two. */
const RUNNING_PROCESS_PATTERNS = ['Aki Dev Sync.app', 'target/debug/aki-dev-sync']

/** Best-effort "is Aki Dev Sync running right now" check - macOS only (CLAUDE.md: this app ships
 * macOS-only). Returns 'running' | 'not-running' | 'unknown'. */
function detectAppRunning() {
  if (os.platform() !== 'darwin') return 'unknown'
  try {
    const running = RUNNING_PROCESS_PATTERNS.some((pattern) => {
      const out = execSync(`pgrep -f ${JSON.stringify(pattern)} || true`, { encoding: 'utf8' })
      return out.trim().length > 0
    })
    return running ? 'running' : 'not-running'
  } catch {
    return 'unknown'
  }
}

function readJson(p) {
  return JSON.parse(fs.readFileSync(p, 'utf8'))
}

/** Same shape as `.akidevsync/project.json`, mirroring `project_config.rs`'s defaults for a Missing file. */
function defaultProjectConfigFile() {
  return { name: '', production_url: '', pull_excludes: [], push_excludes: [], commands: { dev: '', build: '', deploy: '' } }
}

/**
 * Reads one project's `project.json` the same way `project_config.rs::read_blocking` classifies it -
 * 'ok' | 'missing' | 'unavailable' | 'corrupt' - so this script can refuse a write exactly where the Rust
 * writer would (corrupt/unavailable), never guessing past it.
 */
function readProjectConfig(localPath) {
  if (!fs.existsSync(localPath) || !fs.statSync(localPath).isDirectory()) {
    return { status: 'unavailable' }
  }
  const file = path.join(localPath, '.akidevsync', 'project.json')
  if (!fs.existsSync(file)) return { status: 'missing', file: defaultProjectConfigFile() }
  let raw
  try {
    raw = fs.readFileSync(file, 'utf8')
  } catch {
    return { status: 'unavailable' }
  }
  try {
    const parsed = JSON.parse(raw)
    return { status: 'ok', file: { ...defaultProjectConfigFile(), ...parsed, commands: { dev: '', build: '', deploy: '', ...(parsed.commands || {}) } } }
  } catch {
    return { status: 'corrupt' }
  }
}

/** Backs up an existing `project.json` before this script changes it. A `missing` file (nothing on disk
 * yet) has nothing to back up - returns null rather than fabricating an empty backup. */
function backupProjectConfig(localPath) {
  const file = path.join(localPath, '.akidevsync', 'project.json')
  if (!fs.existsSync(file)) return null
  const backupPath = `${file}.bak-${new Date().toISOString().replace(/[:.]/g, '-')}`
  fs.copyFileSync(file, backupPath)
  return backupPath
}

/** Read-modify-write matching `project_config.rs::write_blocking`'s contract: only `commands.deploy`
 * changes, every other key (including unknown ones) is carried forward untouched. */
function writeProjectConfigDeploy(localPath, currentFile, deployCmd) {
  const next = { ...currentFile, commands: { ...currentFile.commands, deploy: deployCmd } }
  const dir = path.join(localPath, '.akidevsync')
  fs.mkdirSync(dir, { recursive: true })
  fs.writeFileSync(path.join(dir, 'project.json'), JSON.stringify(next, null, 2) + '\n')
}

/** The one identity key this script uses: `targets.<move.host>.hooks.post_push_cmd` (`projects.rs`'s
 * `Target.hooks: Option<SyncHooks>`) if that target exists, else the legacy top-level `hooks`
 * (`projects.rs`'s `SyncProject.hooks: SyncHooks`) ONLY when this project's active `remote_host` is
 * that host too - same precedence as the app's own `effectiveHooks`, but scoped to the move's host
 * rather than "whatever host is currently active", since identity here must not depend on a name/path
 * field that may not even be populated (S3: `name` can be empty post-1.32.0, and a basename comparison
 * is case-sensitive and can silently miss the real folder).
 */
function hostPostPushHook(project, host) {
  const target = project.targets?.[host]
  if (target) return target.hooks?.post_push_cmd ?? null
  if (project.remote_host === host) return project.hooks?.post_push_cmd ?? null
  return null
}

/** Every project whose post_push_cmd on the move's host equals a move's quoted text - the plan's own path table is a
 * label for the log output, never the identity check. */
function findMatches(projects, move) {
  return projects.filter((p) => (hostPostPushHook(p, move.host) || '').trim() === move.expectedHook)
}

function planOneMove(projects, move) {
  const matches = findMatches(projects, move)
  if (matches.length !== 1) {
    return {
      move,
      ok: false,
      reason: `expected exactly one project with a ${move.host} post_push_cmd equal to ${JSON.stringify(move.expectedHook)}, found ${matches.length}`,
    }
  }
  const project = matches[0]
  const configRead = readProjectConfig(project.local_path)
  if (configRead.status === 'corrupt' || configRead.status === 'unavailable') {
    return { move, ok: false, reason: `"${project.local_path}" project.json is ${configRead.status} - refusing to write over it (same contract as project_config.rs)` }
  }
  return { move, ok: true, project, configRead, hostHasTarget: !!project.targets?.[move.host] }
}

function printPlanned(planned) {
  for (const p of planned) {
    if (!p.ok) {
      console.log(`\n[SKIP] ${p.move.slug}: ${p.reason}`)
      continue
    }
    const { project, move, configRead } = p
    console.log(`\n[${p.ok ? 'OK' : 'SKIP'}] ${move.slug}  (${project.local_path})`)
    console.log(`  project.json commands.deploy:`)
    console.log(`    before: ${JSON.stringify(configRead.file.commands.deploy || '')}`)
    console.log(`    after:  ${JSON.stringify(move.deployCmd)}`)
    console.log(`  projects.json deploy:`)
    console.log(`    before: ${JSON.stringify(project.deploy || null)}`)
    console.log(`    after:  ${JSON.stringify(deployFor(move))}`)
    console.log(`  projects.json ${p.hostHasTarget ? `targets.${move.host}.hooks` : 'hooks'}.post_push_cmd:`)
    console.log(`    before: ${JSON.stringify(move.expectedHook)}`)
    console.log(`    after:  ${JSON.stringify(null)}`)
  }
}

function applyOneMove(projects, planned) {
  const { project, move, hostHasTarget } = planned
  writeProjectConfigDeploy(project.local_path, planned.configRead.file, move.deployCmd)
  const targets = { ...(project.targets || {}) }
  const existingTarget = targets[move.host] || { remote_path: project.remote_path, hooks: project.hooks || null }
  const nextHooks = { ...(existingTarget.hooks || project.hooks || {}) }
  nextHooks.post_push_cmd = null
  targets[move.host] = { ...existingTarget, hooks: nextHooks }
  project.targets = targets
  project.deploy = deployFor(move)
  if (!hostHasTarget) {
    // Legacy top-level hooks copy: clear the moved field there too so the two copies never disagree.
    project.hooks = { ...(project.hooks || {}), post_push_cmd: null }
  }
}

function run({ apply, forceNotRunning, projectsJsonPath }) {
  if (apply) {
    const runningState = detectAppRunning()
    if (runningState === 'running') {
      console.error('ERROR: Aki Dev Sync is running - quit the app before applying (it holds its own write lock on projects.json).')
      process.exit(1)
    }
    if (runningState === 'unknown' && !forceNotRunning) {
      console.error('Could not verify whether Aki Dev Sync is running - quit the app first, then re-run with --force-not-running.')
      process.exit(1)
    }
  }

  if (!fs.existsSync(projectsJsonPath)) {
    console.error(`ERROR: ${projectsJsonPath} not found`)
    process.exit(1)
  }
  const projects = readJson(projectsJsonPath)
  const planned = MOVES.map((move) => planOneMove(projects, move))
  printPlanned(planned)

  const touched = planned.filter((p) => p.ok)
  const skipped = planned.filter((p) => !p.ok)
  if (skipped.length > 0) {
    console.error(`\nABORTING: ${skipped.length}/${MOVES.length} move(s) failed their precondition - see [SKIP] reasons above. No file was written.`)
    process.exit(1)
  }

  if (!apply) {
    console.log('\nDry run only - re-run with --apply to write. No file was touched.')
    return
  }

  const backupPath = `${projectsJsonPath}.bak-${new Date().toISOString().replace(/[:.]/g, '-')}`
  fs.copyFileSync(projectsJsonPath, backupPath)
  console.log(`\nBacked up ${projectsJsonPath} -> ${backupPath}`)

  // Every project.json this run will touch is backed up BEFORE any of them is written - a failure partway through the write loop below can never leave a touched file without its own backup already in place
  for (const p of touched) backupProjectConfig(p.project.local_path)

  const beforeOthers = snapshotOthers(projects, touched.map((p) => p.project.local_path))
  for (const p of touched) applyOneMove(projects, p)
  const afterOthers = snapshotOthers(projects, touched.map((p) => p.project.local_path))
  if (JSON.stringify(beforeOthers) !== JSON.stringify(afterOthers)) {
    console.error('ABORTING WRITE: an untouched project/host changed - this must never happen. Restore from the backup and report this.')
    process.exit(1)
  }

  fs.writeFileSync(projectsJsonPath, JSON.stringify(projects, null, 2) + '\n')
  console.log(`\nWrote ${projectsJsonPath}. ${touched.length} project(s) migrated: ${touched.map((p) => p.move.slug).join(', ')}.`)
}

/** Multi-entity guard evidence (CLAUDE.md): every project NOT in this run's touched set, byte-compared
 * before and after the write. */
function snapshotOthers(projects, touchedLocalPaths) {
  return projects
    .filter((p) => !touchedLocalPaths.includes(p.local_path))
    .map((p) => JSON.stringify(p))
}

// ---------------------------------------------------------------------------------------------------------
// Self-test: builds throwaway fixtures under os.tmpdir(), runs the real functions above against them ONLY,
// never touches ~/.aki/devsync. Exercises: the 3 real moves + 2 untouched projects x 2 hosts (multi-entity
// guard), a hook-text mismatch (must abort with nothing written), and a missing project (must abort).
// ---------------------------------------------------------------------------------------------------------
function buildFixture(root) {
  fs.mkdirSync(root, { recursive: true })
  function project(id, slug, dir, hooks, extraTargets = {}, host = 'bien') {
    const localPath = path.join(root, dir)
    fs.mkdirSync(path.join(localPath, '.akidevsync'), { recursive: true })
    fs.writeFileSync(
      path.join(localPath, '.akidevsync', 'project.json'),
      JSON.stringify({ name: slug, commands: { dev: 'npm run dev', build: 'npm run build' }, some_future_field: 'do-not-drop-me' }, null, 2),
    )
    return {
      id,
      name: '',
      local_path: localPath,
      remote_host: host,
      targets: { [host]: { remote_path: `~/aki/x/${dir}`, hooks }, ...extraTargets },
    }
  }
  const projects = [
    // Deliberately misleading dir/name (S3: identity is the bien post_push_cmd text, never name/folder) - folder and project.json `name` both say something else entirely; the hook text is the real key.
    project('p-api', 'not-the-real-name', 'totally-different-folder-xyz', { post_push_cmd: 'cd ~/aki/web/api.akitao.com && npm run deploy' }),
    project('p-bot', 'aki-gegrok-bot', 'aki-gegrok-bot', { post_push_cmd: 'cd ~/aki/run/aki-gegrok-bot && bash scripts/deploy-restart.sh' }, {}, 'grokvm'),
    project('p-rule', 'akidevrule', 'akidevrule', { post_push_cmd: "bash -lc 'cd ~/aki/AkiDevRule && ./install.sh'" }),
    // Untouched control group: 2 other projects x 2 hosts each - must stay byte-identical.
    project('p-other1', 'other-project-1', 'other-project-1', { post_push_cmd: 'npm run build' }, {
      akicloud: { remote_path: '~/aki/other1', hooks: { pre_push_cmd: 'echo hi' } },
    }),
    project('p-other2', 'other-project-2', 'other-project-2', {}, {
      akicloud: { remote_path: '~/aki/other2', hooks: null },
    }),
  ]
  const projectsJsonPath = path.join(root, 'projects.json')
  fs.writeFileSync(projectsJsonPath, JSON.stringify(projects, null, 2))
  return { root, projectsJsonPath, projects }
}

function assert(cond, msg) {
  if (!cond) throw new Error(`SELF-TEST FAILED: ${msg}`)
}

function runSelfTest() {
  // Regression guard: the running-app check must catch BOTH the packaged .app
  // AND a `tauri dev` run, never just the one someone happened to test with. Asserting the pattern list
  // directly (rather than actually spawning a background process) is the cheap form of this check -
  // real process spawning + pgrep is the OS-dependent, flaky-under-CI form this self-test avoids.
  assert(RUNNING_PROCESS_PATTERNS.includes('Aki Dev Sync.app'), 'must still catch the packaged .app')
  assert(RUNNING_PROCESS_PATTERNS.includes('target/debug/aki-dev-sync'), 'must also catch a `tauri dev` process, not only the packaged .app')

  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'aki-deploy-hooks-selftest-'))
  try {
    const { projectsJsonPath } = buildFixture(root)
    const beforeOther1 = fs.readFileSync(path.join(root, 'other-project-1', '.akidevsync', 'project.json'), 'utf8')
    const beforeOther2 = fs.readFileSync(path.join(root, 'other-project-2', '.akidevsync', 'project.json'), 'utf8')
    const beforeProjectsJson = fs.readFileSync(projectsJsonPath, 'utf8')

    // 1) Dry run must write nothing at all.
    run({ apply: false, forceNotRunning: true, projectsJsonPath })
    assert(fs.readFileSync(projectsJsonPath, 'utf8') === beforeProjectsJson, 'dry run must not touch projects.json')

    // 2) Apply: the 3 real moves land, the 2 other projects x their hosts stay byte-identical.
    run({ apply: true, forceNotRunning: true, projectsJsonPath })
    const after = readJson(projectsJsonPath)
    const api = after.find((p) => p.id === 'p-api')
    assert(JSON.stringify(api.deploy) === JSON.stringify({ run_on: 'remote', on_push: true, host: 'bien' }), 'api.akitao.com: project deploy not set to bien')
    assert(!('deploy' in api.targets.bien), 'api.akitao.com: deploy must not land in targets.bien')
    assert(api.targets.bien.hooks.post_push_cmd === null, 'api.akitao.com: post_push_cmd not cleared')
    const apiConfig = readJson(path.join(api.local_path, '.akidevsync', 'project.json'))
    assert(apiConfig.commands.deploy === 'npm run deploy', 'api.akitao.com: commands.deploy not written')
    assert(apiConfig.some_future_field === 'do-not-drop-me', 'api.akitao.com: unknown key was dropped from project.json')

    const bot = after.find((p) => p.id === 'p-bot')
    assert(bot.deploy.host === 'grokvm' && bot.targets.grokvm.hooks.post_push_cmd === null, 'aki-gegrok-bot: deploy must name grokvm and its grokvm hook must be cleared')
    assert(readJson(path.join(bot.local_path, '.akidevsync', 'project.json')).commands.deploy === 'bash scripts/deploy-restart.sh', 'aki-gegrok-bot: wrong deploy cmd')

    const rule = after.find((p) => p.id === 'p-rule')
    assert(readJson(path.join(rule.local_path, '.akidevsync', 'project.json')).commands.deploy === './install.sh', 'akidevrule: wrong deploy cmd')

    assert(fs.readFileSync(path.join(root, 'other-project-1', '.akidevsync', 'project.json'), 'utf8') === beforeOther1, 'other-project-1 project.json changed')
    assert(fs.readFileSync(path.join(root, 'other-project-2', '.akidevsync', 'project.json'), 'utf8') === beforeOther2, 'other-project-2 project.json changed')
    const other1 = after.find((p) => p.id === 'p-other1')
    const other2 = after.find((p) => p.id === 'p-other2')
    assert(other1.targets.akicloud.hooks.pre_push_cmd === 'echo hi', 'other-project-1 akicloud target changed')
    assert(other1.targets.bien.hooks.post_push_cmd === 'npm run build', 'other-project-1 bien target changed (must never be touched)')
    assert(!other2.deploy && !other2.targets.bien.deploy, 'other-project-2 must not have gained a deploy key')
    const registryBackups = fs.readdirSync(root).filter((f) => f.startsWith('projects.json.bak-'))
    assert(registryBackups.length === 1, `expected exactly one projects.json backup, found ${registryBackups.length}`)
    for (const p of [api, bot, rule]) {
      const configBackups = fs.readdirSync(path.join(p.local_path, '.akidevsync')).filter((f) => f.startsWith('project.json.bak-'))
      assert(configBackups.length === 1, `expected exactly one project.json backup for ${p.local_path}, found ${configBackups.length}`)
    }

    // 3) A hook-text mismatch must abort the WHOLE run with nothing written.
    const root2 = fs.mkdtempSync(path.join(os.tmpdir(), 'aki-deploy-hooks-selftest-mismatch-'))
    const { projectsJsonPath: pj2 } = buildFixture(root2)
    const projects2 = readJson(pj2)
    projects2.find((p) => p.id === 'p-api').targets.bien.hooks.post_push_cmd = 'cd ~/aki/web/api.akitao.com && npm run deploy:staging'
    fs.writeFileSync(pj2, JSON.stringify(projects2, null, 2))
    const before2 = fs.readFileSync(pj2, 'utf8')
    let threw = false
    const origExit = process.exit
    process.exit = () => { threw = true; throw new Error('__exit__') }
    try {
      run({ apply: true, forceNotRunning: true, projectsJsonPath: pj2 })
    } catch (e) {
      if (e.message !== '__exit__') throw e
    } finally {
      process.exit = origExit
    }
    assert(threw, 'a hook-text mismatch must abort (process.exit)')
    assert(fs.readFileSync(pj2, 'utf8') === before2, 'a mismatched run must write nothing at all')
    fs.rmSync(root2, { recursive: true, force: true })

    // 4) A duplicate match (two projects sharing the exact same bien post_push_cmd text) must abort the
    // WHOLE run with nothing written - "exactly one match" is the precondition, not "at least one".
    const root3 = fs.mkdtempSync(path.join(os.tmpdir(), 'aki-deploy-hooks-selftest-duplicate-'))
    const { projectsJsonPath: pj3 } = buildFixture(root3)
    const projects3 = readJson(pj3)
    const dupDir = path.join(root3, 'duplicate-of-api')
    fs.mkdirSync(path.join(dupDir, '.akidevsync'), { recursive: true })
    fs.writeFileSync(path.join(dupDir, '.akidevsync', 'project.json'), JSON.stringify({ name: 'dup', commands: {} }, null, 2))
    projects3.push({
      id: 'p-dup',
      name: '',
      local_path: dupDir,
      remote_host: 'bien',
      targets: { bien: { remote_path: '~/aki/x/duplicate-of-api', hooks: { post_push_cmd: 'cd ~/aki/web/api.akitao.com && npm run deploy' } } },
    })
    fs.writeFileSync(pj3, JSON.stringify(projects3, null, 2))
    const before3 = fs.readFileSync(pj3, 'utf8')
    threw = false
    process.exit = () => { threw = true; throw new Error('__exit__') }
    try {
      run({ apply: true, forceNotRunning: true, projectsJsonPath: pj3 })
    } catch (e) {
      if (e.message !== '__exit__') throw e
    } finally {
      process.exit = origExit
    }
    assert(threw, 'a duplicate hook-text match must abort (process.exit)')
    assert(fs.readFileSync(pj3, 'utf8') === before3, 'a duplicate-match run must write nothing at all')
    fs.rmSync(root3, { recursive: true, force: true })

    // 5) Ordering: every touched project.json backup must exist BEFORE any project.json content write
    // happens - traced by recording every copyFileSync (a backup) and every project.json
    // content writeFileSync call, in call order, then asserting the last backup precedes the first write.
    const root4 = fs.mkdtempSync(path.join(os.tmpdir(), 'aki-deploy-hooks-selftest-order-'))
    const { projectsJsonPath: pj4 } = buildFixture(root4)
    const order = []
    const origCopyFileSync = fs.copyFileSync
    const origWriteFileSync = fs.writeFileSync
    fs.copyFileSync = (src, dest, ...rest) => {
      if (String(dest).endsWith('.akidevsync/project.json'.replace('/', path.sep)) || String(dest).includes('project.json.bak-')) order.push(`backup:${dest}`)
      return origCopyFileSync(src, dest, ...rest)
    }
    fs.writeFileSync = (dest, ...rest) => {
      if (String(dest).endsWith(path.join('.akidevsync', 'project.json'))) order.push(`write:${dest}`)
      return origWriteFileSync(dest, ...rest)
    }
    try {
      run({ apply: true, forceNotRunning: true, projectsJsonPath: pj4 })
    } finally {
      fs.copyFileSync = origCopyFileSync
      fs.writeFileSync = origWriteFileSync
    }
    const lastBackupIndex = order.reduce((acc, entry, i) => (entry.startsWith('backup:') ? i : acc), -1)
    const firstWriteIndex = order.findIndex((entry) => entry.startsWith('write:'))
    assert(lastBackupIndex !== -1 && firstWriteIndex !== -1, `expected both backup and write entries, got: ${JSON.stringify(order)}`)
    assert(lastBackupIndex < firstWriteIndex, `every project.json backup must happen before any project.json content write, got order: ${JSON.stringify(order)}`)
    fs.rmSync(root4, { recursive: true, force: true })

    console.log('\nSELF-TEST: all checks passed (3 real moves by hook-text identity (misleading name/folder included) + 2x2 untouched-project guard + per-project.json and projects.json backups + mismatch-abort + duplicate-match-abort + dry-run-writes-nothing + backups-before-any-write ordering).')
  } finally {
    fs.rmSync(root, { recursive: true, force: true })
  }
}

const args = parseArgs(process.argv.slice(2))
if (args.selfTest) {
  runSelfTest()
} else {
  run(args)
}
