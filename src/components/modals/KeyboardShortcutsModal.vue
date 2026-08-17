<template>
  <BaseModal :show="show" @close="$emit('close')" container-style="width: 360px; max-width: calc(100vw - 32px);">
    <template #title>
      <i class="fa-solid fa-keyboard"></i> Keyboard Shortcuts
    </template>

    <div class="modal-body">
      <div class="shortcut-row" v-for="s in SHORTCUTS" :key="s.key">
        <span class="shortcut-key">{{ s.key }}</span>
        <span class="shortcut-desc">{{ s.desc }}</span>
      </div>
    </div>
  </BaseModal>
</template>

<script setup>
import BaseModal from './BaseModal.vue';

defineProps({ show: { type: Boolean, default: false } });
defineEmits(['close']);

const SHORTCUTS = [
  { key: 'F1', desc: 'Narrow + Stick Top-Left' },
  { key: 'F2', desc: 'Width 1400px (Ultra)' },
  { key: 'F3', desc: 'Center Primary' },
];
</script>

<style scoped>
.modal-body {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.shortcut-row {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 12px;
}

.shortcut-key {
  flex-shrink: 0;
  min-width: 28px;
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
