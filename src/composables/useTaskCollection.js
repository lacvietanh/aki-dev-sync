import { ref, computed } from 'vue'
import { normalizeTasks, sortTasks, summarize, makeTask } from '../utils/tasks'

/**
 * One task list + one notes field, over ANY data source (a project entity, the global note
 * store). The source is injected as two functions — this composable NEVER imports a store: the
 * project version persists through applyTaskEdit (id-scoped, host-resolved, PERSIST-1) and the
 * global version through applyGlobalNoteEdit (WP-F), and neither can reach the other's data. That
 * separation is what keeps the multi-entity regression guard (CLAUDE.md) satisfied when two
 * unrelated collections share this one factory.
 *
 * @param {() => ({tasks: any[], notes: string})} read   live read of the owning entity. Callers
 *   are expected to have already run the raw list through normalizeTasks() before returning it.
 * @param {(patch: {tasks?: any[], notes?: string}) => any} apply  the ONE persist funnel — must
 *   be an action(). Every mutator below sends only the field(s) it actually changed.
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

  // toggleProp(task,'done') auto-unpins (done tasks never stay pinned); every prop toggle stamps updated_at. Builds a brand new array via map — never mutates `task` or `tasks.value` in place.
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
