<template>
  <BaseModal :show="show" @close="$emit('close')" container-style="width: 90vw; max-width: 1100px;">
    <template #title>
      <i class="fa-solid fa-terminal"></i> Statusline Customizer
    </template>

    <div class="modal-body">
            <div class="preview-box">
              <pre class="preview-line" v-html="previewHtml"></pre>
            </div>

            <div class="section-label">Fields <span class="hint">(toggle, reorder, recolor)</span></div>
            <div class="field-list">
              <div v-for="(field, idx) in cfg.fields" :key="field.key" class="field-row" :class="{ disabled: !field.enabled, 'field-row-sub': isSubField(field.key) }">
                <label class="field-toggle" :title="CATALOG[field.key]?.desc">
                  <input type="checkbox" v-model="field.enabled" />
                  <span class="field-name">{{ CATALOG[field.key]?.label || field.key }}</span>
                </label>
                <template v-if="!isSubField(field.key)">
                  <select v-if="isColorEditable(field.key)" v-model="field.color" class="color-select" title="Label color">
                    <option v-for="c in COLORS" :key="c.key" :value="c.key">{{ c.label }}</option>
                  </select>
                  <span v-else class="color-locked" title="This field's color carries meaning (identity / % / +/-) and isn't user-editable">locked</span>
                  <div class="reorder-btns">
                    <button class="btn-reorder" :disabled="idx === 0" @click="move(idx, -1)" title="Move up">
                      <i class="fa-solid fa-chevron-up"></i>
                    </button>
                    <button class="btn-reorder" :disabled="idx === cfg.fields.length - 1" @click="move(idx, 1)" title="Move down">
                      <i class="fa-solid fa-chevron-down"></i>
                    </button>
                  </div>
                </template>
              </div>
            </div>

            <div class="section-label">Color thresholds <span class="hint">(% at which each tier kicks in)</span></div>
            <div class="threshold-row">
              <label class="threshold-field" title="Yellow tier starts at this %"><span class="dot dot-yellow"></span>≥ <input type="number" min="0" max="100" v-model.number="cfg.thresholds.yellow" /></label>
              <label class="threshold-field" title="Orange tier starts at this %"><span class="dot dot-orange"></span>≥ <input type="number" min="0" max="100" v-model.number="cfg.thresholds.orange" /></label>
              <label class="threshold-field" title="Red tier starts at this %"><span class="dot dot-red"></span>≥ <input type="number" min="0" max="100" v-model.number="cfg.thresholds.red" /></label>
            </div>

            <div class="section-label">Apply to <span class="hint">(local + configured remote hosts)</span></div>
            <div class="host-list">
              <label
                v-for="h in hostOptions"
                :key="h"
                class="host-chip"
                :class="{ active: selectedHosts.includes(h), warn: hostStatus[h] && hostStatus[h].claude_installed && !hostStatus[h].statusline_configured }"
              >
                <input type="checkbox" :value="h" v-model="selectedHosts" />
                {{ h === 'local' ? 'Local (this machine)' : h }}
                <i
                  v-if="hostStatus[h] && hostStatus[h].claude_installed && !hostStatus[h].statusline_configured"
                  class="fa-solid fa-triangle-exclamation warn-icon"
                  title="Statusline not installed on this host yet — auto-installing…"
                ></i>
              </label>
              <div v-if="hostOptions.length === 1" class="hint no-remotes">No remote hosts configured yet — add a project with a remote host to push there too.</div>
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
            <button class="btn-reset" @click="resetToDefault" :disabled="busy" title="Reload the built-in default preset">
              <i class="fa-solid fa-arrow-rotate-left"></i> Reset
            </button>
            <button class="btn-apply" @click="apply" :disabled="busy || selectedHosts.length === 0" title="Write statusline-command.sh + patch settings.json on every checked host">
              <i class="fa-solid" :class="busy ? 'fa-circle-notch fa-spin' : 'fa-paper-plane'"></i>
              {{ busy ? 'Applying…' : `Apply to ${selectedHosts.length} host${selectedHosts.length === 1 ? '' : 's'}` }}
            </button>
    </div>
  </BaseModal>
</template>

<script setup>
import { ref, reactive, computed, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { projects } from '../../store/projectStore';
import BaseModal from './BaseModal.vue';

const props = defineProps({ show: { type: Boolean, default: false } });
defineEmits(['close']);

const STORAGE_KEY = 'aki-statusline-config';

// UI-only metadata for the field keys the Rust side understands (src-tauri/src/statusline.rs).
// Keep in sync with `default_config()` there when adding a new field key.
const CATALOG = {
  identity:    { label: 'Identity (user@host)', desc: 'user@host, fixed cyan/green — always meaningful' },
  cwd:         { label: 'Working directory',    desc: 'Current folder name (~ when at $HOME)' },
  model:       { label: 'Model + effort',       desc: 'Model name (e.g. "sonnet 5") plus effort level' },
  context:     { label: 'Context window',       desc: 'ctx NN% used/max, auto-colored by threshold' },
  rate_limits: { label: 'Rate limits (5h/7d)',  desc: 'Pro/Max usage windows, auto-colored by threshold' },
  rate_reset:  { label: 'Reset ETA',            desc: 'Appends time-until-reset to the rate limits field (e.g. ⟳1h12m) — not a standalone field' },
  cache_pct:   { label: 'Cache hit %',          desc: 'Percent of the most recent request served from cache (green = high hit rate)' },
  cache_tokens:{ label: 'Cache tokens (read/total)', desc: 'Tokens read from cache vs. total input for the most recent request' },
  session:     { label: 'Session (dur/lines/$)',desc: 'Duration, lines +/-, and cost for this session' },
  git_branch:  { label: 'Git branch',           desc: 'Experimental — depends on Claude Code version exposing it' },
};

const COLORS = [
  { key: 'white',   label: 'White',   hex: '#e2e8f0' },
  { key: 'cyan',    label: 'Cyan',    hex: '#22d3ee' },
  { key: 'green',   label: 'Green',   hex: '#34d399' },
  { key: 'blue',    label: 'Blue',    hex: '#60a5fa' },
  { key: 'grey',    label: 'Grey',    hex: '#94a3b8' },
  { key: 'red',     label: 'Red',     hex: '#f87171' },
  { key: 'yellow',  label: 'Yellow',  hex: '#fbbf24' },
  { key: 'magenta', label: 'Magenta', hex: '#e879f9' },
];
const HEX = Object.fromEntries(COLORS.map(c => [c.key, c.hex]));
const COLOR_EDITABLE = new Set(['cwd', 'model', 'session', 'git_branch', 'cache_tokens']);
function isColorEditable(key) { return COLOR_EDITABLE.has(key); }

// Fields that don't render on their own — they append onto another field's group instead
// (e.g. rate_reset attaches its ETA onto rate_limits). Rendered as an indented sub-row with
// no reorder/color controls of their own.
const SUB_FIELDS = new Set(['rate_reset']);
function isSubField(key) { return SUB_FIELDS.has(key); }

function defaultLocalConfig() {
  return {
    fields: [
      { key: 'identity',    enabled: true,  color: 'cyan' },
      { key: 'cwd',         enabled: true,  color: 'blue' },
      { key: 'model',       enabled: true,  color: 'cyan' },
      { key: 'context',     enabled: true,  color: 'white' },
      { key: 'rate_limits', enabled: true,  color: 'white' },
      { key: 'rate_reset',  enabled: false, color: 'grey' },
      { key: 'cache_pct',   enabled: false, color: 'white' },
      { key: 'cache_tokens',enabled: false, color: 'cyan' },
      { key: 'session',     enabled: true,  color: 'grey' },
      { key: 'git_branch',  enabled: false, color: 'magenta' },
    ],
    thresholds: { yellow: 50, orange: 70, red: 85 },
  };
}

function loadCfg() {
  let saved = null;
  try {
    const raw = JSON.parse(localStorage.getItem(STORAGE_KEY) || 'null');
    if (raw && Array.isArray(raw.fields) && raw.thresholds) saved = raw;
  } catch { /* fall through to default */ }
  const def = defaultLocalConfig();
  if (!saved) return def;
  // Field keys added in a later version aren't in an already-saved config — append them with
  // their default state instead of leaving the user stuck on the old catalog. Order and
  // enabled/color of fields the user already has are left untouched.
  const known = new Set(saved.fields.map(f => f.key));
  saved.fields.push(...def.fields.filter(f => !known.has(f.key)));
  return saved;
}

const cfg = reactive(loadCfg());
const busy = ref(false);
const status = reactive({ msg: '', err: false });
const results = ref([]);
const selectedHosts = ref(['local']);
const hostStatus = ref({});

watch(cfg, () => localStorage.setItem(STORAGE_KEY, JSON.stringify(cfg)), { deep: true });

const hostOptions = computed(() => {
  const remotes = new Set();
  for (const p of projects.value) {
    if (p.remote_host && p.remote_host !== 'local' && p.remote_host !== 'localhost') remotes.add(p.remote_host);
  }
  return ['local', ...remotes];
});

watch(() => props.show, async (val) => {
  if (!val) return;
  status.msg = '';
  results.value = [];
  Object.assign(cfg, loadCfg());
  if (localStorage.getItem(STORAGE_KEY) == null) {
    try {
      const remote = await invoke('get_default_statusline_config');
      Object.assign(cfg, remote);
    } catch { /* keep local default */ }
  }
  checkAndAutoInstall();
});

// Warns when a host has Claude Code but no statusline wired up yet, then auto-installs it
// there — hosts with no Claude Code at all are skipped since this app's Claude-only features
// don't apply to them.
async function checkAndAutoInstall() {
  try {
    const list = await invoke('check_statusline_status', { hosts: hostOptions.value });
    const map = {};
    for (const s of list) map[s.host] = s;
    hostStatus.value = map;

    const toInstall = list.filter(s => s.claude_installed && !s.statusline_configured).map(s => s.host);
    if (!toInstall.length) return;

    const config = JSON.parse(JSON.stringify(cfg));
    const autoResults = await invoke('apply_statusline_config', { config, targetHosts: toInstall });
    for (const r of autoResults) {
      if (hostStatus.value[r.host]) hostStatus.value[r.host].statusline_configured = r.ok;
    }
    const ok = autoResults.filter(r => r.ok).map(r => r.host);
    if (ok.length) {
      status.err = false;
      status.msg = `Auto-installed statusline on: ${ok.join(', ')}. Restart Claude Code (or open a new terminal) to see it.`;
    }
  } catch (e) {
    console.error('Statusline status check failed:', e);
  }
}

function move(idx, dir) {
  const to = idx + dir;
  if (to < 0 || to >= cfg.fields.length) return;
  const arr = cfg.fields;
  [arr[idx], arr[to]] = [arr[to], arr[idx]];
}

function resetToDefault() {
  Object.assign(cfg, defaultLocalConfig());
}

async function apply() {
  busy.value = true;
  status.msg = '';
  results.value = [];
  try {
    const config = JSON.parse(JSON.stringify(cfg));
    const hostResults = await invoke('apply_statusline_config', { config, targetHosts: [...selectedHosts.value] });
    results.value = hostResults;
    const failed = hostResults.filter(r => !r.ok);
    status.err = failed.length > 0;
    status.msg = failed.length === 0
      ? `Applied to ${hostResults.length} host${hostResults.length === 1 ? '' : 's'}. Restart Claude Code (or open a new terminal) to see it.`
      : `${hostResults.length - failed.length}/${hostResults.length} hosts applied — see details above.`;
  } catch (e) {
    status.msg = String(e);
    status.err = true;
  } finally {
    busy.value = false;
  }
}

// ---- live preview: mirrors the coloring rules in src-tauri/src/statusline.rs, against fixed
// sample data, purely for on-screen feedback (never sent to the backend). ----
const SAMPLE = {
  user: 'guest', host: 'roscy', cwd: 'Aki-Dev-Sync', model: 'sonnet 5', effort: 'med',
  ctxPct: 72, ctxUsed: '134.4k', ctxMax: '1M',
  rate5h: 42, rate7d: 92, rate5hEta: '1h12m', rate7dEta: '2d3h',
  duration: '12m', linesAdded: 122, linesRemoved: 52, cost: '$1.23',
  gitBranch: 'master',
  cachePct: 78, cacheRead: '12.4k', cacheTotal: '45.2k',
};

function esc(s) { return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;'); }
function span(text, hex) { return `<span style="color:${hex}">${esc(text)}</span>`; }
function tierHex(pct) {
  const t = cfg.thresholds;
  if (pct >= t.red) return '#f87171';
  if (pct >= t.orange) return '#fb923c';
  if (pct >= t.yellow) return '#fbbf24';
  return '#34d399';
}
const GREY_HEX = '#64748b';

function renderField(key) {
  const color = (k, fallback) => HEX[cfg.fields.find(f => f.key === k)?.color] || fallback;
  switch (key) {
    case 'identity':
      return span(SAMPLE.user, '#22d3ee') + span('@', '#e2e8f0') + span(SAMPLE.host, '#34d399');
    case 'cwd':
      return span(SAMPLE.cwd, color('cwd', '#60a5fa'));
    case 'model':
      return span(SAMPLE.model, color('model', '#22d3ee')) + ' ' + span(SAMPLE.effort, GREY_HEX);
    case 'context':
      return span('ctx', '#e2e8f0') + ' ' + span(`${SAMPLE.ctxPct}%`, tierHex(SAMPLE.ctxPct)) + ' ' +
        span(SAMPLE.ctxUsed, '#22d3ee') + span('/', GREY_HEX) + span(SAMPLE.ctxMax, '#22d3ee');
    case 'rate_limits': {
      const resetOn = cfg.fields.find(f => f.key === 'rate_reset')?.enabled;
      const etaSpan = eta => resetOn ? span(' ⟳', GREY_HEX) + span(eta, GREY_HEX) : '';
      return span('5h', '#e2e8f0') + span(':', GREY_HEX) + span(`${SAMPLE.rate5h}%`, tierHex(SAMPLE.rate5h)) + etaSpan(SAMPLE.rate5hEta) +
        '  ' + span('7d', '#e2e8f0') + span(':', GREY_HEX) + span(`${SAMPLE.rate7d}%`, tierHex(SAMPLE.rate7d)) + etaSpan(SAMPLE.rate7dEta);
    }
    case 'rate_reset':
      return '';
    case 'cache_pct':
      return span('cache', '#e2e8f0') + ' ' + span(`${SAMPLE.cachePct}%`, tierHex(100 - SAMPLE.cachePct));
    case 'cache_tokens':
      return span(SAMPLE.cacheRead, color('cache_tokens', '#22d3ee')) + span('/', GREY_HEX) + span(SAMPLE.cacheTotal, color('cache_tokens', '#22d3ee'));
    case 'session': {
      let out = span(SAMPLE.duration, color('session', GREY_HEX));
      if (SAMPLE.linesAdded || SAMPLE.linesRemoved) {
        out += ' ' + span(`+${SAMPLE.linesAdded}`, '#34d399') + span('/', GREY_HEX) + span(`-${SAMPLE.linesRemoved}`, '#f87171');
      }
      out += ' ' + span(SAMPLE.cost, '#22d3ee');
      return out;
    }
    case 'git_branch':
      return span(SAMPLE.gitBranch, color('git_branch', '#e879f9'));
    default:
      return '';
  }
}

const previewHtml = computed(() => {
  const parts = cfg.fields.filter(f => f.enabled).map(f => renderField(f.key)).filter(Boolean);
  return parts.length ? parts.join(span(' | ', GREY_HEX)) : '<span style="color:#374151">(no fields enabled)</span>';
});
</script>

<style scoped>
.modal-body {
  padding: 14px 16px 10px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  overflow-y: auto;
}

.preview-box {
  background: #000;
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 6px;
  padding: 8px 10px;
}

.preview-line {
  font-family: 'JetBrains Mono', 'Fira Code', ui-monospace, monospace;
  font-size: 11px;
  margin: 0;
  white-space: pre-wrap;
  word-break: break-all;
}

.section-label {
  font-size: 10px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.4px;
  color: #94a3b8;
  margin-top: 6px;
}

.hint {
  font-weight: 400;
  text-transform: none;
  color: #4b5563;
  letter-spacing: 0;
}

.field-list {
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.field-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 5px 8px;
  border-radius: 6px;
  background: rgba(255, 255, 255, 0.03);
  border: 1px solid rgba(255, 255, 255, 0.06);
}

.field-row.disabled { opacity: 0.5; }

.field-row-sub {
  margin-left: 18px;
  padding-top: 3px;
  padding-bottom: 3px;
  background: transparent;
  border-style: dashed;
}

.field-toggle {
  display: flex;
  align-items: center;
  gap: 7px;
  flex: 1;
  cursor: pointer;
  min-width: 0;
}

.field-toggle input { accent-color: #d97757; cursor: pointer; flex-shrink: 0; }

.field-name {
  font-size: 11px;
  color: #e2e8f0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.color-select {
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 5px;
  color: #e2e8f0;
  font-size: 10px;
  padding: 3px 4px;
  outline: none;
}

.color-locked {
  font-size: 9px;
  color: #4b5563;
  text-transform: uppercase;
  letter-spacing: 0.3px;
  width: 60px;
  text-align: center;
}

.reorder-btns { display: flex; flex-direction: column; gap: 1px; }

.btn-reorder {
  background: transparent;
  border: none;
  color: #64748b;
  cursor: pointer;
  font-size: 9px;
  padding: 1px 4px;
  line-height: 1;
}

.btn-reorder:hover:not(:disabled) { color: #e2e8f0; }
.btn-reorder:disabled { opacity: 0.25; cursor: not-allowed; }

.threshold-row { display: flex; gap: 10px; flex-wrap: wrap; }

.threshold-field {
  display: flex;
  align-items: center;
  gap: 5px;
  font-size: 11px;
  color: #94a3b8;
}

.threshold-field input {
  width: 42px;
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 5px;
  color: #e2e8f0;
  font-size: 11px;
  padding: 3px 5px;
  outline: none;
}

.dot { width: 8px; height: 8px; border-radius: 50%; display: inline-block; }
.dot-yellow { background: #fbbf24; }
.dot-orange { background: #fb923c; }
.dot-red { background: #f87171; }

.host-list { display: flex; flex-wrap: wrap; gap: 6px; }

.host-chip {
  display: flex;
  align-items: center;
  gap: 5px;
  font-size: 10px;
  color: #94a3b8;
  background: rgba(255, 255, 255, 0.04);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 12px;
  padding: 4px 10px;
  cursor: pointer;
}

.host-chip input { accent-color: #d97757; cursor: pointer; }
.host-chip.active { color: #fba97a; border-color: rgba(217, 119, 87, 0.4); background: rgba(217, 119, 87, 0.1); }
.host-chip.warn { border-color: rgba(251, 191, 36, 0.5); }
.warn-icon { color: #fbbf24; font-size: 9px; }

.no-remotes { width: 100%; font-size: 10px; }

.results-list { display: flex; flex-direction: column; gap: 4px; margin-top: 4px; }

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

.btn-reset, .btn-apply {
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

.btn-reset:disabled, .btn-apply:disabled { opacity: 0.5; cursor: not-allowed; }

.btn-reset {
  background: rgba(255, 255, 255, 0.04);
  border-color: rgba(255, 255, 255, 0.1);
  color: #64748b;
}

.btn-reset:hover:not(:disabled) { background: rgba(255, 255, 255, 0.08); color: #94a3b8; }

.btn-apply {
  flex: 1;
  background: rgba(217, 119, 87, 0.15);
  border-color: rgba(217, 119, 87, 0.45);
  color: #d97757;
}

.btn-apply:hover:not(:disabled) { background: rgba(217, 119, 87, 0.25); color: #fba97a; }

/* Narrow mode (SSoT 700px, main.css) — this file's scoped padding outranks the global
   narrow rule, so the trim has to be repeated here. */
@media (max-width: 700px) {
  .modal-body   { padding: 10px 10px 8px; }
  .modal-footer { padding: 8px 10px 10px; }
}
</style>
