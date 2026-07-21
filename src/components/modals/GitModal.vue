<template>
  <BaseModal :show="showGitModal && !!gitProject" @close="closeGitModal">
    <template #title>
      <div style="display: flex; align-items: center; gap: 8px;">
        <img
          v-if="gitProject && !failedIcons[gitProject.id]"
          :src="`aki-devsync-icon://${gitProject.id}?t=${iconTimestamp}`"
          style="width: 18px; height: 18px; border-radius: 3px; object-fit: contain;"
          alt=""
          @error="failedIcons[gitProject.id] = true"
        />
        <i v-else class="fa-solid fa-code-branch" style="font-size: 18px;"></i>
        <span>Git Version Control: {{ gitProject?.name }}</span>
      </div>
    </template>
    <div class="modal-body scrollable">
      <div v-if="gitProject && projectRuntime[gitProject.id]?.remote_url" class="form-group full-width mb-2">
        <label>Remote Git URL</label>
        <a @click.prevent="openUrl(projectRuntime[gitProject.id].remote_url)" class="git-url-link">
          <i class="fa-brands fa-git-alt mr-1"></i>{{ projectRuntime[gitProject.id].remote_url }}
        </a>
      </div>
      
      <!-- Git Actions Panel -->
      <div class="git-actions-panel mb-3">
        <div class="form-group full-width">
          <label>Commit Message</label>
          <div class="commit-input-wrapper">
            <input
              type="text"
              v-model="commitMessage"
              class="large-input"
              placeholder="Enter commit message (stages all changes)..."
              :disabled="isGitLoading"
              @keyup.enter="handleCommit"
            />
            <button
              class="btn-tech btn-tech-primary"
              :disabled="!commitMessage.trim() || isGitLoading"
              @click="handleCommit"
            >
              <i class="fa-solid fa-check"></i> COMMIT
            </button>
          </div>
        </div>
      </div>

      <div class="form-group full-width mt-3">
        <label class="git-status-header">
          <span><i class="fa-solid fa-clock-rotate-left mr-1"></i> Git Status History</span>
          <button class="btn-refresh-status" @click="handleStatus" :disabled="isGitLoading" title="Refresh Git Status">
            <i class="fa-solid fa-arrows-rotate" :class="{ 'fa-spin': isGitLoading }"></i>
          </button>
        </label>
        <pre class="git-status-log" v-html="formattedGitLog"></pre>
      </div>
    </div>
    <div class="modal-footer">
      <button class="btn-tech btn-tech-secondary" @click="closeGitModal" :disabled="isGitLoading">CLOSE</button>
      <button v-if="projectChangelogText" class="btn-tech btn-tech-secondary" @click="showChangelog = true" :disabled="isGitLoading">
        <i class="fa-solid fa-scroll"></i> CHANGELOG
      </button>
      <button class="btn-tech btn-tech-secondary" @click="handleFetch" :disabled="isGitLoading">
        <i class="fa-solid fa-download"></i> FETCH
      </button>
      <button class="btn-tech btn-tech-secondary" @click="handlePull" :disabled="isGitLoading">
        <i class="fa-solid fa-cloud-arrow-down"></i> PULL
      </button>
      <button class="btn-tech btn-tech-secondary" @click="handlePush" :disabled="isGitLoading">
        <i class="fa-solid fa-upload"></i> PUSH
      </button>
    </div>
    <!-- Project Changelog Preview -->
    <ChangelogModal
      :show="showChangelog"
      :title="'Changelog: ' + gitProject?.name"
      :content="projectChangelogText"
      :projectId="gitProject?.id"
      @close="showChangelog = false"
    />
  </BaseModal>
</template>

<script setup>
import { ref, computed, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { iconTimestamp } from '../../store/projectStore'
import BaseModal from './BaseModal.vue'
import ChangelogModal from './ChangelogModal.vue'
import { useProjects } from '../../composables/useProjects'

const {
  showGitModal,
  gitProject,
  gitStatusText,
  projectRuntime,
  isGitLoading,
  closeGitModal,
  fetchGitStatus,
  runGitFetch,
  runGitPush,
  runGitPull,
  runGitCommit,
  projectChangelogText
} = useProjects()

const commitMessage = ref('')
const showChangelog = ref(false)
const failedIcons = ref({})

// Reset changelog preview state when active project changes to prevent it from carrying over
watch(gitProject, () => {
  showChangelog.value = false
})

const formattedGitLog = computed(() => {
  if (!gitStatusText.value) return ''
  // Escape HTML tags to prevent XSS injection
  let html = gitStatusText.value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')

  // Map ANSI color codes to styled HTML spans. Covers what `git status`/`log` emit plus the
  // extra codes `fetch`/`push`/`pull` use for progress and ref updates (magenta, blue, etc).
  html = html
    .replace(/\u001b\[31m/g, '<span style="color: var(--accent-red, #ef4444);">')
    .replace(/\u001b\[32m/g, '<span style="color: var(--accent-green, #10b981);">')
    .replace(/\u001b\[33m/g, '<span style="color: #f59e0b;">')
    .replace(/\u001b\[34m/g, '<span style="color: #3b82f6;">')
    .replace(/\u001b\[35m/g, '<span style="color: #d946ef;">')
    .replace(/\u001b\[36m/g, '<span style="color: #06b6d4;">')
    .replace(/\u001b\[1m/g, '<span style="font-weight: bold;">')
    .replace(/\u001b\[(?:0)?m/g, '</span>')
    // Any other/unrecognized SGR or cursor-control sequence (e.g. \x1b[K clear-line during
    // fetch/push progress) - drop silently rather than leaving raw escape bytes on screen.
    .replace(/\u001b\[[0-9;]*[a-zA-Z]/g, '')

  return html
})

async function openUrl(url) {
  try { await invoke('macos_open', { args: [url] }) } catch (e) { console.error(e) }
}

async function handleStatus() {
  if (!gitProject.value) return
  await fetchGitStatus(gitProject.value.id)
}

async function handleFetch() {
  if (!gitProject.value) return
  try {
    await runGitFetch(gitProject.value)
  } catch (e) {
    console.error(e)
  }
}

async function handlePush() {
  if (!gitProject.value) return
  try {
    await runGitPush(gitProject.value)
  } catch (e) {
    console.error(e)
  }
}

async function handlePull() {
  if (!gitProject.value) return
  try {
    await runGitPull(gitProject.value)
  } catch (e) {
    console.error(e)
  }
}

async function handleCommit() {
  if (!gitProject.value || !commitMessage.value.trim()) return
  try {
    await runGitCommit(gitProject.value, commitMessage.value)
    commitMessage.value = '' // Clear input on success
  } catch (e) {
    console.error(e)
  }
}
</script>

<style scoped>
.large-input {
  width: 100%;
  padding: 8px 12px;
  background: rgba(5, 7, 12, 0.6);
  border: 1px solid rgba(255, 255, 255, 0.1);
  color: var(--text-light, #f3f4f6);
  border-radius: 4px;
  outline: none;
}
.large-input:focus {
  border-color: var(--accent-cyan, #00d2ff);
  background: rgba(5, 7, 12, 0.8);
}

.commit-input-wrapper {
  display: flex;
  gap: 8px;
  width: 100%;
}
.commit-input-wrapper .large-input {
  flex: 1;
}

.git-status-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  width: 100%;
}
.btn-refresh-status {
  background: transparent;
  border: none;
  color: var(--accent-cyan, #00d2ff);
  cursor: pointer;
  padding: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: transform 0.2s;
}
.btn-refresh-status:hover {
  transform: scale(1.1);
}
.btn-refresh-status:disabled {
  opacity: 0.5;
  cursor: not-allowed;
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
