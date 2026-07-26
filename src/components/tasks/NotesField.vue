<template>
  <div class="project-notes-section">
    <div class="notes-header">
      <span class="notes-title"><i class="fa-regular fa-note-sticky mr-1"></i> {{ label }}</span>
    </div>
    <textarea
      v-model="local"
      @change="handleChange"
      class="project-notes-textarea"
      :placeholder="placeholder"
      :maxlength="maxlength"
      :rows="rows"
    ></textarea>
  </div>
</template>

<script setup>
import { ref, watch } from 'vue'

const props = defineProps({
  modelValue: { type: String, default: '' },
  placeholder: { type: String, default: '' },
  maxlength: { type: Number, default: 1500 },
  rows: { type: Number, default: 2 },
  label: { type: String, default: 'Notes' },
})
const emit = defineEmits(['update:modelValue'])

// Local copy so every keystroke feels instant without persisting on every keystroke — only
// @change (blur / explicit commit) emits, matching the original handleNotesChange semantics
// exactly (trim, then persist).
const local = ref(props.modelValue || '')
watch(
  () => props.modelValue,
  (v) => {
    local.value = v || ''
  }
)

function handleChange() {
  local.value = (local.value || '').trim()
  emit('update:modelValue', local.value)
}
</script>

<style scoped>
.project-notes-section {
  background: rgba(255, 255, 255, 0.015);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  padding: 8px 12px;
}

.notes-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 6px;
}

.notes-title {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.project-notes-textarea {
  background: transparent;
  border: none;
  color: var(--text-light);
  font-size: 12px;
  outline: none;
  padding: 0;
  resize: none;
  overflow-y: hidden;
  font-family: inherit;
  width: 100%;
  line-height: 1.5;
  border-bottom: 1px solid transparent;
  field-sizing: content;
}

.project-notes-textarea:focus {
  border-bottom-color: rgba(255, 255, 255, 0.1);
}
</style>
