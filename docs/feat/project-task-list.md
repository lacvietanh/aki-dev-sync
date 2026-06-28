# Per-project Task List

A lightweight task list attached to each project, for tracking active items, pinned goals, and future wishes. Reached by clicking the Tasks button in the `TASKS` column (placed right before `GIT`).

## Behavior
- **Click to open**: Clicking the Tasks button opens a focused centered modal (`ProjectTasksModal.vue`).
- **Add**: Type in the top input and press Enter or click the `+ Add` button.
- **Controls & States**: Grouped together on the far left rail of each task row:
  - **Checkmark (Done)**: Toggles the task's completion status. Completed tasks are struck through, dimmed (`opacity: 0.45`), and grayed out.
  - **Pin (📌)**: Ghim task lên đầu (disabled when task is done). Displays as an amber thumbtack (`fa-solid fa-thumbtack`) when active.
  - **Wish (🕒)**: Trạng thái "để sau" (disabled when task is done). Displays as a blue clock (`fa-regular fa-clock`) when active.
  - Inactive states are shown as faint icons (`opacity: 0.35`) without a background.
- **Ordering & Transitions**:
  - Active Pinned tasks first $\to$ Active Normal tasks $\to$ Active Wish tasks $\to$ Completed tasks at the bottom.
  - Within each group, tasks are sorted stably by `created_at` (insertion order) to prevent jumping around.
  - Reordering uses Vue `<transition-group>` and native CSS `transform` transitions to slide rows smoothly.
- **Fast Completion**: Pressing the Enter key while focused on a task's Title input instantly toggles its `done` status.
- **Hide Completed**: A checkbox in the header allows hiding finished tasks. When toggled, completed items vanish instantly (`display: none` to avoid layout glitches) while the remaining items slide up smoothly.
- **Badge Indicator**: The Tasks button on the project row displays a badge counting active tasks (todo + wish). The button and badge turn amber if any active task is pinned.

## Data and persistence
- Tasks are persisted config living on the project record (`projects.json`), not ephemeral runtime state. They ride the existing `load_projects` / `save_projects` path.
- A task is created and mutated entirely on the frontend; timestamps come from JS `Date.now()`.
- Rust model:
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
  ```
  Uses `#[serde(default)]` on every task-specific field to ensure backward compatibility when loading older `projects.json` configs.

## Key files
- `src-tauri/src/projects.rs` - `ProjectTask` struct, `tasks` field, serde defaults.
- `src/composables/useProjectTasks.js` - `addTask`, `toggleTaskProp`, `removeTask`, `sortedTasks` (stable sorting logic), `openTaskCount` / `doingCount`.
- `src/components/TaskCell.vue` - the column trigger button and badge.
- `src/components/modals/ProjectTasksModal.vue` - the centered modal dialog containing the list wrapper, input fields, state buttons, hide completed checkbox, and transition styles.
- `src/components/ProjectTable.vue` - `TASKS` column (header + cell), `--grid-cols` layout (desktop/mobile) adjusted to `2.2rem` with a tight `2px` column gap.
- `src/composables/useProjectConfig.js` - `createNewProject` seeds `tasks: []`.
