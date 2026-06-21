<template>
  <div v-if="showSshModal" class="modal-overlay">
    <div class="modal-container ssh-modal">
      <div class="modal-header">
        <h2><i class="fa-solid fa-server mr-1"></i> Chỉnh Sửa SSH Config (~/.ssh/config)</h2>
        <button class="btn-close-modal" @click="closeSshModal"><i class="fa-solid fa-xmark"></i></button>
      </div>
      <div class="modal-body ssh-editor-container">
        <textarea v-model="sshConfigText" class="code-editor" spellcheck="false" placeholder="Host bien-guest\n  HostName 192.168..." @keydown.tab.prevent="handleEditorTab"></textarea>
      </div>
      <div class="modal-footer">
        <div style="display: flex; gap: 8px; margin-right: auto;">
          <button class="btn-tech btn-tech-secondary" @click="undo" title="Hoàn tác (Undo)" :disabled="!hasSshUndo">
            <i class="fa-solid fa-rotate-left"></i> UNDO
          </button>
          <button class="btn-tech btn-tech-secondary" @click="redo" title="Làm lại (Redo)" :disabled="!hasSshRedo">
            <i class="fa-solid fa-rotate-right"></i> REDO
          </button>
        </div>
        <div style="display: flex; gap: 8px;">
          <button class="btn-tech btn-tech-secondary" @click="closeSshModal">HỦY</button>
          <button class="btn-tech btn-tech-primary" @click="save"><i class="fa-solid fa-floppy-disk"></i> LƯU CẤU HÌNH</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { onMounted, onUnmounted } from 'vue';
import { useSsh } from '../../composables/useSsh';
import { useProjects } from '../../composables/useProjects';

const { 
  showSshModal, sshConfigText, hasSshUndo, hasSshRedo, 
  closeSshModal, handleEditorTab, saveSshConfig, undoSshConfig, redoSshConfig 
} = useSsh();

const { saveProjectsList } = useProjects();

function save() {
  saveSshConfig(saveProjectsList);
}

function undo() {
  undoSshConfig(saveProjectsList);
}

function redo() {
  redoSshConfig(saveProjectsList);
}

function handleEsc(e) {
  if (e.key === 'Escape' && showSshModal.value) {
    closeSshModal();
  }
}

onMounted(() => window.addEventListener('keydown', handleEsc, true));
onUnmounted(() => window.removeEventListener('keydown', handleEsc, true));
</script>
