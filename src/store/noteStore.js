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

// The global task list — mirrored exactly like noteContent (shared session state, not per-screen).
// WP-F: the global note gained a shared task list on top of its free-text content, via the same
// useTaskCollection factory the project tasks use (src/composables/useGlobalNote.js).
export const globalTasks = ref([])

// C→H action: a companion editing either the note text or the task list runs the real persist ON
// THE HOST, which mutates the mirrored refs (flows back to every screen) and writes the Mac's disk.
// On the host action(fn)===fn, so this is byte-identical to an inline mutate+invoke.
//
// Only fields ACTUALLY PRESENT in `patch` are applied or sent — this is the multi-entity regression
// guard (CLAUDE.md) applied to the note file: a notes-only save must never wipe globalTasks, and a
// task-only edit must never touch noteContent. Mirrors global_note.rs's read-modify-write contract
// (`None` on the wire = "leave that field on disk alone").
export const applyGlobalNoteEdit = action('noteStore.applyGlobalNoteEdit', (patch) => {
  // `patch.notes` is an alias for `patch.content`: useTaskCollection.js's `setNotes(text)` (the
  // generic engine also used by useProjectTasks.js) calls `apply({ notes: text })` — it knows
  // nothing about this store's own field name, so this store meets that contract here rather than
  // asking the shared engine to special-case one caller's field name.
  const content = patch.content !== undefined ? patch.content : patch.notes
  if (content !== undefined) noteContent.value = content
  if (patch.tasks !== undefined) globalTasks.value = patch.tasks
  return invoke('write_global_note', {
    content: content !== undefined ? content : undefined,
    tasks: patch.tasks !== undefined ? patch.tasks : undefined,
  })
})
