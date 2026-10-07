# Deploy (beside DEV | BUILD)

> updated 2026-09-28 · v1.31.0

Deploy ships the project to production. It is its own action, never a side effect of a sync hook - see `docs/plan/done/deploy-action.md` for the design and `docs/research/akidevsync-project-config-scope-2.md` § F3 for why hooks used to hide deploys and fail silently.

## Where each piece lives

- `commands.deploy` (`<local_path>/.akidevsync/project.json`) - **what** deploys the project (`npm run deploy`, `./scripts/deploy-restart.sh`). No `cd`, no machine path - the app supplies the working directory. Same shape as `commands.dev`/`commands.build`, edited in the same RUN COMMANDS group of the config dialog. Default when unset and unedited: `npm run deploy` if `package.json` declares a `scripts.
  deploy` entry (`check_project_stack`'s `deploy_cmd`, `src-tauri/src/system.rs`), else empty - the button is disabled with a tooltip rather than guessing a command.
- `deploy` (`projects.json`, one per project) - **where** it runs (`run_on`: `"local"` | `"remote"`), whether a push offers it (`on_push`), and for a remote deploy its own SSH `host` plus an optional `path` (empty = the project's `remote_path`). The deploy host is never the sync host implicitly: the sync host is a one-click table dropdown, so a remote deploy that followed it could run on whatever box was picked last (`docs/research/sync-host-safety.md`). A sync host switch (`resolveHostSwitch`) never touches `deploy`.
  With `run_on: remote` and no `host`, the DEPLOY button is disabled with that reason (`deployBlockedReason`). Edited in the config dialog's RUN COMMANDS group, right below the DEV/BUILD/DEPLOY command row.

## Running it

Deploy opens an in-app terminal tab, exactly like DEV/BUILD - never a hidden, ignorable-error subprocess.
Unlike DEV/BUILD (long-running, so a confirmed re-run focuses the existing tab), a confirmed deploy is one-shot: `openRunCommand` always allocates a **fresh** tab for `runKind: 'deploy'`, so a second DEPLOY on an already-finished tab still runs the command instead of silently focusing a dead tab.

- `run_on: local` → a new tab in `local_path` each time, same mechanism as DEV/BUILD (`openRunCommand(project, cmd, 'deploy')`).
- `run_on: remote` → a fresh SSH terminal tab on `deploy.host`: `cd` to the deploy path (kept `~/`-expandable via `shell_quote_remote_path`), then `bash -lc '<cmd>'` so nvm/npm resolve through the login shell (`build_remote_deploy_command`, `src-tauri/src/system.rs`).

## Always asks first

Every deploy - the DEPLOY button or a push-triggered offer - goes through one confirm dialog (`askConfirm`, the same mirrored dialog every other cross-screen decision in this app uses) showing the exact command and `local` / `host:path`. `on_push` only means "after a successful **non-dry** push, show that dialog" (`useSync.js`'s post-push hook, `offerDeployAfterPush`, gated by the pure `shouldOfferDeployAfterPush`) - it never deploys unattended and a dry run never offers it. A local deploy is offered after a push to any host; a remote deploy only after a push to `deploy.host` itself, so a push to a staging box never offers deploying production.

Every confirmed deploy, and every change to a project's `deploy` config, is written to `usage.log` (`auditLog`, `docs/arch/logger.md`).

Deploy has no ignore-errors option (unlike sync hooks' `ignore_hook_errors`) - a failure is simply visible in the terminal tab.

## Visibility without Settings

Two tiny letter-badge overlays on the PUSH button (`CountBadgeWrap.vue`), `position: absolute`, no new row (Extreme Narrow):

- **D** - a push to the current sync host would offer a deploy (the same `shouldOfferDeployAfterPush` decision the post-push offer uses). Tooltip: the deploy command and `host:path`.
- **H** - this host has any configured sync hook (pre/post pull/push). Tooltip: lists the configured hooks.

## Companion phone

`ProjectTable.vue` (the DEPLOY button, the D/H badges) is the one shared component rendered on both the host and the companion screen - no separate phone code path. The confirm dialog and the terminal tab it launches are both mirrored primitives (`askConfirm`, `addTerminalTab`) already used by DEV/BUILD, so a phone-triggered DEPLOY reaches the host the same way a phone-triggered DEV/BUILD does.

## Pure logic and its tests

`src/composables/projectConfigPure.js` (`getDeployCmd` - saved command, falling back to a caller-supplied detected default the same way DEV/BUILD do; `deployRunOn`, `deployOnPush`, `deployRemoteTarget`, `deployBlockedReason`, `deployTargetLabel`, `shouldOfferDeployAfterPush`) - the one Tauri/Vue-free module `node --test` (`npm run test:config`) exercises directly, same convention as every other pure predicate in this file.

`stack_info` (the detected default) never lives on the project object itself - it lives in the separate `projectRuntime` store, keyed by id (`useProjectStack.js`). `src/composables/useDeploy.js::resolveDeployCmd` is the ONE impure resolver that reads it and calls `getDeployCmd(project, detected)`; every production caller - the DEPLOY button, its tooltip, the `D` badge's tooltip, the post-push `on_push` offer, and the confirm dialog - goes through `resolveDeployCmd`, never `getDeployCmd` directly. `useSync.js` resolves the command once per push and passes it into both `shouldOfferDeployAfterPush` (the pure decision, which takes the already-resolved command rather than re-deriving it) and `offerDeployAfterPush` (which no longer re-checks `on_push` or the command itself) - this closes a prior gap where two of the three call sites each independently forgot the detected-command fallback.

`src-tauri/src/system.rs` covers `detect_deploy_cmd` (package.json `scripts.deploy` detection) and `build_remote_deploy_command` (path/`~/` quoting, command-injection neutralization, including a golden exact-string test) with `cargo test`.

## Cross-references

- `docs/plan/done/deploy-action.md` - the plan this feature was built from.
- `docs/feat/sync-flow.md` § 3 - hooks stay sync-only.
- `docs/research/akidevsync-project-config-scope-2.md` § Field placement, § F3.
- `docs/research/sync-host-safety.md` - why deploy names its own host.
