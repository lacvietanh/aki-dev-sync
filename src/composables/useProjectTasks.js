import { ref } from 'vue'
// PERSIST-1 (docs/plan/1.20.0-terminal-and-remote-sync.md §2): every task/notes mutation goes
// through the id-based, host-resolved applyTaskEdit action, never a bare saveProjectsList() —
// that bare call is what shipped a companion's whole `projects` array to disk instead of the
// host's, and is the root cause of the "task note reverts" bug. No import cycle: remoteActions.js
// does not import this module.
import { applyTaskEdit } from '../store/remoteActions'

export const showTasksModal = ref(false)
export const tasksProject = ref(null)

export function openTasksModal(project) {
  tasksProject.value = project
  showTasksModal.value = true
}

export function closeTasksModal() {
  showTasksModal.value = false
  tasksProject.value = null
}

function ensureTasks(project) {
  if (!Array.isArray(project.tasks)) project.tasks = []
  // Backward compatibility migration for older status and state fields
  project.tasks.forEach(t => {
    // Migrate old 'status' (todo, doing, done)
    if (t.status !== undefined) {
      if (t.done === undefined) t.done = (t.status === 'done')
      if (t.pin === undefined) t.pin = (t.status === 'doing')
      delete t.status
    }
    // Migrate old 'state' (pin, wish, "")
    if (t.state !== undefined) {
      if (t.pin === undefined) t.pin = (t.state === 'pin')
      if (t.wish === undefined) t.wish = (t.state === 'wish')
      delete t.state
    }
    // Ensure defaults
    if (t.done === undefined) t.done = false
    if (t.pin === undefined) t.pin = false
    if (t.wish === undefined) t.wish = false
  })
  return project.tasks
}

export function sortedTasks(project) {
  return [...ensureTasks(project)].sort((a, b) => {
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

export function openTaskCount(project) {
  return ensureTasks(project).filter((t) => !t.done).length
}

export function doingCount(project) {
  // Counts active pinned tasks (serves as the highlighted "pin" badge alert on project table row trigger)
  return ensureTasks(project).filter((t) => !t.done && t.pin).length
}

export function addTask(project, title) {
  const text = (title || '').trim()
  if (!text) return null
  const now = Date.now()
  const task = {
    id: 'task-' + now,
    title: text,
    detail: '',
    done: false,
    pin: false,
    wish: false,
    created_at: now,
    updated_at: now,
  }
  ensureTasks(project).push(task)
  applyTaskEdit(project.id, { tasks: project.tasks })
  return task
}

// Takes the owning project (or lets the caller resolve one) explicitly — a bare `task` carries no
// project reference, and PERSIST-1 requires the persist to be id-based and host-resolved, so the
// caller must supply which project this task belongs to (ProjectTasksModal.vue already has
// `tasksProject` in scope for every call site).
export function toggleTaskProp(project, task, prop) {
  task[prop] = !task[prop]
  if (prop === 'done' && task.done) {
    task.pin = false
  }
  task.updated_at = Date.now()
  if (project) applyTaskEdit(project.id, { tasks: project.tasks })
}

export function removeTask(project, task) {
  const tasks = ensureTasks(project)
  const i = tasks.findIndex((t) => t.id === task.id)
  if (i !== -1) {
    tasks.splice(i, 1)
    applyTaskEdit(project.id, { tasks: project.tasks })
  }
}
