import { ref } from 'vue'
// PERSIST-1 (docs/plan/done/1.20.0-terminal-and-remote-sync.md §2): every task/notes mutation goes
// through the id-based, host-resolved applyTaskEdit action, never a bare saveProjectsList() —
// that bare call is what shipped a companion's whole `projects` array to disk instead of the
// host's, and is the root cause of the "task note reverts" bug. No import cycle: remoteActions.js
// does not import this module.
import { applyTaskEdit } from '../store/remoteActions'
import { normalizeTasks, countOpen, countPinned } from '../utils/tasks'
import { useTaskCollection } from './useTaskCollection'

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

/** The shared task engine (src/composables/useTaskCollection.js) wired to ONE project. `read`
 *  always runs the raw list through normalizeTasks() (never mutates projectRef.value.tasks
 *  itself); `apply` persists through applyTaskEdit, which is id-scoped and host-resolved
 *  (PERSIST-1) — this is the ONLY funnel a mutation from this collection can reach disk through. */
export function useProjectTaskCollection(projectRef) {
  return useTaskCollection({
    read: () => ({
      tasks: normalizeTasks(projectRef.value?.tasks || []),
      notes: projectRef.value?.notes || '',
    }),
    apply: (patch) => applyTaskEdit(projectRef.value.id, patch),
  })
}

// Thin, non-mutating re-exports over src/utils/tasks.js — kept so existing callers (TaskCell.vue,
// ProjectTable.vue) don't have to import utils/tasks.js directly and so the count/normalize step
// stays in exactly one place. Unlike the old versions, these NEVER write project.tasks back.
export function openTaskCount(project) {
  return countOpen(normalizeTasks(project?.tasks || []))
}

export function doingCount(project) {
  return countPinned(normalizeTasks(project?.tasks || []))
}
