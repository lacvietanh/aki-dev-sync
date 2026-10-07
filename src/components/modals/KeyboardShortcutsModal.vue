<template>
  <BaseModal :show="show" @close="$emit('close')" container-style="width: 360px; max-width: calc(100vw - 32px);">
    <template #title>
      <i class="fa-solid fa-keyboard"></i> Keyboard Shortcuts
    </template>

    <div class="modal-body shortcuts-body">
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
      { key: 'F1', desc: 'Narrow window, docked top-left of the current monitor' },
      { key: 'F2', desc: 'Toggle ultra-wide (1400px) / narrow window' },
      { key: 'F3', desc: 'Centered on the primary monitor' },
      { key: 'F12', desc: 'Toggle pin (always on top, all spaces)' },
      { key: 'F1–F3, F12', desc: 'reach the app even while the terminal is focused' },
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
.shortcuts-body {
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
  color: var(--slate-500);
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
  color: var(--cyan-200);
  background: var(--surface-deep);
  border: 1px solid var(--accent-cyan-edge);
  border-radius: 4px;
}

.shortcut-desc {
  color: var(--slate-400);
}
</style>
