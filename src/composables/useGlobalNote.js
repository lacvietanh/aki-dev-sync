import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export const showGlobalNote = ref(false)
export const noteContent = ref('')
export const noteSaving = ref(false)

let saveTimer = null

export async function openGlobalNote() {
  showGlobalNote.value = true
  try {
    noteContent.value = await invoke('read_global_note')
  } catch (_) {}
}

export function closeGlobalNote() {
  flushSave()
  showGlobalNote.value = false
}

export function onNoteInput(val) {
  noteContent.value = val
  clearTimeout(saveTimer)
  saveTimer = setTimeout(flushSave, 500)
}

async function flushSave() {
  clearTimeout(saveTimer)
  noteSaving.value = true
  try {
    await invoke('write_global_note', { content: noteContent.value })
  } catch (_) {}
  noteSaving.value = false
}
