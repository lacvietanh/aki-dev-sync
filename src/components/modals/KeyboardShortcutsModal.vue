<template>
  <BaseModal :show="show" @close="$emit('close')" container-style="width: 360px; max-width: calc(100vw - 32px);">
    <template #title>
      <i class="fa-solid fa-keyboard"></i> Keyboard Shortcuts
    </template>

    <div class="modal-body">
      <div class="shortcut-group" v-for="g in GROUPS" :key="g.title">
        <div class="shortcut-group-title">{{ g.title }}</div>
        <div class="shortcut-row" v-for="s in g.items" :key="s.key">
          <span class="shortcut-key">{{ s.key }}</span>
          <span class="shortcut-desc">{{ s.desc }}</span>
        </div>
      </div>
    </div>
  </BaseModal>
</template>

<script setup>
import BaseModal from './BaseModal.vue';

defineProps({ show: { type: Boolean, default: false } });
defineEmits(['close']);

const GROUPS = [
  {
    title: 'Window',
    items: [
      { key: 'F1', desc: 'Narrow window, docked top-left' },
      { key: 'F2', desc: 'Ultra-wide window (1400px)' },
      { key: 'F3', desc: 'Centered on the primary monitor' },
    ],
  },
  {
    title: 'Terminal (when a terminal is focused)',
    items: [
      { key: '⌘T', desc: 'New terminal tab' },
      { key: '⌘W', desc: 'Close current tab' },
      { key: '⌘⇧[', desc: 'Previous tab' },
      { key: '⌘⇧]', desc: 'Next tab' },
      { key: '⌘+', desc: 'Zoom in terminal font' },
      { key: '⌘-', desc: 'Zoom out terminal font' },
      { key: '⌘0', desc: 'Reset terminal font size' },
    ],
  },
];
</script>

<style scoped>
.modal-body {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.shortcut-group {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.shortcut-group-title {
  font-size: 10px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: #64748b;
}

.shortcut-row {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 12px;
}

.shortcut-key {
  flex-shrink: 0;
  min-width: 40px;
  text-align: center;
  padding: 2px 6px;
  font-size: 10px;
  font-weight: 700;
  color: #a5f3fc;
  background: #0b1220;
  border: 1px solid rgba(0, 210, 255, 0.35);
  border-radius: 4px;
}

.shortcut-desc {
  color: #94a3b8;
}
</style>
