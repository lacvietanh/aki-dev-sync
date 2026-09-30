<template>
  <BaseModal :show="show" @close="$emit('close')" container-style="width: 360px; max-width: calc(100vw - 32px);">
    <template #title>
      <i class="fa-solid fa-sliders"></i> Claude Code Profile
      <span class="scope-tag" title="This always edits ~/.claude/settings.json on this machine - there is no remote-host target">
        <i class="fa-solid fa-laptop-code"></i> Local
      </span>
    </template>

    <div class="modal-body profile-body">
            <div class="proxy-fields">
              <input v-model="cfg.endpoint" class="field-input" type="url" placeholder="Endpoint URL" title="env.ANTHROPIC_BASE_URL - proxy API base URL" spellcheck="false" />
              <div class="key-row">
                <input v-model="cfg.apiKey" class="field-input key-input" :type="showKey ? 'text' : 'password'" placeholder="API Key" title="env.ANTHROPIC_AUTH_TOKEN - proxy API key" spellcheck="false" />
                <button class="btn-eye" @click="showKey = !showKey" :title="showKey ? 'Hide key' : 'Show key'">
                  <i class="fa-regular" :class="showKey ? 'fa-eye' : 'fa-eye-slash'"></i>
                </button>
              </div>
              <input v-model="cfg.modelOpus" class="field-input" type="text" :placeholder="DEFAULTS.opus" title="env.ANTHROPIC_DEFAULT_OPUS_MODEL - leave blank to use default" spellcheck="false" />
              <input v-model="cfg.modelSonnet" class="field-input" type="text" :placeholder="DEFAULTS.sonnet" title="env.ANTHROPIC_DEFAULT_SONNET_MODEL - leave blank to use default" spellcheck="false" />
              <input v-model="cfg.modelHaiku" class="field-input" type="text" :placeholder="DEFAULTS.haiku" title="env.ANTHROPIC_DEFAULT_HAIKU_MODEL - leave blank to use default" spellcheck="false" />
            </div>

            <div v-if="status.msg" class="status-msg u-select-text" :class="status.err ? 'err' : 'ok'">
              <i class="fa-solid" :class="status.err ? 'fa-triangle-exclamation' : 'fa-check-circle'"></i>
              {{ status.msg }}
            </div>
          </div>

          <div class="modal-footer modal-footer-form">
            <button
                    v-if="currentMode === 'native'"
                    class="btn-modal-action btn-proxy"
                    @click="applyMode('proxy')"
                    :disabled="busy"
                    title="Write proxy config into ~/.claude/settings.json. Restart Claude Code to apply.">
              <i class="fa-solid" :class="busy ? 'fa-circle-notch fa-spin' : 'fa-network-wired'"></i>
              {{ busy ? 'Patching…' : 'Patch Proxy' }}
            </button>
            <button
                    v-else-if="currentMode === 'proxy'"
                    class="btn-modal-action btn-native"
                    @click="applyMode('native')"
                    :disabled="busy"
                    title="Remove all proxy keys from ~/.claude/settings.json">
              <i class="fa-solid" :class="busy ? 'fa-circle-notch fa-spin' : 'fa-house-signal'"></i>
              {{ busy ? 'Restoring…' : 'Back to Native' }}
            </button>
    </div>
  </BaseModal>
</template>

<script setup>
import { ref, reactive, watch } from 'vue';
import { invoke } from '../../utils/tauri';
import { claudeMode as currentMode, refreshClaudeMode } from '../../store/claudeModeStore';
import BaseModal from './BaseModal.vue';

const props = defineProps({ show: { type: Boolean, default: false } });
defineEmits(['close']);

const STORAGE_KEY = 'aki-claude-proxy-cfg';
const DEFAULTS = { opus: 'opus', sonnet: 'sonnet', haiku: 'haiku' };

function loadCfg() {
  try { return { endpoint: '', apiKey: '', modelOpus: '', modelSonnet: '', modelHaiku: '', ...JSON.parse(localStorage.getItem(STORAGE_KEY) || '{}') }; }
  catch { return { endpoint: '', apiKey: '', modelOpus: '', modelSonnet: '', modelHaiku: '' }; }
}

const cfg = reactive(loadCfg());
const showKey = ref(false);
const busy = ref(false);
const status = reactive({ msg: '', err: false });

watch(cfg, () => localStorage.setItem(STORAGE_KEY, JSON.stringify({ ...cfg })));

watch(() => props.show, async (val) => {
  if (!val) return;
  status.msg = '';
  Object.assign(cfg, loadCfg());
  await refreshClaudeMode();
});

async function applyMode(mode) {
  busy.value = true;
  status.msg = '';
  try {
    await invoke('set_claude_profile', mode === 'proxy' ? {
      mode: 'proxy',
      endpoint: cfg.endpoint || null,
      apiKey: cfg.apiKey || null,
      modelOpus: cfg.modelOpus || DEFAULTS.opus,
      modelSonnet: cfg.modelSonnet || DEFAULTS.sonnet,
      modelHaiku: cfg.modelHaiku || DEFAULTS.haiku,
    } : {
      mode: 'native', endpoint: null, apiKey: null,
      modelOpus: null, modelSonnet: null, modelHaiku: null,
    });
    // Re-read ~/.claude/settings.json directly to prevent polling native quota for a proxied CLI (gates usageMonitorRegistry).
    await refreshClaudeMode();
    status.msg = mode === 'proxy'
      ? 'Proxy applied. Restart Claude Code to take effect.'
      : 'Restored to native. Restart Claude Code to take effect.';
    status.err = false;
  } catch (e) {
    status.msg = String(e);
    status.err = true;
  } finally {
    busy.value = false;
  }
}
</script>

<style scoped>
.profile-body {
  padding: 14px 16px 10px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.proxy-fields {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.field-input {
  width: 100%;
  background: var(--bg-tertiary);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  padding: 7px 10px;
  font-size: 11px;
  color: var(--slate-200);
  font-family: 'JetBrains Mono', 'Fira Code', ui-monospace, monospace;
  outline: none;
  transition: border-color 0.15s;
  box-sizing: border-box;
}

.field-input:focus {
  border-color: rgba(217, 119, 87, 0.5);
}

.field-input::placeholder {
  color: var(--gray-700);
}

.key-row {
  display: flex;
  gap: 4px;
  align-items: center;
}

.key-input {
  flex: 1;
  min-width: 0;
}

.btn-eye {
  background: transparent;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  color: var(--slate-500);
  cursor: pointer;
  padding: 6px 8px;
  font-size: 11px;
  transition: color 0.15s, background 0.15s;
}

.btn-eye:hover {
  color: var(--slate-400);
  background: var(--surface-hover);
}

.btn-native,
.btn-proxy {
  flex: 1;
}

.btn-native {
  background: var(--surface-faint);
  border-color: var(--border-color);
  color: var(--slate-500);
}

.btn-native:hover:not(:disabled) {
  background: var(--surface-hover);
  color: var(--slate-400);
}

.btn-proxy {
  background: var(--brand-wash);
  border-color: var(--brand-line);
  color: var(--brand);
}

.btn-proxy:hover:not(:disabled) {
  background: var(--brand-edge);
  color: var(--brand-light);
}

/* Narrow mode (SSoT 700px, main.css): scoped padding outranks global narrow rule so trim is repeated here. */
@media (max-width: 700px) {
  .modal-body   { padding: 10px 10px 8px; }
  .modal-footer { padding: 8px 10px 10px; }
}
</style>
