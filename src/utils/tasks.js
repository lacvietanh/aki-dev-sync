// Pure task-list functions shared by every task collection (project tasks, global note tasks).
// No Vue, no persistence, no store import — the data source is always injected by the caller
// (see src/composables/useTaskCollection.js). Every function here is non-mutating: it returns a
// NEW array of NEW task objects rather than editing the input in place. This matters because two
// independent collections (a project's tasks, the global note's tasks) may run through the same
// helpers in the same session — a mutating helper would risk one collection's edit leaking into
// an object still referenced by another (the multi-entity regression guard in CLAUDE.md).

/** Migrate legacy shapes (status→done/pin, state→pin/wish) and backfill defaults. Returns a NEW
 *  array of NEW task objects; never mutates `list` or any element of it. */
export function normalizeTasks(list) {
  if (!Array.isArray(list)) return []
  return list.map((t) => {
    const task = { ...t }

    // Migrate old 'status' (todo, doing, done)
    if (task.status !== undefined) {
      if (task.done === undefined) task.done = (task.status === 'done')
      if (task.pin === undefined) task.pin = (task.status === 'doing')
      delete task.status
    }
    // Migrate old 'state' (pin, wish, "")
    if (task.state !== undefined) {
      if (task.pin === undefined) task.pin = (task.state === 'pin')
      if (task.wish === undefined) task.wish = (task.state === 'wish')
      delete task.state
    }
    // Ensure defaults
    if (task.done === undefined) task.done = false
    if (task.pin === undefined) task.pin = false
    if (task.wish === undefined) task.wish = false

    return task
  })
}

/** The existing 3-tier sort, verbatim: uncompleted before completed; among uncompleted, pinned
 *  first then wish last; stable fallback to insertion order (oldest first). Returns a new array
 *  (does not sort `tasks` in place). */
export function sortTasks(tasks) {
  return [...tasks].sort((a, b) => {
    // 1. Uncompleted tasks first, completed tasks at the bottom
    if (a.done !== b.done) {
      return a.done ? 1 : -1
    }
    // 2. Active tasks sorted by pin status, then wish status
    if (!a.done) {
      if (a.pin !== b.pin) {
        return a.pin ? -1 : 1
      }
      if (a.wish !== b.wish) {
        return a.wish ? 1 : -1
      }
    }
    // 3. Fallback to stable insertion order (oldest first)
    return a.created_at - b.created_at
  })
}

export function countOpen(tasks) {
  return tasks.filter((t) => !t.done).length
}

export function countPinned(tasks) {
  return tasks.filter((t) => !t.done && t.pin).length
}

export function makeTask(title) {
  const now = Date.now()
  return {
    id: 'task-' + now,
    title,
    detail: '',
    done: false,
    pin: false,
    wish: false,
    created_at: now,
    updated_at: now,
  }
}

/** Identical shape to what TaskCell.vue / ProjectTasksModal.vue compute today. */
export function summarize(tasks) {
  const total = tasks.length
  const open = countOpen(tasks)
  const doing = countPinned(tasks)
  return { total, open, doing, todo: open - doing, done: total - open }
}
