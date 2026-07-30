<template>
  <BaseModal :show="showGlobalNote" @close="closeGlobalNote" container-style="max-width: 720px; width: 90vw">
    <template #title>
      <i class="fa-solid fa-note-sticky mr-1"></i> Global Note
    </template>
    <div class="modal-body note-body scrollable">
      <NotesField
        class="global-notes-field"
        :model-value="noteContent"
        @update:model-value="onNoteInput"
        label="Global Note"
        placeholder="Ghi chú tổng hợp..."
        :maxlength="100000"
        :rows="2"
      />

      <TaskListPanel
        :tasks="collection.orderedTasks.value"
        :summary="collection.summary.value"
        :hide-completed="collection.hideCompleted.value"
        @add="collection.addTask"
        @toggle="collection.toggleProp"
        @remove="collection.removeTask"
        @update:title="collection.updateTitle"
        @update:detail="collection.updateDetail"
        @update:hide-completed="(v) => (collection.hideCompleted.value = v)"
      />
    </div>
    <div class="modal-footer note-footer">
      <span class="save-status">{{ noteSaving ? 'Saving...' : 'Auto-saved' }}</span>
      <button class="btn-secondary" @click="closeGlobalNote">Close</button>
    </div>
  </BaseModal>
</template>

<script setup>
import BaseModal from './BaseModal.vue'
import NotesField from '../tasks/NotesField.vue'
import TaskListPanel from '../tasks/TaskListPanel.vue'
import {
  showGlobalNote, noteContent, noteSaving, closeGlobalNote, onNoteInput,
  useGlobalTaskCollection,
} from '../../composables/useGlobalNote'

const collection = useGlobalTaskCollection()
</script>

<style scoped>
.note-body {
  padding: 12px 16px;
}

/* Preserve the big monospace look the Global Note has always had, via :deep() overrides on the
   shared NotesField (which otherwise renders the compact project-notes style). */
.global-notes-field {
  margin-bottom: 12px;
}

.global-notes-field :deep(.project-notes-section) {
  background: #0d1117;
  border: none;
  border-radius: 6px;
  padding: 14px 16px;
}

.global-notes-field :deep(.project-notes-textarea) {
  /* min-height is derived, not guessed: font-size 13px x line-height 1.6 = 20.8px per line;
     2 lines (this file's `:rows="2"`) = 41.6px, rounded up to 42px. The textarea itself has zero
     padding/border (NotesField.vue's base rule), so that's the exact content-box floor for 2 rows -
     not a round number like the old 320px/190px.
     field-sizing is reset to `fixed` (the property's initial value), overriding NotesField's base
     `field-sizing: content`. That base value is inert here anyway - WKWebView (this app's Tauri
     runtime is WebKit/Safari, not Chromium) does not implement `field-sizing` - but resetting it
     removes any dependency on that support existing, now or in a future WebKit version, since an
     auto-content-sizing textarea is exactly the kind of box that can fight a manual `resize` drag.
     Sizing here is deliberately driven only by min-height/max-height/resize, the same mechanism
     this file always used successfully (only the floor's px value was ever wrong) - dropping the
     floor outright (rows="2" + no min-height at all, tried previously) left the box with no
     explicit CSS height for `resize` to use as its drag baseline, which is what broke dragging. */
  field-sizing: fixed;
  min-height: 42px;
  max-height: 60vh;
  color: #e2e8f0;
  font-size: 13px;
  font-family: 'JetBrains Mono', 'Fira Mono', monospace;
  line-height: 1.6;
  resize: vertical;
}

.global-notes-field :deep(.project-notes-textarea::placeholder) {
  color: #475569;
}

.note-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 16px;
  border-top: 1px solid rgba(255, 255, 255, 0.07);
}

.save-status {
  font-size: 11px;
  color: #475569;
  letter-spacing: 0.3px;
}

/* Narrow mode (SSoT 700px, main.css) - this file's scoped .note-body/.note-footer padding outranks
   the global `.modal-body`/`.modal-footer` trim (data-v specificity), so it has to be repeated
   here, as ChangelogModal/ClaudeProfileModal/SshConfigModal/UpdateModal already do. */
@media (max-width: 700px) {
  .note-body {
    padding: 10px;
  }

  .note-footer {
    padding: 8px 10px;
  }
}
</style>
