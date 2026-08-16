<template>
  <BaseModal :show="show" @close="$emit('close')" container-style="width: 440px; max-width: calc(100vw - 32px);">
    <template #title>
      <i class="fa-solid fa-shield-halved"></i> Pre-allow AGY Commands
    </template>

    <div class="modal-body">
      <p class="allowlist-blurb">
        Merges a checked-in, recommended set of dev commands (<code>git status</code>, <code>curl</code>,
        <code>jq</code>, <code>agy</code> itself, ...) into <code>permissions.allow</code> on the selected
        host(s)' <code>~/.gemini/antigravity-cli/settings.json</code>, so a new machine or a new agy account
        stops getting a permission prompt for every routine command. A union merge - nothing else in that
        file is touched, and re-running it is harmless.
      </p>

      <div class="section-label">Target Hosts</div>
      <div class="host-list">
        <label v-for="h in hostOptions" :key="h" class="host-chip" :class="{ active: selectedHosts.includes(h) }">
          <input type="checkbox" :value="h" v-model="selectedHosts" />
          {{ h === 'local' ? 'Local' : h }}
        </label>
        <div v-if="hostOptions.length === 1" class="hint no-remotes">No remote hosts configured yet - add a project with a remote host to seed there too.</div>
      </div>

      <div v-if="results.length" class="results-list">
        <div v-for="r in results" :key="r.host" class="result-row" :class="r.ok ? 'ok' : 'err'">
          <i class="fa-solid" :class="r.ok ? 'fa-check-circle' : 'fa-triangle-exclamation'"></i>
          <span class="result-host">{{ r.host }}</span>
          <span class="result-msg">{{ r.message }}</span>
        </div>
      </div>

      <div v-if="status.msg" class="status-msg" :class="status.err ? 'err' : 'ok'">
        <i class="fa-solid" :class="status.err ? 'fa-triangle-exclamation' : 'fa-check-circle'"></i>
        {{ status.msg }}
      </div>
    </div>

    <div class="modal-footer">
      <button class="btn-modal-action btn-apply" @click="seedAllowlist" :disabled="busy || selectedHosts.length === 0" title="Merge the recommended allowlist into settings.json on the selected host(s). Only permissions.allow is touched.">
        <i class="fa-solid" :class="busy ? 'fa-circle-notch fa-spin' : 'fa-paper-plane'"></i>
        {{ busy ? 'Seeding…' : `Seed allowlist to ${selectedHosts.length} host${selectedHosts.length === 1 ? '' : 's'}` }}
      </button>
    </div>
  </BaseModal>
</template>

<script setup>
import { ref, reactive, watch, computed } from 'vue';
import { invoke } from '../../utils/tauri';
import { projects } from '../../store/projectStore';
import BaseModal from './BaseModal.vue';

const props = defineProps({ show: { type: Boolean, default: false } });
defineEmits(['close']);

const busy = ref(false);
const status = reactive({ msg: '', err: false });
const results = ref([]);
const selectedHosts = ref(['local']);

const hostOptions = computed(() => {
  const remotes = new Set();
  for (const p of projects.value) {
    if (p.remote_host && p.remote_host !== 'local' && p.remote_host !== 'localhost') remotes.add(p.remote_host);
  }
  return ['local', ...remotes];
});

watch(() => props.show, (val) => {
  if (!val) return;
  status.msg = '';
  results.value = [];
});

async function seedAllowlist() {
  busy.value = true;
  status.msg = '';
  results.value = [];
  try {
    const hostResults = await invoke('apply_gemini_allowlist', { targetHosts: [...selectedHosts.value] });
    results.value = hostResults;
    const failed = hostResults.filter(r => !r.ok);
    status.err = failed.length > 0;
    status.msg = failed.length === 0
      ? `Allowlist seeded on ${hostResults.length} host${hostResults.length === 1 ? '' : 's'}. Restart agy to pick it up.`
      : `${hostResults.length - failed.length}/${hostResults.length} hosts seeded - see details above.`;
  } catch (e) {
    status.msg = String(e);
    status.err = true;
  } finally {
    busy.value = false;
  }
}
</script>

<style scoped>
.allowlist-blurb {
  font-size: 11px;
  line-height: 1.5;
  color: #94a3b8;
  margin: 0 0 12px;
}
.allowlist-blurb code { color: #cbd5e1; }

.section-label {
  font-size: 10px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: #64748b;
  margin-bottom: 6px;
}

.host-list { display: flex; flex-wrap: wrap; gap: 4px; }

.host-chip {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 10px;
  color: #94a3b8;
  background: rgba(255, 255, 255, 0.04);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 3px;
  padding: 2px 5px;
  cursor: pointer;
}

.host-chip.active { color: #fba97a; border-color: rgba(217, 119, 87, 0.4); background: rgba(217, 119, 87, 0.1); }

.no-remotes { width: 100%; font-size: 10px; }

.results-list { display: flex; flex-direction: column; gap: 4px; margin-top: 10px; }

.result-row {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 10px;
  padding: 5px 8px;
  border-radius: 5px;
}

.result-row.ok { background: rgba(16, 185, 129, 0.08); color: #34d399; }
.result-row.err { background: rgba(239, 68, 68, 0.08); color: #f87171; }
.result-host { font-weight: 700; flex-shrink: 0; }
.result-msg { color: #94a3b8; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

.status-msg {
  font-size: 11px;
  padding: 7px 10px;
  border-radius: 6px;
  display: flex;
  align-items: flex-start;
  gap: 7px;
  line-height: 1.4;
  margin-top: 10px;
}

.status-msg i { margin-top: 1px; flex-shrink: 0; }

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
  flex-shrink: 0;
}

.btn-apply {
  flex: 1;
  background: rgba(217, 119, 87, 0.15);
  border-color: rgba(217, 119, 87, 0.45);
  color: #d97757;
}

.btn-apply:hover:not(:disabled) { background: rgba(217, 119, 87, 0.25); color: #fba97a; }
</style>
