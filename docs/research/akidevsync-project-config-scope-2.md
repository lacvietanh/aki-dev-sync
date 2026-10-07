# Where each setting and state value lives — four owners, two kinds

Status: amended 2026-09-28

**Start time:** 2026-09-27

## Initial purpose

Successor to `docs/research/akidevsync-project-config-scope.md`, whose Decision ("keep everything in `projects.json`, no split") no longer holds. Three pressures reopened it the same day:

1. **Sync state belongs to a (project, host) pair but is stored per project.** A baseline written against `akicloud` was compared against `bien` (`docs/plan/done/conflict-detection-and-agy-report.md` § Evidence). The host dropdown in the project table (note `task-1786480500476`, shipped) makes switching hosts a one-click action, so this is now routine.
2. **Owner direction:** a project's settings should live with the project.
3. **A Deploy action beside DEV|BUILD** (notes `task-1790401977743`, `task-1790490810608`). Deploy is currently scattered: some projects deploy from an `npm` script, some from a push hook.

The first draft (`.akidevsync/local/`, a folder inside the project that rsync never transfers) failed on use cases the owner raised: the app running on Linux too, one folder opened by two machines, git, an SSH alias as the key, and a moved `local_path`. That draft is rejected below.

Context at the time: the app ships macOS-only (`CLAUDE.md`); 30 projects on two hosts (`bien`, `akicloud`); last public release 1.31.0.

## Strategy

1. Two classification questions per value: **(a)** if the project folder were copied to another machine, would this value still be correct there? **(b)** is it human intent (config) or app observation (state)?
2. Walk the history of every field (CHANGELOG, `projects.rs`, `sync.rs`, task notes) to find old bugs the new layout must not bring back.
3. Read the owner's real `~/.aki/devsync/projects.json` for how hooks and command overrides are actually used.
4. Put each option through `METHOD-deep-think.md` critique: steelman, inversion, pre-mortem.

## Checklist

- [x] Classify every `SyncProject` field on both axes.
- [x] Read the real hook and override values (9 of 30 projects use them).
- [x] Trace the history behind each field: 1.9.3, 1.13.0 `sync_git`, 1.20.0, 1.22.0, 1.24.x, EC-3/EC-3b, the macOS stock-rsync false positives.
- [x] Read the rsync argument builder and the hook executor for bugs the new layout touches.
- [x] Critique the chosen layout and the rejected ones.

## Result

### The model — four owners × two kinds

| Owner | Question it answers | Config (human intent) | State (app observation) |
|---|---|---|---|
| **Project** — the code, the same on every machine | what the project *is* | `<project>/.akidevsync/project.json`, travels with rsync (and git) | — (`notes.json` is user content, already here since 1.22.0) |
| **Machine** — this app install | how this machine works | `~/.aki/devsync/*` (SSH, refresh, window; unchanged) | same folder (unchanged) |
| **Project on this machine** | how this machine holds this project | `projects.json` entry | — |
| **Project on this machine × one host** | this machine's relationship with one remote copy | `projects.json` entry → `targets.<host>` | `~/.aki/devsync/state/<project_id>/<host>/` |

### Field placement

| Field | Owner · kind | Why |
|---|---|---|
| `name` | project · config | Label of the project itself. Unreadable folder → render `basename(local_path)`; no second copy in the registry |
| `production_url` | project · config | Property of the deployed project |
| `pull_excludes`, `push_excludes` | project · config | Build output and caches of the code (`node_modules/`, `.git/`, `__pycache__/`); no machine paths in any of the 30 real projects |
| `commands.dev`, `commands.build` (today `*_cmd_override`) | project · config | All 9 real values are project scripts with no machine path (`npm start`, `npm run build`, `./in*sh`). "LOCAL ONLY" in the UI means *runs on this machine*, not *differs per machine* |
| `commands.deploy` (new) | project · config | Same shape as dev/build; the deploy plan owns it |
| `id` | project-on-machine · config | Minted locally; another machine mints its own |
| `local_path`, list order, `disabled` | project-on-machine · config | Differ per machine by construction |
| `dry_run`, `delete_on_pull`, `delete_on_push` | project-on-machine · config | Operator's safety policy, not the code's. Reopen if one host needs different delete behavior |
| `remote_host` | project-on-machine · config | The **active** target, a pointer into `targets` |
| `targets.<host>.remote_path` | target · config | A path on that host. Remembered per host, so switching the dropdown back restores it |
| `targets.<host>.hooks` | target · config | Real hooks embed remote paths (`cd ~/aki/web/api.akitao.com && …`), so they are only valid on the host they were written for |
| `targets.<host>.deploy` (`run_on`, `on_push`) | target · config | Where a deploy runs and whether a push offers one depend on the host, not the code (deploy plan) |
| baseline, `last_sync_action/time/status` | target · state | The last common state is a property of the pair. The per-host dir name replaces `last_sync_host` |

`project.json` is edited only through the app (or by hand, re-read on Refresh). Every other config value stays in the one per-machine `projects.json`, so the single-writer property the first research doc defended is kept for everything that is not project-wide.

### Findings from the history sweep (bugs the layout must fix or avoid)

| # | Finding | Evidence | Where it is handled |
|---|---|---|---|
| F1 | **A mirror PULL overwrites a newer `.akidevsync/` file with the remote's older copy.** Mirror mode drops `-u` (`sync.rs` `build_rsync_args`), and the `P` filter only blocks deletion. `delete_on_pull` defaults to on. Affects `notes.json` today and would affect `project.json` | code reading, `sync.rs:651-681`; not reproduced | **closed as by-design** (owner, 2026-09-27): `.akidevsync/` is ordinary project data, so mirror semantics apply to it as to code; plan 2's `⚠` surfaces a both-sides edit before the sync. No special transfer path |
| F2 | **A merge-mode push makes the baseline forget files deleted locally.** The baseline is a walk of the local tree after *every* full sync (`sync.rs:846`). A push without `--delete` leaves the file on the remote, the new baseline no longer lists it, and the next status check calls it "remote created" (pull). This inverts EC-3 | code reading | layout plan (baseline records only what the sync made common) |
| F3 | **Deploy hooks run on the wrong machine and fail silently.** Owner, 2026-09-27: deploys are done inconsistently (the push button, `npm run deploy`, or a hand-run script), and push should only push. 3 projects (`akidevrule`, `aki-gegrok-bot`, `api.akitao.com`) have `post_push_cmd` = `cd ~/aki/… && <deploy>` with `run_hooks_on_remote: false`, so it runs under `sh -c` **on the Mac**, where those directories do not exist. `ignore_hook_errors: true` downgrades the failure to a console `[WARN]` | `execute_hook` (`sync.rs:411`), `projects.json`, `sh -c 'cd ~/aki/run/aki-gegrok-bot'` → exit 1 on the Mac | deploy plan (owner confirms whether those deploys ever ran) |
| F4 | `tasks`/`notes`/`ProjectTask` still in `projects.rs`, promised gone in 1.23.0 | `projects.rs` comment | layout plan |
| F5 | The host dropdown keeps `remote_path` when the host changes | `ProjectTable.vue` `setRemoteHost` | layout plan (`targets.<host>.remote_path`) |

### History lessons the plans carry (never repeat)

- **1.9.3:** a function that clears a multi-entity store is scoped to one entity and named for that scope; verify with ≥2 projects × ≥2 hosts.
- **1.13.0 `sync_git`:** a migration that deletes a key must stop serde re-materializing it (`skip_serializing_if`), with a load → save test.
- **1.22.0:** folder unreadable (unmounted volume) → skip and retry, never migrate against an empty directory; a file already in the repo wins over the app's copy.
- **1.20.0:** a late async read never lands on top of an edit (per-id generation counter, `projectNotesStore.js`).
- **1.24.x Refresh all + note `task-1787801054655`:** config is re-read on Refresh and when its dialog opens; no restart, no watcher.
- **Remote Control:** every config edit goes through the mirrored action. The old SSH-modal selector broke over a phone because it wrote the value directly.
- **1.7.1 / app-data move:** a one-shot migration runs before the logger opens and is idempotent.
- **Stock macOS rsync false positives:** any new rsync flag is checked against the binary the app actually resolves.

### Critique

- **Steelman `.akidevsync/local/` (rejected):** everything about a project in one folder, visible next to the code. Fails: a folder shared by two machines (external drive, network volume, cloud sync) gets one state for two machines; the same name means the opposite thing on the Linux side; it needs a gitignore rule nobody enforces; a `local_path` move has to carry it.
- **Steelman "everything travels":** one file, zero machine registry. Fails on F3's evidence: hooks and host paths are wrong on any other machine.
- **Steelman "nothing travels" (the first research doc):** single writer, no new corruption surface. Fails the owner's stated need, and dev/build/deploy commands *are* project facts: a clone on another machine has to re-type them.
- **Attack the chosen layout:** `project.json` inherits `notes.json`'s two-writer hazard (Mac and remote both edit it). Accepted because (1) it is edited rarely, (2) plan 2 shows an edit on both sides as `⚠` before the sync, and (3) git is the recovery path, as for notes. Would show up as a setting silently reverting after a sync. Reopen then.
- **Pre-mortem:** "six months later, deploys still fire from push hooks." Prevented by making Deploy a first-class action and showing a hook badge on the button (deploy plan), not by trusting migration by content guessing.
- **Second order:** the companion phone renders the same rows, so every new field reaches it through the existing mirrored state, with no new transport.

### Verification

Classification: static (code + real `projects.json`). F2: code reading only, **unverified at runtime**; each plan carries a test that reproduces it before the fix. F3: the failing `cd` was reproduced on the Mac; whether those deploys ever ran is **unverified** (the sync console is not persisted to `usage.log`).

### Corroborating links

`docs/research/akidevsync-project-config-scope.md` (the field table this refines), `docs/plan/done/1.22.0-notes-json-ssot.md` (the precedent for `.akidevsync/`), `docs/feat/sync-flow.md` §2 (baseline rules), `docs/research/startup-cached-state-audit.md` (read on use, no watcher).

## Decision

**Action.** Three plans, in order:

1. `docs/plan/done/settings-and-state-layout.md` — the four-owner layout, F2, F4, F5.
2. `docs/plan/done/conflict-detection-and-agy-report.md` — reads the per-host baseline from plan 1.
3. `docs/plan/done/deploy-action.md` — Deploy beside DEV|BUILD, hooks per host, F3.

Rejected: `.akidevsync/local/` · everything travels · nothing travels (reasons under Critique).

**Cross-references:** `docs/plan/backlog.md` #6; notes `task-1785676763350`, `task-1787393248179`, `task-1789390613266`, `task-1790401977743`, `task-1790490810608`.

## Amendments

**2026-09-28.** F5's field-placement column (`targets.<host>.remote_path`) was wrong. The finding itself — "the host dropdown keeps `remote_path` when the host changes" — was read as a bug to fix by making the path follow the host, when the owner's actual requirement (stated 2026-09-28) is the opposite: a project has exactly ONE remote directory regardless of host, so the path must NEVER change on a host switch. `hooks` (and `deploy`, added by plan 3) really are per-host — they were bundled with `remote_path` into the same `Target` struct on the assumption all three shared one placement rule, without re-checking that assumption against `remote_path` specifically. Checked against the real registry as part of the fix: no project has ever recorded two different paths across hosts, confirming the per-host storage was never actually exercised as designed, only as the friction it created (a new host prompting for a path that had never changed). Fixed directly in `docs/plan/done/settings-and-state-layout.md` § Amendments; this Decision's "three plans, in order" and the rejected alternatives above are otherwise unchanged.

**2026-09-28 (2).** § Field placement, row `targets.<host>.deploy`: deploy is no longer per-host. It is one project-level `deploy` that names its own `host`, because a deploy that follows the active sync host runs wherever the one-click dropdown last pointed — `docs/research/sync-host-safety.md` § R4. The rest of this Decision stands.
