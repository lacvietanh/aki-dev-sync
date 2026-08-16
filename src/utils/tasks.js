// Pure non-mutating task list helpers (CLAUDE.md multi-entity regression guard).
// Migrates legacy shapes (status/state -> done/pin/wish) and returns a new task array.
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

// 3-tier task sort: uncompleted (pinned -> wish -> insertion order) before completed tasks.
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
