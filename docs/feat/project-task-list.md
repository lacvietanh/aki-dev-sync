# Task List & Notes — shared engine, two data sources

A lightweight task list and note-taking module, available in two places that now share one engine: **per-project** (the `TASKS` column, right before `GIT`) and **Global Note** (the titlebar sticky-note icon). Same add/pin/wish/done/notes behavior in both — only what backs the data differs.

## Shared architecture

| Layer | File | Owns |
| :-- | :-- | :-- |
| Pure logic | `src/utils/tasks.js` | `normalizeTasks` (migrations, non-mutating), `sortTasks` (3-tier order), `countOpen`/`countPinned`/`summarize`, `makeTask`. No Vue, no persistence. |
| Factory | `src/composables/useTaskCollection.js` | `useTaskCollection({ read, apply })` — one task list + one notes field over *any* data source. Every mutator (`addTask`, `toggleProp`, `removeTask`, `updateTitle`, `updateDetail`, `setNotes`) builds a **new** array and hands it to `apply()`; nothing is ever mutated in place. `hideCompleted` is local per instance (transient UI, never persisted). |
| List UI | `src/components/tasks/TaskListPanel.vue` | The whole task list markup: summary bar, hide-completed toggle, add-row, transition-group ordering, per-row pin/wish/title/detail/time/check/copy/delete. Presentational — takes `tasks`/`summary`/`hideCompleted` as props, emits the edits back up. |
| Notes UI | `src/components/tasks/NotesField.vue` | The auto-growing notes textarea (`field-sizing: content`), auto-trim on change. |
| Badges | `src/components/tasks/TaskCountBadges.vue` | The two absolute-overlay count badges (pinned amber, open white) — shared by `TaskCell.vue` (project row) and `AppHeader.vue` (Global Note button). |

Two data sources plug into the same factory:

- **Project tasks** — `useProjectTaskCollection(projectRef)` in `src/composables/useProjectTasks.js`. Reads `project.tasks`/`project.notes`; persists through `applyTaskEdit(projectId, patch)`, which is id-scoped and host-resolved (only ever touches the one project it names).
- **Global tasks** — `useGlobalTaskCollection()` in `src/composables/useGlobalNote.js`. Reads the mirrored `globalTasks`/`noteContent` refs; persists through `applyGlobalNoteEdit(patch)` (`src/store/noteStore.js`), which is a read-modify-write over `globalnote.json` — a field the caller doesn't include is left untouched on disk, never cleared.

**Why this matters for correctness (multi-entity regression guard, CLAUDE.md):** `useTaskCollection` never imports a store directly — it only ever sees the two functions passed in — so a bug in the Global Note wiring cannot reach a project's tasks, and vice versa. Each `apply` function is its own scoped persist funnel; there is no shared "clear everything" path.

## Reached by

- **Project**: clicking the Tasks button in the `TASKS` column opens `ProjectTasksModal.vue` — header + `NotesField` + `TaskListPanel`, wired to `useProjectTaskCollection`.
- **Global**: clicking the titlebar sticky-note icon opens `GlobalNoteModal.vue` — the existing big monospace notes textarea (same `NotesField`, with `:deep()` style overrides for its larger look) + `TaskListPanel`, wired to `useGlobalTaskCollection`. Its `:deep()` override also resets the textarea's sizing from `NotesField`'s base `field-sizing: content` (inert in WKWebView) to a fixed `min-height: 42px` (2 rows) + `max-height: 60vh` + `resize: vertical`, so the drag handle has an explicit baseline. Below 700px width the modal locally repeats the narrow-mode padding trim on its own `.note-body`/`.note-footer` (scope-override outranks `main.css`'s global rule — same pattern as ChangelogModal/ClaudeProfileModal/SshConfigModal/UpdateModal). The titlebar button itself shows `TaskCountBadges` for the global list's pinned/open counts.

## Behavior
- **Click to open**: Clicking the Tasks button opens a focused centered modal (`ProjectTasksModal.vue`).
- **Add**: Type in the top input and press Enter or click the `+ Add` button.
- **Controls & States**: Grouped together on the far left rail of each task row:
  - **Checkmark (Done)**: Toggles the task's completion status. Completed tasks are struck through, dimmed (`opacity: 0.45`), and grayed out. Marking a task as completed automatically unpins it (sets `pin` status to `false`).
  - **Pin (📌)**: Ghim task lên đầu (disabled when task is done). Displays as an amber thumbtack (`fa-solid fa-thumbtack`) when active.
  - **Wish (🕒)**: Trạng thái "để sau" (disabled when task is done). Displays as a blue clock (`fa-regular fa-clock`) when active.
  - Inactive states are shown as faint icons (`opacity: 0.35`) without a background.
- **Copy Task Text**: A clipboard icon next to the delete button copies the task's title and description to the clipboard.
  - Format: `[Title]\n[Detail]` (or just `[Title]` if no detail is present).
  - Feedback: The copy icon temporarily switches to a green checkmark (`fa-check`) for 1.5 seconds to acknowledge successful copying.
- **Ordering & Transitions**:
  - Active Pinned tasks first $\to$ Active Normal tasks $\to$ Active Wish tasks $\to$ Completed tasks at the bottom.
  - Within each group, tasks are sorted stably by `created_at` (insertion order) to prevent jumping around.
  - Reordering uses Vue `<transition-group>` and native CSS `transform` transitions to slide rows smoothly.
- **Fast Completion**: Pressing the Enter key while focused on a task's Title input instantly toggles its `done` status.
- **Hide Completed**: A checkbox in the header allows hiding finished tasks. When toggled, completed items vanish instantly (`display: none` to avoid layout glitches) while the remaining items slide up smoothly.
- **Badge Indicator**: The Tasks button on the project row displays a badge counting active tasks (todo + wish). The button and badge turn amber if any active task is pinned.

---

## Project Notes

In addition to individual tasks, a general **Project Notes** card is placed at the top of the task window. This notes card acts as a fast, zero-context-switching scratchpad for credentials, staging URLs, deploy commands, or other specific project configurations.

- **Auto-save**: Edits inside the text box are automatically saved back to disk when the textarea loses focus or when editing completes (the `change` event fires).
- **Auto-trim**: Leading and trailing whitespaces and empty newlines are automatically cleaned using `.trim()` upon save, keeping the layout clean and preventing blank rows from bloating database file records.
- **Native Autogrow Height**: Replaced heavy JS keypress height listeners with CSS native **`field-sizing: content;`** on both the project notes and task detail textareas. Textareas resize instantly and smoothly on macOS WebKit (Tauri) without scrollbars or layout shifts.

---

## Data and persistence

### Project tasks & notes — `<local_path>/.akidevsync/notes.json` (since 1.22.0)

**The local repo is the source of truth for a project; a remote host is only somewhere its code runs.** Tasks and notes therefore live in the project's own working directory, not in the app's central `projects.json`. The file is meant to be committed — that is the point of the move. Design record: `docs/plan/done/1.22.0-notes-json-ssot.md`. Owner: `src-tauri/src/project_notes.rs`, the only place in the Rust tree that spells `.akidevsync/`.

**Self-hosting wrinkle, noted 2026-07-30, unresolved.** This app is itself a project the app can manage, so its own repo also grows a `.akidevsync/notes.json` — and this one is real: it holds the owner's actual task list for this project, not a fixture. It is gitignored in *this* repo rather than committed, which is the opposite of the paragraph above ("the file is meant to be committed — that is the point of the move"). That is a live tension, not a settled exception: "tasks travel with the repo so they're shareable" argues for committing it here of all places, while "this is the owner's private working list, not something to publish in an open-source repo" argues against. Left for the owner to decide; nothing here should be read as the resolution.

```json
{
  "about": "https://github.com/lacvietanh/aki-dev-sync",
  "schema": 1,
  "notes": "Staging URL, deploy notes…",
  "tasks": [ /* shape owned by src/utils/tasks.js — opaque to Rust */ ],
  "updated_at": 1753600000123
}
```

`about` and `schema` are rewritten on every write, so a hand-deleted value self-heals; `updated_at` is stamped by Rust and is the staleness fence (below). Every field carries `#[serde(default)]`, so a file written by a future version — or hand-trimmed to `{"notes":"x"}` — still loads.

**The read returns a STATUS, not a defaulted struct.** This is the load-bearing difference from `global_note.rs`, which may `unwrap_or_default()` a corrupt file because nothing else writes it and it lives in app data. Here the file sits in a git repo the user also edits and pulls over, on a path that may be an unmounted external volume, so "could not read it" is ordinary and recoverable:

| `status` | Means | Writable? |
| :-- | :-- | :-- |
| `ok` | read and parsed | yes |
| `missing` | directory readable, no file yet (fresh clone, first note) | yes — the write creates it |
| `unavailable` | `local_path` is not a readable directory, or the file cannot be read | **no** |
| `corrupt` | present but not valid JSON (git conflict markers) | **no** |

Collapsing those four into "empty" is exactly the data-loss bug the type exists to prevent: the UI would show an empty note and then save that emptiness over the user's real one. `isProjectNotesWritable(id)` (`src/composables/useProjectNotes.js`) is the SINGLE predicate every read-only state in the UI reads — no component re-derives it. Not writable ⇒ `TaskCell` dims and drops its badges (never `0`, which would claim there are no tasks), the modal title gains the status as a suffix with the parse error in its tooltip, the notes field is `readonly` and the task controls are disabled — all on elements that already existed (Extreme Narrow).

**Writes are read-modify-write under one global async mutex**, then `write_atomic` (temp + rename). `None` for a field means *leave what is on disk alone*, never *clear it* — so a `git pull` that changed `notes` survives a task-only write. Same-field races are last-write-wins but never silent: if the on-disk `updated_at` is newer than the caller's, the write still lands and reports `clobbered: true`, which raises a Toast pointing at git. No CRDT, no conflict UI — git is the recovery path.

**A late read must never land on top of an edit.** The read is `spawn_blocking` precisely because it can stall for tens of seconds on an unhealthy network mount, so this sequence is reachable: modal opens → refresh starts → user edits → `applyTaskEdit` writes and re-seeds → *then* the pre-edit read resolves. Applying it would revert the edit on screen and persist that reverted content on the next save — the 1.20.0 bug in a new place. So `projectNotesStore.js` keeps a **per-id generation counter**: every mutation bumps it, every async read captures it at start and drops its own result if it is no longer current. The same counter is why a `local_path` change sets the entry to `'unknown'` **synchronously** before kicking off the re-read — until that read lands, the old directory's entry would otherwise still answer `ok`/writable while `applyTaskEdit` already writes to the new path.

Writes are additionally **serialised per project id** (`queueNotesWrite` in `remoteActions.js`), so the second of two rapid edits reads a `baseUpdatedAt` that already reflects the first. Without it, every fast second edit would report `clobbered` against our own previous write — a false "someone else changed this file" alarm, which is worse than none, since that toast is the only signal telling the user to go look at git. Rust's global mutex makes the *file* safe; this queue makes the caller's view of `updated_at` *truthful*. Different halves, neither replaces the other.

**One writer, one funnel.** `applyTaskEdit` (`src/store/remoteActions.js`) is the only call site of `write_project_notes`, including the migration. PERSIST-1 is unchanged: it is still an `action()`, so a phone's edit is dispatched as an intent and the whole body runs on the Mac. **That is the entire companion story** — reads reach a phone through the mirrored `src/store/projectNotesStore.js` (every `isRef` export of `src/store/*.js` is auto-mirrored), so no command was added to `COMPANION_ALLOWED_COMMANDS`. The refused-write guard runs *before* the store is mutated, or the UI would show a change that never reached disk and the next `broadcastFull()` would silently revert it — the exact shape of the 1.20.0 "task note reverts" bug.

**Excludes.** `.akidevsync/` is in both `pull_excludes` and `push_excludes` by default, and a one-time `migrateNotesExcludes` adds it to existing projects. The pull side is not a preference: `delete_on_pull` defaults to `true`, mirror mode passes `--delete`, and the remote does not have the directory — so one PULL without the entry **deletes the task list**. The push side is excluded because the host is not a consumer and the notes field's own placeholder invites credentials.

**Migration off `projects.json`** runs once per launch in `loadData`, after hydration, in `migrateLegacyProjectNotes`. Flagless idempotence: a record with no `tasks`/`notes` key is by definition migrated, and Rust enforces the one-way property —

```rust
#[serde(default, skip_serializing_if = "Option::is_none")] pub tasks: Option<Vec<ProjectTask>>,
#[serde(default, skip_serializing_if = "Option::is_none")] pub notes: Option<String>,
```

so a cleared key is never re-materialized, not even by a stale companion array (the `sync_git` precedent). An on-disk file with content **wins** over the legacy fields; an `unavailable`/`corrupt` directory is skipped entirely and retried next launch. Both fields and `ProjectTask` are deleted in 1.23.0.

### Global tasks — `{appDataDir}/globalnote.json`

Unchanged, and deliberately not moved: the Global Note is not project-scoped, so it has no repo to live in (`src-tauri/src/global_note.rs`).

```rust
pub struct GlobalNoteFile {
    #[serde(default)] pub content: String,
    #[serde(default)] pub tasks: Vec<serde_json::Value>,  // opaque to Rust
}
```

`tasks` is `serde_json::Value`, not a typed struct — Rust never inspects or validates task shape, for either data source; `src/utils/tasks.js` is the one owner of the task schema and its migrations. `write_global_note(content: Option<String>, tasks: Option<Vec<Value>>)` uses the same leave-`None`-alone read-modify-write contract as `write_project_notes`. Timestamps are generated on the frontend via `Date.now()`.

---

## Key files

- `src/utils/tasks.js` - pure task functions (`normalizeTasks`, `sortTasks`, `summarize`, `makeTask`), shared by both data sources.
- `src/composables/useTaskCollection.js` - the factory (`useTaskCollection({ read, apply })`).
- `src/components/tasks/TaskListPanel.vue`, `NotesField.vue`, `TaskCountBadges.vue` - shared presentational components.
- `src/composables/useProjectTasks.js` - `useProjectTaskCollection(projectRef)`, plus modal open/close state (`showTasksModal`, `tasksProject`).
- `src/composables/useGlobalNote.js` - `useGlobalTaskCollection()`, plus `initGlobalNote`/`openGlobalNote`/`onNoteInput`.
- `src/store/noteStore.js` - `noteContent`, `globalTasks` (mirrored), `applyGlobalNoteEdit` (the read-modify-write persist funnel).
- `src-tauri/src/project_notes.rs` - the notes file: path, schema, tagged read status, locked read-modify-write, `read_project_notes` / `read_project_notes_map` / `write_project_notes`.
- `src/store/projectNotesStore.js` - `projectNotes` (mirrored) + the three id-scoped accessors; the companion read path.
- `src/composables/useProjectNotes.js` - hydrate/refresh, `isProjectNotesWritable`, `projectNotesFor`, `migrateLegacyProjectNotes`.
- `src/store/remoteActions.js` - `applyTaskEdit` (the ONE writer) and `requestProjectNotesRefresh`.
- `src-tauri/src/projects.rs` - `ProjectTask` struct and the DEPRECATED `notes`/`tasks` `Option` fields on `SyncProject` (removed in 1.23.0).
- `src-tauri/src/global_note.rs` - `GlobalNoteFile`, `read_global_note`/`write_global_note` commands.
- `src/components/TaskCell.vue` - project row's trigger button, using `TaskCountBadges`.
- `src/components/modals/ProjectTasksModal.vue`, `GlobalNoteModal.vue` - the two modals, each just header/footer + `NotesField` + `TaskListPanel`.
- `src/components/ProjectTable.vue` - `TASKS` column placement and layout.
- `src/components/AppHeader.vue` - Global Note button, with `TaskCountBadges` for pinned/open counts.
- `src/composables/useProjectConfig.js` - new-project defaults (including the `.akidevsync/` excludes), `migrateNotesExcludes`, and the hydrate + migrate sequence inside `loadData`.
