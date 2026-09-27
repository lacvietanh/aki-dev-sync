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
                                     remote_host (active target), targets.<host>.{remote_path, hooks}
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
- [ ] **F2 — the baseline records only what the sync made common.** After a push without `--delete`, carry forward every entry of the previous baseline whose path is now missing locally: that deletion did not reach the remote, so the file is not common yet. A mirror push (deletion propagated) and any pull (a merge pull restores the file, a mirror pull matches the remote) need no carry-over. Test: delete a file locally → merge push → next status check still classifies it as *local deleted*, not *remote created*.

### B. Store

- [ ] `project_config.rs` (new, sole owner of `project.json`): read returns a status (`ok | missing | unreadable | conflicted`), never a defaulted struct, the same contract as `read_project_notes`. Write is read-modify-write + `write_atomic`, under the same per-project queue as notes.
- [ ] `sync_state.rs` (new, sole owner of `state/`): `read_baseline(id, host)`, `write_baseline(id, host, remote_path, files)`, `write_last_sync(id, host, …)`. Function names say their scope (1.9.3 guard). No function wipes more than one `(id, host)` dir. Deleting a project removes only its own `state/<id>/`.
- [ ] `SyncProject`: add `targets: BTreeMap<String, Target { remote_path, hooks }>` (`#[serde(default)]`); `remote_path` and `hooks` read through the active target.

### C. Migration (one shot, idempotent, before the logger opens)

- [ ] Per project: `name`, `production_url`, excludes, `*_cmd_override` → `project.json`, **only if the file does not exist** (a file already in the repo wins, 1.22.0). Folder unreadable → skip, retry next launch.
- [ ] `remote_host` + `remote_path` + `hooks` → `targets.<remote_host>`.
- [ ] `baselines/<id>.json` → `state/<id>/<last_sync_host or remote_host>/baseline.json` with the current `remote_path`; `last_sync_*` → `last_sync.json` beside it.
- [ ] Deprecated fields keep `#[serde(default, skip_serializing_if = "Option::is_none")]` for one release, with a load → save test proving they do not come back (1.13.0 `sync_git` lesson). Same pass removes F4: `tasks`, `notes`, `ProjectTask`.
- [ ] The default-exclude migration (`__pycache__/`, 1.24.x) now writes `project.json`; unreadable folders are skipped and retried.
- Known, intended consequence: every reachable repo gains an untracked `.akidevsync/project.json`, the same way `notes.json` arrived in 1.22.0 (a tool's dot folder beside the code). The app never edits a repo's `.gitignore`.

### D. Frontend

- [ ] `useProjectConfig.js` reads `project.json` at load, on titlebar Refresh and when the config dialog opens; a late read never overwrites an edit (generation counter, as `projectNotesStore.js`).
- [ ] Every edit goes through the existing mirrored actions so a paired phone still works (Remote Control lesson).
- [ ] Host dropdown (`setRemoteHost`): switching restores `targets.<host>.remote_path`; a host without an entry asks for its path in the existing config dialog instead of silently reusing the old one (F5).
- [ ] Config dialog: project-wide fields and this-machine fields in their existing sections, each section labeled by where it is saved (`in the project` / `on this Mac`). No new rows in the table (Extreme Narrow).
- [ ] Unreadable folder: row renders from the registry with `basename(local_path)`, config dialog read-only with the reason, the same as Tasks.

### E. Verify

- [ ] Rust tests: F2, migration idempotency (run twice → same files), load → save drops deprecated keys, `(id, host)` isolation: writing one host's state leaves every other host's and project's files byte-identical. ≥2 projects × ≥2 hosts (CLAUDE.md multi-entity guard).
- [ ] Rehearse the migration on a **copy** of the owner's real `~/.aki/devsync/` (previous state, `release.B5`): 30 projects, both hosts. Assert the counts: 30 registry entries, one `project.json` per reachable folder, one state dir per existing baseline.
- [ ] Docs: `docs/arch` store section, `docs/feat/sync-flow.md` (baseline location and F2 rule), `docs/feat/project-task-list.md` (a mirror pull lets the remote copy win, as for code), `README.md`, `IntroModal.vue`.

## Mac checks after the code is done

The code and tests above can be written and run on Linux. These need the real app on the Mac. Leave them unticked until run there:

- [ ] First launch of the new build against the real `~/.aki/devsync/`: every project still listed, same order, same host; no push/pull badge lights up that was dark before the upgrade.
- [ ] Host dropdown on a project synced with both hosts: switching back and forth restores each host's path.
- [ ] Unmount an external volume holding a project: that row shows unavailable, others unaffected.

## Cross-references

- `docs/research/akidevsync-project-config-scope-2.md` — decision record; supersedes `akidevsync-project-config-scope.md`.
- `docs/plan/done/1.22.0-notes-json-ssot.md` — precedent for `.akidevsync/` and the unreadable-folder contract.
- `docs/plan/backlog.md` #6 — notes `task-1785676763350`, `task-1789390613266`.
