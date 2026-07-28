import { ref } from 'vue'
// PERSIST-1 (docs/plan/done/1.20.0-terminal-and-remote-sync.md §2): every task/notes mutation goes
// through the id-based, host-resolved applyTaskEdit action, never a bare saveProjectsList() —
// that bare call is what shipped a companion's whole `projects` array to disk instead of the
// host's, and is the root cause of the "task note reverts" bug. No import cycle: remoteActions.js
// does not import this module.
import { applyTaskEdit } from '../store/remoteActions'
import { normalizeTasks, countOpen, countPinned } from '../utils/tasks'
import { projectNotesFor } from './useProjectNotes'
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
 *  always runs the raw list through normalizeTasks() (never mutates the store entry itself);
 *  `apply` persists through applyTaskEdit, which is id-scoped and host-resolved
 *  (PERSIST-1) — this is the ONLY funnel a mutation from this collection can reach disk through. */
export function useProjectTaskCollection(projectRef) {
  return useTaskCollection({
    read: () => {
      // 1.22.0: the data comes from `<local_path>/.akidevsync/notes.json` via the mirrored
      // projectNotesStore, not from the project record. `apply` is untouched — the funnel is the
      // point and it did not move.
      const { tasks, notes } = projectNotesFor(projectRef.value?.id)
      return { tasks: normalizeTasks(tasks), notes }
    },
    apply: (patch) => applyTaskEdit(projectRef.value.id, patch),
  })
}

// Thin, non-mutating re-exports over src/utils/tasks.js — kept so existing callers (TaskCell.vue,
// ProjectTable.vue) don't have to import utils/tasks.js directly and so the count/normalize step
// stays in exactly one place. These never write anything back.
//
// They take the project (not just an id) because every call site already has one, but they read
// through `project.id` — the tasks no longer live on the record (1.22.0).
export function openTaskCount(project) {
  return countOpen(normalizeTasks(projectNotesFor(project?.id).tasks))
}

export function doingCount(project) {
  return countPinned(normalizeTasks(projectNotesFor(project?.id).tasks))
}

/** Total task count for one project — the third number TaskCell's badge summary needs, alongside
 *  the two above, without any component reaching into the store's shape itself. */
export function totalTaskCount(project) {
  return projectNotesFor(project?.id).tasks.length
}
