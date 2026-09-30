# Deploy as its own action, hooks back to sync-only

**Order: plan 3 of 3.** Depends on `docs/plan/settings-and-state-layout.md` (plan 1): `commands.deploy` lives in `project.json`, `hooks` live under `targets.<host>` in `projects.json`, and `deploy` is one top-level field per project in `projects.json` (amended 2026-09-28, see § Amendments). Independent of plan 2. Target release: 1.32.0.

Why deploy settings are split between the project and the host: `docs/research/akidevsync-project-config-scope-2.md` § Field placement.

## Problem

Deploy has no home, so each project improvised one (notes `task-1790401977743`, `task-1790490810608`):

| Project | Today | What is wrong |
|---|---|---|
| `api.akitao.com` | `post_push_cmd`: `cd ~/aki/web/api.akitao.com && npm run deploy` | deploy hidden inside sync (SRP); runs **on the Mac** (`run_hooks_on_remote: false`) where the path does not exist; `ignore_hook_errors: true` hides the failure |
| `aki-gegrok-bot` | `post_push_cmd`: `cd ~/aki/run/aki-gegrok-bot && bash scripts/deploy-restart.sh` | same three problems; the `cd` target equals this host's `remote_path`, so the app can supply it |
| `akidevrule` | `post_push_cmd`: `bash -lc 'cd ~/aki/AkiDevRule && ./install.sh'` | same three problems |
| `cloud.akivn.net` | `post_push_cmd` on remote: `npm install && npm run build` | a remote build step; legitimate as a sync hook, but invisible without opening Settings |
| other projects | `npm run deploy` typed by hand, or nothing | no one-click path, no confirmation |

Owner, 2026-09-27: deploys happen inconsistently (sometimes the PUSH button, sometimes `npm run deploy`, sometimes a hand-run script), and push should only push. This plan applies to every project type; web Nuxt/Cloudflare and Tauri projects already have their own release ritual and simply fill `commands.deploy` if they want the button.

F3 (research doc): the first three have not been deploying from push at all if the path is really missing on the Mac. The `cd` was reproduced failing there; whether the owner deploys them another way is unknown.

## Design

**Separation of responsibilities.**
- **Sync hook** = prepare or finish *the sync itself* on one host (install deps, rebuild after files land). Stays, per host (`targets.<host>.hooks`), because its commands contain that host's paths.
- **Deploy** = ship the project to production. Its own action, never a side effect of a hook.

**Where each piece lives.**
- `project.json` → `commands.deploy`: *what* deploys the project (`npm run deploy`, `./scripts/deploy-restart.sh`). No `cd`, no machine path: the working directory is supplied by the app.
- `deploy` (project-level, `projects.json`) → `{ run_on: "local" | "remote", on_push: bool, host, path }`: *where* it runs, whether a push offers it, and for a remote deploy its own named host (`path` empty = the project's `remote_path`). Never the sync host implicitly (§ Amendments).
- Default when unset: `commands.deploy` = `npm run deploy` if `package.json` has a `deploy` script (stack detection, as for dev/build), else empty → the button is disabled with a tooltip saying why.

**Running it.** Deploy opens an in-app terminal tab like DEV/BUILD (`openRunCommand`, new `runKind: 'deploy'`), so output, prompts (e.g. `wrangler login`) and failures are visible. It is never a hidden `spawn_and_stream` with ignorable errors.
- `run_on: local` → tab in `local_path`.
- `run_on: remote` → the existing SSH terminal tab for that host, `cd` to the target's `remote_path` (shell-quoted, path already validated; real paths start with `~/`, which quoting would stop expanding, so a leading `~/` becomes `"$HOME"/` + the quoted rest), then `bash -lc '<cmd>'` so nvm/npm resolve through the login shell.

**Always ask first** (owner rule in `task-1790401977743`). Every deploy, button or push-triggered, goes through one confirm dialog showing the exact command and `local` / `host:path`. `on_push` only means "after a successful non-dry push, show that dialog" (for a remote deploy, only a push to `deploy.host`); it never deploys unattended.

**Visibility without Settings** (`task-1790490810608`, Extreme Narrow): `position: absolute` badge overlays on the PUSH button beside the existing delete badge, with no new row:
- `D` when a push to the current sync host would offer the deploy;
- `H` when this host has any sync hook. Tooltip lists the commands.

## Execution steps

**Deviation from this doc's own wording:** "existing `BaseModal`" (§ Design) is implemented as `askConfirm` (`src/store/dialogStore.js` + `DialogHost.vue`) instead - the app's one actual reusable, mirrored confirm-dialog mechanism (already used by `setRemoteHost` for an equivalent decision), which also gives companion-phone mirroring for free rather than requiring a second implementation. Reported here rather than silently diverging from the written plan.

**Deviation (Slice B, closing challenger-plan3 S1):** `openRunCommand`'s DEV/BUILD tab-reuse semantics ("focus a live tab, respawn a dead one") do not fit a one-shot action - a confirmed deploy on an already-live DEPLOY tab must still run, never just focus a finished tab. Fix: `runKind: 'deploy'` skips the reuse lookup and always allocates a fresh tab; DEV/BUILD are unchanged.

- [x] `commands.deploy` in `project.json` (plan 1's store) + stack detection of a `deploy` script (`system.rs` beside `dev_cmd`/`build_cmd`).
- [x] `deploy` in `projects.json` (`#[serde(default)]`, project-level since 2026-09-28), edited in the config dialog's RUN COMMANDS group.
- [x] DEPLOY button in the OPEN popup beside DEV | BUILD, disabled with a reason when empty or when the folder is unreachable (`localBlocked`).
- [x] Confirm dialog (`askConfirm`, the app's one existing mirrored decision dialog - not a new `BaseModal` instance, see "Deviation" note below), then `openRunCommand(project, cmd, 'deploy')` or the remote-terminal variant (`build_remote_deploy_command`).
- [x] After a successful non-dry push with `on_push`: the same dialog. Dry run → never offered.
- [x] `D` / `H` badge overlays on PUSH via `CountBadgeWrap`; tooltips.
- [x] Companion phone: DEPLOY and the confirm dialog go through mirrored actions, like DEV/BUILD.
- [x] `ignore_hook_errors` stays for sync hooks only; deploy has no ignore option.
- [x] Move the three deploy hooks (direction confirmed by the owner; each command is the hook's own, with the `cd` dropped because it equals the host's `remote_path`):
  - `api.akitao.com` → `commands.deploy: npm run deploy`, `deploy: { run_on: remote, on_push: true, host: bien }`, remove `post_push_cmd`.
  - `aki-gegrok-bot` → `commands.deploy: bash scripts/deploy-restart.sh`, `deploy: { run_on: remote, on_push: true, host: grokvm }`, remove `post_push_cmd`.
  - `akidevrule` → `commands.deploy: ./install.sh`, `deploy: { run_on: remote, on_push: true, host: bien }`, remove `post_push_cmd`.
  - `cloud.akivn.net` keeps its remote build hook (it is a sync step); it now shows `H`.
  - Mac-only, held UNTICKED pending the "Mac checks" section below: `node scripts/migrate-deploy-hooks.mjs` (dry run — prints exact before/after, writes nothing), then `node scripts/migrate-deploy-hooks.mjs --apply` (quit the app first; backs up `projects.json` and, per project, `project.json` before touching either; refuses and writes nothing if any of the three hooks' current text does not match this step's quoted text, or if a project's `project.json` is corrupt/unavailable). **Deviation (Slice B, closing challenger-plan3 S3):** each project is identified purely by whether its `targets.bien.hooks.post_push_cmd` (or the legacy top-level `hooks.post_push_cmd` when `remote_host` is `bien` and no `bien` target exists yet) equals this step's quoted text — never by `name`/`local_path` basename, which can be empty or differently-cased on the real registry. Exactly one match is required per command; zero or more than one aborts the whole run with nothing written. Every touched `project.json` is backed up before any of them is written, not interleaved with the write loop (Slice C, closing lead#13's NIT), and the `--self-test` traces the write order to prove it. Self-test against throwaway fixtures only: `node scripts/migrate-deploy-hooks.mjs --self-test` (now also covers a control project whose folder/name differ from its hook text, a duplicate-hook-text abort, that both `project.json` and `projects.json` backups are actually created, and that every backup precedes any content write).
- [x] Tests (Slice B/C rounds, closing challenger-plan3 B2/S1-S4 and the pass-2 BLOCKER): `shouldOfferDeployAfterPush` (`projectConfigPure.js`) is the pure, tested decision behind `on_push`, taking the already-resolved deploy command (`resolveDeployCmd`, `useDeploy.js`) as an input rather than deriving it itself - dry run, specific-paths push, `on_push` off, a pull, and no command at all (detected or saved) all assert `false`; a real non-dry push with `on_push` on and a resolved command asserts `true` for both `run_on: local` and `run_on: remote`, and a *detected-only* command (nothing saved to `commands.deploy`) also asserts `true` (`config-store.test.mjs` - this replaces the pass-2 gap, where two of the three call sites each independently forgot the detected-command fallback so the button worked but the `on_push` offer silently never fired for a detected-only project). `build_remote_deploy_command` (`system.rs`): the existing path/cmd-quoting test, a remote path with spaces, a rejected option-shaped host, the loose single-quote injection check, and a new golden exact-string test (independent quoting logic, not calling `shell_quote`) with both `'` and `$(...)` in the command - this last one is the one that would actually fail if the inner `shell_quote(&cmd)` were removed. `getDeployCmd`'s stack-detected fallback (B1) and `deployOnPush` following `on_push` regardless of `run_on` (S2) both have direct unit tests. `buildProjectListSavePayload` has a 2-projects x 2-hosts test (S4) asserting every other host/project stays `JSON.stringify`-identical after editing one host's deploy. Local deploy cwd: verified by static reading only - `openRunCommand`'s local branch always passes `project.local_path` as `cwd`, identical to DEV/BUILD's existing (untested) cwd, so a new pure wrapper around one property read would be an abstraction with no evidence behind it (pattern.A2).
- [x] Docs: new `docs/feat/deploy.md`; `docs/feat/sync-flow.md` (hooks are sync-only), `README.md`, `IntroModal.vue`.

## Mac checks after the code is done

- [x] ~~Before removing the three hooks: push one of them on the current build and read the sync console.~~ Settled without a push (2026-09-28): all three hooks had `run_hooks_on_remote: false`, and none of their `cd` targets exists on the Mac (`ls ~/aki/web/api.akitao.com ~/aki/run/aki-gegrok-bot ~/aki/AkiDevRule` → no such file), so every one failed silently under `ignore_hook_errors`. `[WARN] post_push hook failed (ignored)` confirms F3; a successful deploy means the hook ran somewhere this analysis missed, so stop and re-check `execute_hook`.
- [ ] DEPLOY on `api.akitao.com` (remote, `bien`): confirm dialog shows `bien:~/aki/web/api.akitao.com`, the tab shows `npm run deploy` output, and production reflects it (`release.B11`: check a real endpoint, not only the exit code).
- [ ] Push with `on_push` on: dialog appears once; Cancel deploys nothing.
- [ ] `D`/`H` overlays in narrow and wide windows; the PUSH button does not grow.

## Cross-references

- `docs/research/akidevsync-project-config-scope-2.md` — F3 and the field placement.
- `docs/plan/settings-and-state-layout.md` — the stores this plan writes to.
- `docs/plan/backlog.md` #6 — notes `task-1790401977743`, `task-1790490810608`.

## Amendments

- 2026-09-28 · § Design, § Execution: `deploy` moved from `targets.<host>.deploy` to one project-level field that names its own `host` (+ optional `path`). A per-host deploy ran on whichever sync host was active, and a switch to a host with a saved target but no deploy of its own inherited the previous host's deploy; the sync host is a one-click table dropdown with no confirm. Decision record: `docs/research/sync-host-safety.md`. No registry held a `deploy` value yet (measured 0/30), so nothing was migrated; `scripts/migrate-deploy-hooks.mjs` now writes the new shape, self-test passing.
- 2026-09-28 · § Execution: migration applied to the real registry (backups `projects.json.bak-2026-09-28T08-36-04-336Z` and one `project.json.bak-*` per project). `aki-gegrok-bot` deploys on `grokvm`, not `bien`: its hook lived in `targets.grokvm` and its process runs there (`ps` on grokvm: `node /home/box/aki/run/aki-gegrok-bot/index.js`; bien has the directory but no process). The script now carries a host per move. Only those three registry entries changed (diffed against a pre-run copy).
