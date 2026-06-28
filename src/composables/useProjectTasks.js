import { ref } from 'vue'
import { saveProjectsList } from './useProjectConfig'

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
  saveProjectsList()
  return task
}

export function toggleTaskProp(task, prop) {
  task[prop] = !task[prop]
  task.updated_at = Date.now()
  saveProjectsList()
}

export function removeTask(project, task) {
  const tasks = ensureTasks(project)
  const i = tasks.findIndex((t) => t.id === task.id)
  if (i !== -1) {
    tasks.splice(i, 1)
    saveProjectsList()
  }
}
