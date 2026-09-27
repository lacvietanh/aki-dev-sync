# Deploy as its own action, hooks back to sync-only

**Order: plan 3 of 3.** Depends on `docs/plan/settings-and-state-layout.md` (plan 1): `commands.deploy` lives in `project.json`, and `deploy`/`hooks` live under `targets.<host>` in `projects.json`. Independent of plan 2. Target release: 1.32.0.

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
- `targets.<host>.deploy` → `{ run_on: "local" | "remote", on_push: bool }`: *where* it runs and whether a push to this host offers it.
- Default when unset: `commands.deploy` = `npm run deploy` if `package.json` has a `deploy` script (stack detection, as for dev/build), else empty → the button is disabled with a tooltip saying why.

**Running it.** Deploy opens an in-app terminal tab like DEV/BUILD (`openRunCommand`, new `runKind: 'deploy'`), so output, prompts (e.g. `wrangler login`) and failures are visible. It is never a hidden `spawn_and_stream` with ignorable errors.
- `run_on: local` → tab in `local_path`.
- `run_on: remote` → the existing SSH terminal tab for that host, `cd` to the target's `remote_path` (shell-quoted, path already validated; real paths start with `~/`, which quoting would stop expanding, so a leading `~/` becomes `"$HOME"/` + the quoted rest), then `bash -lc '<cmd>'` so nvm/npm resolve through the login shell.

**Always ask first** (owner rule in `task-1790401977743`). Every deploy, button or push-triggered, goes through one confirm dialog showing the exact command and `local` / `host:path`. `on_push` only means "after a successful non-dry push to this host, show that dialog"; it never deploys unattended.

**Visibility without Settings** (`task-1790490810608`, Extreme Narrow): `position: absolute` badge overlays on the PUSH button beside the existing delete badge, with no new row:
- `D` when `targets.<host>.deploy.on_push` is on;
- `H` when this host has any sync hook. Tooltip lists the commands.

## Execution steps

- [ ] `commands.deploy` in `project.json` (plan 1's store) + stack detection of a `deploy` script (`system.rs` beside `dev_cmd`/`build_cmd`).
- [ ] `targets.<host>.deploy` in `projects.json` (`#[serde(default)]`), edited in the config dialog's host section.
- [ ] DEPLOY button in the OPEN popup beside DEV | BUILD, disabled with a reason when empty or when the folder is unreachable (`localBlocked`).
- [ ] Confirm dialog (existing `BaseModal`), then `openRunCommand(project, cmd, 'deploy')` or the remote-terminal variant.
- [ ] After a successful non-dry push with `on_push`: the same dialog. Dry run → never offered.
- [ ] `D` / `H` badge overlays on PUSH via `CountBadgeWrap`; tooltips.
- [ ] Companion phone: DEPLOY and the confirm dialog go through mirrored actions, like DEV/BUILD.
- [ ] `ignore_hook_errors` stays for sync hooks only; deploy has no ignore option.
- [ ] Move the three deploy hooks (direction confirmed by the owner; each command is the hook's own, with the `cd` dropped because it equals the host's `remote_path`):
  - `api.akitao.com` → `commands.deploy: npm run deploy`, `bien.deploy: { run_on: remote, on_push: true }`, remove `post_push_cmd`.
  - `aki-gegrok-bot` → `commands.deploy: bash scripts/deploy-restart.sh`, `bien.deploy: { run_on: remote, on_push: true }`, remove `post_push_cmd`.
  - `akidevrule` → `commands.deploy: ./install.sh`, `bien.deploy: { run_on: remote, on_push: true }`, remove `post_push_cmd`.
  - `cloud.akivn.net` keeps its remote build hook (it is a sync step); it now shows `H`.
- [ ] Tests: deploy argument/cwd builder (local and remote, a path with spaces, a `~/` path that must still expand), stack detection with and without a `deploy` script, `on_push` never fires on dry run.
- [ ] Docs: new `docs/feat/deploy.md`; `docs/feat/sync-flow.md` (hooks are sync-only), `README.md`, `IntroModal.vue`.

## Mac checks after the code is done

- [ ] Before removing the three hooks: push one of them on the current build and read the sync console. `[WARN] post_push hook failed (ignored)` confirms F3; a successful deploy means the hook ran somewhere this analysis missed, so stop and re-check `execute_hook`.
- [ ] DEPLOY on `api.akitao.com` (remote, `bien`): confirm dialog shows `bien:~/aki/web/api.akitao.com`, the tab shows `npm run deploy` output, and production reflects it (`release.B11`: check a real endpoint, not only the exit code).
- [ ] Push with `on_push` on: dialog appears once; Cancel deploys nothing.
- [ ] `D`/`H` overlays in narrow and wide windows; the PUSH button does not grow.

## Cross-references

- `docs/research/akidevsync-project-config-scope-2.md` — F3 and the field placement.
- `docs/plan/settings-and-state-layout.md` — the stores this plan writes to.
- `docs/plan/backlog.md` #6 — notes `task-1790401977743`, `task-1790490810608`.
