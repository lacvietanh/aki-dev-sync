import { ref } from 'vue'
import { invoke } from '../utils/tauri'
import { action } from '../services/action'

// Mirrored global note text (SCOPE-1: shared session state across host and companion).
export const noteContent = ref('')

// Mirrored global task list (WP-F: shared across host and companion via useTaskCollection).
export const globalTasks = ref([])

// Host action: applies partial patch to mirrored refs and persists without wiping omitted fields.
export const applyGlobalNoteEdit = action('noteStore.applyGlobalNoteEdit', (patch) => {
  // Support patch.notes alias from generic useTaskCollection contract.
  const content = patch.content !== undefined ? patch.content : patch.notes
  if (content !== undefined) noteContent.value = content
  if (patch.tasks !== undefined) globalTasks.value = patch.tasks
  return invoke('write_global_note', {
    content: content !== undefined ? content : undefined,
    tasks: patch.tasks !== undefined ? patch.tasks : undefined,
  })
})

