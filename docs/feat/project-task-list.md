# Per-project Task List

A lightweight task list attached to each project, for tracking what is being done,
planned, and finished while switching between projects. Reached by hovering the
Tasks button in the new `TASKS` column (placed right before `GIT`).

## Behavior
- **Hover to open**: hovering the Tasks button reveals a popover with the full task
  list (no truncation). The popover stays open while the cursor is inside it and
  also while any input inside has focus, so editing is not interrupted.
- **Add**: type in the top input and press Enter or click `+`.
- **Status tag**: each task starts as `TODO`. Clicking the colored tag cycles
  `todo -> doing -> done -> todo`. DOING is amber, TODO is cyan, DONE is muted and
  struck through. Each row also carries a left rail in the same status color so the
  eye lands on active (DOING) work first when scanning.
- **Ordering**: tasks are shown DOING first, then TODO, then DONE. Order is stable
  within each group (insertion order). This keeps active work at the top.
- **Header summary**: the popover header shows a compact count of doing / todo /
  done for an instant overview of the project state.
- **Edit**: title and an optional detail line are inline inputs; they commit on
  change (blur or Enter), not on every keystroke. The detail input stays hidden
  until the row is hovered or focused (or already has detail), to keep rows clean.
- **Delete**: the `x` on the right (revealed on row hover) removes a task.
- **Badge**: the Tasks button shows the count of open tasks (todo + doing). The
  button and badge turn amber when at least one task is `doing`.
- **Styling**: uses the shared design tokens (`--accent-cyan`, `--accent-amber`,
  `--text-muted`, etc.) and the same button/transition language as the project row,
  so the cell stays visually consistent with the rest of the table.

## Data and persistence
- Tasks are persisted config living on the project record (`projects.json`), not
  ephemeral runtime state. They ride the existing `load_projects` / `save_projects`
  path. No dedicated Tauri command exists or is needed.
- A task is created and mutated entirely on the frontend; timestamps come from JS
  `Date.now()`, the same approach used for project ids.
- Rust model: `ProjectTask { id, title, detail, status, created_at, updated_at }`
  with `tasks: Vec<ProjectTask>` on `SyncProject`. `#[serde(default)]` on `tasks`
  and on every non-id `ProjectTask` field, so older `projects.json` records without
  the field still load (project serde-default pitfall).

## Key files
- `src-tauri/src/projects.rs` - `ProjectTask` struct, `tasks` field, serde defaults.
- `src/composables/useProjectTasks.js` - add/update/cycle/remove, `sortedTasks`
  (doing-first), `openTaskCount` / `doingCount`. Every mutation calls
  `saveProjectsList()`.
- `src/components/TaskCell.vue` - the column button, badge, and hover popover with
  the full editable list. Reuses the Open-popup hover/positioning pattern.
- `src/components/ProjectTable.vue` - `TASKS` column (header + cell), `--grid-cols`
  updated for desktop and the `<=800px` breakpoint.
- `src/composables/useProjectConfig.js` - `createNewProject` seeds `tasks: []`.

## Design notes
- Config and tasks are kept separate on purpose: the project config modal is for
  configuration; tasks are a working feature reached from the row, not from config.
- First version is intentionally minimal. Deferred (only if needed): reordering,
  due dates, kanban columns, markdown detail, search.
