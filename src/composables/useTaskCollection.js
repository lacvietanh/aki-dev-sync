import { ref, computed } from 'vue'
import { normalizeTasks, sortTasks, summarize, makeTask } from '../utils/tasks'

/**
 * Task collection factory over injected { read, apply } without direct store imports.
 * Isolates project persistence (applyTaskEdit, id-scoped, PERSIST-1) from global note persistence (applyGlobalNoteEdit, WP-F) to satisfy multi-entity regression guard (CLAUDE.md).
 * @param {() => ({tasks: any[], notes: string})} read - Live read of owning entity (must supply normalizeTasks output).
 * @param {(patch: {tasks?: any[], notes?: string}) => any} apply - Persist funnel sending only changed fields.
 */
export function useTaskCollection({ read, apply }) {
  const hideCompleted = ref(false)

  const tasks = computed(() => read().tasks || [])
  const notes = computed(() => read().notes || '')

  const orderedTasks = computed(() => {
    let list = sortTasks(tasks.value)
    if (hideCompleted.value) {
      list = list.filter((t) => !t.done)
    }
    return list
  })

  const summary = computed(() => summarize(tasks.value))

  function addTask(title) {
    const text = (title || '').trim()
    if (!text) return null
    const task = makeTask(text)
    apply({ tasks: [...tasks.value, task] })
    return task
  }

  // toggleProp auto-unpins on done; every toggle stamps updated_at.
  function toggleProp(task, prop) {
    const now = Date.now()
    const next = tasks.value.map((t) => {
      if (t.id !== task.id) return t
      const updated = { ...t, [prop]: !t[prop], updated_at: now }
      if (prop === 'done' && updated.done) updated.pin = false
      return updated
    })
    apply({ tasks: next })
  }

  function removeTask(task) {
    apply({ tasks: tasks.value.filter((t) => t.id !== task.id) })
  }

  function updateTitle(task, title) {
    const next = tasks.value.map((t) =>
      t.id === task.id ? { ...t, title, updated_at: Date.now() } : t
    )
    apply({ tasks: next })
  }

  function updateDetail(task, detail) {
    const trimmed = (detail || '').trim()
    const next = tasks.value.map((t) =>
      t.id === task.id ? { ...t, detail: trimmed, updated_at: Date.now() } : t
    )
    apply({ tasks: next })
  }

  function setNotes(text) {
    apply({ notes: (text || '').trim() })
  }

  return {
    tasks,
    orderedTasks,
    summary,
    notes,
    hideCompleted,
    addTask,
    toggleProp,
    removeTask,
    updateTitle,
    updateDetail,
    setNotes,
  }
}
