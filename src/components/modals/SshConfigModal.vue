<template>
  <BaseModal :show="showSshModal" @close="closeSshModal" container-class="ssh-modal">
    <template #title>
      <i class="fa-solid fa-server mr-1"></i> SSH Config (~/.ssh/config)
    </template>
    
    <div class="active-host-row">
      <div class="host-selector-wrapper">
        <i class="fa-solid fa-cloud text-amber mr-1"></i>
        <span class="selector-label">Claude Code Remote Host:</span>
        <select v-model="selectedSshHost" class="host-select">
          <option value="" disabled>Select Host</option>
          <option v-for="host in sshHosts" :key="host" :value="host">{{ host }}</option>
        </select>
      </div>
    </div>

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
  sshHosts, selectedSshHost,
  showSshModal, sshConfigText, hasSshUndo, hasSshRedo,
  closeSshModal, handleEditorTab, saveSshConfig, undoSshConfig, redoSshConfig
} = useSsh()

const { saveProjectsList } = useProjects()

function save() { saveSshConfig(saveProjectsList) }
function undo() { undoSshConfig(saveProjectsList) }
function redo() { redoSshConfig(saveProjectsList) }
</script>

<style scoped>
.active-host-row {
  padding: 12px 20px 0 20px;
  display: flex;
  justify-content: flex-start;
}

.host-selector-wrapper {
  display: flex;
  align-items: center;
  gap: 8px;
  background: rgba(255, 255, 255, 0.02);
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 6px;
  padding: 6px 12px;
}

.selector-label {
  font-size: 11px;
  font-weight: 700;
  color: var(--text-light);
  letter-spacing: 0.5px;
}

.host-select {
  background-color: var(--bg-tertiary);
  color: var(--text-light);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 4px;
  padding: 2px 6px;
  height: 24px;
  font-size: 11px;
  font-family: inherit;
  outline: none;
  cursor: pointer;
  transition: all 0.2s ease;
  box-shadow: 0 1px 2px rgba(0,0,0,0.1);
}

.host-select:hover {
  background-color: rgba(255, 255, 255, 0.08);
  border-color: rgba(255, 255, 255, 0.2);
}

.host-select:focus {
  border-color: var(--accent-cyan);
  box-shadow: 0 0 0 2px rgba(0, 210, 255, 0.2);
}
</style>
