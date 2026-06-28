# Plan: Per-project Task List

## Goal
Give each project a lightweight task list so the user can track, across constant
context switches, what they are doing, plan to do, and have done on that project.
The list must be reachable in one move from the project row and must surface the
active ("doing") work first.

## Why this shape (first-principles)
- The task list is **persisted config that belongs to a project**, so it lives on
  the project record (`projects.json`) and rides the existing `load_projects` /
  `save_projects` path. No new Tauri command is needed.
- Creation and mutation are **frontend-only**, mirroring the existing project CRUD
  flow (the array is mutated in the store, then `save_projects` persists it).
  Timestamps come from the frontend `Date.now()`, same as project ids.
- Access is **hover**, reusing the proven Open-popup interaction (enter/leave grace
  timer + `mouseenter` on the popup keeps it open) so editing inside the popover is
  stable. This avoids a new modal layer (YAGNI) for the first version.

## Data model (Rust, `src-tauri/src/projects.rs`)
```rust
pub struct ProjectTask {
    pub id: String,
    pub title: String,
    pub detail: String,     // plain text, optional
    pub status: String,     // "todo" | "doing" | "done"
    pub created_at: u64,
    pub updated_at: u64,
}
```
- Add `tasks: Vec<ProjectTask>` to `SyncProject`.
- `#[serde(default)]` on `tasks` and on every non-id `ProjectTask` field so older
  `projects.json` records (no `tasks` key) deserialize without dropping data
  (project serde pitfall).
- Extend the `make_project` test helper with `tasks: vec![]`.

## Frontend
### `src/composables/useProjectTasks.js` (new)
Owns all task logic against `project.tasks`, persisting through the existing
`saveProjectsList()`:
- `addTask(project, title)` - push a `todo` task, focus stays for fast entry
- `updateTaskTitle` / `updateTaskDetail(project, task, value)` - edit + touch `updated_at`
- `cycleStatus(project, task)` - `todo -> doing -> done -> todo`
- `removeTask(project, task)`
- `sortedTasks(project)` - computed order: `doing`, then `todo`, then `done`
  (done sinks to the bottom like Google Tasks); stable within a group
- `openTaskCount(project)` / `doingCount(project)` - for the cell badge

### `src/components/TaskCell.vue` (new)
- Column button with a tasks icon plus a count badge (open = todo + doing);
  accent state when at least one task is `doing`.
- `aria-label` on the icon-only button.
- Hover opens a popover holding the **full task list** (no truncation): an add-task
  input on top, then each task on one row: a small colored status tag (DOING amber /
  TODO cyan / DONE muted) that cycles on click, the title (inline editable), an
  optional detail line (plain text), a small relative timestamp, and a delete action.
- Reuses the Open-popup hover/positioning logic and CSS conventions already in
  `ProjectTable.vue` (popup-fade transition, flip-up when near the viewport bottom).

### `src/components/ProjectTable.vue`
- Insert a `TASKS` header cell and a `<TaskCell :project="p" />` row cell directly
  **before** the GIT column.
- Update `--grid-cols` for desktop and the `<=800px` breakpoint to add the new column.

### `src/composables/useProjectConfig.js`
- `createNewProject` initializes `tasks: []` (serde default already covers old data;
  this keeps new records explicit).

## UX rules followed
- Buttons are verb-first ("Add task"); the status tag is a noun label.
- One canonical term: "Task" / "Tasks" everywhere (no synonym drift).
- No em/en dashes in UI copy.

## Out of scope (later, only if needed)
- Drag to reorder tasks, due dates, kanban columns, markdown in detail, search.

## Validation
- Static review of the Rust struct + serde defaults (no build on this machine).
- Frontend: hover a row's Tasks button -> add, edit, cycle status (doing sorts to
  top), delete; reload app and confirm tasks persist from `projects.json`.
- Confirm an old `projects.json` without `tasks` still loads (serde default).
