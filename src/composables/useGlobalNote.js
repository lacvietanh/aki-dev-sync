import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export const showGlobalNote = ref(false)
export const noteContent = ref('')
export const noteSaving = ref(false)

let saveTimer = null
let pendingSave = null

export async function initGlobalNote() {
  // Silent load on startup — just populates noteContent so AppHeader can
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
  noteContent.value = val
  clearTimeout(saveTimer)
  saveTimer = setTimeout(flushSave, 500)
}

function flushSave() {
  clearTimeout(saveTimer)
  if (pendingSave) return pendingSave
  noteSaving.value = true
  pendingSave = invoke('write_global_note', { content: noteContent.value })
    .catch(() => {})
    .finally(() => {
      noteSaving.value = false
      pendingSave = null
    })
  return pendingSave
}
