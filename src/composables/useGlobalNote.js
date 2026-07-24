import { ref } from 'vue'
import { invoke } from '../utils/tauri'
import { noteContent, saveNote } from '../store/noteStore'

// noteContent now lives in store/noteStore.js so it mirrors host→companion (shared session state).
// Re-exported here so existing importers (AppHeader, GlobalNoteModal) don't change their import path.
export { noteContent }

// Transient, clicker-local UI — not shared state, so intentionally NOT in a store.
export const showGlobalNote = ref(false)
export const noteSaving = ref(false)

let saveTimer = null
let pendingSave = null

export async function initGlobalNote() {
  // Silent load on startup - just populates noteContent so AppHeader can
  // show the yellow indicator without the user needing to open the note.
  try {
    noteContent.value = await invoke('read_global_note')
  } catch (_) {}
}

export async function openGlobalNote() {
  showGlobalNote.value = true
  // Wait out any save still in flight so we don't clobber the just-saved
  // content with a stale disk read (see closeGlobalNote/flushSave).
  if (pendingSave) await pendingSave
  try {
    noteContent.value = await invoke('read_global_note')
  } catch (_) {}
}

export async function closeGlobalNote() {
  await flushSave()
  showGlobalNote.value = false
}

export function onNoteInput(val) {
  // Set locally for immediate echo in the textarea. On the host this IS the mirrored ref, so the
  // edit streams live to any connected phone. On a companion this only touches the phone's copy for
  // display; the host's authoritative copy is set by the saveNote action below and mirrors back.
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
  pendingSave = Promise.resolve(saveNote(noteContent.value))
    .catch(() => {})
    .finally(() => {
      noteSaving.value = false
      pendingSave = null
    })
  return pendingSave
}
