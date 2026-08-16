<template>
  <BaseModal :show="show" @close="$emit('close')" container-style="width: 90vw; max-width: 1100px;">
    <template #title>
      <div class="modal-title-wrap">
        <span class="modal-title-text">
          <i class="fa-solid fa-terminal"></i> Statusline Customizer
        </span>
        <div class="header-target-selector">
          <span class="target-label">Apply to:</span>
          <label class="target-check-item">
            <input type="checkbox" value="ag" v-model="selectedTargets" />
            <span>agy</span>
          </label>
          <label class="target-check-item">
            <input type="checkbox" value="cc" v-model="selectedTargets" />
            <span>claude</span>
          </label>
        </div>
      </div>
    </template>

    <div class="modal-body">
            <div class="preview-box">
              <!-- u-select-text (main.css): line opts out of app-wide no-selection default for easy copying. -->
              <pre class="preview-line u-select-text" v-html="previewHtml"></pre>
            </div>

            <div class="section-label fields-header">
              <span>Fields <span class="hint">(drag the grip to reorder)</span></span>
              <button type="button" class="toggle-all-btn" @click="toggleAllFields">{{ allFieldsEnabled ? 'Uncheck all' : 'Check all' }}</button>
            </div>
            <!-- Pinned first: rendered without drag grip outside TransitionGroup. -->
            <div class="row-item pinned-row" :title="CATALOG.cli_tag.desc">
              <i class="fa-solid fa-thumbtack pin-icon" title="Fixed position - always first"></i>
              <label class="ctl-toggle">
                <input type="checkbox" :checked="fieldEnabled('cli_tag')" @change="setFieldEnabled('cli_tag', $event.target.checked)" />
                <span class="ctl-label">{{ CATALOG.cli_tag.label }}</span>
              </label>
              <!-- Account is clustered with tag on same background, excluded from draggable list. -->
              <label class="ctl-toggle" :title="CATALOG.account.desc">
                <input type="checkbox" :checked="fieldEnabled('account')" @change="setFieldEnabled('account', $event.target.checked)" />
                <span class="ctl-label">{{ CATALOG.account.label }}</span>
              </label>
              <label class="ctl-trunc" :title="truncTitle('account')">
                <i class="fa-solid fa-scissors"></i>
                <input type="number" :min="TRUNC_MIN" :max="truncMax('account')" :value="cfg.trunc.account" @change="setTrunc('account', $event.target.value)" />
              </label>
            </div>
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
                  <div v-if="row.note" class="row-note">{{ row.note }}</div>
                  <div v-for="(line, li) in row.lines" :key="li" class="row-line" :class="{ disabled: !lineActive(line) }">
                    <template v-for="(seg, si) in segmentsOf(line)" :key="si">
                      <span v-if="seg.spacer" class="ctl-spacer"></span>
                      <span v-else class="ctl-seg" :class="{ tight: seg.tight }">
                        <template v-for="(control, ci) in seg.controls" :key="ci">
                          <label v-if="control.type === 'toggle'" class="ctl-toggle" :class="{ 'ctl-blocked': controlBlocked(control) }" :title="CATALOG[control.key]?.desc">
                            <input type="checkbox" :disabled="controlBlocked(control)" :checked="fieldEnabled(control.key)" @change="setFieldEnabled(control.key, $event.target.checked)" />
                            <span class="ctl-label">{{ control.label }}</span>
                          </label>
                          <!-- Hint text hidden on narrow screens (u-narrow-hide, main.css). -->
                          <span v-else-if="control.type === 'parts'" class="ctl-parts u-narrow-hide">
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
                              :title="colorTitle(control.key)"
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
                                :title="swatchTitle(c)"
                                @click="pickColor(control.key, c.key)"
                              ></button>
                            </span>
                          </span>
                          <span v-else-if="control.type === 'color'" class="color-dynamic" :title="ladderTitle">
                            <span v-for="t in ladder" :key="t.key" class="tier-dot" :style="{ background: t.hex }" :title="swatchTitle(t)"></span>
                          </span>
                          <label v-else-if="control.type === 'trunc'" class="ctl-trunc" :title="truncTitle(control.name)">
                            <i class="fa-solid fa-scissors"></i>
                            <input
                              type="number" :min="TRUNC_MIN" :max="truncMax(control.name)"
                              :value="cfg.trunc[control.name]"
                              @change="setTrunc(control.name, $event.target.value)"
                            />
                          </label>
                        </template>
                      </span>
                    </template>
                  </div>
                </div>
              </div>
            </TransitionGroup>

            <div class="section-label">Dynamic color <span class="hint">(each band starts at the % below it)</span></div>
            <!-- Ladder drawn to scale: band width proportional to 0-100 share with input under left edge. -->
            <div class="ladder-bar">
              <div v-for="t in ladder" :key="t.key" class="ladder-cell" :style="{ flexGrow: Math.max(1, t.to - t.from) }">
                <span class="ladder-seg" :style="{ background: t.hex }" :title="swatchTitle(t)"></span>
                <input
                  v-if="t.key !== CALM.key"
                  type="number" min="0" max="100"
                  v-model.number="cfg.thresholds[t.key]"
                  :title="`${t.key} starts at this %`"
                />
                <span v-else class="ladder-zero" title="Everything below the green threshold">0</span>
              </div>
            </div>

            <!-- Alternating background shades on neutral ramp distinguish blocks without fighting text color. -->
            <div class="section-label">
              <span>Block background</span>
              <label class="sep-toggle" title="Pad every block with a space on each side, so the shades read as separate plates instead of one strip. The tag cluster is never padded.">
                <input type="checkbox" v-model="cfg.separate" />
                <span>separate</span>
              </label>
            </div>
            <div class="zebra-rows">
              <div v-for="slot in ['a', 'b']" :key="slot" class="zebra-row">
                <span class="zebra-name">{{ slot === 'a' ? 'odd' : 'even' }}</span>
                <button
                  v-for="n in ZEBRA_SHADES"
                  :key="n"
                  type="button"
                  class="zebra-swatch"
                  :class="{ sel: cfg.zebra[slot] === n }"
                  :style="{ background: zebraHex(n) }"
                  :title="`ANSI 48;5;${n}`"
                  @click="cfg.zebra[slot] = n"
                ></button>
              </div>
            </div>

            <div class="section-label">Target Hosts <span class="hint">(tag per CLI found: filled = statusline live)</span></div>
            <div class="host-list">
              <label
                v-for="h in hostOptions"
                :key="h"
                class="host-chip"
                :class="{ active: selectedHosts.includes(h) }"
              >
                <input type="checkbox" :value="h" v-model="selectedHosts" />
                {{ h === 'local' ? 'Local' : h }}
                <!-- Tag per host CLI: lit = live statusline, hollow amber = CLI present but unwired, absent = not installed. -->
                <span
                  v-for="t in cliTags(h)"
                  :key="t.cli"
                  class="cli-tag"
                  :class="t.configured ? 'on' : 'off'"
                  :title="t.title"
                >{{ t.cli }}</span>
              </label>
              <div v-if="hostOptions.length === 1" class="hint no-remotes">No remote hosts configured yet - add a project with a remote host to push there too.</div>
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
            <button class="btn-modal-action btn-reset" @click="resetToDefault" :disabled="busy" title="Reload the built-in default preset">
              <i class="fa-solid fa-arrow-rotate-left"></i> Reset
            </button>
            <button class="btn-modal-action btn-apply" @click="apply" :disabled="busy || selectedHosts.length === 0 || selectedTargets.length === 0" :title="applyTitle">
              <i class="fa-solid" :class="busy ? 'fa-circle-notch fa-spin' : 'fa-paper-plane'"></i>
              {{ busy ? 'Applying…' : `Apply to ${selectedHosts.length} host${selectedHosts.length === 1 ? '' : 's'}` }}
            </button>
    </div>
  </BaseModal>
</template>

<script setup>
import { ref, reactive, computed, watch, onMounted, onBeforeUnmount } from 'vue';
import { invoke } from '../../utils/tauri';
import { projects } from '../../store/projectStore';
import BaseModal from './BaseModal.vue';
import { STATUSLINE_COLORS, STATUSLINE_TIERS, swatchTitle } from '../../utils/statuslineColors';

const props = defineProps({ show: { type: Boolean, default: false } });
defineEmits(['close']);

const STORAGE_KEY = 'aki-statusline-config';
// Bumped whenever the defaults themselves change meaning (colors, field set, thresholds). A stored config from an older version is discarded outright rather than translated - see loadCfg.
const CONFIG_VERSION = 3;
const TARGETS_KEY = 'aki-statusline-targets';

// UI metadata for statusline field keys; keep in sync with `default_config()` in src-tauri/src/statusline.rs.
const CATALOG = {
  cli_tag:     { label: 'CLI tag', desc: 'Colored tag at the very start of the line identifying which CLI rendered it - always first, cannot be reordered' },
  identity_user: { label: 'user', desc: 'Local username, cut to the width set on this row' },
  identity_host: { label: 'host', desc: 'Short hostname, cut to the width set on this row' },
  account:       { label: 'Account', desc: 'Active account name - the domain is dropped first ("lva@akitao.com" becomes "lva"), then cut to the width set on this row' },
  cwd:         { label: 'Working directory',    desc: 'Current folder name (~ when at $HOME)' },
  model:       { label: 'Model',                desc: 'Model name, e.g. "sonnet 5"' },
  effort:      { label: 'effort',               desc: 'Effort level (med/high), always grey - a qualifier of the model, not its own signal' },
  context:     { label: 'Context window',       desc: 'ctx NN% used/max, auto-colored by threshold' },
  rate_limits_5h: { label: '5h',          desc: '5-hour Pro/Max usage window, auto-colored by threshold' },
  rate_reset_5h:  { label: 'Reset ETA',   desc: 'Appends time-until-reset to the 5h window (e.g. 1h12m)' },
  rate_limits_7d: { label: '7d',          desc: '7-day Pro/Max usage window, auto-colored by threshold' },
  rate_reset_7d:  { label: 'Reset ETA',   desc: 'Appends time-until-reset to the 7d window (e.g. 2d3h)' },
  cache:       { label: 'Cache', desc: 'Master switch for the cache block - the two readings below only print while this is on' },
  cache_pct:   { label: 'Cache hit %',          desc: 'Percent of the most recent request served from cache (green = high hit rate)' },
  cache_tokens:{ label: 'Cache tokens (read)',  desc: 'Tokens read from cache for the most recent request, grey and not colorable' },
  session:     { label: 'Session (dur/lines/$)',desc: 'Duration, lines +/- (bold green/red), and cost for this session' },
  git_branch:  { label: 'Git branch',           desc: 'Experimental - depends on Claude Code version exposing it' },
  ram:         { label: 'System RAM',           desc: 'Whole-machine memory in use (⚅ NN%) - read from the OS, not from the CLI payload' },
};

// The ladder, lowest tier first, mirroring color_for_pct() in the script - see docs/feat/statusline-customizer.md § The dynamic-color ladder.
const TIERS = STATUSLINE_TIERS;
const TIER_BY_KEY = Object.fromEntries(TIERS.map(t => [t.key, t]));
// The four upper tiers are exactly the stored threshold keys; the bottom one is not stored.
const CALM = TIERS[0];
const TIER_KEYS = ['green', 'yellow', 'orange', 'red'];

// Scaling ceiling for session cost dynamic color (mirrors COST_FULL_USD in src-tauri/src/statusline.rs).
const COST_FULL_USD = 30;

// TRUNC_MIN/MAX bounds - rationale: docs/feat/statusline-customizer.md § Truncate widths.
const TRUNC_MIN = 3;
const TRUNC_MAX = 12;
// Per-field ceiling. Not uniform on purpose: a directory or branch name needs more room before it stops being recognisable than a user or account name does. Anything absent uses TRUNC_MAX.
const TRUNC_MAX_FOR = { cwd: 15, branch: 15 };
function truncMax(name) { return TRUNC_MAX_FOR[name] || TRUNC_MAX; }
// Which fields own a truncate width, and the cfg.trunc key each one reads.
const TRUNC_FOR = { identity_user: 'user', identity_host: 'host', account: 'account', cwd: 'cwd', git_branch: 'branch' };

// Field dependencies (DEPENDS) - see docs/feat/statusline-customizer.md § Gated fieldset (non-destructive master switch).
const DEPENDS = {
  effort: 'model',
  rate_reset_5h: 'rate_limits_5h',
  rate_reset_7d: 'rate_limits_7d',
  cache_pct: 'cache',
  cache_tokens: 'cache',
};

// Zebra picker restricted to the neutral ramp - see docs/feat/statusline-customizer.md § Zebra background, not a separator.
const ZEBRA_SHADES = [16, ...Array.from({ length: 12 }, (_, i) => 232 + i)];
// xterm-256 greyscale: 232 is #080808 and each step adds 10. 16 is pure #000000.
function zebraHex(n) {
  if (n === 16) return '#000000';
  const v = 8 + (n - 232) * 10;
  const h = v.toString(16).padStart(2, '0');
  return `#${h}${h}${h}`;
}

function setTrunc(name, raw) {
  cfg.trunc[name] = clampTrunc(raw, defaultLocalConfig().trunc[name], name);
}
function truncTitle(name) {
  const extra = name === 'account'
    ? ' - applied after the domain is stripped ("lva@akitao.com" is cut to "lva" first)'
    : '';
  return `Truncate to N characters (${TRUNC_MIN}-${truncMax(name)})${extra}`;
}

const COLORS = STATUSLINE_COLORS;
const HEX = Object.fromEntries(COLORS.map(c => [c.key, c.hex]));
const COLOR_BY_KEY = Object.fromEntries(COLORS.map(c => [c.key, c]));
// The two colors the script hardwires for anything the user cannot pick: WHITE for a label, GREY for a secondary reading. Same records as the picker's white/grey, so they stay one value.
const WHITE_HEX = HEX.white;
const GREY_HEX = HEX.grey;
// Tooltip for the current-color button: the code the terminal will print, plus what clicking does.
function colorTitle(key) {
  const c = COLOR_BY_KEY[fieldByKey(key)?.color];
  return c ? `${swatchTitle(c)} - click to change` : 'Pick a color';
}
// The six color-editable fields (COLOR_EDITABLE) - see docs/feat/statusline-customizer.md § Color doctrine.
const COLOR_EDITABLE = new Set(['identity_user', 'identity_host', 'account', 'cwd', 'model', 'git_branch']);
function isColorEditable(key) { return COLOR_EDITABLE.has(key); }

// Declarative row/group catalog; group.sep matches block assembly in src-tauri/src/statusline-unified.sh.
const GROUPS = [
  {
    id: 'identity',
    keys: ['identity_user', 'identity_host'],
    sep: '@',
    // Grey, not white: the '@' is glue between two names, not a label of its own (matches script, which prints it with $GREY).
    sepColor: GREY_HEX,
    lines: [
      { segments: [
        { controls: [{ type: 'master', keys: ['identity_user', 'identity_host'], label: 'id' }] },
        { tight: true, controls: [
          { type: 'toggle', key: 'identity_user', label: 'user' },
          { type: 'color', key: 'identity_user' },
          { type: 'trunc', name: 'user' },
          { type: 'parts', items: ['@'] },
          { type: 'toggle', key: 'identity_host', label: 'host' },
          { type: 'color', key: 'identity_host' },
          { type: 'trunc', name: 'host' },
        ] },
      ] },
    ],
  },
  {
    id: 'model',
    keys: ['model', 'effort'],
    // Empty, not a space: model and effort read as one token ("Opus4.8med"). Every pixel counts.
    sep: '',
    lines: [
      { segments: [
        { controls: [
          { type: 'toggle', key: 'model', label: 'model' },
          { type: 'color', key: 'model' },
        ] },
        { controls: [
          { type: 'toggle', key: 'effort', label: 'effort' },
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
        { type: 'color', key: 'context' },
        { type: 'parts', items: ['token in+out', '/Max Token'] },
      ] },
    ],
  },
  {
    id: 'quota',
    keys: ['rate_limits_5h', 'rate_reset_5h', 'rate_limits_7d', 'rate_reset_7d'],
    sep: ' ',
    // AGY splits its quota into a Gemini pool and a 3P (Claude/GPT) pool; the script picks the pool matching the running model, so there is nothing to configure here.
    note: 'AGY: pool follows the selected model',
    lines: [
      { segments: [
        { controls: [
          { type: 'toggle', key: 'rate_limits_5h', label: '5h' },
          { type: 'parts', items: ['%'] },
          { type: 'color', key: 'rate_limits_5h' },
        ] },
        { spacer: true },
        { controls: [
          { type: 'toggle', key: 'rate_reset_5h', label: 'Reset ETA' },
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
          { type: 'toggle', key: 'rate_reset_7d', label: 'Reset ETA' },
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
    keys: ['cache', 'cache_pct', 'cache_tokens'],
    // The block leads with the '↬' glyph, which cache_pct prints itself - no extra group label.
    sep: ' ',
    lines: [
      { controls: [
        // A real gate field, not a UI-only master: the two readings are meaningless without the block itself, so the dependency is stored, not simulated in the component.
        { type: 'toggle', key: 'cache', label: 'Cache' },
        { type: 'toggle', key: 'cache_pct', label: '%' },
        { type: 'parts', items: ['hit/max Token'] },
        { type: 'toggle', key: 'cache_tokens', label: 'read' },
        { type: 'parts', items: ['Token'] },
      ] },
    ],
  },
];

// Statusline defaults SSoT (mirrors reference implementation in src-tauri/src/statusline-unified.sh).
function defaultLocalConfig() {
  return {
    fields: [
      // Pinned first - see docs/feat/statusline-customizer.md § The pinned cli_tag cluster.
      { key: 'cli_tag',        enabled: true,  color: '' },
      // Rendered inside the tag cluster, not as a block of its own - see the pinned row.
      { key: 'account',        enabled: true,  color: 'grey' },
      { key: 'identity_user',  enabled: true,  color: 'white' },
      { key: 'identity_host',  enabled: true,  color: 'white' },
      { key: 'cwd',            enabled: true,  color: 'magenta' },
      { key: 'model',          enabled: true,  color: 'cyan' },
      { key: 'effort',         enabled: true,  color: 'grey' },
      { key: 'context',        enabled: true,  color: 'white' },
      // cache sits directly after context: the two are read together ("how many tokens, and how much of it came from cache"). Order fixed in docs/ref/statusline-unified-spec.md §8.2.
      { key: 'cache',          enabled: true,  color: '' },
      { key: 'cache_pct',      enabled: true,  color: 'grey' },
      { key: 'cache_tokens',   enabled: false, color: 'grey' },
      { key: 'rate_limits_5h', enabled: true,  color: 'white' },
      { key: 'rate_reset_5h',  enabled: true,  color: 'grey' },
      { key: 'rate_limits_7d', enabled: true,  color: 'white' },
      { key: 'rate_reset_7d',  enabled: true,  color: 'grey' },
      { key: 'session',        enabled: true,  color: 'grey' },
      { key: 'git_branch',     enabled: true,  color: 'magenta' },
      { key: 'ram',            enabled: true,  color: 'grey' },
    ],
    // Uneven by design: a wide calm green band, then bands that narrow as the number gets urgent. Must stay identical to color_for_pct() in src-tauri/src/statusline-unified.sh.
    thresholds: { green: 20, yellow: 51, orange: 75, red: 90 },
    // Per-field trunc defaults - range and account domain-stripping: docs/feat/statusline-customizer.md § Truncate widths (docs/ref/statusline-unified-spec.md §8.2).
    trunc: { account: 4, user: 5, host: 6, cwd: 12, branch: 10 },
    // Zebra shades - see docs/feat/statusline-customizer.md § Zebra background, not a separator.
    zebra: { a: 16, b: 235 },
    // On by default: a space either side of each block lets the two shades read as separate plates. Off, blocks butt directly together and the boundary is the shade change alone.
    separate: true,
    version: CONFIG_VERSION,
  };
}

// Same contract the shell script clamps to: floor of 3 everywhere, per-field ceiling. A non-numeric or out-of-range value falls back rather than propagating into a generated script.
function clampTrunc(val, fallback, name) {
  const n = Math.round(Number(val));
  if (!Number.isFinite(n)) return fallback;
  return Math.min(truncMax(name), Math.max(TRUNC_MIN, n));
}

// Loads saved config; drops unknown keys and falls back to defaults without migration.
function loadCfg() {
  const def = defaultLocalConfig();
  let saved = null;
  try {
    const raw = JSON.parse(localStorage.getItem(STORAGE_KEY) || 'null');
    if (raw && Array.isArray(raw.fields) && raw.version === CONFIG_VERSION) saved = raw;
  } catch { /* fall through to default */ }
  if (!saved) return def;

  const defByKey = new Map(def.fields.map(f => [f.key, f]));
  const fields = saved.fields
    .filter(f => f && defByKey.has(f.key))
    .map(f => ({ key: f.key, enabled: !!f.enabled, color: f.color || defByKey.get(f.key).color }));
  // A field the saved config never heard of drops in at its default position, not at the bottom.
  const have = new Set(fields.map(f => f.key));
  def.fields.forEach((f, i) => { if (!have.has(f.key)) fields.splice(i, 0, { ...f }); });
  // cli_tag and account are the pinned cluster and have no draggable row, so the UI could never fix them if a stored order put them elsewhere - force them back to the front.
  for (const key of ['account', 'cli_tag']) {
    const idx = fields.findIndex(f => f.key === key);
    if (idx > 0) fields.unshift(fields.splice(idx, 1)[0]);
  }

  const thresholds = { ...def.thresholds };
  for (const k of Object.keys(def.thresholds)) {
    if (typeof saved.thresholds?.[k] === 'number') thresholds[k] = saved.thresholds[k];
  }
  const trunc = { ...def.trunc };
  for (const k of Object.keys(def.trunc)) trunc[k] = clampTrunc(saved.trunc?.[k], def.trunc[k], k);
  const separate = typeof saved.separate === 'boolean' ? saved.separate : def.separate;
  const zebra = { ...def.zebra };
  for (const k of ['a', 'b']) {
    if (ZEBRA_SHADES.includes(saved.zebra?.[k])) zebra[k] = saved.zebra[k];
  }
  return { version: CONFIG_VERSION, fields, thresholds, trunc, zebra, separate };
}

const cfg = reactive(loadCfg());
const busy = ref(false);
const status = reactive({ msg: '', err: false });
const results = ref([]);
const selectedHosts = ref(['local']);
// Target CLIs for Apply; both enabled by default and persisted separately from field config.
const selectedTargets = ref(loadTargets());
const hostStatus = ref({});

// The two CLI tags drawn inside a host chip. Built here rather than in the template so the chip stays one line and the "only what the host actually has" rule lives in exactly one place.
const CLI_TAGS = [
  { cli: 'CC', name: 'Claude Code', present: 'cc_present', configured: 'cc_configured' },
  { cli: 'AG', name: 'AGY CLI', present: 'ag_present', configured: 'ag_configured' },
];
function cliTags(host) {
  const s = hostStatus.value[host];
  if (!s) return [];
  return CLI_TAGS.filter(t => s[t.present]).map(t => ({
    cli: t.cli,
    configured: s[t.configured],
    title: s[t.configured]
      ? `${t.name}: statusline installed and wired up`
      : `${t.name}: found, but no statusline wired up yet`,
  }));
}

function loadTargets() {
  try {
    const raw = JSON.parse(localStorage.getItem(TARGETS_KEY) || 'null');
    if (Array.isArray(raw) && raw.length) return raw.filter(t => t === 'ag' || t === 'cc');
  } catch { /* fall through to default */ }
  return ['ag', 'cc'];
}

watch(cfg, () => localStorage.setItem(STORAGE_KEY, JSON.stringify(cfg)), { deep: true });
watch(selectedTargets, v => localStorage.setItem(TARGETS_KEY, JSON.stringify(v)), { deep: true });

// The tooltip names the files Apply will actually write, so it can never promise a Claude Code write while the "claude" box is unticked.
const applyTitle = computed(() => {
  if (!selectedTargets.value.length) return 'Tick at least one target (agy / claude) above';
  const what = [];
  // Both targets get the same two writes: the script, and the settings key that points the CLI at it. Naming only the script would under-promise - and hide the half whose absence is silent.
  if (selectedTargets.value.includes('cc')) what.push('~/.claude/statusline-command.sh + patch settings.json');
  if (selectedTargets.value.includes('ag')) what.push('~/.gemini/antigravity-cli/statusline.sh + patch settings.json');
  return `Write ${what.join(' and ')} on every checked host`;
});

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
  // No backend default to fetch: defaultLocalConfig() is the standard - see docs/feat/statusline-customizer.md § SSOT: the Vue component.
  Object.assign(cfg, loadCfg());
  resetMasterChecked();
  checkAndAutoInstall();
});

// Auto-repairs Claude Code only - see docs/feat/statusline-customizer.md § Rollout to hosts.
async function checkAndAutoInstall() {
  try {
    const list = await invoke('check_statusline_status', { hosts: hostOptions.value });
    const map = {};
    for (const s of list) map[s.host] = s;
    hostStatus.value = map;

    const toInstall = list.filter(s => s.cc_present && !s.cc_configured).map(s => s.host);
    if (!toInstall.length) return;

    const config = JSON.parse(JSON.stringify(cfg));
    // The probe checks Claude Code only, so the repair must write the same target - rationale: docs/feat/statusline-customizer.md § Rollout to hosts.
    const autoResults = await invoke('apply_statusline_config', {
      config,
      targetHosts: toInstall,
      selectedTargets: ['cc']
    });
    for (const r of autoResults) {
      if (hostStatus.value[r.host]) hostStatus.value[r.host].cc_configured = r.ok;
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
  // Gate off does not cascade into children - non-destructive fieldset rule: docs/feat/statusline-customizer.md § Gated fieldset (non-destructive master switch).
}
// Master checkbox is independent, one-way state - see docs/feat/statusline-customizer.md § What each field prints.
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
// Locked while its DEPENDS parent is inactive - see docs/feat/statusline-customizer.md § Gated fieldset (non-destructive master switch).
function controlBlocked(control) {
  const dep = DEPENDS[control.key];
  return !!dep && !fieldActive(dep);
}
// Enabled AND reachable. This - not `enabled` - is what the preview and the generator must ask.
function fieldActive(key) {
  if (!fieldEnabled(key)) return false;
  const dep = DEPENDS[key];
  return dep ? fieldActive(dep) : true;
}
const allFieldsEnabled = computed(() => cfg.fields.every(f => f.enabled));
function toggleAllFields() {
  const val = !allFieldsEnabled.value;
  cfg.fields.forEach(f => setFieldEnabled(f.key, val));
  resetMasterChecked();
}

// A line is authored either as one flat `controls` list or as explicit `segments` when part of it is its own visual unit - normalized here so the template renders one shape, not two.
function segmentsOf(line) { return line.segments || [{ controls: line.controls }]; }

// Color swatch control (extreme-narrow rule); one picker open at a time keyed by field.
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

// Ladder shown wherever a field's color is computed rather than chosen - see docs/feat/statusline-customizer.md § The dynamic-color ladder.
const ladder = computed(() => [
  { ...CALM, from: 0, to: cfg.thresholds.green },
  ...TIER_KEYS.map((key, i) => ({
    ...TIER_BY_KEY[key],
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

// Projects cfg.fields into display rows; pinned `cli_tag` & `account` are excluded from draggable list.
const rows = computed(() => {
  // `account` joins cli_tag in the pinned row: the script prints it glued to the tag, sharing that cluster's own background, so it has no position of its own to drag it to.
  const consumed = new Set(['cli_tag', 'account']);
  const result = [];
  for (const f of cfg.fields) {
    if (consumed.has(f.key)) continue;
    const group = GROUPS.find(g => g.keys.includes(f.key));
    if (group) {
      result.push({ id: group.id, keys: group.keys, lines: group.lines, note: group.note });
      group.keys.forEach(k => consumed.add(k));
    } else {
      consumed.add(f.key);
      result.push({
        id: f.key,
        keys: [f.key],
        lines: [{ controls: [
          { type: 'toggle', key: f.key, label: CATALOG[f.key]?.label || f.key },
          { type: 'color', key: f.key },
          // Bare rows that carry a name the statusline has to cut short get their width input here, from the same TRUNC_FOR map the preview reads - not a per-field special case.
          ...(TRUNC_FOR[f.key] ? [{ type: 'trunc', name: TRUNC_FOR[f.key] }] : []),
        ] }],
      });
    }
  }
  return result;
});

// ---- drag-and-drop reorder (native HTML5 DnD, row-granularity - a group always moves whole) ----
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

// Rebuilds cfg.fields order from a row-id order, reusing field objects in place - see docs/feat/statusline-customizer.md § Fields, groups, blocks.
function applyRowOrder(idOrder) {
  const byId = new Map(rows.value.map(r => [r.id, r]));
  const newFields = [];
  // rows excludes cli_tag entirely, so idOrder never mentions it - re-prepend it explicitly or a drag reorder would silently drop it from cfg.fields instead of just leaving it in place.
  const pinned = fieldByKey('cli_tag');
  if (pinned) newFields.push(pinned);
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
    const hostResults = await invoke('apply_statusline_config', {
      config,
      targetHosts: [...selectedHosts.value],
      selectedTargets: [...selectedTargets.value]
    });
    results.value = hostResults;
    const failed = hostResults.filter(r => !r.ok);
    status.err = failed.length > 0;
    status.msg = failed.length === 0
      ? `Applied to ${hostResults.length} host${hostResults.length === 1 ? '' : 's'}. Restart Claude Code (or open a new terminal) to see it.`
      : `${hostResults.length - failed.length}/${hostResults.length} hosts applied - see details above.`;
  } catch (e) {
    status.msg = String(e);
    status.err = true;
  } finally {
    busy.value = false;
  }
}

// ---- live preview: mirrors rendering rules in src-tauri/src/statusline-unified.sh against fixed sample data ----
const SAMPLE = {
  user: 'aki', host: 'akitao', account: 'user-a@example.com', cwd: 'Aki-Dev-Sync', model: 'sonnet 5', effort: 'med',
  ctxPct: 72, ctxUsed: '134.4k', ctxMax: '1M',
  rate5h: 42, rate7d: 92, rate5hEta: '1h12m', rate7dEta: '2d3h',
  duration: '12m', linesAdded: 122, linesRemoved: 52, cost: '$8.40', costUsd: 8.4,
  gitBranch: 'master',
  cachePct: 78, cacheRead: '12.4k',
  ramPct: 24,
};

function esc(s) { return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;'); }
// `bold` is for the session +/- line count glyphs - too small to read at normal weight.
function span(text, hex, bold) { return `<span style="color:${hex}${bold ? ';font-weight:700' : ''}">${esc(text)}</span>`; }
// Only the CLI tag has a background - it's a fixed brand color, not a value-driven foreground - so it gets its own tiny helper rather than overloading `span`'s (hex, bold) signature.
function bgSpan(text, bgHex, fgHex) { return `<span style="background:${bgHex};color:${fgHex};padding:0 3px">${esc(text)}</span>`; }
// Same walk as color_for_pct() in the script, against the same thresholds and now the same colors.
function tierHex(pct) {
  const t = cfg.thresholds;
  for (let i = TIER_KEYS.length - 1; i >= 0; i--) {
    if (pct >= t[TIER_KEYS[i]]) return TIER_BY_KEY[TIER_KEYS[i]].hex;
  }
  return CALM.hex;
}
// The preview truncates exactly where the script does, so the number the user types is visibly the number that lands on the terminal.
function cut(text, name) { return String(text).slice(0, cfg.trunc[name]); }

function renderField(key) {
  const color = (k, fallback) => HEX[cfg.fields.find(f => f.key === k)?.color] || fallback;
  switch (key) {
    case 'cli_tag': {
      // Tag + account cluster, one preview stands in for both CLIs - see docs/feat/statusline-customizer.md § The pinned cli_tag cluster.
      const acct = fieldEnabled('account')
        // Domain first, width second - the same order the script uses. Cutting the raw address instead would show "user@" and teach the wrong lesson about what the number does.
        ? bgSpan(' ' + cut(SAMPLE.account.split('@')[0], 'account'), '#d0d0d0', HEX[fieldByKey('account')?.color] || GREY_HEX)
        : '';
      return bgSpan('CC', '#EEEEEE', '#FF8700') + acct + ' ' + bgSpan('AG', '#EEEEEE', '#0087FF') + acct;
    }
    case 'identity_user':
      return span(cut(SAMPLE.user, 'user'), color('identity_user', WHITE_HEX));
    case 'identity_host':
      return span(cut(SAMPLE.host, 'host'), color('identity_host', WHITE_HEX));
    case 'account':
      // Printed by the cli_tag case above, never as a block of its own.
      return '';
    case 'cwd':
      return span(cut(SAMPLE.cwd, 'cwd'), color('cwd', HEX.magenta));
    case 'model':
      return span(SAMPLE.model, color('model', HEX.cyan));
    case 'effort':
      return span(SAMPLE.effort, GREY_HEX);
    case 'context':
      // Token count is colored against 200k scale without displaying percentage.
      return span('ctx', WHITE_HEX) + span(SAMPLE.ctxUsed, tierHex(SAMPLE.ctxPct)) +
        span('/', GREY_HEX) + span(SAMPLE.ctxMax, GREY_HEX);
    case 'rate_limits_5h': {
      const resetOn = fieldEnabled('rate_reset_5h');
      const eta = resetOn ? span(SAMPLE.rate5hEta, GREY_HEX) : '';
      return span('5h:', WHITE_HEX) + span(`${SAMPLE.rate5h}%`, tierHex(SAMPLE.rate5h)) + eta;
    }
    case 'rate_limits_7d': {
      const resetOn = fieldEnabled('rate_reset_7d');
      const eta = resetOn ? span(SAMPLE.rate7dEta, GREY_HEX) : '';
      return span('7d:', WHITE_HEX) + span(`${SAMPLE.rate7d}%`, tierHex(SAMPLE.rate7d)) + eta;
    }
    case 'rate_reset_5h':
    case 'rate_reset_7d':
      return '';
    case 'cache_pct':
      // Static grey in the script, not on the ladder - a high cache hit rate is good news and must not shout in red.
      return span('↬', WHITE_HEX) + span(`${SAMPLE.cachePct}%`, GREY_HEX);
    case 'cache_tokens':
      return span(SAMPLE.cacheRead, GREY_HEX);
    case 'session': {
      let out = span('ss', WHITE_HEX) + span(SAMPLE.duration, color('session', GREY_HEX));
      if (SAMPLE.linesAdded || SAMPLE.linesRemoved) {
        out += ' ' + span(`+${SAMPLE.linesAdded}`, TIER_BY_KEY.green.hex, true) + span('/', GREY_HEX) + span(`-${SAMPLE.linesRemoved}`, TIER_BY_KEY.red.hex, true);
      }
      out += ' ' + span(SAMPLE.cost, tierHex(Math.min(100, SAMPLE.costUsd / COST_FULL_USD * 100)));
      return out;
    }
    case 'git_branch':
      return span(cut(SAMPLE.gitBranch, 'branch'), color('git_branch', HEX.magenta));
    case 'ram':
      // Static grey like the script: whole-machine RAM is context, not something the ladder should escalate about mid-session.
      return span('⚅', WHITE_HEX) + span(`${SAMPLE.ramPct}%`, color('ram', GREY_HEX));
    default:
      return '';
  }
}

// Mirrors the script's block assembly (block_order) - see docs/feat/statusline-customizer.md § Fields, groups, blocks.
const previewHtml = computed(() => {
  const blocks = [];
  const done = new Set();
  for (const f of cfg.fields) {
    if (!fieldActive(f.key) || done.has(f.key)) continue;
    const group = GROUPS.find(g => g.keys.includes(f.key));
    if (!group) {
      const html = renderField(f.key);
      // `tag` marks the cluster that paints its own background - see the zebra loop below.
      if (html) blocks.push({ html, tag: f.key === 'cli_tag' });
      continue;
    }
    group.keys.forEach(k => done.add(k));
    const inner = group.keys.filter(fieldActive).map(renderField).filter(Boolean);
    // `sep === undefined` vs explicit '' - rationale: docs/feat/statusline-customizer.md § Fields, groups, blocks.
    const raw = group.sep === undefined ? ' ' : group.sep;
    const sep = raw && raw !== ' ' ? span(raw, group.sepColor || WHITE_HEX) : raw;
    if (inner.length) {
      const joined = inner.join(sep);
      blocks.push({ html: joined, tag: false });
    }
  }
  const parts = blocks.filter(b => b && b.html);
  if (!parts.length) return '<span style="color:#374151">(no fields enabled)</span>';
  // Tag cluster is excluded from the zebra alternation - see docs/feat/statusline-customizer.md § The pinned cli_tag cluster.
  let zi = 0;
  return parts
    .map(({ html, tag }) => {
      // The tag cluster paints its own background and is never padded - it has to stay glued to the account name beside it.
      if (tag) return `<span class="pv-block">${html}</span>`;
      const bg = zebraHex(zi++ % 2 === 0 ? cfg.zebra.a : cfg.zebra.b);
      // A NON-BREAKING space, not a plain one: leading/trailing whitespace inside an inline-block is collapsed away by the HTML layout, which made this toggle look like it did nothing.
      const pad = cfg.separate ? '\u00a0' : '';
      return `<span class="pv-block" style="background:${bg}">${pad}${html}${pad}</span>`;
    })
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
  /* Prevents modal widening on narrow windows with unbroken preview blocks. */
  overflow-x: auto;
}

.preview-line {
  font-family: 'JetBrains Mono', 'Fira Code', ui-monospace, monospace;
  font-size: 11px;
  margin: 0;
  /* Wrapping is controlled by .pv-block, not by the text itself - see previewHtml. */
  white-space: normal;
  word-break: normal;
}

/* Statusline block: inline-block + nowrap breaks between blocks; zebra background provides separation. */
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
.fields-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}
.toggle-all-btn {
  font-size: 10px;
  font-weight: 600;
  text-transform: none;
  letter-spacing: 0;
  color: #94a3b8;
  background: none;
  border: 1px solid #374151;
  border-radius: 4px;
  padding: 1px 7px;
  cursor: pointer;
  transition: color 0.15s, border-color 0.15s;
}
.toggle-all-btn:hover {
  color: #e2e8f0;
  border-color: #6b7280;
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

/* Drop-position indicator: accent line on target edge without extra DOM. */
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

.pinned-row { margin-bottom: 3px; }
.pin-icon {
  color: #475569;
  font-size: 10px;
  align-self: center;
  padding: 0 2px;
  flex-shrink: 0;
  cursor: default;
}

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

/* Custom checkbox shape: hollow when off, drawn check when on to avoid confusion with color swatches. */
.ctl-toggle input[type="checkbox"],
.host-chip input[type="checkbox"],
.sep-toggle input[type="checkbox"],
.target-check-item input[type="checkbox"] {
  appearance: none;
  -webkit-appearance: none;
  width: 12px;
  height: 12px;
  margin: 0;
  flex-shrink: 0;
  border: 1px solid rgba(255, 255, 255, 0.3);
  border-radius: 2px;
  background: transparent;
  position: relative;
  cursor: pointer;
}

.ctl-toggle input[type="checkbox"]:checked,
.host-chip input[type="checkbox"]:checked,
.sep-toggle input[type="checkbox"]:checked,
.target-check-item input[type="checkbox"]:checked {
  background: #d97757;
  border-color: #d97757;
}

/* The check itself: two borders rotated into a tick, so it needs no font or icon file. */
.ctl-toggle input[type="checkbox"]:checked::after,
.host-chip input[type="checkbox"]:checked::after,
.sep-toggle input[type="checkbox"]:checked::after,
.target-check-item input[type="checkbox"]:checked::after {
  content: '';
  position: absolute;
  left: 3px;
  top: 0.5px;
  width: 3px;
  height: 6px;
  border: solid #fff;
  border-width: 0 1.5px 1.5px 0;
  transform: rotate(45deg);
}

.ctl-toggle input[type="checkbox"]:disabled { opacity: 0.4; cursor: not-allowed; }

.ctl-master { font-weight: 600; }

/* Blocked toggle visual: dimmed and not actionable when parent field is inactive (CLAUDE.md). */
.ctl-blocked { cursor: not-allowed; opacity: 0.4; }
.ctl-blocked input { cursor: not-allowed; }

/* Non-interactive field format preview tokens. */
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

/* Flexible clamped gap between left and right control clusters in a row. */
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

/* 2x4 swatch grid filled column-first for compact width. */
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

/* Separate switch in section label per extreme-narrow rule (CLAUDE.md). */
.sep-toggle {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  margin-left: 10px;
  font-size: 9px;
  font-weight: 400;
  text-transform: none;
  color: #64748b;
  cursor: pointer;
}
.sep-toggle input { margin: 0; }

/* Recessed row note for fixed behaviours without dedicated toggles. */
.row-note {
  font-size: 9px;
  color: #475569;
  font-style: italic;
  line-height: 1.2;
}

/* Inline per-field truncate width input. */
.ctl-trunc {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  color: #64748b;
  font-size: 10px;
  cursor: pointer;
  flex-shrink: 0;
}
.ctl-trunc input {
  width: 30px;
  padding: 1px 2px;
  background: #0f172a;
  border: 1px solid #1e293b;
  border-radius: 3px;
  color: #cbd5e1;
  font-size: 10px;
  text-align: center;
}
.ctl-trunc input:focus {
  outline: none;
  border-color: #334155;
}

/* Zebra background pickers: two rows of neutral shades only. */
.zebra-rows {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.zebra-row {
  display: flex;
  align-items: center;
  gap: 3px;
}
.zebra-name {
  width: 32px;
  color: #64748b;
  font-size: 10px;
}
.zebra-swatch {
  width: 16px;
  height: 16px;
  border: 1px solid #1e293b;
  border-radius: 3px;
  cursor: pointer;
  padding: 0;
  flex-shrink: 0;
}
.zebra-swatch.sel {
  border-color: #38bdf8;
  box-shadow: 0 0 0 1px #38bdf8;
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

/* TransitionGroup reorder animation for draggable rows. */
.row-move { transition: transform 0.2s ease; }
.row-enter-active, .row-leave-active { transition: opacity 0.15s ease; }
.row-enter-from, .row-leave-to { opacity: 0; }
.row-leave-active { position: absolute; }

.host-list { display: flex; flex-wrap: wrap; gap: 4px; }

/* Square, tight, data-dense - a pill with 10px side padding spent most of its width on air. */
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

/* Per-CLI state inside chip: lit = renders line, hollow amber = unwired, distinct from swatch. */
.cli-tag {
  font-size: 8px;
  line-height: 1;
  font-weight: 700;
  letter-spacing: 0.03em;
  padding: 2px 3px;
  border-radius: 2px;
  border: 1px solid transparent;
}
.cli-tag.on { color: #0f172a; background: #4ade80; }
.cli-tag.off { color: #fbbf24; border-color: rgba(251, 191, 36, 0.55); background: transparent; }

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

.modal-title-wrap {
  display: inline-flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
}

.modal-title-text {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.header-target-selector {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  margin-left: 8px;
}

.target-label {
  font-size: 11px;
  font-weight: 500;
  color: #94a3b8;
}

.target-check-item {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 11px;
  font-weight: 600;
  color: #cbd5e1;
  cursor: pointer;
}

/* Narrow mode (SSoT 700px, main.css): scoped padding overrides global narrow rule. */
@media (max-width: 700px) {
  .modal-body   { padding: 10px 10px 8px; }
  .modal-footer { padding: 8px 10px 10px; }

  /* Wrapping flex title opts out of modal h2 single-line clamp (main.css) to preserve target selector. */
  .modal-title-wrap {
    white-space: normal;
    overflow: visible;
    gap: 8px;
  }
  .header-target-selector { margin-left: 0; gap: 8px; }

  /* Reduced flexible gap on narrow screens to prevent control clipping. */
  .ctl-spacer { flex-basis: 6px; }
}
</style>
