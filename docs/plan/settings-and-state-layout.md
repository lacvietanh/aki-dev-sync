# Settings and state layout — four owners, two kinds

**Order: plan 1 of 3.** Plans 2 (`docs/plan/conflict-detection-and-agy-report.md`) and 3 (`docs/plan/deploy-action.md`) both build on the layout defined here; 2 and 3 are independent of each other. Target release: 1.32.0.

Decision record and field-by-field reasoning: `docs/research/akidevsync-project-config-scope-2.md`. This plan does not restate it.

## Target layout

```
<local_path>/.akidevsync/            travels with the project (rsync, git)
  notes.json                         unchanged (1.22.0)
  project.json                       name, production_url, pull/push_excludes, commands.{dev,build}

~/.aki/devsync/                      this machine only
  projects.json                      registry: id, local_path, order, disabled, dry_run, delete_on_*,
                                     remote_host (active target), remote_path (project fact, 1.32.1 § Amendments),
                                     targets.<host>.hooks
  state/<project_id>/<host>/
    baseline.json                    { remote_path, files: {path → mtime} }
    last_sync.json                   { action, time, status }
```

- `baselines/` and `last_sync_host` disappear. "Which host did I last sync with" = the newest `state/<id>/*/last_sync.json`.
- A baseline whose recorded `remote_path` differs from the target's current one counts as **no baseline** (the path changed, so the ancestor is someone else's).
- The host dir name is the SSH alias, validated with `validate_remote_host`. Renaming an alias loses that host's state; with no baseline the status check falls back to today's counts and never reports a false conflict. Accepted.

## Execution steps

### A. Fix before anything moves

- **`.akidevsync/` is ordinary project data, no special path.** It is counted by the badges and transferred by the same rsync, under the same mode rules as the code: a mirror pull lets the remote copy of `notes.json`/`project.json` win, exactly as it does for a source file. The protection is visibility, not a second transfer: plan 2 shows a file edited on both sides as `⚠` before the sync runs. The one existing rule, the `P` filter that stops `--delete` from erasing a folder the sender never had, stays (its tests stay green). Nothing to build here; F1 in the research doc is closed as by-design.
- [x] **F2 — the baseline records only what the sync made common.** After a push without `--delete`, carry forward every entry of the previous baseline whose path is now missing locally: that deletion did not reach the remote, so the file is not common yet. A mirror push (deletion propagated) and any pull (a merge pull restores the file, a mirror pull matches the remote) need no carry-over. Test: delete a file locally → merge push → next status check still classifies it as *local deleted*, not *remote created*.

### B. Store

- [x] `project_config.rs` (new, sole owner of `project.json`): read returns a status (`ok | missing | unavailable | corrupt` - same names as `read_project_notes`), never a defaulted struct. Write is read-modify-write + `write_atomic`, under the same per-project queue as notes.
- [x] `sync_state.rs` (new, sole owner of `state/`): `read_baseline(id, host)`, `write_baseline(id, host, remote_path, files)`, `write_last_sync(id, host, …)`. Function names say their scope (1.9.3 guard). No function wipes more than one `(id, host)` dir. Deleting a project removes only its own `state/<id>/`.
- [x] `SyncProject`: add `targets: BTreeMap<String, Target { remote_path, hooks }>` (`#[serde(default)]`); `remote_path` and `hooks` read through the active target. **Corrected 1.32.1, see § Amendments: `remote_path` is a project fact and no longer lives in `Target` going forward — `hooks` (and `deploy`, added by plan 3) still do.**

### C. Migration (one shot, idempotent, before the logger opens)

- [x] Per project: `name`, `production_url`, excludes, `*_cmd_override` → `project.json`, **only if the file does not exist** (a file already in the repo wins, 1.22.0). Folder unreadable → skip, retry next launch.
  - Deviation: seeded via an async command (`write_project_configs_if_missing`) invoked from `useProjectConfig.js::loadData` at boot load only (`isBootLoad`, gated on the same `showToast` flag that already distinguishes boot from a titlebar Refresh), never on a Refresh — not from Rust's boot-time `setup()` migration either. Idempotent per project (`SeedOutcome`), so a project that already has the file is always a no-op. Outcome (only-if-missing, retried next launch) holds.
- [x] `remote_host` + `remote_path` + `hooks` → `targets.<remote_host>`.
- [x] `baselines/<id>.json` → `state/<id>/<last_sync_host or remote_host>/baseline.json` with the current `remote_path`; `last_sync_*` → `last_sync.json` beside it.
- [x] Deprecated fields keep `#[serde(default, skip_serializing_if = "Option::is_none")]` for one release, with a load → save test proving they do not come back (1.13.0 `sync_git` lesson). Same pass removes F4: `tasks`, `notes`, `ProjectTask`.
- [x] The default-exclude migration (`__pycache__/`, 1.24.x) now writes `project.json`; unreadable folders are skipped and retried.
  - Deviation: runs from JS (`migrateProjectConfigPycacheExcludes`, after hydrate) against `project.json` directly via the existing RMW `write_project_config` command, not through the registry's `pull_excludes`/`push_excludes` fields — those are stripped to `[]` for an `ok`-status project by the save funnel (P1 fix), so checking them would either no-op or re-add the exclude on every launch. Outcome (each project.json gains `__pycache__/` once) holds; idempotency is by a run-once, per-project `localStorage` marker (key `aki-pycache-exclude-migration-v1`), not a content check — a user who deliberately removes `__pycache__/` from their project.json is never overridden again.
- Known, intended consequence: every reachable repo gains an untracked `.akidevsync/project.json`, the same way `notes.json` arrived in 1.22.0 (a tool's dot folder beside the code). The app never edits a repo's `.gitignore`.

### D. Frontend

- [x] `useProjectConfig.js` reads `project.json` at load, on titlebar Refresh and when the config dialog opens; a late read never overwrites an edit (generation counter, as `projectNotesStore.js`).
- [x] Every edit goes through the existing mirrored actions so a paired phone still works (Remote Control lesson).
- [x] Host dropdown (`setRemoteHost`): switching restores `targets.<host>.remote_path`; a host without an entry asks for its path in the existing config dialog instead of silently reusing the old one (F5). **Corrected 1.32.1, see § Amendments: `remote_path` no longer changes on a host switch at all — only `hooks`/`deploy` restore per host.**
- [x] Config dialog: project-wide fields and this-machine fields in their existing sections, each section labeled by where it is saved (`in the project` / `on this Mac`). No new rows in the table (Extreme Narrow).
- [x] Unreadable folder: row renders from the registry with `basename(local_path)`, config dialog read-only with the reason, the same as Tasks.

### E. Verify

- [x] Rust tests: F2, migration idempotency (run twice → same files), load → save drops deprecated keys, `(id, host)` isolation: writing one host's state leaves every other host's and project's files byte-identical. ≥2 projects × ≥2 hosts (CLAUDE.md multi-entity guard).
- [ ] Rehearse the migration on a **copy** of the owner's real `~/.aki/devsync/` (previous state, `release.B5`): 30 projects, both hosts. Assert the counts: 30 registry entries, one `project.json` per reachable folder, one state dir per existing baseline. **Mac-only, left unticked per the maker-plan1 brief.**
- [x] Docs: `docs/arch/settings-and-state.md` (new, the four owners / sole-owner modules / migration order / deprecated-field table), `docs/feat/sync-flow.md` (baseline location, F2 rule, S2 note on merge-push badges), `docs/feat/project-task-list.md` (Key files reduced to a pointer at the new arch doc), `README.md`, `IntroModal.vue` — all confirmed current against the final code. Linked from `docs/index.md`.

## Mac checks after the code is done

The code and tests above can be written and run on Linux. These need the real app on the Mac. Leave them unticked until run there:

- [ ] First launch of the new build against the real `~/.aki/devsync/`: every project still listed, same order, same host; no push/pull badge lights up that was dark before the upgrade.
- [ ] Host dropdown on a project synced with both hosts: switching back and forth keeps the same remote path throughout, and restores each host's own hooks/deploy config (1.32.1 § Amendments).
- [ ] Unmount an external volume holding a project: that row shows unavailable, others unaffected.

## Cross-references

- `docs/research/akidevsync-project-config-scope-2.md` — decision record; supersedes `akidevsync-project-config-scope.md`.
- `docs/plan/done/1.22.0-notes-json-ssot.md` — precedent for `.akidevsync/` and the unreadable-folder contract.
- `docs/plan/backlog.md` #6 — notes `task-1785676763350`, `task-1789390613266`.

## Amendments

**2026-09-28, 1.32.1 — `remote_path` moved back out of `targets.<host>`.** This plan's original design (§ B, § D above) put `remote_path` beside `hooks` in `targets.<remote_host>`, reasoning it was a per-host fact the same way a real hook's embedded path is. It is not: a project has exactly ONE remote directory regardless of which host serves it — the owner never actually uses two different paths for the same project across hosts (confirmed against the real registry: no project has ever recorded a second, different path).
Storing it per-host meant the first sync to a never-before-used host forced re-entering a path that had never actually changed, and blocked the switch behind a confirm dialog until the user did.

Fix, applied directly to the files this plan already names — not a new plan, since the shape (four owners, `targets.<host>` for genuinely host-specific facts) is unchanged, only which field belongs where:
- `SyncProject.remote_path` (`projects.rs`) is the single, always-persisted source again; `Target.remote_path` is deprecated-for-one-release, migration-only (mirrors the existing deprecated-field pattern this plan already uses for `last_sync_*`).
- `sync_state::migrate_settings_and_state` gained a lift-back step: a `targets.<host>.remote_path` left by a 1.32.0-era save is moved onto the project's own top-level field (once, then cleared from every target).
- `resolveHostSwitch`/`setRemoteHost`/the config dialog's own host select no longer touch `remote_path` at all on a host switch — only `hooks`/`deploy` still restore per host. The "new remote host has no saved path" confirm dialog is gone entirely; there is nothing left to ask about.

`hooks` and `deploy` are unaffected — a real hook embeds host-specific shell text, and where a deploy runs is genuinely a fact about the host, not the project. Only `remote_path` was ever misclassified.
