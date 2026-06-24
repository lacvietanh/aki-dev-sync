<template>
  <BaseModal :show="showGitModal && !!gitProject" @close="closeGitModal">
    <template #title>
      <i class="fa-solid fa-code-branch mr-1"></i> Git Version Control: {{ gitProject?.name }}
    </template>
    <div class="modal-body scrollable">
      <div class="alert-box info mb-2">
        <i class="fa-solid fa-circle-info"></i> Git integration is coming in the next update. This will allow you to view diffs, write commit messages, and push directly to GitHub/GitLab.
      </div>
      <div v-if="gitProject && projectRuntime[gitProject.id]?.remote_url" class="form-group full-width mb-2">
        <label>Remote Git URL</label>
        <a @click.prevent="openUrl(projectRuntime[gitProject.id].remote_url)" class="git-url-link">
          <i class="fa-brands fa-git-alt mr-1"></i>{{ projectRuntime[gitProject.id].remote_url }}
        </a>
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
import { invoke } from '@tauri-apps/api/core'
import BaseModal from './BaseModal.vue'
import { useProjects } from '../../composables/useProjects'

const { showGitModal, gitProject, gitStatusText, projectRuntime, closeGitModal } = useProjects()

async function openUrl(url) {
  try { await invoke('macos_open', { args: [url] }) } catch (e) { console.error(e) }
}
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
  color: var(--text-muted);
  border-radius: 4px;
}

.git-url-link {
  display: inline-flex;
  align-items: center;
  font-family: monospace;
  font-size: 12px;
  color: var(--accent-cyan, #00d2ff);
  cursor: pointer;
  word-break: break-all;
  text-decoration: none;
  opacity: 0.85;
  transition: opacity 0.2s;
}
.git-url-link:hover { opacity: 1; text-decoration: underline; }
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
