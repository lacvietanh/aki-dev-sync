<template>
  <BaseModal :show="showGlobalNote" @close="closeGlobalNote" container-style="max-width: 600px; width: 90vw">
    <template #title>
      <i class="fa-solid fa-note-sticky mr-1"></i> Global Note
    </template>
    <div class="modal-body note-body">
      <textarea
        class="note-area"
        :value="noteContent"
        @input="onNoteInput($event.target.value)"
        placeholder="Ghi chú tổng hợp..."
        spellcheck="false"
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
import { showGlobalNote, noteContent, noteSaving, closeGlobalNote, onNoteInput } from '../../composables/useGlobalNote'
</script>

<style scoped>
.note-body {
  padding: 0;
}

.note-area {
  width: 100%;
  min-height: 320px;
  max-height: 60vh;
  background: #0d1117;
  color: #e2e8f0;
  border: none;
  border-top: 1px solid rgba(255, 255, 255, 0.07);
  padding: 14px 16px;
  font-size: 13px;
  font-family: 'JetBrains Mono', 'Fira Mono', monospace;
  line-height: 1.6;
  resize: vertical;
  outline: none;
  box-sizing: border-box;
  display: block;
  field-sizing: content;
}

.note-area::placeholder {
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
