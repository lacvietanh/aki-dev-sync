<template>
  <BaseModal :show="show" @close="$emit('close')" container-class="changelog-modal">
    <template #title><i class="fa-solid fa-scroll mr-1"></i> Changelog</template>
    <div class="modal-body changelog-body" v-html="renderedChangelog" ref="bodyRef" />
  </BaseModal>
</template>

<script setup>
import { computed, ref, watch } from 'vue';
import BaseModal from './BaseModal.vue';
import changelogText from '../../../CHANGELOG.md?raw';
import { renderMarkdown, runMermaid } from '../../utils/markdown';

defineProps({ show: Boolean });
defineEmits(['close']);

const bodyRef = ref(null);
const renderedChangelog = computed(() => renderMarkdown(changelogText));

watch(() => bodyRef.value, (el) => {
  if (el) runMermaid();
});
</script>

<style scoped>
.changelog-body {
  padding: 20px 24px;
  overflow-y: auto;
  max-height: calc(80vh - 60px);
  font-size: 13px;
  line-height: 1.6;
  color: #e2e8f0;
}

/* markdown headings */
.changelog-body :deep(h3) {
  font-size: 14px;
  font-weight: 800;
  color: var(--accent-cyan);
  margin: 20px 0 8px;
  padding-bottom: 4px;
  border-bottom: 1px solid rgba(0, 210, 255, 0.15);
}
.changelog-body :deep(h4) {
  font-size: 12px;
  font-weight: 700;
  color: #9CA3AF;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  margin: 14px 0 6px;
}
.changelog-body :deep(ul) {
  padding-left: 18px;
  margin: 0 0 8px;
}
.changelog-body :deep(li) {
  margin-bottom: 5px;
  color: #d1d5db;
}
.changelog-body :deep(strong) {
  color: #f3f4f6;
  font-weight: 700;
}
.changelog-body :deep(code) {
  background: rgba(255, 255, 255, 0.08);
  border-radius: 3px;
  padding: 1px 4px;
  font-family: monospace;
  font-size: 11px;
  color: #a5f3fc;
}
.changelog-body :deep(hr) {
  border: none;
  border-top: 1px solid rgba(255, 255, 255, 0.07);
  margin: 20px 0;
}
.changelog-body :deep(a) {
  color: var(--accent-blue);
  text-decoration: none;
}
.changelog-body :deep(a:hover) {
  text-decoration: underline;
}
</style>
