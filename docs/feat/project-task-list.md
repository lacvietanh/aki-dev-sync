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
- **Global**: clicking the titlebar sticky-note icon opens `GlobalNoteModal.vue` — the existing big monospace notes textarea (same `NotesField`, with `:deep()` style overrides for its larger look) + `TaskListPanel`, wired to `useGlobalTaskCollection`. The titlebar button itself shows `TaskCountBadges` for the global list's pinned/open counts.

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

**Project tasks** are persisted directly inside the project record in `projects.json`, via the existing `load_projects` / `save_projects` lifecycle:
```rust
pub struct ProjectTask {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub done: bool,
    pub pin: bool,
    pub wish: bool,
    pub created_at: u64,
    pub updated_at: u64,
}

pub struct SyncProject {
    // ...
    #[serde(default)]
    pub tasks: Vec<ProjectTask>,
    #[serde(default)]
    pub notes: String,
}
```

**Global tasks** are persisted in `{appDataDir}/globalnote.json`, alongside the note content (`src-tauri/src/global_note.rs`):
```rust
pub struct GlobalNoteFile {
    #[serde(default)] pub content: String,
    #[serde(default)] pub tasks: Vec<serde_json::Value>,  // opaque to Rust — see below
}
```
`tasks` is `serde_json::Value`, not a typed struct — Rust never inspects or validates task shape there; `src/utils/tasks.js` is the one owner of the task schema and its migrations, for both data sources. `write_global_note(content: Option<String>, tasks: Option<Vec<Value>>)` is read-modify-write: a field left as `None` is left untouched on disk, so a notes-only save can never wipe the task list and a task-only edit can never touch the note text.

Both structs use `#[serde(default)]` on every task/notes field, so an older file (project or global) missing these fields deserializes cleanly with empty defaults — no migration step, no dropped record. Timestamps are generated on the frontend via `Date.now()`.

---

## Key files

- `src/utils/tasks.js` - pure task functions (`normalizeTasks`, `sortTasks`, `summarize`, `makeTask`), shared by both data sources.
- `src/composables/useTaskCollection.js` - the factory (`useTaskCollection({ read, apply })`).
- `src/components/tasks/TaskListPanel.vue`, `NotesField.vue`, `TaskCountBadges.vue` - shared presentational components.
- `src/composables/useProjectTasks.js` - `useProjectTaskCollection(projectRef)`, plus modal open/close state (`showTasksModal`, `tasksProject`).
- `src/composables/useGlobalNote.js` - `useGlobalTaskCollection()`, plus `initGlobalNote`/`openGlobalNote`/`onNoteInput`.
- `src/store/noteStore.js` - `noteContent`, `globalTasks` (mirrored), `applyGlobalNoteEdit` (the read-modify-write persist funnel).
- `src-tauri/src/projects.rs` - `ProjectTask` struct, `notes`/`tasks` fields on `SyncProject`.
- `src-tauri/src/global_note.rs` - `GlobalNoteFile`, `read_global_note`/`write_global_note` commands.
- `src/components/TaskCell.vue` - project row's trigger button, using `TaskCountBadges`.
- `src/components/modals/ProjectTasksModal.vue`, `GlobalNoteModal.vue` - the two modals, each just header/footer + `NotesField` + `TaskListPanel`.
- `src/components/ProjectTable.vue` - `TASKS` column placement and layout.
- `src/components/AppHeader.vue` - Global Note button, with `TaskCountBadges` for pinned/open counts.
- `src/composables/useProjectConfig.js` - Project initialization seeding (`tasks: []` and runtime setup).
