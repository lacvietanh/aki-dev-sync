# Project config into `.akidevsync/` — field classification & scope decision

**Start time:** 2026-09-09

## Initial purpose

Backlog `B2` (`docs/plan/backlog.md`, note `task-1785676763350`) asks whether the per-project config that today lives only in the app's central `projects.json` should partly move into the project's own repo, at `<local_path>/.akidevsync/`, alongside `notes.json` (already there since 1.22.0, `docs/plan/done/1.22.0-notes-json-ssot.md`).

The note itself flags the blocking constraint: config holds **machine-specific paths**. This app mirrors one repo between a Mac and a remote dev box, and the two sides see different `local_path` and `remote_host` values — committing either would break the other side. The note's own required process: classify every field as *project-attribute* (travels with the repo — name, ordering, feature toggles) or *machine-attribute* (stays local — path, host, credential), then either design a split or keep the status quo after the analysis.

## Strategy

1. Read the live `SyncProject` struct (`src-tauri/src/projects.rs`) field by field — the actual schema, not a guess or the note's paraphrase.
2. For each field, decide: does its *value* depend on which machine/host pairing is running this app, or only on the project's own code/intent?
3. Weigh the SSoT cost (`pattern.A1`) of splitting one project's config across two files (`projects.json` centrally + `<repo>/.akidevsync/config.json`) against the benefit a note from 2026-08 estimated but did not measure.
4. Conclude: propose a split, or record why the status quo already satisfies the note's goal.

## Checklist

- [x] Enumerate every `SyncProject` field with its current storage and mutation path.
- [x] Classify each field as project-attribute, machine-attribute, or runtime/derived (neither — write-only history, never something to "configure").
- [x] Check whether any candidate project-attribute field already leaks a machine-specific value in practice (e.g. a hook command with an absolute local path baked in).
- [x] Weigh split cost vs. benefit against `pattern.A1` (Single Source of Truth) and the concurrency/partial-write cost of a second store for the same entity.

## Result

### Field-by-field classification

Source: `src-tauri/src/projects.rs:39-78` (`SyncProject`), cross-checked against write sites in `src/composables/useProjectConfig.js`.

| Field | Depends on machine/host pairing? | Class | Why |
|---|---|---|---|
| `id` | — | **generated, not configured** | Assigned locally when a project is added (`useProjectConfig.js:278`); a second machine adding the same repo would mint its own `id`. Never a value to travel. |
| `name` | No | **project** | Display label for the project itself. |
| `local_path` | **Yes** | **machine** | The whole reason B2 exists — literally different per machine by construction. |
| `remote_host` | **Yes** | **machine** | An SSH host alias defined in *this machine's* `~/.ssh/config` (`sshHosts.value` from `get_ssh_hosts`, `useProjectConfig.js:124`); the alias may not even exist on another machine. |
| `remote_path` | **Yes** | **machine** | Paired with `remote_host` — a path on that specific remote. A different Mac syncing the same repo to a different remote (or a different subdirectory convention on the same remote) needs its own value. |
| `production_url` | No | **project** | Intrinsic to the deployed project, independent of who runs the sync. |
| `pull_excludes` / `push_excludes` | No (usually) | **project, with a caveat** | Patterns like `node_modules/`, `.git/`, `dist/` describe the project's own build output, not the machine. The caveat: nothing stops an owner from adding a machine-specific absolute path here today (untested, but the field's *shape* — free-text glob list — allows it). |
| `hooks.{pre,post}_{pull,push}_cmd` | Sometimes | **project, with a caveat** | A hook is conceptually "run this project's build/test step around sync" — project-level intent. In practice a command string can embed a local absolute path or a machine-only binary, and nothing validates otherwise. |
| `hooks.run_hooks_on_remote` / `hooks.ignore_hook_errors` | No | **project** | Policy about *how* hooks run, not tied to a specific machine. |
| `dev_cmd_override` / `build_cmd_override` | Rarely | **project** | Same shape as hooks: intent is project-level, but a free-text command can still embed a local path. |
| `dry_run` | No, but personal | **machine/operator preference** | A safety toggle for *this operator's* comfort running sync from *this* machine — not a property of the project's code. |
| `delete_on_pull` / `delete_on_push` | No, but personal | **machine/operator preference** | Same reasoning: sync-direction destructive-mode policy is about how one operator runs this specific Mac↔remote pairing, not an attribute of the project's source. |
| `disabled` | Yes (per pairing) | **machine** | "Skip this project in background polling" is scoped to *this app instance's* background scheduler (`docs/feat/background-refresh.md`), not to the project. |
| `last_sync_action` / `last_sync_time` / `last_sync_host` / `last_sync_status` | Yes | **runtime/derived** | Write-only history of what this machine last did. Never user-configured; irrelevant to "config" entirely. |
| `sync_git` | — | **deprecated** | Kept only for one release's migration path; not a live field. |
| `tasks` / `notes` | — | **already migrated** | Moved to `.akidevsync/notes.json` in 1.22.0; the deprecated fields here are migration-compat only. |

### What's actually left as a clean "project attribute" candidate

`name`, `production_url`, `pull_excludes`, `push_excludes`, `hooks.*`, `dev_cmd_override`, `build_cmd_override` — seven fields/groups, none of them large, none of them frequently edited once a project is set up.

### Cost of splitting them out

Moving even this narrow set into `<local_path>/.akidevsync/config.json` means:
- **Two writers for one project's identity.** `saveConfig()` (`useProjectConfig.js`) currently writes one record to one file. A split means every edit either writes two files (and now needs a two-phase-commit answer for "what if the second write fails") or the UI reads from one file and writes to two, which is exactly the shape `pattern.A1` (Single Source of Truth) warns against — for the "config" of one project to be legible, both files have to be consulted and reconciled, forever.
- **A new corruption/staleness class overlapping the one `.akidevsync/notes.json` already carries.** `project_notes.rs`'s own doc comment (`src-tauri/src/project_notes.rs:3-5`) is explicit that this path may be on an **unmounted external volume** or hand-edited via git pull from another machine — `read_project_notes` already returns a status, not a defaulted struct, to cope with that. A config split inherits the identical hazard for zero new capability, since nothing today actually needs `pull_excludes`/`hooks`/etc. to be *shared* — no multi-machine or multi-user workflow for this app has ever asked to reuse another machine's exclude list or hook command.
- **The caveat fields undercut the premise.** `hooks.*` and the two `*_cmd_override` fields can already embed a machine-specific absolute path or binary name in free text — moving the *container* into the repo does not make its *contents* portable, so the split would ship an attractive nuisance: it invites committing a hook that only works on the author's Mac.

### Benefit actually being solved for

Re-reading the note: it never states a concrete pain (no "I switched machines and lost my excludes", no "I wanted to share hook commands with a teammate"). The benefit is speculative — "these seven fields *could* travel with the repo" — against a real, demonstrated cost (SSoT split, a second corruption surface, a caveat that defeats the portability goal for two of the seven fields anyway).

## Decision

**Giữ nguyên — không tách config ra `.akidevsync/`.** All project config stays exactly where it is today, in the app's central `projects.json`. The field-by-field table above is the artifact the note asked for; it shows the machine-attribute set (`local_path`, `remote_host`, `remote_path`, `disabled`, `dry_run`, `delete_on_pull`, `delete_on_push`) is the majority, and the remaining project-attribute set is small enough, and edited rarely enough, that a second store buys no measured benefit while adding a real SSoT/corruption cost this file documents above.

**Reopen trigger:** if a concrete need shows up — e.g. an owner actually wants `pull_excludes` or a hook command to travel with the repo across machines, or a team-sharing use case for this app emerges — re-open with that specific need named, and scope the split to only the fields that need it (start from the "clean candidate" list above, not the full struct).

## Cross-refs

- `docs/plan/backlog.md` B2 — the originating note.
- `docs/plan/done/1.22.0-notes-json-ssot.md` — the precedent this note extrapolated from, and the system that already handles the one field (`notes`/`tasks`) that *did* have a demonstrated need to move.
- `src-tauri/src/project_notes.rs` — the corruption/staleness handling a config split would inherit.
- `pattern.A1` (Single Source of Truth), `pattern.B3` (critique gate before any new abstraction).
