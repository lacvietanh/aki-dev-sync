// Per-project task list logic. Tasks are persisted config living on
// `project.tasks` and ride the existing save_projects path via saveProjectsList().
// Created/mutated entirely on the frontend, mirroring the project CRUD flow.
import { saveProjectsList } from './useProjectConfig'

export const TASK_STATUSES = ['todo', 'doing', 'done']

// Display order: doing first, then todo, then done (done sinks to the bottom).
const STATUS_ORDER = { doing: 0, todo: 1, done: 2 }

function ensureTasks(project) {
  if (!Array.isArray(project.tasks)) project.tasks = []
  return project.tasks
}

// Stable sort by status priority; insertion order is preserved within a group.
export function sortedTasks(project) {
  const tasks = ensureTasks(project)
  return [...tasks].sort(
    (a, b) => (STATUS_ORDER[a.status] ?? 1) - (STATUS_ORDER[b.status] ?? 1)
  )
}

export function openTaskCount(project) {
  return ensureTasks(project).filter((t) => t.status !== 'done').length
}

export function doingCount(project) {
  return ensureTasks(project).filter((t) => t.status === 'doing').length
}

export function addTask(project, title) {
  const text = (title || '').trim()
  if (!text) return null
  const now = Date.now()
  const task = {
    id: 'task-' + now,
    title: text,
    detail: '',
    status: 'todo',
    created_at: now,
    updated_at: now,
  }
  ensureTasks(project).push(task)
  saveProjectsList()
  return task
}

export function updateTaskTitle(project, task, value) {
  const text = (value || '').trim()
  if (!text || text === task.title) return
  task.title = text
  task.updated_at = Date.now()
  saveProjectsList()
}

export function updateTaskDetail(project, task, value) {
  const text = value || ''
  if (text === task.detail) return
  task.detail = text
  task.updated_at = Date.now()
  saveProjectsList()
}

// todo -> doing -> done -> todo
export function cycleStatus(project, task) {
  const next = (STATUS_ORDER[task.status] === undefined)
    ? 'doing'
    : TASK_STATUSES[(TASK_STATUSES.indexOf(task.status) + 1) % TASK_STATUSES.length]
  task.status = next
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
