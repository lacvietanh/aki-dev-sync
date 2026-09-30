<template>
  <BaseModal :show="show" @close="$emit('close')" container-style="max-width: 440px">
    <template #title>
      <i class="fa-solid fa-wand-magic-sparkles mr-1"></i> AI Settings
    </template>
    <div class="modal-body">
      <p class="text-muted intro-note">
        Used by <b>Explain</b> in the sync popup, via the <code>agy</code> CLI. The model name carries its thinking level (High / Medium / Low).
      </p>

      <div class="ai-row">
        <div class="ai-title">Model</div>
        <select v-model="local.agy_model" class="ai-select">
          <option v-for="m in models" :key="m.id" :value="m.id">{{ m.label }}</option>
          <option v-if="isUnlisted" :value="local.agy_model">{{ local.agy_model }}</option>
        </select>
      </div>
      <div v-if="modelsError" class="ai-error">{{ modelsError }}</div>

      <div class="ai-row">
        <div class="ai-title">Explain prompt</div>
        <button class="btn-secondary" :disabled="promptText === DEFAULT_EXPLAIN_PROMPT" @click="promptText = DEFAULT_EXPLAIN_PROMPT">Reset to default</button>
      </div>
      <textarea v-model="promptText" class="ai-prompt u-select-text" spellcheck="false"></textarea>
      <div class="ai-hint">The sync data (counts, per-folder breakdown, conflict diffs) is appended after this text automatically.</div>
    </div>
    <div class="modal-footer">
      <div></div>
      <div>
        <button class="btn-secondary mr-1" @click="$emit('close')">Cancel</button>
        <button class="btn-save" @click="save"><i class="fa-solid fa-floppy-disk mr-1"></i> Save</button>
      </div>
    </div>
  </BaseModal>
</template>

<script setup>
import { computed, reactive, ref, watch } from 'vue'
import BaseModal from './BaseModal.vue'
import { invoke } from '../../utils/tauri'
import { aiSettings, setAiSettings, DEFAULT_EXPLAIN_PROMPT, effectiveExplainPrompt, effectiveAgyModel } from '../../store/aiSettingsStore'

const props = defineProps({ show: Boolean })
const emit = defineEmits(['close'])

const local = reactive({ ...aiSettings.value, agy_model: effectiveAgyModel() })
const promptText = ref(effectiveExplainPrompt())
const models = ref([])
const modelsError = ref('')

const isUnlisted = computed(() => local.agy_model && !models.value.some((m) => m.id === local.agy_model))

watch(() => props.show, async (v) => {
  if (!v) return
  Object.assign(local, aiSettings.value, { agy_model: effectiveAgyModel() })
  promptText.value = effectiveExplainPrompt()
  modelsError.value = ''
  try {
    models.value = await invoke('list_agy_models')
  } catch (e) {
    modelsError.value = String(e)
  }
})

function save() {
  setAiSettings({ ...local, explain_prompt: promptText.value.trim() === DEFAULT_EXPLAIN_PROMPT ? '' : promptText.value })
  emit('close')
}
</script>

<style scoped>
.ai-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 10px 0;
  font-size: 13px;
}
.ai-title { color: var(--slate-200); font-weight: 600; }
.ai-select {
  width: 250px;
  background: var(--surface-panel);
  border: 1px solid var(--gray-700);
  color: var(--slate-200);
  border-radius: 4px;
  padding: 4px 6px;
  font-size: 13px;
}
.ai-select:focus { outline: none; border-color: var(--blue-500); }
.ai-prompt {
  width: 100%;
  height: 220px;
  resize: vertical;
  background: var(--surface-panel);
  border: 1px solid var(--gray-700);
  color: var(--slate-200);
  border-radius: 4px;
  padding: 6px;
  font-size: 11px;
  font-family: monospace;
}
.ai-prompt:focus { outline: none; border-color: var(--blue-500); }
.ai-hint { color: var(--slate-500); font-size: 11px; margin-top: 4px; }
.ai-error { color: var(--accent-red); font-size: 11px; }
</style>
