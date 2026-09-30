# Sync host safety — frozen badges, wrong-host pushes, implicit deploy host

## Start time

2026-09-28, during the 1.32.0 pre-release work (last released: 1.31.0).

## Initial purpose

Two symptoms, one question: can the app send files or a deploy to a host the owner did not mean, and would anyone notice?

- The ⚠/PUSH/PULL badges of `oscarfamily.vn` stayed frozen on an old value with no error anywhere.
- `aki-gegrok-bot` and `akiprx` were found with `remote_host: grokvm` and Mirror PUSH on, pointing at live runtime directories (`~/aki/run/...`), while production directories for them also exist on `bien` (activity 2026-09-27).

Constraints: single owner, macOS-only app, 30 registered projects, UX is paramount and the table is Extreme Narrow (no new rows or banners).

## Strategy

Reproduce each failure with real rsync/ssh against the real hosts instead of reading code alone; trace every path from a table click to a remote write (sync, deploy, host switch); size each guard by irreversibility (`METHOD-proportionality.md` B1), not frequency.

## Checklist

- [x] Reproduce the frozen badge: run the exact rsync calls `check_sync_status` makes, per call.
- [x] Measure how many projects use a `~/` remote path.
- [x] Trace `check_sync_status` failure handling end to end (Rust `?` → JS `catch`).
- [x] Inventory every way `remote_host` can change and what records it.
- [x] Trace where a remote deploy runs and what a host switch does to it.
- [x] Check what the mirror previews show for an empty or missing destination.
- [x] Dry-run a real mirror PUSH of both production projects to see what runtime data it would delete or overwrite.

## Result

**R1 — frozen badges.** `rsync --files-from=<list> ... host:~/path` fails every time: `--files-from` (and `-s`/`--secluded-args`) send the remote path without remote-shell parsing, so the literal `~` reaches the receiver as a directory name (`change_dir "/home/<u>/~/..."`, exit 12/3). `host:path` (home-relative) works. Only the conflict-detection checksum-residue call uses `--files-from`; its error propagated through `compute_sync_status_full`'s `?`, so `check_sync_status` failed, and `useSyncStatus.js` swallowed it (`catch (_) {}`), leaving the last good badges on screen indefinitely. Scope: 28 of 30 projects use `~/` paths, and every one whose status included a same-size both-changed file froze.

**R2 — nothing records where a sync went.** `setRemoteHost` (table dropdown) switches with no confirm and no log; the config dialog likewise. Sync success writes `state/<id>/<host>/last_sync.json` (latest only). How the two production projects came to point at `grokvm` cannot be reconstructed.

**R3 — the previews are blind on a first sync.** The delete and overwrite previews list what the destination would lose; an empty or nonexistent destination loses nothing, so a first PUSH to the wrong host shows no dialog at all.

**R4 — deploy followed the sync host.** `targets.<host>.deploy` with `run_on: remote` ran on the active `remote_host`, and `resolveHostSwitch` carried the current deploy onto a host that had a saved target but no deploy of its own. The sync host is the one-click dropdown from R2.

**R5 — runtime data in the push set.** A mirror PUSH of `aki-gegrok-bot` would have deleted/overwritten its live `data/` on the server; `akiprx` already excluded `.state/`.

### Verification

- R1: reproduced on `bien`, `grokvm` and `akicloud` (both the failing `~/` form and the working home-relative form); after the fix, `cargo test --lib` 355 passed, including new `home_relative`/`remote_rsync_arg` cases (`~/app`→`app`, `~`→`.`, `~//`→`.`, `~user/app` unchanged).
- R2, R3, R4: static reading of `remoteActions.js::setRemoteHost`, `useSync.js::startSync`, `projectConfigPure.js::resolveHostSwitch`, `useDeploy.js::confirmAndDeploy`.
- R4 migration need: 0 of 30 registry entries held any `deploy` value (measured on `~/.aki/devsync/projects.json`).
- R5: real mirror-PUSH dry-runs to `grokvm` after the exclude change: 0 lines touching `data/` (`aki-gegrok-bot`) or `.state/` (`akiprx`).
- Unverified — needs a runtime check: the dashed stale outline and tooltip, the first-sync dialog, the deploy host picker, and the audit lines appearing in `usage.log` from a real click.

### Corroborating links

- `CHANGELOG.md` history: "Delete preview error silently swallowed" — the same swallow-the-error class as R1.
- `docs/plan/conflict-detection-and-agy-report.md` § Amendments — the overwrite preview this doc's R3 extends.

## Decision

Owner-approved choices (2026-09-28): runtime data is never pushed; a remote deploy names its own host.

- **Action**
  - R1: `home_relative` (`sync.rs`) makes every `~/` remote path home-relative in `remote_rsync_arg`; a failing checksum step now reports its files as `verified: false` conflicts instead of failing the whole check, and each distinct status failure is logged once per (project, host) (`log_status_anomaly`). The UI keeps the last counts but marks both buttons `btn-sync-stale` with the error in the tooltip (`useSyncStatus.js`, `ProjectTable.vue`).
  - R2: `logger::audit` — always written to `usage.log`: every sync (`run_sync`), confirmed deploy and deploy-config change, and host switch from table or settings (`src/utils/auditLog.js`). `docs/arch/logger.md`.
  - R3: the first real PUSH/PULL between a project and a host with no `last_sync.json` asks first, naming `host:path`; unreadable history asks (`useSync.js::hostHasSyncHistory`). `docs/feat/sync-flow.md` § Safety Guard.
  - R4: `deploy` is one project-level field `{ run_on, on_push, host, path }`; a remote deploy without `host` is disabled with its reason; the post-push offer and `D` badge fire only when the push went to `deploy.host`. `docs/feat/deploy.md`, `docs/plan/deploy-action.md` § Amendments, `scripts/migrate-deploy-hooks.mjs`.
  - R5: `data/` added to `aki-gegrok-bot`'s `push_excludes` (project-owned `.akidevsync/project.json`).
- **Rejected/closed**
  - Confirm on every host-dropdown change: taxes routine switching and still leaves the transfer itself unguarded; the first-sync confirm guards the actual write.
  - Keep per-host deploy and add a warning: the host stays implicit, which is the defect.
  - A separate audit log file: `usage.log` holds only errors outside debug mode, so its 1 MB cap keeps a long history; one pipeline, one place to look.
  - Resolving `~` via an extra `ssh host echo ~` round trip: costs a call per poll for what a string transform settles.
- **Reopen if:** a project needs to deploy to more than one host; `usage.log` truncation evicts audit lines before they are needed (debug sessions write far more); a future rsync expands `~` in `--files-from` paths (the transform stays correct either way).
- **Cross-references:** `docs/arch/settings-and-state.md` (deploy is no longer a `targets` field); `docs/research/akidevsync-project-config-scope-2.md` § Field placement (its deploy placement is superseded here).
