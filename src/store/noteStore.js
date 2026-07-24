import { ref } from 'vue'
import { invoke } from '../utils/tauri'
import { action } from '../services/action'

// The global note is SHARED session state (SCOPE-1), so its content lives in a store/*.js ref that
// mirrors host→companion automatically — a phone opening the note sees the Mac's current text, and
// the Mac sees edits typed on the phone. Before this it was a plain composable ref that never
// mirrored, and the save `invoke('write_global_note')` only hit the Mac's disk, so a companion edit
// never reached the Mac's live UI (the ACT-1 bug class). `showGlobalNote`/`noteSaving` stay local to
// the clicker in useGlobalNote — they are transient modal-open / spinner UI, not shared state.
export const noteContent = ref('')

// C→H action: a companion typing runs the real persist ON THE HOST, which mutates the mirrored
// `noteContent` (flows back to every screen) and writes the Mac's disk. On the host action(fn)===fn,
// so this is byte-identical to the old inline mutate+invoke. Returns the persist promise so the
// clicker's flushSave spinner can await it on the host.
export const saveNote = action('noteStore.saveNote', (content) => {
  noteContent.value = content
  return invoke('write_global_note', { content })
})
