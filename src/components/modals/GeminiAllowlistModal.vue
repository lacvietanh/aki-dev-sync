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

      <div class="allowlist-section-label">Target Hosts</div>
      <div class="host-list">
        <label v-for="h in hostOptions" :key="h" class="host-chip" :class="{ active: selectedHosts.includes(h) }">
          <input type="checkbox" :value="h" v-model="selectedHosts" />
          {{ h === 'local' ? 'Local' : h }}
        </label>
        <div v-if="hostOptions.length === 1" class="hint no-remotes">No remote hosts configured yet - add a project with a remote host to seed there too.</div>
      </div>

      <div v-if="results.length" class="results-list allowlist-results">
        <div v-for="r in results" :key="r.host" class="result-row" :class="r.ok ? 'ok' : 'err'">
          <i class="fa-solid" :class="r.ok ? 'fa-check-circle' : 'fa-triangle-exclamation'"></i>
          <span class="result-host">{{ r.host }}</span>
          <span class="result-msg">{{ r.message }}</span>
        </div>
      </div>

      <div v-if="status.msg" class="status-msg allowlist-status" :class="status.err ? 'err' : 'ok'">
        <i class="fa-solid" :class="status.err ? 'fa-triangle-exclamation' : 'fa-check-circle'"></i>
        {{ status.msg }}
      </div>
    </div>

    <div class="modal-footer modal-footer-form">
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
  color: var(--slate-400);
  margin: 0 0 12px;
}
.allowlist-blurb code { color: var(--slate-300); }

.allowlist-section-label {
  font-size: 10px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--slate-500);
  margin-bottom: 6px;
}

.allowlist-results { margin-top: 10px; }

.allowlist-status {
  margin-top: 10px;
}

</style>
