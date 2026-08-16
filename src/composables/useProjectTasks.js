import { ref } from 'vue'
// PERSIST-1: all task mutations route through host-resolved applyTaskEdit, avoiding companion list clobber.
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

/** Shared task engine wired to one project; reads normalized tasks and persists through applyTaskEdit (PERSIST-1). */
export function useProjectTaskCollection(projectRef) {
  return useTaskCollection({
    read: () => {
      // 1.22.0: Data reads from projectNotesStore (.akidevsync/notes.json SSOT) rather than project record.
      const { tasks, notes } = projectNotesFor(projectRef.value?.id)
      return { tasks: normalizeTasks(tasks), notes }
    },
    apply: (patch) => applyTaskEdit(projectRef.value.id, patch),
  })
}

// Non-mutating task count helpers reading notes SSOT via project.id; keeps count logic centralized.
export function openTaskCount(project) {
  return countOpen(normalizeTasks(projectNotesFor(project?.id).tasks))
}

export function doingCount(project) {
  return countPinned(normalizeTasks(projectNotesFor(project?.id).tasks))
}

/** Total task count for one project badge summary. */
export function totalTaskCount(project) {
  return projectNotesFor(project?.id).tasks.length
}
