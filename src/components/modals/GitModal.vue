<template>
  <BaseModal :show="showGitModal && !!gitProject" @close="closeGitModal">
    <template #title>
      <i class="fa-solid fa-code-branch mr-1"></i> Git Version Control: {{ gitProject?.name }}
    </template>
    <div class="modal-body scrollable">
      <div class="alert-box info mb-2">
        <i class="fa-solid fa-circle-info"></i> Git integration is coming in the next update. This will allow you to view diffs, write commit messages, and push directly to GitHub/GitLab.
      </div>
      <div class="form-group full-width">
        <label>Commit Message</label>
        <input type="text" class="large-input" placeholder="WIP: Implementation pending..." disabled />
      </div>
      <div class="form-group full-width mt-3">
        <label><i class="fa-solid fa-clock-rotate-left mr-1"></i> Git Status History</label>
        <pre class="git-status-log">{{ gitStatusText }}</pre>
      </div>
    </div>
    <div class="modal-footer">
      <button class="btn-tech btn-tech-secondary" @click="closeGitModal">CLOSE</button>
      <button class="btn-tech btn-tech-primary" disabled><i class="fa-solid fa-cloud-arrow-up"></i> COMMIT & PUSH</button>
    </div>
  </BaseModal>
</template>

<script setup>
import BaseModal from './BaseModal.vue'
import { useProjects } from '../../composables/useProjects'

const { showGitModal, gitProject, gitStatusText, closeGitModal } = useProjects()
</script>

<style scoped>
.alert-box.info {
  background: rgba(0, 210, 255, 0.1);
  border: 1px solid rgba(0, 210, 255, 0.3);
  color: #a5f3fc;
  padding: 12px;
  border-radius: 4px;
  font-size: 13px;
  display: flex;
  align-items: center;
  gap: 8px;
}
.large-input {
  width: 100%;
  padding: 8px 12px;
  background: rgba(5, 7, 12, 0.6);
  border: 1px solid rgba(255, 255, 255, 0.1);
  color: #9CA3AF;
  border-radius: 4px;
}
.mt-3 {
  margin-top: 1rem;
}
.mr-1 {
  margin-right: 0.25rem;
}
.git-status-log {
  background: #0d1117;
  color: #e6edf3;
  padding: 12px;
  border-radius: 6px;
  border: 1px solid #30363d;
  font-family: 'Courier New', Courier, monospace;
  font-size: 13px;
  white-space: pre-wrap;
  word-wrap: break-word;
  max-height: 250px;
  overflow-y: auto;
}
</style>
