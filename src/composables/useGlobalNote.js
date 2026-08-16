import { ref, computed } from 'vue'
import { invoke } from '../utils/tauri'
import { noteContent, globalTasks, applyGlobalNoteEdit } from '../store/noteStore'
import { useTaskCollection } from './useTaskCollection'
import { normalizeTasks, summarize } from '../utils/tasks'

// Re-export store refs for backward compatibility; state mirrors host→companion in noteStore.js.
export { noteContent, globalTasks }

// Transient, clicker-local UI — not shared state, so intentionally NOT in a store.
export const showGlobalNote = ref(false)
export const noteSaving = ref(false)

let saveTimer = null
let pendingSave = null

export async function initGlobalNote() {
  // Silent startup load to populate noteContent/globalTasks for AppHeader badges without opening modal.
  try {
    const file = await invoke('read_global_note')
    noteContent.value = file.content
    globalTasks.value = file.tasks
  } catch (_) {}
}

export async function openGlobalNote() {
  showGlobalNote.value = true
  // Wait out any save still in flight so we don't clobber the just-saved content with a stale disk read (see closeGlobalNote/flushSave).
  if (pendingSave) await pendingSave
  try {
    const file = await invoke('read_global_note')
    noteContent.value = file.content
    globalTasks.value = file.tasks
  } catch (_) {}
}

export async function closeGlobalNote() {
  await flushSave()
  showGlobalNote.value = false
}

export function onNoteInput(val) {
  // Immediate local echo in textarea; host mirrors live, companion syncs authoritative copy via applyGlobalNoteEdit.
  noteContent.value = val
  clearTimeout(saveTimer)
  saveTimer = setTimeout(flushSave, 500)
}

function flushSave() {
  clearTimeout(saveTimer)
  if (pendingSave) return pendingSave
  noteSaving.value = true
  // Persist via action (content-only protects tasks against debounce overwrite; Promise.resolve normalizes companion stub).
  pendingSave = Promise.resolve(applyGlobalNoteEdit({ content: noteContent.value }))
    .catch(() => {})
    .finally(() => {
      noteSaving.value = false
      pendingSave = null
    })
  return pendingSave
}

// Global task collection routed via applyGlobalNoteEdit (isolated from project tasks.json multi-entity state).
export function useGlobalTaskCollection() {
  return useTaskCollection({
    read: () => ({ tasks: normalizeTasks(globalTasks.value), notes: noteContent.value }),
    apply: (patch) => applyGlobalNoteEdit(patch),
  })
}

// For the header badge (AppHeader.vue) — pinned/open counts without opening the modal.
export const globalNoteSummary = computed(() => summarize(normalizeTasks(globalTasks.value)))
