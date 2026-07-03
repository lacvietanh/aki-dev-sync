<template>
  <BaseModal :show="show" @close="$emit('close')" container-class="update-modal">
    <template #title>
      <div style="display: flex; align-items: center; gap: 8px;">
        <i class="fa-solid fa-circle-arrow-up" style="font-size: 16px; color: var(--accent-cyan);"></i>
        <span>Update Available — v{{ version }}</span>
      </div>
    </template>
    <div class="modal-body update-body">
      <div class="update-body-content" v-html="renderedNotes" />
    </div>
    <div class="update-footer">
      <button class="btn-tech btn-tech-secondary" @click="$emit('close')">Later</button>
      <button class="btn-tech btn-tech-primary" @click="download" :disabled="!downloadUrl">
        <i class="fa-solid fa-download"></i> Download DMG
      </button>
    </div>
  </BaseModal>
</template>

<script setup>
import { computed } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import BaseModal from './BaseModal.vue';
import { renderMarkdown } from '../../utils/markdown';

const props = defineProps({
  show: Boolean,
  version: String,
  notes: String,
  downloadUrl: String,
});
defineEmits(['close']);

const renderedNotes = computed(() => renderMarkdown(props.notes || '_No release notes provided._'));

function download() {
  if (!props.downloadUrl) return;
  invoke('macos_open', { args: [props.downloadUrl] }).catch(console.error);
}
</script>

<style scoped>
.update-body {
  padding: 20px 24px;
  overflow-y: auto;
  max-height: calc(70vh - 110px);
}
.update-body-content {
  font-size: 13px;
  line-height: 1.6;
  color: #e2e8f0;
}
.update-body-content :deep(h3) {
  font-size: 14px;
  font-weight: 800;
  color: var(--accent-cyan);
  margin: 16px 0 8px;
  padding-bottom: 4px;
  border-bottom: 1px solid rgba(0, 210, 255, 0.15);
}
.update-body-content :deep(h4) {
  font-size: 12px;
  font-weight: 700;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.5px;
  margin: 12px 0 6px;
}
.update-body-content :deep(ul) {
  padding-left: 18px;
  margin: 0 0 8px;
}
.update-body-content :deep(li) {
  margin-bottom: 5px;
  color: #d1d5db;
}
.update-body-content :deep(strong) {
  color: #f3f4f6;
  font-weight: 700;
}
.update-body-content :deep(code) {
  background: rgba(255, 255, 255, 0.08);
  border-radius: 3px;
  padding: 1px 4px;
  font-family: monospace;
  font-size: 11px;
  color: #a5f3fc;
}
.update-body-content :deep(a) {
  color: var(--accent-blue);
  text-decoration: none;
}
.update-body-content :deep(a:hover) {
  text-decoration: underline;
}
.update-footer {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
  padding: 14px 24px;
  border-top: 1px solid rgba(255, 255, 255, 0.07);
}
</style>
