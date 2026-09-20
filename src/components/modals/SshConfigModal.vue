<template>
  <BaseModal :show="showSshModal" @close="closeSshModal" container-class="ssh-modal">
    <template #title>
      <i class="fa-solid fa-server mr-1"></i> SSH Config (~/.ssh/config)
    </template>
    
    <div class="modal-body ssh-editor-container">
      <textarea v-model="sshConfigText" class="code-editor" spellcheck="false" placeholder="Host bien-guest\n  HostName 192.168..." @keydown.tab.prevent="handleEditorTab"></textarea>
    </div>
    <div class="modal-footer">
      <div class="row-gap-8 push-left">
        <button class="btn-tech btn-tech-secondary" @click="undo" title="Undo" :disabled="!hasSshUndo">
          <i class="fa-solid fa-rotate-left"></i> UNDO
        </button>
        <button class="btn-tech btn-tech-secondary" @click="redo" title="Redo" :disabled="!hasSshRedo">
          <i class="fa-solid fa-rotate-right"></i> REDO
        </button>
      </div>
      <div class="row-gap-8">
        <button class="btn-tech btn-tech-secondary" @click="closeSshModal">CANCEL</button>
        <button class="btn-tech btn-tech-primary" @click="save"><i class="fa-solid fa-floppy-disk"></i> SAVE</button>
      </div>
    </div>
  </BaseModal>
</template>

<script setup>
import BaseModal from './BaseModal.vue'
import { useSsh } from '../../composables/useSsh'

const {
  showSshModal, sshConfigText, hasSshUndo, hasSshRedo,
  closeSshModal, handleEditorTab, saveSshConfig, undoSshConfig, redoSshConfig
} = useSsh()

// Save, undo, and redo actions delegate to useSsh composable.
function save() { saveSshConfig() }
function undo() { undoSshConfig() }
function redo() { redoSshConfig() }
</script>

<style scoped>
.row-gap-8 { display: flex; gap: 8px; }
.push-left { margin-right: auto; }
/* Narrow mode container padding (<=700px). */
@media (max-width: 700px) {
  .ssh-editor-container {
    padding: 10px;
  }
}
</style>
