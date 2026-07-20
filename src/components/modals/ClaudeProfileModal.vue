<template>
  <BaseModal :show="show" @close="$emit('close')" container-style="width: 360px; max-width: calc(100vw - 32px);">
    <template #title>
      <i class="fa-solid fa-sliders"></i> Claude Code Profile
      <span class="scope-tag" title="This always edits ~/.claude/settings.json on this machine — there is no remote-host target">
        <i class="fa-solid fa-laptop-code"></i> Local
      </span>
    </template>

    <div class="modal-body">
            <div class="proxy-fields">
              <input v-model="cfg.endpoint" class="field-input" type="url" placeholder="Endpoint URL" title="env.ANTHROPIC_BASE_URL — proxy API base URL" spellcheck="false" />
              <div class="key-row">
                <input v-model="cfg.apiKey" class="field-input key-input" :type="showKey ? 'text' : 'password'" placeholder="API Key" title="env.ANTHROPIC_AUTH_TOKEN — proxy API key" spellcheck="false" />
                <button class="btn-eye" @click="showKey = !showKey" :title="showKey ? 'Hide key' : 'Show key'">
                  <i class="fa-regular" :class="showKey ? 'fa-eye' : 'fa-eye-slash'"></i>
                </button>
              </div>
              <input v-model="cfg.modelOpus" class="field-input" type="text" :placeholder="DEFAULTS.opus" title="env.ANTHROPIC_DEFAULT_OPUS_MODEL — leave blank to use default" spellcheck="false" />
              <input v-model="cfg.modelSonnet" class="field-input" type="text" :placeholder="DEFAULTS.sonnet" title="env.ANTHROPIC_DEFAULT_SONNET_MODEL — leave blank to use default" spellcheck="false" />
              <input v-model="cfg.modelHaiku" class="field-input" type="text" :placeholder="DEFAULTS.haiku" title="env.ANTHROPIC_DEFAULT_HAIKU_MODEL — leave blank to use default" spellcheck="false" />
            </div>

            <div v-if="status.msg" class="status-msg" :class="status.err ? 'err' : 'ok'">
              <i class="fa-solid" :class="status.err ? 'fa-triangle-exclamation' : 'fa-check-circle'"></i>
              {{ status.msg }}
            </div>
          </div>

          <div class="modal-footer">
            <button
                    v-if="currentMode === 'native'"
                    class="btn-proxy"
                    @click="applyMode('proxy')"
                    :disabled="busy"
                    title="Write proxy config into ~/.claude/settings.json. Restart Claude Code to apply.">
              <i class="fa-solid" :class="busy ? 'fa-circle-notch fa-spin' : 'fa-network-wired'"></i>
              {{ busy ? 'Patching…' : 'Patch Proxy' }}
            </button>
            <button
                    v-else-if="currentMode === 'proxy'"
                    class="btn-native"
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
import { invoke } from '@tauri-apps/api/core';
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
    currentMode.value = mode;
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
.modal-backdrop {
  position: fixed;
  inset: 0;
  z-index: 9000;
  background: rgba(0, 0, 0, 0.55);
  display: flex;
  align-items: center;
  justify-content: center;
}

.scope-tag {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 9px;
  font-weight: 700;
  color: #94a3b8;
  background: rgba(255, 255, 255, 0.06);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 4px;
  padding: 2px 6px;
  letter-spacing: 0.3px;
  margin-left: 2px;
}

.scope-tag i {
  color: #94a3b8;
  font-size: 9px;
}

.modal-body {
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
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 6px;
  padding: 7px 10px;
  font-size: 11px;
  color: #e2e8f0;
  font-family: 'JetBrains Mono', 'Fira Code', ui-monospace, monospace;
  outline: none;
  transition: border-color 0.15s;
  box-sizing: border-box;
}

.field-input:focus {
  border-color: rgba(217, 119, 87, 0.5);
}

.field-input::placeholder {
  color: #374151;
}

.key-row {
  display: flex;
  gap: 4px;
  align-items: center;
}

.key-input {
  flex: 1;
}

.btn-eye {
  background: transparent;
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 6px;
  color: #64748b;
  cursor: pointer;
  padding: 6px 8px;
  font-size: 11px;
  transition: color 0.15s, background 0.15s;
}

.btn-eye:hover {
  color: #94a3b8;
  background: rgba(255, 255, 255, 0.07);
}

.status-msg {
  font-size: 11px;
  padding: 7px 10px;
  border-radius: 6px;
  display: flex;
  align-items: flex-start;
  gap: 7px;
  line-height: 1.4;
}

.status-msg i {
  margin-top: 1px;
  flex-shrink: 0;
}

.status-msg.ok {
  background: rgba(16, 185, 129, 0.1);
  color: #34d399;
  border: 1px solid rgba(16, 185, 129, 0.2);
}

.status-msg.err {
  background: rgba(239, 68, 68, 0.1);
  color: #f87171;
  border: 1px solid rgba(239, 68, 68, 0.2);
}

.modal-footer {
  display: flex;
  gap: 8px;
  padding: 10px 16px 14px;
  border-top: 1px solid rgba(255, 255, 255, 0.07);
}

.btn-native,
.btn-proxy {
  flex: 1;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 7px 12px;
  border-radius: 6px;
  font-size: 11px;
  font-weight: 600;
  cursor: pointer;
  border: 1px solid transparent;
  transition: all 0.15s;
}

.btn-native:disabled,
.btn-proxy:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn-native {
  background: rgba(255, 255, 255, 0.04);
  border-color: rgba(255, 255, 255, 0.1);
  color: #64748b;
}

.btn-native:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.08);
  color: #94a3b8;
}

.btn-proxy {
  background: rgba(217, 119, 87, 0.15);
  border-color: rgba(217, 119, 87, 0.45);
  color: #d97757;
}

.btn-proxy:hover:not(:disabled) {
  background: rgba(217, 119, 87, 0.25);
  color: #fba97a;
}
</style>
