# Project config + per-host state into `.akidevsync/`

**Order: plan 1 of 2.** Must land before `docs/plan/conflict-detection-and-agy-report.md` (plan 2), whose per-host baseline lives in the layout defined here.

## Why this reopens a closed decision

`docs/research/akidevsync-project-config-scope.md` (note `task-1785676763350`) decided to keep all config in the global `projects.json`, with a reopen trigger: "a concrete need shows up". Two have:

1. **State belongs to a (project, host) pair, but is stored per project.** The sync baseline (`~/.aki/devsync/baselines/{project_id}.json`) and `last_sync_*` record one state per project, while a project can sync with several hosts (host dropdown, note `task-1786480500476`). Live evidence: on `tuvi.akinet.me` the baseline had been written against `akicloud` while the badge compared against `bien` (plan 2, § Evidence).
2. **Owner direction (2026-09-27):** a project's settings live with the project, not in one global file.

The research's objections still hold for anything that **travels** with rsync (machine paths leaking to the other side, two writers for one record). This design answers them by making config and state **local-only**: they live in the project folder but never leave the machine.

## Target layout

```
<local_path>/.akidevsync/
  notes.json                      travels with rsync (unchanged, 1.22.0)
  local/                          never transferred, never counted in badges
    config.json                   project settings: name, production_url, excludes, hooks, cmd overrides, delete/dry-run toggles
    hosts/<host>/config.json      per-host settings: remote_path
    hosts/<host>/state.json       per-host state: last_sync_action/time/status
    hosts/<host>/baseline.json    per-host sync baseline (path → mtime)
```

Global `~/.aki/devsync/projects.json` shrinks to a registry: `id`, `local_path`, `disabled`, order. `last_sync_host` disappears — "which host did I last sync with" becomes the newest `hosts/*/state.json`.

## Decisions

- Decided: `local/` excluded from rsync in both directions by a built-in rule (not a user-editable exclude) · because a traveling state/config file regresses on every mirror pull (the remote's older copy overwrites the local one) and writing the baseline after a sync would itself light the push badge · rejected: config travels with the repo (brings back the research's machine-path and two-writer objections) · reopen if: the owner wants a setting shared across machines — then design a merge rule for that field only.
- Decided: `remote_path` is per host, delete/dry-run toggles stay project-level · because the path is a property of the remote; the toggles are an operator policy nobody has asked to vary per host · reopen if: a host needs different delete behavior.
- Decided: folder unreadable (volume not mounted) → the row renders from the registry with `basename(local_path)` and an "unavailable" state; nothing is cached twice · because a copy of `name` in the registry would be a second source of truth.
- Decided: config is read on Refresh, as today (`docs/research/startup-cached-state-audit.md`); no file watcher.

## Execution steps

- [ ] `project_store.rs`: read/write `local/config.json`, `local/hosts/<host>/{config,state}.json`; host dir name validated with `validate_remote_host`; a missing file returns a status, not a defaulted struct (same contract as `read_project_notes`).
- [ ] Built-in `--exclude=.akidevsync/local/` in `direction_excludes` (sync, status check, delete preview); unit test that it is present in all three arg builders.
- [ ] One-shot idempotent migration per project: `projects.json` fields → `local/config.json`; `remote_path` → `hosts/<remote_host>/config.json`; `last_sync_*` + `baselines/{id}.json` → `hosts/<last_sync_host>/`. Old fields stay readable (`#[serde(default)]`) for one release, then removed. Same pass: remove the `tasks`/`notes` fields and `ProjectTask` from `projects.rs`, whose comment promised removal in 1.23.0 and never happened.
- [ ] Frontend `useProjectConfig.js` reads/writes through the new commands; host dropdown switches the active `hosts/<host>/` set.
- [ ] Verify with ≥2 projects and ≥2 hosts per project (CLAUDE.md multi-entity guard): switching host or editing one project leaves every other project's and host's files untouched.
- [x] Amend `docs/research/akidevsync-project-config-scope.md` with the reopen record.
- [ ] Update `docs/arch` for the store, `docs/feat/sync-flow.md` (baseline location), `README.md`, `IntroModal.vue`.

## Cross-references

- `docs/research/akidevsync-project-config-scope.md` — the decision this reopens; its field table is still the classification source.
- `docs/plan/conflict-detection-and-agy-report.md` — plan 2, consumes `hosts/<host>/baseline.json`.
- `docs/plan/backlog.md` #6 — tracks both plans in order; notes `task-1785676763350` (reopened) and `task-1787393248179`.
- `docs/plan/done/1.22.0-notes-json-ssot.md` — precedent for `.akidevsync/` and the unreadable-folder contract.
