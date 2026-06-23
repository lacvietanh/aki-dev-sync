<template>
  <BaseModal :show="showSshModal" @close="closeSshModal" container-class="ssh-modal">
    <template #title>
      <i class="fa-solid fa-server mr-1"></i> SSH Config (~/.ssh/config)
    </template>
    <div class="modal-body ssh-editor-container">
      <textarea v-model="sshConfigText" class="code-editor" spellcheck="false" placeholder="Host bien-guest\n  HostName 192.168..." @keydown.tab.prevent="handleEditorTab"></textarea>
    </div>
    <div class="modal-footer">
      <div style="display: flex; gap: 8px; margin-right: auto;">
        <button class="btn-tech btn-tech-secondary" @click="undo" title="Undo" :disabled="!hasSshUndo">
          <i class="fa-solid fa-rotate-left"></i> UNDO
        </button>
        <button class="btn-tech btn-tech-secondary" @click="redo" title="Redo" :disabled="!hasSshRedo">
          <i class="fa-solid fa-rotate-right"></i> REDO
        </button>
      </div>
      <div style="display: flex; gap: 8px;">
        <button class="btn-tech btn-tech-secondary" @click="closeSshModal">CANCEL</button>
        <button class="btn-tech btn-tech-primary" @click="save"><i class="fa-solid fa-floppy-disk"></i> SAVE</button>
      </div>
    </div>
  </BaseModal>
</template>

<script setup>
import BaseModal from './BaseModal.vue'
import { useSsh } from '../../composables/useSsh'
import { useProjects } from '../../composables/useProjects'

const {
  showSshModal, sshConfigText, hasSshUndo, hasSshRedo,
  closeSshModal, handleEditorTab, saveSshConfig, undoSshConfig, redoSshConfig
} = useSsh()

const { saveProjectsList } = useProjects()

function save() { saveSshConfig(saveProjectsList) }
function undo() { undoSshConfig(saveProjectsList) }
function redo() { redoSshConfig(saveProjectsList) }
</script>
