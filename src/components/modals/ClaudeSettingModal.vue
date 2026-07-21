<template>
  <BaseModal :show="show" @close="$emit('close')" container-style="width: 90vw; max-width: 1100px;">
    <template #title>
      <i class="fa-solid fa-terminal"></i> Statusline Customizer
    </template>

    <div class="modal-body">
            <div class="preview-box">
              <pre class="preview-line" v-html="previewHtml"></pre>
            </div>

            <div class="section-label">Fields <span class="hint">(drag the grip to reorder)</span></div>
            <TransitionGroup tag="div" name="row" class="field-list">
              <div
                v-for="row in rows"
                :key="row.id"
                class="row-item"
                :class="{ dragging: dragRowId === row.id, 'drop-before': dropIndicator.id === row.id && dropIndicator.pos === 'before', 'drop-after': dropIndicator.id === row.id && dropIndicator.pos === 'after' }"
                draggable="true"
                @dragstart="onDragStart(row, $event)"
                @dragover.prevent="onDragOver(row, $event)"
                @drop.prevent="onDrop(row, $event)"
                @dragend="onDragEnd"
              >
                <i class="fa-solid fa-grip-vertical drag-handle" title="Drag to reorder"></i>
                <div class="row-lines">
                  <div v-for="(line, li) in row.lines" :key="li" class="row-line" :class="{ disabled: !lineActive(line) }">
                    <template v-for="(seg, si) in segmentsOf(line)" :key="si">
                      <span v-if="seg.spacer" class="ctl-spacer"></span>
                      <span v-else class="ctl-seg" :class="{ tight: seg.tight }">
                        <template v-for="(control, ci) in seg.controls" :key="ci">
                          <label v-if="control.type === 'toggle'" class="ctl-toggle" :class="{ 'ctl-blocked': controlBlocked(control) }" :title="CATALOG[control.key]?.desc">
                            <input type="checkbox" :disabled="controlBlocked(control)" :checked="fieldEnabled(control.key)" @change="setFieldEnabled(control.key, $event.target.checked)" />
                            <span class="ctl-label">{{ control.label }}</span>
                          </label>
                          <span v-else-if="control.type === 'parts'" class="ctl-parts">
                            <span v-for="(p, pi) in control.items" :key="pi" class="ctl-part">{{ p }}</span>
                          </span>
                          <label v-else-if="control.type === 'master'" class="ctl-toggle ctl-master" :title="`Enable/disable the whole ${control.label} group`">
                            <input type="checkbox" :checked="isMasterChecked(control.keys)" @change="toggleMaster(control.keys, $event.target.checked)" />
                            <span class="ctl-label">{{ control.label }}</span>
                          </label>
                          <span v-else-if="control.type === 'color' && isColorEditable(control.key)" class="color-pick">
                            <button
                              type="button"
                              class="swatch swatch-current"
                              :style="{ background: HEX[fieldByKey(control.key).color] }"
                              :title="`Color: ${fieldByKey(control.key).color} — click to change`"
                              @click.stop="togglePicker(control.key)"
                            ></button>
                            <span v-if="pickerFor === control.key" class="swatch-pop" @click.stop>
                              <button
                                v-for="c in COLORS"
                                :key="c.key"
                                type="button"
                                class="swatch"
                                :class="{ sel: fieldByKey(control.key).color === c.key }"
                                :style="{ background: c.hex }"
                                :title="c.label"
                                @click="pickColor(control.key, c.key)"
                              ></button>
                            </span>
                          </span>
                          <span v-else-if="control.type === 'color'" class="color-dynamic" :title="ladderTitle">
                            <span v-for="t in ladder" :key="t.key" class="tier-dot" :style="{ background: t.hex }"></span>
                          </span>
                        </template>
                      </span>
                    </template>
                  </div>
                </div>
              </div>
            </TransitionGroup>

            <div class="section-label">Dynamic color <span class="hint">(each band starts at the % below it)</span></div>
            <!-- The ladder drawn to scale: every band's width is its share of 0-100, and the number
                 sits under the band's own left edge, so the input's position *is* its meaning — no
                 </≥ symbols to decode. -->
            <div class="ladder-bar">
              <div v-for="t in ladder" :key="t.key" class="ladder-cell" :style="{ flexGrow: Math.max(1, t.to - t.from) }">
                <span class="ladder-seg" :style="{ background: t.hex }"></span>
                <input
                  v-if="t.key !== 'blue'"
                  type="number" min="0" max="100"
                  v-model.number="cfg.thresholds[t.key]"
                  :title="`${t.key} starts at this %`"
                />
                <span v-else class="ladder-zero" title="Everything below the green threshold">0</span>
              </div>
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
import { ref, reactive, computed, watch, onMounted, onBeforeUnmount } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { projects } from '../../store/projectStore';
import BaseModal from './BaseModal.vue';
import { STATUSLINE_COLORS } from '../../utils/statuslineColors';

const props = defineProps({ show: { type: Boolean, default: false } });
defineEmits(['close']);

const STORAGE_KEY = 'aki-statusline-config';

// UI-only metadata for the field keys the Rust side understands (src-tauri/src/statusline.rs).
// Keep in sync with `default_config()` there when adding a new field key.
const CATALOG = {
  identity_user: { label: 'user', desc: 'Local username, truncated to 5 characters' },
  identity_host: { label: 'host', desc: 'Short hostname, truncated to 5 characters' },
  cwd:         { label: 'Working directory',    desc: 'Current folder name (~ when at $HOME)' },
  model:       { label: 'Model',                desc: 'Model name, e.g. "sonnet 5"' },
  effort:      { label: 'effort',               desc: 'Effort level (med/high), always grey — a qualifier of the model, not its own signal' },
  context:     { label: 'Context window',       desc: 'ctx NN% used/max, auto-colored by threshold' },
  rate_limits_5h: { label: '5h',          desc: '5-hour Pro/Max usage window, auto-colored by threshold' },
  rate_reset_5h:  { label: 'Reset ETA',   desc: 'Appends time-until-reset to the 5h window (e.g. 1h12m)' },
  rate_limits_7d: { label: '7d',          desc: '7-day Pro/Max usage window, auto-colored by threshold' },
  rate_reset_7d:  { label: 'Reset ETA',   desc: 'Appends time-until-reset to the 7d window (e.g. 2d3h)' },
  cache_pct:   { label: 'Cache hit %',          desc: 'Percent of the most recent request served from cache (green = high hit rate)' },
  cache_tokens:{ label: 'Cache tokens (read)',  desc: 'Tokens read from cache for the most recent request, grey and not colorable' },
  session:     { label: 'Session (dur/lines/$)',desc: 'Duration, lines +/- (bold green/red), and cost for this session' },
  git_branch:  { label: 'Git branch',           desc: 'Experimental — depends on Claude Code version exposing it' },
};

// The dynamic-color ladder, lowest tier first. `blue` has no threshold of its own — it is
// everything below `green`, i.e. "plenty left". Mirrors color_for_pct() in
// src-tauri/src/statusline.rs; the hexes are the terminal palette's rendering of those ANSI codes.
const TIER_HEX = { blue: '#60a5fa', green: '#34d399', yellow: '#fbbf24', orange: '#fb923c', red: '#f87171' };
const TIER_KEYS = ['green', 'yellow', 'orange', 'red'];

// The dollar spend a session's cost is scaled against before being run through the ladder.
// Mirrors COST_FULL_USD in src-tauri/src/statusline.rs.
const COST_FULL_USD = 30;

const COLORS = STATUSLINE_COLORS;
const HEX = Object.fromEntries(COLORS.map(c => [c.key, c.hex]));
const COLOR_EDITABLE = new Set(['identity_user', 'identity_host', 'cwd', 'model', 'git_branch']);
function isColorEditable(key) { return COLOR_EDITABLE.has(key); }

// Declarative row/group catalog (RULE-design-core: one shape for every row, not a hardcoded
// special case per group). A GROUP describes 1..N lines of controls; every key it lists moves
// together as one draggable row. Any cfg.fields key NOT listed in any group here renders as a
// plain 1-line, 1-toggle(+color) row — see `rows` computed below, which is the only place that
// tells bare fields and groups apart, and it does so from data, not markup.
//
// Control types:
//   toggle  — one field's enable checkbox. `dependsOn` greys it out and forces it off while the
//             named field is off (effort only means something next to a model name).
//   master  — a UI-only checkbox reflecting/setting several fields at once. No field key of its own.
//   color   — the field's color picker, or the recessed "Dynamic color" note when the color is
//             computed from the value rather than chosen (see COLOR_EDITABLE).
//   parts   — non-interactive: names the pieces the field actually prints, in print order, so the
//             row shows what it builds instead of only what it is called.
//
// A line normally holds one flat list of `controls`. A line may instead hold `segments` — several
// sub-lists that each render as one flex unit. Special segment flags:
//   tight   — visually brackets its controls with a subtle border (identity's `user @ host`).
//   spacer  — an empty flex-1 gap; separates left controls from right controls in the same row.
//
// `groupLabel` on a group: a fixed white label prepended in the preview when ≥1 member is enabled.
// Needed when the label must persist regardless of which sub-field is on (cache's "cache" prefix).
//
// `GROUPS` here mirrors the `GROUPS` const in src-tauri/src/statusline.rs, which is what actually
// joins a group's members with the group's own separator instead of the ` | ` one. Both the
// members and the separator must match; adding a group needs an entry in both files.
const GROUPS = [
  {
    id: 'identity',
    keys: ['identity_user', 'identity_host'],
    sep: '@',
    lines: [
      { segments: [
        { controls: [{ type: 'master', keys: ['identity_user', 'identity_host'], label: 'id' }] },
        { spacer: true },
        { tight: true, controls: [
          { type: 'toggle', key: 'identity_user', label: 'user' },
          { type: 'color', key: 'identity_user' },
          { type: 'parts', items: ['@'] },
          { type: 'toggle', key: 'identity_host', label: 'host' },
          { type: 'color', key: 'identity_host' },
        ] },
      ] },
    ],
  },
  {
    id: 'model',
    keys: ['model', 'effort'],
    sep: ' ',
    lines: [
      { segments: [
        { controls: [
          { type: 'toggle', key: 'model', label: 'model' },
          { type: 'color', key: 'model' },
        ] },
        { spacer: true },
        { controls: [
          { type: 'toggle', key: 'effort', label: 'effort', dependsOn: 'model' },
        ] },
      ] },
    ],
  },
  {
    id: 'context',
    keys: ['context'],
    lines: [
      { controls: [
        { type: 'toggle', key: 'context', label: 'Context window' },
        { type: 'parts', items: ['%'] },
        { type: 'color', key: 'context' },
        { type: 'parts', items: ['token in+out', '/Max Token'] },
      ] },
    ],
  },
  {
    id: 'quota',
    keys: ['rate_limits_5h', 'rate_reset_5h', 'rate_limits_7d', 'rate_reset_7d'],
    sep: ' ',
    lines: [
      { segments: [
        { controls: [
          { type: 'toggle', key: 'rate_limits_5h', label: '5h' },
          { type: 'parts', items: ['%'] },
          { type: 'color', key: 'rate_limits_5h' },
        ] },
        { spacer: true },
        { controls: [
          { type: 'toggle', key: 'rate_reset_5h', label: 'Reset ETA', dependsOn: 'rate_limits_5h' },
        ] },
      ] },
      { segments: [
        { controls: [
          { type: 'toggle', key: 'rate_limits_7d', label: '7d' },
          { type: 'parts', items: ['%'] },
          { type: 'color', key: 'rate_limits_7d' },
        ] },
        { spacer: true },
        { controls: [
          { type: 'toggle', key: 'rate_reset_7d', label: 'Reset ETA', dependsOn: 'rate_limits_7d' },
        ] },
      ] },
    ],
  },
  {
    id: 'session',
    keys: ['session'],
    lines: [
      { controls: [
        { type: 'toggle', key: 'session', label: 'ss' },
        { type: 'parts', items: ['duration', 'Line +/-', '$'] },
        { type: 'color', key: 'session' },
      ] },
    ],
  },
  {
    id: 'cache',
    keys: ['cache_pct', 'cache_tokens'],
    groupLabel: 'cache',
    sep: ' ',
    lines: [
      { controls: [
        { type: 'master', keys: ['cache_pct', 'cache_tokens'], label: 'Cache' },
        { type: 'toggle', key: 'cache_pct', label: '%' },
        { type: 'parts', items: ['hit/max Token'] },
        { type: 'toggle', key: 'cache_tokens', label: 'read' },
        { type: 'parts', items: ['Token'] },
      ] },
    ],
  },
];

// Color doctrine these defaults follow (docs/feat/statusline-customizer.md): white = labels,
// cyan = ordinary information, grey = supporting detail, dynamic = must be noticed. "Which
// machine" (host=magenta) and "where am I" (cwd=yellow) get standout hues — what the eye hunts
// for first with several terminals open. user=grey (present but not the focal point).
// Mirrors default_config() in statusline.rs.
function defaultLocalConfig() {
  return {
    fields: [
      { key: 'identity_user',  enabled: true,  color: 'grey' },
      { key: 'identity_host',  enabled: true,  color: 'magenta' },
      { key: 'cwd',            enabled: true,  color: 'yellow' },
      { key: 'model',          enabled: true,  color: 'cyan' },
      { key: 'effort',         enabled: true,  color: 'grey' },
      { key: 'context',        enabled: true,  color: 'white' },
      { key: 'rate_limits_5h', enabled: true,  color: 'white' },
      { key: 'rate_reset_5h',  enabled: false, color: 'grey' },
      { key: 'rate_limits_7d', enabled: true,  color: 'white' },
      { key: 'rate_reset_7d',  enabled: false, color: 'grey' },
      { key: 'session',        enabled: true,  color: 'grey' },
      { key: 'git_branch',     enabled: true,  color: 'magenta' },
      { key: 'cache_pct',      enabled: false, color: 'white' },
      { key: 'cache_tokens',   enabled: false, color: 'grey' },
    ],
    // Uneven by design: wide calm bands at the bottom, narrow urgent ones at the top.
    // Mirrors default_config() in src-tauri/src/statusline.rs.
    thresholds: { green: 25, yellow: 51, orange: 75, red: 90 },
  };
}

// Migrates older config shapes onto the current key set, preserving the user's enabled/color
// choices. No-op once a config is already migrated. Runs before loadCfg's generic
// append-unknown-keys step, so a key handled here lands in its right position rather than being
// appended to the bottom of the list.
function migrateOldKeys(saved) {
  // Splices the new keys in at the old key's own index, so a user who had put the field somewhere
  // other than the end of the list keeps that position.
  // `colors` lets a split assign a different color per new key; omitted, both inherit the old
  // field's color.
  const splitInPlace = (oldKey, newKeys, colors) => {
    const idx = saved.fields.findIndex(f => f.key === oldKey);
    if (idx === -1) return;
    const old = saved.fields[idx];
    saved.fields.splice(idx, 1, ...newKeys.map((key, i) => ({ key, enabled: old.enabled, color: colors?.[i] ?? old.color })));
  };
  // identity's two halves used to be hardcoded cyan/green with no user control; seed the new
  // editable colors with exactly those, so nothing changes on screen until the user picks.
  splitInPlace('identity', ['identity_user', 'identity_host'], ['cyan', 'green']);
  splitInPlace('rate_limits', ['rate_limits_5h', 'rate_limits_7d']);
  splitInPlace('rate_reset', ['rate_reset_5h', 'rate_reset_7d']);

  // `effort` used to be printed unconditionally as part of the model block; it is now its own
  // toggleable field. Seed it from the model's state so an existing statusline looks identical
  // until the user changes it, and place it directly after model rather than at the bottom.
  if (!saved.fields.some(f => f.key === 'effort')) {
    const idx = saved.fields.findIndex(f => f.key === 'model');
    if (idx !== -1) saved.fields.splice(idx + 1, 0, { key: 'effort', enabled: saved.fields[idx].enabled, color: 'grey' });
  }

  // The ladder gained a `green` tier below the old bottom tier. A saved config predating it has no
  // value, which would render an empty number input and make every low value fall through to blue.
  const defThresholds = defaultLocalConfig().thresholds;
  for (const [key, val] of Object.entries(defThresholds)) {
    if (typeof saved.thresholds[key] !== 'number') saved.thresholds[key] = val;
  }
}

function loadCfg() {
  let saved = null;
  try {
    const raw = JSON.parse(localStorage.getItem(STORAGE_KEY) || 'null');
    if (raw && Array.isArray(raw.fields) && raw.thresholds) saved = raw;
  } catch { /* fall through to default */ }
  const def = defaultLocalConfig();
  if (!saved) return def;
  migrateOldKeys(saved);
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
  resetMasterChecked();
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

function fieldByKey(key) { return cfg.fields.find(f => f.key === key); }
function fieldEnabled(key) { return !!fieldByKey(key)?.enabled; }
function setFieldEnabled(key, val) {
  const f = fieldByKey(key);
  if (f) f.enabled = val;
  // Turning a parent off takes its dependants down with it, so the stored config can never hold a
  // dependant that is on while the field it qualifies is off — the preview and the generated
  // script both read `enabled` directly and would otherwise disagree with the greyed-out checkbox.
  if (!val) {
    for (const g of GROUPS) {
      for (const line of g.lines) {
        for (const c of segmentsOf(line).flatMap(s => s.controls || [])) {
          if (c.type === 'toggle' && c.dependsOn === key) setFieldEnabled(c.key, false);
        }
      }
    }
  }
}
// A master checkbox (identity's "id", cache's "Cache") decides its children's on/off — it does
// NOT reflect them. Toggling one child (e.g. cache's "%") must never flip the master's own
// checked state; only clicking the master itself does. So its checked value is independent state,
// seeded once from the children (every() at first render) and never recomputed from them after —
// see resetMasterChecked(), called wherever cfg is reloaded wholesale.
const masterChecked = ref({});
function masterKey(keys) { return keys.join(','); }
function isMasterChecked(keys) {
  const k = masterKey(keys);
  if (!(k in masterChecked.value)) masterChecked.value[k] = keys.every(fieldEnabled);
  return masterChecked.value[k];
}
function toggleMaster(keys, val) {
  masterChecked.value[masterKey(keys)] = val;
  keys.forEach(k => setFieldEnabled(k, val));
}
function resetMasterChecked() { masterChecked.value = {}; }
function controlBlocked(control) { return !!control.dependsOn && !fieldEnabled(control.dependsOn); }

// A line is authored either as one flat `controls` list or as explicit `segments` when part of it
// is its own visual unit — normalized here so the template renders one shape, not two.
function segmentsOf(line) { return line.segments || [{ controls: line.controls }]; }

// The color control is a swatch, not a named dropdown: the name of a color says less than the
// color, and costs several times the width (extreme-narrow rule, CLAUDE.md). One picker is open at
// a time, keyed by field.
const pickerFor = ref(null);
function togglePicker(key) { pickerFor.value = pickerFor.value === key ? null : key; }
function pickColor(key, color) {
  const f = fieldByKey(key);
  if (f) f.color = color;
  pickerFor.value = null;
}
const closePicker = () => { pickerFor.value = null; };
onMounted(() => window.addEventListener('click', closePicker));
onBeforeUnmount(() => window.removeEventListener('click', closePicker));

// The ladder, as the user's own thresholds currently define it — shown wherever a field's color is
// computed rather than chosen, so "auto" says which colors and at what point instead of just
// asserting that something happens.
const ladder = computed(() => [
  { key: 'blue', hex: TIER_HEX.blue, from: 0, to: cfg.thresholds.green },
  ...TIER_KEYS.map((key, i) => ({
    key,
    hex: TIER_HEX[key],
    from: cfg.thresholds[key],
    to: i + 1 < TIER_KEYS.length ? cfg.thresholds[TIER_KEYS[i + 1]] : 100,
  })),
]);

const ladderTitle = computed(() =>
  'Color follows the value, not a fixed choice: ' +
  ladder.value.map(t => `${t.key} ${t.from === 0 ? '<' + t.to : t.from + '-' + t.to}%`).join(', ')
);
function lineActive(line) {
  return segmentsOf(line).flatMap(s => s.controls || []).some(c => {
    if (c.type === 'toggle') return fieldEnabled(c.key);
    if (c.type === 'master') return c.keys.some(fieldEnabled);
    return false;
  });
}

// Projects the flat, persisted `cfg.fields` array into display rows: a row is either one of the
// declarative GROUPS above (all its keys collapsed into one draggable unit) or a single bare
// field rendered through the exact same 1-line/1-toggle(+color) shape. Row order follows first
// appearance in `cfg.fields`, so this stays a pure read projection — `cfg.fields` remains the
// only stored order (see `applyRowOrder`, which writes back through this same shape).
const rows = computed(() => {
  const consumed = new Set();
  const result = [];
  for (const f of cfg.fields) {
    if (consumed.has(f.key)) continue;
    const group = GROUPS.find(g => g.keys.includes(f.key));
    if (group) {
      result.push({ id: group.id, keys: group.keys, lines: group.lines });
      group.keys.forEach(k => consumed.add(k));
    } else {
      consumed.add(f.key);
      result.push({
        id: f.key,
        keys: [f.key],
        lines: [{ controls: [
          { type: 'toggle', key: f.key, label: CATALOG[f.key]?.label || f.key },
          { type: 'color', key: f.key },
        ] }],
      });
    }
  }
  return result;
});

// ---- drag-and-drop reorder (native HTML5 DnD, row-granularity — a group always moves whole) ----
const dragRowId = ref(null);
const dropIndicator = reactive({ id: null, pos: null }); // pos: 'before' | 'after'

function onDragStart(row, e) {
  dragRowId.value = row.id;
  e.dataTransfer.effectAllowed = 'move';
  e.dataTransfer.setData('text/plain', row.id);
}

function onDragOver(row, e) {
  if (!dragRowId.value || row.id === dragRowId.value) return;
  const rect = e.currentTarget.getBoundingClientRect();
  dropIndicator.id = row.id;
  dropIndicator.pos = (e.clientY - rect.top) < rect.height / 2 ? 'before' : 'after';
}

function onDrop(row) {
  if (dragRowId.value && row.id !== dragRowId.value) {
    const order = rows.value.map(r => r.id);
    const fromIdx = order.indexOf(dragRowId.value);
    let toIdx = order.indexOf(row.id);
    if (dropIndicator.pos === 'after') toIdx += 1;
    if (fromIdx < toIdx) toIdx -= 1;
    order.splice(fromIdx, 1);
    order.splice(toIdx, 0, dragRowId.value);
    applyRowOrder(order);
  }
  clearDrag();
}

function onDragEnd() { clearDrag(); }
function clearDrag() { dragRowId.value = null; dropIndicator.id = null; dropIndicator.pos = null; }

// Rebuilds the flat, persisted `cfg.fields` array from a new row-id order, expanding each row
// back to its member keys and reusing the existing field objects (so enabled/color survive the
// move untouched). Mutating in place keeps the array's reactivity + the existing deep `watch`
// (localStorage persistence) and `previewHtml` wired without any new watcher.
function applyRowOrder(idOrder) {
  const byId = new Map(rows.value.map(r => [r.id, r]));
  const newFields = [];
  for (const id of idOrder) {
    const row = byId.get(id);
    if (!row) continue;
    for (const key of row.keys) {
      const f = fieldByKey(key);
      if (f) newFields.push(f);
    }
  }
  cfg.fields.splice(0, cfg.fields.length, ...newFields);
}

function resetToDefault() {
  Object.assign(cfg, defaultLocalConfig());
  resetMasterChecked();
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
  duration: '12m', linesAdded: 122, linesRemoved: 52, cost: '$8.40', costUsd: 8.4,
  gitBranch: 'master',
  cachePct: 78, cacheRead: '12.4k', cacheTotal: '45.2k',
};

function esc(s) { return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;'); }
// `bold` is for the session +/- line count glyphs — too small to read at normal weight.
function span(text, hex, bold) { return `<span style="color:${hex}${bold ? ';font-weight:700' : ''}">${esc(text)}</span>`; }
function tierHex(pct) {
  const t = cfg.thresholds;
  if (pct >= t.red) return TIER_HEX.red;
  if (pct >= t.orange) return TIER_HEX.orange;
  if (pct >= t.yellow) return TIER_HEX.yellow;
  if (pct >= t.green) return TIER_HEX.green;
  return TIER_HEX.blue;
}
const GREY_HEX = '#64748b';

function renderField(key) {
  const color = (k, fallback) => HEX[cfg.fields.find(f => f.key === k)?.color] || fallback;
  switch (key) {
    case 'identity_user':
      return span(SAMPLE.user, color('identity_user', '#22d3ee'));
    case 'identity_host':
      return span(SAMPLE.host, color('identity_host', '#34d399'));
    case 'cwd':
      return span(SAMPLE.cwd, color('cwd', '#60a5fa'));
    case 'model':
      return span(SAMPLE.model, color('model', '#22d3ee'));
    case 'effort':
      return span(SAMPLE.effort, GREY_HEX);
    case 'context':
      return span('ctx', '#e2e8f0') + ' ' + span(`${SAMPLE.ctxPct}%`, tierHex(SAMPLE.ctxPct)) + ' ' +
        span(`${SAMPLE.ctxUsed}/${SAMPLE.ctxMax}`, GREY_HEX);
    case 'rate_limits_5h': {
      const resetOn = fieldEnabled('rate_reset_5h');
      const eta = resetOn ? span(` ${SAMPLE.rate5hEta}`, GREY_HEX) : '';
      return span('5h', '#e2e8f0') + span(':', GREY_HEX) + span(`${SAMPLE.rate5h}%`, tierHex(SAMPLE.rate5h)) + eta;
    }
    case 'rate_limits_7d': {
      const resetOn = fieldEnabled('rate_reset_7d');
      const eta = resetOn ? span(` ${SAMPLE.rate7dEta}`, GREY_HEX) : '';
      return span('7d', '#e2e8f0') + span(':', GREY_HEX) + span(`${SAMPLE.rate7d}%`, tierHex(SAMPLE.rate7d)) + eta;
    }
    case 'rate_reset_5h':
    case 'rate_reset_7d':
      return '';
    case 'cache_pct':
      return span(`${SAMPLE.cachePct}%`, tierHex(100 - SAMPLE.cachePct));
    case 'cache_tokens':
      return span(SAMPLE.cacheRead, GREY_HEX);
    case 'session': {
      let out = span('ss', '#e2e8f0') + ' ' + span(SAMPLE.duration, color('session', GREY_HEX));
      if (SAMPLE.linesAdded || SAMPLE.linesRemoved) {
        out += ' ' + span(`+${SAMPLE.linesAdded}`, '#34d399', true) + span('/', GREY_HEX) + span(`-${SAMPLE.linesRemoved}`, '#f87171', true);
      }
      out += ' ' + span(SAMPLE.cost, tierHex(Math.min(100, SAMPLE.costUsd / COST_FULL_USD * 100)));
      return out;
    }
    case 'git_branch':
      return span(SAMPLE.gitBranch, color('git_branch', '#e879f9'));
    default:
      return '';
  }
}

// Mirrors the join in statusline.rs: a group's enabled members are joined by the group's own `sep`
// into one block, and only whole blocks get the ` | ` separator. A group contributes its block
// once, at the position of its first enabled member.
const previewHtml = computed(() => {
  const blocks = [];
  const done = new Set();
  for (const f of cfg.fields) {
    if (!f.enabled || done.has(f.key)) continue;
    const group = GROUPS.find(g => g.keys.includes(f.key));
    if (!group) {
      blocks.push(renderField(f.key));
      continue;
    }
    group.keys.forEach(k => done.add(k));
    const inner = group.keys.filter(fieldEnabled).map(renderField).filter(Boolean);
    // `sep` defaults to a space for groups that don't declare one (matches statusline.rs); a
    // non-space separator is printed white, like a label, since it is punctuation the user reads.
    const sep = group.sep && group.sep !== ' ' ? span(group.sep, '#e2e8f0') : ' ';
    if (inner.length) {
      const joined = inner.join(sep);
      blocks.push(group.groupLabel ? span(group.groupLabel, '#e2e8f0') + ' ' + joined : joined);
    }
  }
  const parts = blocks.filter(Boolean);
  if (!parts.length) return '<span style="color:#374151">(no fields enabled)</span>';
  // Each block, plus the separator that follows it, is one unbreakable inline-block: the preview
  // wraps between blocks the way the real statusline does in a narrow terminal, instead of
  // breaking mid-word and splitting a value away from its label.
  return parts
    .map((html, i) => `<span class="pv-block">${html}${i < parts.length - 1 ? span(' | ', GREY_HEX) : ''}</span>`)
    .join('');
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
  /* Wrapping is controlled by .pv-block, not by the text itself — see previewHtml. */
  white-space: normal;
  word-break: normal;
}

/* One statusline block (a field, or a whole group) plus its trailing separator. inline-block +
   nowrap makes the line break between blocks rather than inside one. */
:deep(.pv-block) {
  display: inline-block;
  white-space: nowrap;
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

.row-item {
  display: flex;
  align-items: stretch;
  gap: 6px;
  padding: 5px 6px;
  border-radius: 6px;
  background: rgba(255, 255, 255, 0.03);
  border: 1px solid rgba(255, 255, 255, 0.06);
  position: relative;
}

.row-item.dragging { opacity: 0.4; }

/* Drop-position indicator: a thin accent line on the edge the dragged row would land on —
   communicated via an existing element's border, no extra DOM. */
.row-item.drop-before { box-shadow: inset 0 2px 0 0 #d97757; }
.row-item.drop-after { box-shadow: inset 0 -2px 0 0 #d97757; }

.drag-handle {
  color: #475569;
  font-size: 10px;
  cursor: grab;
  align-self: center;
  padding: 0 2px;
  flex-shrink: 0;
}

.drag-handle:active { cursor: grabbing; }

.row-lines {
  display: flex;
  flex-direction: column;
  gap: 3px;
  flex: 1;
  min-width: 0;
}

.row-line {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.row-line.disabled { opacity: 0.5; }

.ctl-toggle {
  display: flex;
  align-items: center;
  gap: 5px;
  cursor: pointer;
  min-width: 0;
}

.ctl-toggle input { accent-color: #d97757; cursor: pointer; flex-shrink: 0; }

.ctl-master { font-weight: 600; }

/* A toggle whose parent field is off: still readable, clearly not actionable. No extra row or
   explanatory text — the greying is the whole message (extreme-narrow rule, CLAUDE.md). */
.ctl-blocked { cursor: not-allowed; opacity: 0.4; }
.ctl-blocked input { cursor: not-allowed; }

/* Non-interactive: the pieces the field prints, in print order, so the row shows its shape.
   Monospace + very low contrast so it reads as a hint, never as a control. */
.ctl-parts {
  display: flex;
  align-items: center;
  gap: 4px;
  min-width: 0;
  overflow: hidden;
}

.ctl-part {
  font-family: 'JetBrains Mono', 'Fira Code', ui-monospace, monospace;
  font-size: 9px;
  color: #64748b;
  background: rgba(255, 255, 255, 0.04);
  border-radius: 3px;
  padding: 1px 4px;
  white-space: nowrap;
}

.ctl-label {
  font-size: 11px;
  color: #e2e8f0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* A segment groups controls inside one row-line. `tight` visually brackets a unit (e.g. user @ host). */
.ctl-seg {
  display: flex;
  align-items: center;
  gap: 5px;
}

.ctl-seg.tight {
  background: rgba(255, 255, 255, 0.04);
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 4px;
  padding: 2px 7px;
  gap: 5px;
}

/* Flexible gap between left and right control clusters in a row (identity id vs user@host,
   model swatch vs effort, 5h/7d toggle vs Reset ETA). Clamped so it's ≈3vw but never tiny. */
.ctl-spacer { flex: 0 0 clamp(12px, 3vw, 40px); }

/* Color swatch button (current color) + its pop-out palette. One picker open at a time. */
.color-pick {
  position: relative;
  display: flex;
  align-items: center;
  margin-left: auto;
}

.swatch {
  width: 14px;
  height: 14px;
  border-radius: 3px;
  border: 1px solid rgba(255, 255, 255, 0.18);
  cursor: pointer;
  padding: 0;
  flex-shrink: 0;
  transition: border-color 0.1s;
}

.swatch:hover { border-color: rgba(255, 255, 255, 0.45); }

.swatch.sel {
  outline: 2px solid rgba(255, 255, 255, 0.65);
  outline-offset: 1px;
}

/* 2 columns x 4 rows, filled column-first (top-to-bottom then next column) — a vertical grid
   takes less horizontal space than the old single row of 8 swatches. */
.swatch-pop {
  position: absolute;
  right: 0;
  top: calc(100% + 5px);
  display: grid;
  grid-template-columns: repeat(2, 14px);
  grid-template-rows: repeat(4, 14px);
  grid-auto-flow: column;
  gap: 4px;
  background: #0f172a;
  border: 1px solid rgba(255, 255, 255, 0.14);
  border-radius: 6px;
  padding: 6px;
  z-index: 20;
}

/* Recessed indicator for fields whose color follows the value (not a fixed choice). */
.color-dynamic {
  display: flex;
  align-items: center;
  gap: 3px;
  margin-left: auto;
}

/* The gradient dots shown inline in the field's row when color is dynamic. */
.tier-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex-shrink: 0;
}

/* Dynamic-color threshold bar: proportional colored bands + minimal number inputs. */
.ladder-bar {
  display: flex;
  align-items: flex-start;
  gap: 3px;
}

.ladder-cell {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 2px;
  min-width: 24px;
}

.ladder-seg {
  width: 100%;
  height: 6px;
  border-radius: 2px;
}

.ladder-cell input[type="number"] {
  width: 30px;
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 3px;
  color: #94a3b8;
  font-size: 10px;
  padding: 1px 3px;
  outline: none;
  -moz-appearance: textfield;
}

.ladder-cell input[type="number"]::-webkit-inner-spin-button,
.ladder-cell input[type="number"]::-webkit-outer-spin-button { -webkit-appearance: none; }

.ladder-zero {
  font-size: 10px;
  color: #4b5563;
  padding: 1px 3px;
  font-family: 'JetBrains Mono', ui-monospace, monospace;
}

/* TransitionGroup reorder animation — rows slide into their new slot; enter/leave stay minimal
   since rows are only added/removed on Reset, not during a drag (drag mutates order in place). */
.row-move { transition: transform 0.2s ease; }
.row-enter-active, .row-leave-active { transition: opacity 0.15s ease; }
.row-enter-from, .row-leave-to { opacity: 0; }
.row-leave-active { position: absolute; }

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
