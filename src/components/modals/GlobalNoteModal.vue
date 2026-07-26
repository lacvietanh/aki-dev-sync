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
        :rows="8"
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
  min-height: 320px;
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
</style>
