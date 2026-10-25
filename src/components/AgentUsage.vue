<template>
  <div class="agent-usage-card">
    <!-- Header shell for Antigravity and Claude Code. -->
    <div class="agent-header" :class="{ 'claudecode-custom-header': isCc }">
      <div class="agent-title-group">
        <div class="agent-icon-wrapper" :class="isCc ? null : currentSourceType">
          <img v-if="isCc" src="/claude-icon.png" class="agent-img-icon icon-glow" alt="Claude Code" />
          <i
            v-else-if="currentSourceType === 'cli'"
            class="fa-solid fa-terminal agent-cli-icon clickable-icon"
            @click="handleIconClick"
            title="CLI Active"
          ></i>
          <img
            v-else-if="currentSourceType === 'desktop' || currentSourceType === 'desktop_cli'"
            src="/antigravity-app-icon.png"
            class="agent-img-icon ag-desktop-icon clickable-icon"
            alt="AG"
            @click="handleIconClick"
            title="AG Active"
          />
          <img
            v-else
            src="/antigravity-icon.png"
            class="agent-img-icon ag-ide-icon clickable-icon"
            alt="IDE"
            @click="handleIconClick"
            title="IDE Active"
          />
        </div>
        <div :class="isCc ? 'agent-name-row' : 'agent-name'">
          <span :class="isCc ? 'agent-name u-narrow-hide' : 'u-narrow-hide'">{{ displayName }}</span>
          <span v-if="planBadge" class="agent-plan-badge" :class="isCc ? 'claude' : 'ag'">
            {{ planBadge }}
          </span>
          <span
            v-if="data?.email"
            class="ag-account-wrap"
            role="button"
            tabindex="0"
            title="Switch account view"
          >
            <span class="agent-account ag-account-trigger">
              <template v-if="showEmail">
                {{ truncEmail(data.email) }}
              </template>
              <template v-else>
                <span class="email-prefix">{{ getEmailPrefix(data.email) }}</span><span class="email-blurred-fixed">••••••••</span>
              </template>
            </span>
            <div class="ag-account-menu" :class="popupPosition" @click.stop>
              <button
                v-for="acc in accounts"
                :key="acc.accountKey || acc.email"
                class="ag-account-item"
                :class="{ 'is-current': isAccountCurrent(acc) }"
                @click="pickAccount(acc)"
              >
                <span class="ag-account-left">
                  <template v-if="!isCc">
                    <i v-if="acc.sourceType === 'cli'" class="fa-solid fa-terminal ag-account-type-icon cli" title="CLI"></i>
                    <img v-else-if="acc.sourceType === 'desktop' || acc.sourceType === 'desktop_cli'" src="/antigravity-app-icon.png" class="ag-account-type-icon desktop" alt="" title="AG" />
                    <img v-else src="/antigravity-icon.png" class="ag-account-type-icon ide" alt="" title="IDE" />
                  </template>
                  <span v-if="showEmail" class="ag-account-email u-select-text">{{ acc.email }}</span>
                  <span v-else class="ag-account-email-masked">
                    <span class="email-prefix">{{ getEmailPrefix(acc.email) }}</span><span class="email-blurred-fixed">••••••••</span>
                  </span>
                </span>
                <span class="ag-account-metacol">
                  <span
                    v-if="isAccountLive(acc)"
                    class="ag-live-dot"
                    title="Live account"
                  ></span>
                  <span class="ag-account-time">{{ formatAgo(acc.fetchedAt) }}</span>
                </span>
              </button>
            </div>
          </span>
          <button v-if="data?.email" class="btn-eye-inline" @click.stop="$emit('toggle-email')" :title="showEmail ? 'Hide email' : 'Show email'" :aria-label="showEmail ? 'Hide email' : 'Show email'">
            <i class="fa-regular" :class="showEmail ? 'fa-eye' : 'fa-eye-slash'"></i>
          </button>
          <span v-if="ccOrgName" class="agent-org u-select-text" :class="{ 'email-blurred': !showEmail }">· {{ ccOrgName }}</span>
        </div>
      </div>

      <!-- Cache/age note badge and reload button. -->
      <div class="agent-status-badges">
        <span v-if="ageNote" class="cached-note" :title="ageNote.title">{{ ageNote.text }}</span>
        <button class="btn-ui-action btn-reload" @click="$emit('retry')" :disabled="loading || sourceOff" :title="sourceOff ? (locked ? 'Monitor only for native Claude - Proxy mode active' : 'Monitoring off') : loading ? 'Loading data' : 'Refresh Data'" :aria-label="loading ? 'Loading data' : 'Refresh Data'">
          <RefreshRing :interval-s="sourceOff ? 0 : refreshSettings.usage_interval_s" :refresh-key="drainKey" :overlay="true" />
          <i class="fa-solid" :class="loading ? 'fa-circle-notch fa-spin' : 'fa-rotate-right'"></i>
        </button>
      </div>
    </div>

    <div class="agent-body">
      <div v-if="uiStatus.kind === 'error'" class="usage-error">
        <span><i class="fa-solid fa-triangle-exclamation mr-1"></i> {{ uiStatus.text }}</span>
      </div>

      <!-- Skeleton bars loader -->
      <div v-else-if="uiStatus.kind === 'loading'" class="usage-circles-skeleton">
        <div v-if="agentId === 'claudecode'" class="cc-skeleton-block">
          <div class="skeleton-bar-header"></div>
          <div class="skeleton-bar-track"></div>
          <div class="skeleton-bar-time"></div>
        </div>
        <div v-else class="ag-bars-skeleton">
          <div v-for="i in 3" :key="i" class="ag-skeleton-pool">
            <div class="skeleton-pool-title"></div>
            <div class="skeleton-pool-windows">
              <div v-for="j in (i === 3 ? 1 : 2)" :key="j" class="skeleton-win-col">
                <div class="skeleton-win-meta"></div>
                <div class="skeleton-win-track"></div>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- Off state takes priority over stale cached bars. -->
      <div v-else-if="uiStatus.kind === 'off'" class="usage-empty">
        <i class="fa-solid" :class="uiStatus.icon"></i><br>
        <span>{{ uiStatus.text }}</span>
      </div>

      <div v-else-if="uiStatus.kind === 'empty'" class="usage-empty">
        <i class="fa-solid" :class="uiStatus.icon"></i><br>
        <span>{{ uiStatus.text }}</span>
      </div>

      <div v-else-if="uiStatus.kind === 'data'" class="usage-bars-container">
        <!-- Render Claude Code generic rate limit progress bars (docs/ref/claude-quota-buckets.md) -->
        <template v-if="agentId === 'claudecode'">
          <div class="cc-bars-block">
            <!-- Rate limits buckets dynamically mapped from rate_limits. -->
            <div
              v-for="b in ccBuckets"
              :key="b.key"
              class="cc-usage-bar"
              :class="{ 'is-muted': b.muted }"
              :title="b.muted ? 'Dimmed - the 7-day pool is full (100%), so this reading no longer changes anything' : null"
            >
              <div class="cc-bar-header">
                <span class="cc-bar-label">{{ b.label }}</span>
                <span class="cc-bar-pct" :class="b.colorClass">{{ b.pct }}%</span>
              </div>
              <div class="cc-progress-track">
                <div class="cc-progress-fill" :class="b.colorClass" :style="{ width: (b.pct || 0) + '%' }"></div>
              </div>
              <div class="cc-reset-line" :class="{ 'is-na': !b.resetsAt }">
                <template v-if="b.resetLine.val">
                  <span class="time-label">Reset </span><span class="time-val">{{ b.resetLine.val }}</span>
                  <span v-if="b.resetLine.abs" class="time-abs"> ({{ b.resetLine.abs }})</span>
                </template>
                <span v-else class="time-label">{{ b.resetLine.label }}</span>
              </div>
            </div>
            <div v-if="isCached" class="cc-waiting-line">Waiting for next Claude Code session</div>
          </div>
        </template>

        <!-- Render Antigravity compact pool bars (3 pools) -->
        <template v-else-if="agentId === 'antigravity'">
          <div class="ag-bars-block">
            <div
              v-for="pool in agPools"
              :key="pool.key"
              class="ag-pool-card"
            >
              <div class="ag-pool-header">
                <span class="ag-pool-title">{{ pool.label }}</span>
              </div>
              <div class="ag-pool-windows">
                <div
                  v-for="w in pool.windows"
                  :key="w.key"
                  class="ag-win-col"
                  :class="{ 'is-muted': w.muted }"
                  :title="w.tooltip"
                >
                  <div class="ag-win-meta">
                    <div class="ag-win-left">
                      <span class="ag-win-tag">{{ w.label }}</span>
                      <span class="ag-pct-val" :class="w.colorClass">{{ w.pctText }}</span>
                    </div>
                    <div class="ag-win-right" :class="{ 'is-na': !w.resetsAt }">
                      <span class="ag-time-val">{{ w.resetCountdown }}</span>
                    </div>
                  </div>
                  <div class="ag-track">
                    <div
                      class="ag-track-fill"
                      :class="w.colorClass"
                      :style="{ width: w.fillPct + '%' }"
                    ></div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </template>
      </div>
    </div>
  </div>
</template>

<script>
// Module-scoped clock for relative timestamps across mounted usage cards.
import { ref as _ref } from 'vue';

export const agoNow = _ref(Math.floor(Date.now() / 1000));
let agoTimer = null;
let agoHolders = 0;

export function retainAgoClock() {
  if (agoHolders++ === 0) {
    agoTimer = setInterval(() => { agoNow.value = Math.floor(Date.now() / 1000); }, 10000);
  }
}
export function releaseAgoClock() {
  if (--agoHolders === 0 && agoTimer) {
    clearInterval(agoTimer);
    agoTimer = null;
  }
}
</script>

<script setup>
// @docs docs/arch/usage-claudecode.md
// @docs docs/arch/usage-antigravity.md
// @docs docs/plan/done/1.16.1-ag-usage.md
import { computed, ref, watch, onMounted, onUnmounted } from 'vue';
import { invoke } from '../utils/tauri';
import { rightDockActive } from '../composables/useRightDockLayout';
import RefreshRing from './RefreshRing.vue';
import { refreshSettings } from '../store/refreshStore';

const props = defineProps({
  agentId: String,
  agentName: String,
  data: Object,
  loading: Boolean,
  error: String,
  // Unix seconds the displayed reading was written (null when no mtime reported).
  dataAt: { type: Number, default: null },
  isCached: { type: Boolean, default: false },
  cachedAt: { type: Number, default: null },
  showEmail: { type: Boolean, default: true },
  sourceOff: { type: Boolean, default: false },
  // True when sourceOff is locked (e.g. Proxy mode active).
  locked: { type: Boolean, default: false },
  // Multi-account view: accounts seen on this host, the slot's pinned one, and the live ones
  accounts: { type: Array, default: () => [] },
  viewingEmail: { default: null },
  activeEmail: { default: null },
  activeEmails: { type: Object, default: () => new Set() },
  popupPosition: { type: String, default: 'popup-pos-tl' }
});

// SSoT for body view priority: error > off > loading > empty > data.
const uiStatus = computed(() => {
  if (props.error) return { kind: 'error', text: props.error };
  if (props.sourceOff) {
    return {
      kind: 'off',
      icon: 'fa-power-off mb-1',
      text: props.locked ? 'Monitor only for native Claude - Proxy mode active' : 'Monitoring off',
    };
  }
  if (props.loading && !props.data) return { kind: 'loading' };
  if (!props.data) {
    return {
      kind: 'empty',
      icon: props.agentId === 'antigravity' ? 'fa-circle-info mb-1' : 'fa-hourglass-empty mb-1',
      text: props.agentId === 'antigravity' ? 'Not connected - open & sign in to Antigravity to monitor' : 'No data - waiting for next session',
    };
  }
  return { kind: 'data' };
});

const emit = defineEmits(['retry', 'select-account', 'toggle-email']);

function isAccountCurrent(acc) {
  const key = acc.accountKey || acc.email;
  if (props.viewingEmail) {
    return key === props.viewingEmail || acc.email === props.viewingEmail;
  }
  return key === props.activeEmail || acc.email === props.activeEmail;
}

function pickAccount(acc) {
  const key = acc.accountKey || acc.email;
  emit('select-account', key);
}
const currentSourceType = computed(() => {
  return props.data?.sourceType || 'ide';
});

const isCc = computed(() => props.agentId === 'claudecode');

const displayName = computed(() => {
  if (props.agentId !== 'antigravity') return props.agentName;
  if (currentSourceType.value === 'cli') return 'CLI';
  if (currentSourceType.value === 'ide') return 'IDE';
  return 'AG';
});

function getEmailPrefix(email) {
  if (!email) return '';
  return email.slice(0, 4);
}

function isAccountLive(acc) {
  if (!acc || props.isCached) return false;
  const key = acc.accountKey || acc.email;
  const email = acc.email;
  const inActive = props.activeEmail === key || props.activeEmail === email ||
    (props.activeEmails?.has(key) || props.activeEmails?.has(email));
  if (!inActive) return false;
  if (acc.fetchedAt && (agoNow.value - acc.fetchedAt) > 600) return false;
  return true;
}

// Mirrors the 700px @container main-view breakpoint; the right dock caps dashboard-left under it, so window width alone under-truncates.
const windowNarrow = ref(typeof window !== 'undefined' && window.innerWidth <= 700);
function updateWindowNarrow() { windowNarrow.value = window.innerWidth <= 700; }
const isNarrow = computed(() => rightDockActive.value || windowNarrow.value);
function truncEmail(email) {
  const max = isNarrow.value ? 7 : 12;
  return email.length > max ? email.slice(0, max) + '…' : email;
}
onMounted(() => {
  window.addEventListener('resize', updateWindowNarrow);
});
onUnmounted(() => {
  window.removeEventListener('resize', updateWindowNarrow);
});

// Antigravity 2.1.1+ Groups & Buckets detection
const quotaSummaryGroups = computed(() => {
  if (props.agentId !== 'antigravity' || !props.data?.quotaSummary) return null;
  return props.data.quotaSummary.groups || null;
});

// The pools agy reports today, in display order; any other group renders after them under its own name.
const AG_POOLS = {
  gemini: { rank: 0, label: 'Gemini' },
  claude: { rank: 1, label: 'Claude/GPT' },
  argon: { rank: 2, label: 'Argon 4' },
};

function agPoolKey(displayName) {
  const lower = (displayName || '').toLowerCase();
  // Before gemini: the vendor names this pool "Gemini 4 Argon".
  if (lower.includes('argon')) return 'argon';
  if (lower.includes('gemini')) return 'gemini';
  if (lower.includes('claude') || lower.includes('3p') || lower.includes('gpt')) return 'claude';
  return null;
}

function agWindow({ key, label, name, fraction, resetTime, muted = false, disabled = false }, now) {
  const pct = fraction !== undefined ? Math.round((1 - fraction) * 100) : null;
  const resetsAt = resetTime ? Math.floor(new Date(resetTime).getTime() / 1000) : null;
  const reset = formatResetLine(resetsAt, now);
  const parts = [name, pct !== null ? `${pct}% used` : 'N/A'];
  if (reset.val) parts.push(`Resets in ${reset.val} (${reset.abs})`);
  else if (resetsAt) parts.push('Reset passed - refreshing');
  else if (pct !== null) parts.push('Dynamic Window');
  if (muted) parts.push(disabled ? 'Dimmed: disabled' : 'Dimmed: 7D pool full');
  return {
    key,
    label,
    resetsAt,
    muted,
    pctText: pct !== null ? `${pct}%` : 'N/A',
    colorClass: muted ? 'color-muted' : pctColorClass(pct),
    fillPct: Math.max(0, Math.min(100, pct || 0)),
    resetCountdown: reset.val ?? reset.label,
    tooltip: parts.join(' · '),
  };
}

const agPools = computed(() => {
  if (props.agentId !== 'antigravity') return [];
  const now = agoNow.value;
  const groups = quotaSummaryGroups.value;

  if (groups?.length) {
    const taken = new Set();
    return groups.map((g, idx) => {
      // A second group matching a known pool (say, a new Gemini tier) gets its own key and the vendor's name, never a duplicate card.
      const known = agPoolKey(g.displayName);
      const own = known && !taken.has(known);
      if (own) taken.add(known);
      const label = own ? AG_POOLS[known].label : (g.displayName || '').replace(/\s+models$/i, '').trim();
      const buckets = Array.isArray(g.buckets) ? g.buckets : [];
      const is5h = b => b.window === '5h' || !!b.bucketId?.includes('5h');
      const weekly = buckets.find(b => b.window === 'weekly' || b.bucketId?.includes('weekly'));
      const weeklyFull = weekly?.remainingFraction !== undefined && Math.round((1 - weekly.remainingFraction) * 100) >= 100;
      const windows = [...buckets].sort((a, b) => is5h(b) - is5h(a)).map(b => {
        const short = is5h(b);
        const disabled = b.disabled === true;
        return agWindow({
          key: b.bucketId || b.id || (short ? '5H' : '7D'),
          label: short ? '5H' : '7D',
          name: b.displayName || `${label} ${short ? '5-Hour' : 'Weekly'} Limit`,
          fraction: b.remainingFraction,
          resetTime: b.resetTime,
          muted: short && (weeklyFull || disabled),
          disabled,
        }, now);
      });
      return { key: own ? known : `group-${idx}`, label, rank: own ? AG_POOLS[known].rank : 100 + idx, windows };
    }).sort((a, b) => a.rank - b.rank);
  }

  // Readings without groups (Antigravity before 2.1.1, or an older offline cache entry) carry one figure per model.
  const models = props.data?.models;
  if (!models?.length) return [];
  const legacyPool = (m, key) => m && {
    key,
    label: AG_POOLS[key].label,
    windows: [agWindow({ key: `${key}-5h`, label: '5H', name: m.label || AG_POOLS[key].label, fraction: m.remainingPercentage, resetTime: m.resetTime }, now)],
  };
  return [
    legacyPool(models.find(m => m.label?.toLowerCase().includes('gemini')), 'gemini'),
    legacyPool(models.find(m => !m.label?.toLowerCase().includes('gemini')), 'claude'),
  ].filter(Boolean);
});

// Refetch once when a shown reset passes. Only while bars are on screen: an error view includes a halted monitor, which must stay halted until the user acts (usageMonitor.js haltPolling).
const shownResets = computed(() => (isCc.value ? ccBuckets.value : agPools.value.flatMap(p => p.windows)).map(w => w.resetsAt).filter(Boolean));
watch(agoNow, (now, prev) => {
  if (uiStatus.value.kind === 'data' && shownResets.value.some(t => prev <= t && now > t)) emit('retry');
});

function formatResetLine(resetsAt, nowSec) {
  if (!resetsAt) return { label: 'N/A', val: null, abs: '' };
  const diff = resetsAt - nowSec;
  if (diff <= 0) return { label: 'ready', val: null, abs: '' };
  const days = Math.floor(diff / 86400);
  const hours = Math.floor((diff % 86400) / 3600);
  const minutes = Math.floor((diff % 3600) / 60);
  let val = '';
  if (days > 0) val = `${days}d${hours}h`;
  else if (hours > 0) val = `${hours}h${minutes}m`;
  else val = minutes > 0 ? `${minutes}m` : '<1m';
  const d = new Date(resetsAt * 1000);
  const month = d.toLocaleString('en-US', { month: 'short' });
  const hh = String(d.getHours()).padStart(2, '0');
  const mm = String(d.getMinutes()).padStart(2, '0');
  return { label: null, val, abs: `${hh}:${mm} ${month}${d.getDate()}` };
}

function pctColorClass(pct) {
  if (pct === null) return 'color-na';
  if (pct <= 70) return 'color-safe';
  if (pct <= 90) return 'color-warning';
  return 'color-danger';
}

// Refcounted relative timestamp clock across all mounted cards.
onMounted(retainAgoClock);
onUnmounted(releaseAgoClock);

// Gates whether the header prints the data-age text: older than 10m, unknown mtime, or past 5h reset.
const shouldShowAge = computed(() => {
  if (!props.data) return false;
  // No mtime reported: age is unknown.
  if (!props.dataAt) return true;
  if (agoNow.value - props.dataAt > 600) return true;
  const fh = props.data?.rate_limits?.five_hour;
  return !!(fh && fh.resets_at > 0 && agoNow.value > fh.resets_at);
});

// Relative age formatter: <1m / Nm / NhNm / Nh.
function formatAgo(sec) {
  if (!sec) return '';
  const diffS = agoNow.value - sec;
  if (diffS < 60) return '<1m';
  const mins = Math.floor(diffS / 60);
  if (mins < 60) return `${mins}m`;
  const hrs = Math.floor(mins / 60);
  const rem = mins % 60;
  return rem > 0 ? `${hrs}h${rem}m` : `${hrs}h`;
}

// Absolute HH:MM for tooltips.
function formatAbsTime(sec) {
  if (!sec) return '';
  const d = new Date(sec * 1000);
  const hh = String(d.getHours()).padStart(2, '0');
  const mm = String(d.getMinutes()).padStart(2, '0');
  return `${hh}:${mm}`;
}

const cachedAgo = computed(() => (props.cachedAt ? `${formatAgo(props.cachedAt)} ago` : ''));
const cachedAbsTime = computed(() => formatAbsTime(props.cachedAt));

// Claude Code header age.
const dataAgo = computed(() => (props.dataAt ? `${formatAgo(props.dataAt)} ago` : ''));
const dataAbsTime = computed(() => formatAbsTime(props.dataAt));

// Header age note for active/cached AG accounts or aged Claude Code data.
const ageNote = computed(() => {
  if (props.agentId === 'antigravity') {
    return props.isCached ? { text: cachedAgo.value, title: `Data cached at ${cachedAbsTime.value}` } : null;
  }
  return (shouldShowAge.value && dataAgo.value) ? { text: dataAgo.value, title: `Data from ${dataAbsTime.value}` } : null;
});

// Claude Code bucket display order.
const CC_BUCKET_ORDER = [
  'five_hour',
  'seven_day',
  'seven_day_opus',
  'seven_day_sonnet',
  'seven_day_fable',
  'seven_day_mythos',
  'seven_day_oauth_apps',
];
const CC_BUCKET_LABELS = {
  five_hour: '5-Hour',
  seven_day: '7-Day',
  seven_day_opus: '7-Day Opus',
  seven_day_sonnet: '7-Day Sonnet',
  seven_day_fable: '7-Day Fable',
  seven_day_mythos: '7-Day Mythos',
  seven_day_oauth_apps: '7-Day OAuth apps',
};

function titleCaseWords(s) {
  return s.split('_').filter(Boolean).map(w => w.charAt(0).toUpperCase() + w.slice(1)).join(' ');
}

// Format readable labels for known and dynamic rate limit bucket keys.
function ccBucketLabel(key) {
  if (CC_BUCKET_LABELS[key]) return CC_BUCKET_LABELS[key];
  if (key.startsWith('seven_day_')) return `7-Day ${titleCaseWords(key.slice('seven_day_'.length))}`;
  if (key.startsWith('five_hour_')) return `5-Hour ${titleCaseWords(key.slice('five_hour_'.length))}`;
  return titleCaseWords(key);
}

function ccBucketRank(key) {
  const i = CC_BUCKET_ORDER.indexOf(key);
  return i === -1 ? CC_BUCKET_ORDER.length : i;
}

const ccBuckets = computed(() => {
  const rl = props.data?.rate_limits;
  if (!rl || typeof rl !== 'object') return [];
  const keys = Object.keys(rl).filter(k => {
    const v = rl[k];
    // Skip empty or non-numeric bucket entries.
    return v && typeof v === 'object' && typeof v.used_percentage === 'number' && Number.isFinite(v.used_percentage);
  });
  keys.sort((a, b) => {
    const ra = ccBucketRank(a), rb = ccBucketRank(b);
    return ra !== rb ? ra - rb : a.localeCompare(b);
  });
  return keys.map(key => {
    const entry = rl[key];
    const pct = Math.round(entry.used_percentage);
    // five_hour keeps its two historical quirks; every other bucket is plain.
    const isFiveHour = key === 'five_hour';
    const resetsAt = isFiveHour ? cc5hResetsAt.value : (entry.resets_at ?? null);
    const muted = isFiveHour && cc5hMutedBy7d.value;
    return {
      key,
      label: ccBucketLabel(key),
      pct,
      resetsAt,
      muted,
      colorClass: muted ? 'color-muted' : pctColorClass(pct),
      resetLine: formatResetLine(resetsAt, agoNow.value),
    };
  });
});

const cc5hResetsAt = computed(() => {
  const r5 = props.data?.rate_limits?.five_hour?.resets_at ?? null;
  // Treat 5h reset as unknown when equal to 7d idle boundary.
  const r7 = props.data?.rate_limits?.seven_day?.resets_at ?? null;
  if (r5 && r7 && r5 === r7) return null;
  return r5;
});
// Dim 5H bar when shared 7D pool is full (100%).
const cc5hMutedBy7d = computed(() => cc7dPct.value !== null && cc7dPct.value >= 100);

// Shared 7D percentage for 5H dimming evaluation.
const cc7dPct = computed(() => { const v = props.data?.rate_limits?.seven_day?.used_percentage; return v != null ? Math.round(v) : null; });

// Org name: skip Anthropic's auto-generated default "email's Organization"
const ccOrgName = computed(() => {
  const org = props.data?.orgName;
  if (!org) return null;
  if (props.data?.email && org === `${props.data.email}'s Organization`) return null;
  return org;
});

// SVG ring - restarts on refresh complete or when interval setting changes
const drainKey = ref(0);
watch(() => props.loading, (newVal, oldVal) => {
  if (oldVal === true && newVal === false) drainKey.value++;
});
watch(() => refreshSettings.value.usage_interval_s, () => {
  drainKey.value++;
});

const claudeTierDisplay = computed(() => {
  if (!props.data) return '';

  if (props.data.rateLimitTier && props.data.rateLimitTier !== 'Unknown') {
    let tier = props.data.rateLimitTier;
    let cleaned = tier.replace(/^(default_)?claude_/, '').replace(/_/g, ' ');

    if (cleaned.toLowerCase() === 'ai' && props.data.subscriptionType && props.data.subscriptionType !== 'Unknown') {
      return props.data.subscriptionType.charAt(0).toUpperCase() + props.data.subscriptionType.slice(1);
    }

    return cleaned.split(' ').map(word => {
      if (word.toLowerCase() === 'max') return 'Max';
      if (word.toLowerCase() === 'pro') return 'Pro';
      if (/^\d+x$/i.test(word)) return word.toLowerCase();
      return word.charAt(0).toUpperCase() + word.slice(1);
    }).join(' ');
  }

  if (props.data.subscriptionType && props.data.subscriptionType !== 'Unknown') {
    return props.data.subscriptionType.charAt(0).toUpperCase() + props.data.subscriptionType.slice(1);
  }

  return '';
});

const planBadge = computed(() => (isCc.value
  ? claudeTierDisplay.value
  : props.data?.userTier?.name?.replace(/\b(Google|AI)\b/gi, '').trim()) || '');

async function handleIconClick() {
  if (props.agentId === 'antigravity') {
    try {
      await invoke("macos_open", { args: ["-a", "Antigravity"] });
    } catch (e) {
      console.error("Failed to open Antigravity:", e);
    }
  }
}
</script>

<style scoped>
.agent-usage-card {
  background: transparent;
  border: none;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 200px;
  flex: 1;
  box-shadow: none;
}


.agent-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  width: 100%;
  margin-bottom: 2px;
}

.claudecode-custom-header {
  margin-bottom: 4px;
}

.agent-name-row {
  display: flex;
  align-items: center;
  gap: 6px;
}

.agent-title-group {
  display: flex;
  align-items: center;
  gap: 2px;
}

.agent-icon-wrapper {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
}

.agent-icon-wrapper.ide .ag-ide-icon {
  filter: drop-shadow(0 0 2px rgba(34, 211, 238, 0.5));
}

.agent-cli-icon {
  font-size: 13px;
  color: #c084fc;
  filter: drop-shadow(0 0 2px rgba(192, 132, 252, 0.5));
}

.agent-img-icon {
  width: 18px;
  height: 18px;
  border-radius: 3px;
  object-fit: contain;
}

.agent-name {
  font-size: 12px;
  font-weight: 700;
  color: var(--text-light);
  display: flex;
  align-items: center;
  gap: 4px;
}

.agent-account {
  font-size: 10px;
  color: var(--text-darker);
  font-weight: 500;
}

.btn-eye-inline {
  background: transparent;
  border: none;
  color: var(--text-darker);
  cursor: pointer;
  padding: 0 2px;
  font-size: 9px;
  line-height: 1;
  opacity: 0.4;
  transition: opacity 0.15s ease, color 0.15s ease;
}
.btn-eye-inline:hover {
  opacity: 1;
  color: var(--text-muted);
}

.email-blurred {
  filter: blur(3px);
  user-select: none;
  pointer-events: none;
  transition: filter 0.2s;
}

/* Account-switch dropdown (anchored under the email) */
.ag-account-wrap {
  position: relative;
  cursor: pointer;
}
.ag-account-trigger {
  transition: color 0.15s ease;
}
.ag-account-wrap:hover .ag-account-trigger:not(.email-blurred) {
  color: var(--accent-cyan);
}
.ag-account-menu {
  position: absolute;
  z-index: 50;
  min-width: 185px;
  max-width: 280px;
  padding: 4px;
  background: #1a1d23; /* solid - --bg-tertiary is near-transparent and would show through */
  border: 1px solid var(--border-color);
  border-radius: 6px;
  box-shadow: 0 6px 18px var(--shadow-ink);
  display: flex;
  flex-direction: column;
  gap: 1px;
  opacity: 0;
  visibility: hidden;
  pointer-events: none;
  transition: opacity 0.15s ease, visibility 0.15s ease;
}

.ag-account-wrap:hover .ag-account-menu {
  opacity: 1;
  visibility: visible;
  pointer-events: auto;
}
/* Smart 4-position pattern classes with transparent bridge to prevent hover loss */
.ag-account-menu::after {
  content: '';
  position: absolute;
  left: 0;
  right: 0;
  height: 8px;
}
.ag-account-menu.popup-pos-tl {
  top: calc(100% + 2px);
  bottom: auto;
  left: 0;
  right: auto;
  transform-origin: top left;
}
.ag-account-menu.popup-pos-tl::after,
.ag-account-menu.popup-pos-tr::after {
  top: -8px;
}
.ag-account-menu.popup-pos-tr {
  top: calc(100% + 2px);
  bottom: auto;
  right: 0;
  left: auto;
  transform-origin: top right;
}
.ag-account-menu.popup-pos-bl {
  bottom: calc(100% + 2px);
  top: auto;
  left: 0;
  right: auto;
  transform-origin: bottom left;
}
.ag-account-menu.popup-pos-bl::after,
.ag-account-menu.popup-pos-br::after {
  bottom: -8px;
}
.ag-account-menu.popup-pos-br {
  bottom: calc(100% + 2px);
  top: auto;
  right: 0;
  left: auto;
  transform-origin: bottom right;
}

.ag-account-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  width: 100%;
  padding: 4px 6px;
  background: transparent;
  border: none;
  border-radius: 4px;
  cursor: pointer;
  color: var(--text-muted);
  font-size: 10px;
  text-align: left;
  transition: background 0.12s ease;
}
.ag-account-item:hover {
  background: var(--bg-tertiary);
}
.ag-account-item.is-current {
  background: var(--accent-cyan-wash);
  color: var(--accent-cyan);
}

.ag-account-left {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  overflow: hidden;
}

.ag-account-type-icon {
  width: 12px;
  height: 12px;
  flex-shrink: 0;
  object-fit: contain;
}

.ag-account-type-icon.cli {
  font-size: 10px;
  color: #c084fc;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.ag-account-email {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ag-account-email-masked {
  display: inline-flex;
  align-items: center;
  white-space: nowrap;
}

.email-prefix {
  font-weight: 600;
}

.email-blurred-fixed {
  filter: blur(3.5px);
  user-select: none;
  pointer-events: none;
  display: inline-block;
  width: 3rem;
  overflow: hidden;
  vertical-align: middle;
  opacity: 0.65;
  margin-left: 1px;
}

.ag-account-metacol {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  flex-shrink: 0;
}
.ag-account-time {
  font-size: 9px;
  color: var(--text-darker);
}
.ag-live-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex-shrink: 0;
  background: var(--green-500);
  box-shadow: 0 0 4px rgba(34, 197, 94, 0.6);
}


.agent-org {
  font-size: 10px;
  color: var(--text-darker);
  font-weight: 400;
  opacity: 0.7;
}

.agent-plan-badge {
  background: rgba(6, 182, 212, 0.1);
  color: var(--cyan-200);
  padding: 1px 5px;
  border-radius: 4px;
  font-size: 9px;
  font-weight: 600;
  letter-spacing: 0.5px;
  line-height: 1.2;
}

.agent-plan-badge.claude {
  background: var(--brand-wash);
  color: var(--brand);
}

.agent-plan-badge.ag {
  background: rgba(37, 99, 235, 0.12);
  color: #93c5fd;
  border: 1px solid rgba(147, 197, 253, 0.2);
}

.agent-status-badges {
  display: flex;
  align-items: center;
  gap: 6px;
}

/* Data-age note, shared by AG's cached reading and Claude Code's aged reading: plain amber text, not a badge box (keeps the header narrow - no padding, border or background to pay for) */
.cached-note {
  font-size: 9px;
  font-weight: 600;
  color: rgba(251, 146, 60, 0.75);
}

.btn-ui-action {
  background: transparent;
  border: none;
  color: var(--text-muted);
  cursor: pointer;
  padding: 4px;
  border-radius: 4px;
  transition: background 0.2s ease, color 0.2s ease;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.btn-ui-action:hover {
  background: var(--bg-tertiary);
  color: var(--text-light);
}

.btn-ui-action:disabled {
  opacity: 0.4;
  cursor: not-allowed;
  pointer-events: none;
}

.agent-body {
  flex: 1;
  display: flex;
  flex-direction: column;
  justify-content: center;
}

.usage-bars-container {
  display: flex;
  flex-direction: column;
  justify-content: center;
  align-items: center;
  width: 100%;
}

/* Antigravity compact bar quota stack */
.ag-bars-block {
  display: flex;
  flex-direction: column;
  gap: 4px;
  width: 100%;
}

.ag-pool-card {
  display: flex;
  flex-direction: column;
  gap: 2px;
  width: 100%;
}

.ag-pool-header {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  line-height: 1.1;
}

.ag-pool-title {
  font-size: 9.5px;
  font-weight: 700;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.4px;
}

.ag-pool-windows {
  display: flex;
  gap: 6px;
  width: 100%;
}

.ag-win-col {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
  cursor: default;
}

.ag-win-col.is-muted {
  opacity: 0.45;
  transition: opacity 0.2s ease;
}

.ag-win-meta {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  line-height: 1;
}

.ag-win-left {
  display: inline-flex;
  align-items: baseline;
  gap: 4px;
}

.ag-win-tag {
  font-size: 9px;
  font-weight: 700;
  color: var(--text-muted);
}

.ag-pct-val {
  font-size: 9.5px;
  font-weight: 700;
}

.ag-pct-val.color-safe {
  color: var(--accent-green);
}

.ag-pct-val.color-warning {
  color: var(--accent-amber);
}

.ag-pct-val.color-danger {
  color: var(--accent-red);
}

.ag-pct-val.color-muted,
.ag-pct-val.color-na {
  color: var(--text-darker);
}

.ag-win-right {
  font-size: 8.5px;
  font-weight: 700;
  color: var(--text-light);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.ag-win-right.is-na {
  color: var(--text-darker);
}

.ag-track {
  width: 100%;
  height: 4px;
  background: var(--surface-hover);
  border-radius: 2px;
  overflow: hidden;
}

.ag-track-fill {
  height: 100%;
  border-radius: 2px;
  transition: width 0.5s ease;
}

.ag-track-fill.color-safe {
  background: var(--accent-green);
}

.ag-track-fill.color-warning {
  background: var(--accent-amber);
}

.ag-track-fill.color-danger {
  background: var(--accent-red);
}

.ag-track-fill.color-muted {
  background: var(--border-strong);
}

.ag-track-fill.color-na {
  background: var(--surface-hover);
}

/* Skeleton loader for AG compact bars */
.usage-circles-skeleton {
  display: flex;
  flex-direction: column;
  gap: 6px;
  width: 100%;
}

.ag-bars-skeleton {
  display: flex;
  flex-direction: column;
  gap: 4px;
  width: 100%;
}

.ag-skeleton-pool {
  display: flex;
  flex-direction: column;
  gap: 2px;
  width: 100%;
}

.skeleton-pool-title {
  width: 36px;
  height: 8px;
  border-radius: 2px;
  background: var(--surface-hover);
  animation: usage-pulse 1.5s infinite ease-in-out;
}

.skeleton-pool-windows {
  display: flex;
  gap: 6px;
  width: 100%;
}

.skeleton-win-col {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.skeleton-win-meta {
  width: 100%;
  height: 8px;
  border-radius: 2px;
  background: var(--surface-hover);
  animation: usage-pulse 1.5s infinite ease-in-out;
}

.skeleton-win-track {
  width: 100%;
  height: 4px;
  border-radius: 2px;
  background: var(--surface-hover);
  animation: usage-pulse 1.5s infinite ease-in-out;
}

.clickable-icon {
  cursor: pointer;
}

@keyframes usage-pulse {
  0% {
    opacity: 0.6;
  }

  50% {
    opacity: 0.3;
  }

  100% {
    opacity: 0.6;
  }
}

.usage-error {
  font-size: 11px;
  color: var(--accent-red);
  background: var(--accent-red-wash);
  padding: 8px;
  border-radius: 4px;
  width: 100%;
}

.usage-empty {
  text-align: center;
  font-size: 11px;
  color: var(--text-darker);
  padding: 12px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  width: 100%;
}

/* CC horizontal bars */
.cc-bars-block {
  display: flex;
  flex-direction: column;
  gap: 8px;
  width: 100%;
}

.cc-usage-bar {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.cc-bar-header {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
}

.cc-bar-label {
  font-size: 10px;
  font-weight: 700;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.cc-bar-pct {
  font-size: 12px;
  font-weight: 700;
}

.cc-bar-pct.color-safe {
  color: var(--accent-green);
}

.cc-bar-pct.color-warning {
  color: var(--accent-amber);
}

.cc-bar-pct.color-danger {
  color: var(--accent-red);
}

.cc-bar-pct.color-na {
  color: var(--text-darker);
}

/* Dimmed 5h indicator when pool 7d is full. */
.cc-bar-pct.color-muted {
  color: var(--text-darker);
}

.cc-usage-bar.is-muted {
  opacity: 0.45;
  transition: opacity 0.2s ease;
}

.cc-progress-track {
  height: 5px;
  background: var(--surface-hover);
  border-radius: 3px;
  overflow: hidden;
}

.cc-progress-fill {
  height: 100%;
  border-radius: 3px;
  transition: width 0.6s ease;
}

.cc-progress-fill.color-safe {
  background: var(--accent-green);
}

.cc-progress-fill.color-warning {
  background: var(--accent-amber);
}

.cc-progress-fill.color-danger {
  background: var(--accent-red);
}

.cc-progress-fill.color-na {
  background: var(--surface-hover);
}

.cc-progress-fill.color-muted {
  background: rgba(255, 255, 255, 0.22);
}

.cc-reset-line {
  font-size: 9px;
  font-weight: 500;
  color: var(--text-muted);
}

.cc-reset-line.is-na {
  color: var(--text-darker);
}

.cc-waiting-line {
  font-size: 9px;
  font-weight: 500;
  color: rgba(251, 146, 60, 0.75);
  text-align: center;
}

/* CC skeleton */
.cc-skeleton-block {
  display: flex;
  flex-direction: column;
  gap: 6px;
  width: 100%;
}

.skeleton-bar-header {
  height: 10px;
  width: 50px;
  border-radius: 2px;
  background: var(--bg-tertiary);
  animation: usage-pulse 1.5s infinite ease-in-out;
}

.skeleton-bar-track {
  height: 5px;
  width: 100%;
  border-radius: 3px;
  background: var(--bg-tertiary);
  animation: usage-pulse 1.5s infinite ease-in-out;
}

.skeleton-bar-time {
  height: 9px;
  width: 130px;
  border-radius: 2px;
  background: var(--bg-tertiary);
  animation: usage-pulse 1.5s infinite ease-in-out;
}

/* Reload button - circular, hosts the countdown ring */
.btn-reload {
  position: relative;
  overflow: visible;
  border-radius: 50% !important;
  border: 1px solid var(--border-color) !important;
}

.btn-reload:hover {
  border-color: var(--border-strong) !important;
}


/* Time parts used in CC bar reset line */
.cc-reset-line .time-label {
  color: var(--text-muted);
  font-weight: 500;
}

.cc-reset-line .time-val {
  color: rgba(255, 255, 255, 0.88);
  font-weight: 700;
}

.cc-reset-line .time-abs {
  color: var(--text-muted);
  font-weight: 400;
}

/* Responsive card shrink in narrow container (<=700px). */
@container main-view (max-width: 700px) {
  .agent-usage-card {
    min-width: 0;
  }
}
</style>
