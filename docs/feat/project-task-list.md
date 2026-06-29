# Per-project Task List & Notes

A lightweight task list and note-taking module attached to each project for tracking active items, pinned goals, future wishes, and general project context/credentials. Reached by clicking the Tasks button in the `TASKS` column (placed right before `GIT`).

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

- Tasks and Notes are persisted directly inside the project record in `projects.json`, not in temporary state. They utilize the existing `load_projects` / `save_projects` lifecycle.
- Timestamps are generated on the frontend via `Date.now()`.
- Rust models:
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
  Uses `#[serde(default)]` on every task-specific and notes field to ensure backward compatibility when loading older config schemas.

---

## Key files

- `src-tauri/src/projects.rs` - `ProjectTask` struct, `notes` and `tasks` fields, and serde defaults.
- `src/composables/useProjectTasks.js` - Task state utilities: `addTask`, `toggleTaskProp`, `removeTask`, `sortedTasks` (stable sorting logic).
- `src/components/TaskCell.vue` - Column trigger button and state badge rendering.
- `src/components/modals/ProjectTasksModal.vue` - Centered modal containing the Notes textarea, Task input list, copy action triggers, and native styling rules.
- `src/components/ProjectTable.vue` - `TASKS` column placement and layouts.
- `src/composables/useProjectConfig.js` - Project initialization seeding (`tasks: []` and runtime setup).
