import { ref, computed } from 'vue'
import { invoke } from '../utils/tauri'
import { noteContent, globalTasks, applyGlobalNoteEdit } from '../store/noteStore'
import { useTaskCollection } from './useTaskCollection'
import { normalizeTasks, summarize } from '../utils/tasks'

// noteContent/globalTasks now live in store/noteStore.js so they mirror host→companion (shared
// session state). Re-exported here so existing importers (AppHeader, GlobalNoteModal) don't change
// their import path.
export { noteContent, globalTasks }

// Transient, clicker-local UI — not shared state, so intentionally NOT in a store.
export const showGlobalNote = ref(false)
export const noteSaving = ref(false)

let saveTimer = null
let pendingSave = null

export async function initGlobalNote() {
  // Silent load on startup - just populates noteContent/globalTasks so AppHeader can show the
  // amber indicator and the task-count badges without the user needing to open the note.
  try {
    const file = await invoke('read_global_note')
    noteContent.value = file.content
    globalTasks.value = file.tasks
  } catch (_) {}
}

export async function openGlobalNote() {
  showGlobalNote.value = true
  // Wait out any save still in flight so we don't clobber the just-saved
  // content with a stale disk read (see closeGlobalNote/flushSave).
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
  // Set locally for immediate echo in the textarea. On the host this IS the mirrored ref, so the
  // edit streams live to any connected phone. On a companion this only touches the phone's copy for
  // display; the host's authoritative copy is set by the applyGlobalNoteEdit action below and
  // mirrors back.
  noteContent.value = val
  clearTimeout(saveTimer)
  saveTimer = setTimeout(flushSave, 500)
}

function flushSave() {
  clearTimeout(saveTimer)
  if (pendingSave) return pendingSave
  noteSaving.value = true
  // Persist through the store action so a companion's edit reaches the Mac: on the host it mutates
  // noteContent + writes disk; from a companion it ships an intent the host runs. Promise.resolve
  // normalizes the companion stub's `undefined` return into an awaitable for the spinner.
  // content-only: `tasks` is omitted, so a debounced text edit can never touch the task list
  // (global_note.rs's read-modify-write contract).
  pendingSave = Promise.resolve(applyGlobalNoteEdit({ content: noteContent.value }))
    .catch(() => {})
    .finally(() => {
      noteSaving.value = false
      pendingSave = null
    })
  return pendingSave
}

// The global note's task collection — same factory the project tasks use (useProjectTaskCollection
// in useProjectTasks.js), just pointed at the mirrored globalTasks/noteContent refs instead of a
// project entity. Every mutator here routes through applyGlobalNoteEdit, so it can never reach a
// project's tasks.json fields (the multi-entity regression guard, CLAUDE.md).
export function useGlobalTaskCollection() {
  return useTaskCollection({
    read: () => ({ tasks: normalizeTasks(globalTasks.value), notes: noteContent.value }),
    apply: (patch) => applyGlobalNoteEdit(patch),
  })
}

// For the header badge (AppHeader.vue) — pinned/open counts without opening the modal.
export const globalNoteSummary = computed(() => summarize(normalizeTasks(globalTasks.value)))
