<template>
  <div class="agent-usage-card">
    <!-- ONE header shell for both agents. Only the title group differs; the badge + reload group on
         the right was byte-identical in each branch and now exists once. -->
    <div class="agent-header" :class="{ 'claudecode-custom-header': agentId === 'claudecode' }">
      <!-- Claude Code title -->
      <div v-if="agentId === 'claudecode'" class="agent-title-group">
        <div class="agent-icon-wrapper">
          <img src="/claude-icon.png" class="agent-img-icon icon-glow" alt="Claude Code" />
        </div>
        <div class="agent-name-row">
          <span class="agent-name u-narrow-hide">{{ agentName }}</span>
          <span v-if="data && claudeTierDisplay" class="agent-plan-badge claude">
            {{ claudeTierDisplay }}
          </span>
          <!-- u-select-text (main.css): an account address is copy-worthy, so it opts out of the
               app-wide no-selection default - but `.email-blurred` is scoped, so it still outranks
               this class and a hidden address stays unselectable as well as unreadable. -->
          <span v-if="data?.email" class="agent-account u-select-text" :class="{ 'email-blurred': !showEmail }">{{ truncEmail(data.email) }}</span>
          <button v-if="data?.email" class="btn-eye-inline" @click.stop="$emit('toggle-email')" :title="showEmail ? 'Hide email' : 'Show email'" :aria-label="showEmail ? 'Hide email' : 'Show email'">
            <i class="fa-regular" :class="showEmail ? 'fa-eye' : 'fa-eye-slash'"></i>
          </button>
          <span v-if="ccOrgName" class="agent-org u-select-text" :class="{ 'email-blurred': !showEmail }">· {{ ccOrgName }}</span>
        </div>
      </div>

      <!-- Antigravity title (keep tiny logo + email) -->
      <div v-else class="agent-title-group">
        <div class="agent-icon-wrapper" :class="currentSourceType">
          <i
            v-if="currentSourceType === 'cli'"
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
        <div class="agent-info">
          <div class="agent-name">
            <span class="u-narrow-hide">{{ agDisplayName }}</span>
            <span v-if="data && data.userTier?.name" class="agent-plan-badge ag">
              {{ data.userTier.name.replace(/\b(Google|AI)\b/gi, '').trim() }}
            </span>
            <span
              v-if="data && data.email"
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
                    <i v-if="acc.sourceType === 'cli'" class="fa-solid fa-terminal ag-account-type-icon cli" title="CLI"></i>
                    <img v-else-if="acc.sourceType === 'desktop' || acc.sourceType === 'desktop_cli'" src="/antigravity-app-icon.png" class="ag-account-type-icon desktop" alt="" title="AG" />
                    <img v-else src="/antigravity-icon.png" class="ag-account-type-icon ide" alt="" title="IDE" />
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
            <button v-if="data && data.email" class="btn-eye-inline" @click.stop="$emit('toggle-email')" :title="showEmail ? 'Hide email' : 'Show email'" :aria-label="showEmail ? 'Hide email' : 'Show email'">
              <i class="fa-regular" :class="showEmail ? 'fa-eye' : 'fa-eye-slash'"></i>
            </button>
          </div>
        </div>
      </div>

      <!-- One age note (AG: cached-at, Claude Code: data-age), else the Stale badge for the one case
           where the age is genuinely unknown - never print a guessed number. -->
      <div class="agent-status-badges">
        <span v-if="ageNote" class="cached-note" :title="ageNote.title">{{ ageNote.text }}</span>
        <span v-else-if="stale" class="badge-stale" title="Data is older than 10 minutes">Stale</span>
        <button class="btn-ui-action btn-reload" :class="{ 'error-state': error, 'is-loading': loading }" @click="!loading && !sourceOff && $emit('retry')" :disabled="loading || sourceOff" :title="sourceOff ? (locked ? 'Monitor only for native Claude - Proxy mode active' : 'Monitoring off') : loading ? 'Loading data' : 'Refresh Data'" :aria-label="loading ? 'Loading data' : 'Refresh Data'">
          <RefreshRing :interval-s="sourceOff ? 0 : refreshSettings.usage_interval_s" :refresh-key="drainKey" :overlay="true" />
          <i class="fa-solid" :class="loading ? 'fa-circle-notch fa-spin' : 'fa-rotate-right'"></i>
        </button>
      </div>
    </div>

    <div class="agent-body">
      <div v-if="uiStatus.kind === 'error'" class="usage-error">
        <span><i class="fa-solid fa-triangle-exclamation mr-1"></i> {{ uiStatus.text }}</span>
      </div>

      <!-- Skeleton circles with fieldset wrapper for AG -->
      <div v-else-if="uiStatus.kind === 'loading'" class="usage-circles-skeleton">
        <div v-if="agentId === 'claudecode'" class="cc-skeleton-block">
          <div class="skeleton-bar-header"></div>
          <div class="skeleton-bar-track"></div>
          <div class="skeleton-bar-time"></div>
        </div>
        <div v-else class="circles-row">
          <fieldset class="zone-fieldset zone-gemini skeleton-zone">
            <legend class="zone-legend">Gemini</legend>
            <div class="zone-content">
              <div v-for="i in 2" :key="i" class="skeleton-circle-wrapper">
                <div class="skeleton-circle"></div>
                <div class="skeleton-text skeleton-text-15"></div>
                <div class="skeleton-text skeleton-text-25"></div>
              </div>
            </div>
          </fieldset>
          <fieldset class="zone-fieldset zone-claude skeleton-zone">
            <legend class="zone-legend">Claude/OSS</legend>
            <div class="zone-content">
              <div v-for="i in 2" :key="i" class="skeleton-circle-wrapper">
                <div class="skeleton-circle"></div>
                <div class="skeleton-text skeleton-text-15"></div>
                <div class="skeleton-text skeleton-text-25"></div>
              </div>
            </div>
          </fieldset>
        </div>
      </div>

      <!-- Off state (manual toggle OR locked-by-proxy) takes priority over stale cached bars  - 
           this must not require `!data` to trigger, otherwise flipping the source off leaves
           the last-fetched bars on screen until the next app launch. -->
      <div v-else-if="uiStatus.kind === 'off'" class="usage-empty">
        <i class="fa-solid" :class="uiStatus.icon"></i><br>
        <span>{{ uiStatus.text }}</span>
      </div>

      <div v-else-if="uiStatus.kind === 'empty'" class="usage-empty">
        <i class="fa-solid" :class="uiStatus.icon"></i><br>
        <span>{{ uiStatus.text }}</span>
      </div>

      <div v-else-if="uiStatus.kind === 'data'" class="usage-bars-container">
        <!-- Render Claude Code specific circular progress (2 circles) -->
        <template v-if="agentId === 'claudecode'">
          <div class="cc-bars-block">
            <!-- One bar per rate_limits bucket, whatever the map happens to contain. Anthropic has
                 already added model-scoped weeklies (seven_day_opus/_sonnet, _oauth_apps) and can add
                 more (e.g. seven_day_fable) with no client change; hardcoding two bars here was the
                 single place in this pipeline that dropped them. See docs/ref/claude-quota-buckets.md. -->
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

        <!-- Render Antigravity specific circular progress (4 circles bo trong 2 fieldset) -->
        <template v-else-if="agentId === 'antigravity'">
          <div class="circles-row">
            <fieldset class="zone-fieldset zone-gemini">
              <legend class="zone-legend">Gemini</legend>
              <div class="zone-content">
                <UsageCircle
                             label="Gemini 5-Hour Limit"
                             subLabel="5H"
                             :percentage="gemini5hData ? gemini5hData.percentage : null"
                             :resetsAt="gemini5hData ? gemini5hData.resetsAt : null"
                             :muted="gemini5hMutedByWeekly"
                             :muted-reason="MUTED_BY_WEEKLY_REASON"
                             @timeout="$emit('retry')" />
                <UsageCircle
                             label="Gemini Weekly Limit"
                             subLabel="7D"
                             :percentage="geminiWeeklyPct"
                             :resetsAt="geminiWeeklyBucket?.resetTime ? Math.floor(new Date(geminiWeeklyBucket.resetTime).getTime() / 1000) : null"
                             @timeout="$emit('retry')" />
              </div>
            </fieldset>

            <fieldset class="zone-fieldset zone-claude">
              <legend class="zone-legend">Claude/OSS</legend>
              <div class="zone-content">
                <UsageCircle
                             label="Claude & GPT 5-Hour Limit"
                             subLabel="5H"
                             :percentage="claude5hData ? claude5hData.percentage : null"
                             :resetsAt="claude5hData ? claude5hData.resetsAt : null"
                             :muted="claude5hMutedByWeekly"
                             :muted-reason="MUTED_BY_WEEKLY_REASON"
                             @timeout="$emit('retry')" />
                <UsageCircle
                             label="Claude & GPT Weekly Limit"
                             subLabel="7D"
                             :percentage="claudeWeeklyPct"
                             :resetsAt="claudeWeeklyBucket?.resetTime ? Math.floor(new Date(claudeWeeklyBucket.resetTime).getTime() / 1000) : null"
                             @timeout="$emit('retry')" />
              </div>
            </fieldset>
          </div>
        </template>
      </div>
    </div>
  </div>
</template>

<script>
// Module scope, so ONE clock serves every mounted card. Each card used to start its own 10s
// setInterval computing the identical integer - four slots meant four timers waking the webview
// four times as often for one number. Refcounted rather than left running: with no card mounted
// there is nothing to re-render.
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
import UsageCircle from './UsageCircle.vue';
import RefreshRing from './RefreshRing.vue';
import { refreshSettings } from '../store/refreshStore';

const props = defineProps({
  agentId: String,
  agentName: String,
  data: Object,
  loading: Boolean,
  error: String,
  // Unix seconds the displayed reading was written - the clock BOTH the age label and `stale` below are derived from. Null when the host reported no mtime.
  dataAt: { type: Number, default: null },
  isCached: { type: Boolean, default: false },
  cachedAt: { type: Number, default: null },
  showEmail: { type: Boolean, default: true },
  sourceOff: { type: Boolean, default: false },
  // True when sourceOff is forced (not user-toggled) - e.g. Claude Code local monitoring locked off while Proxy mode is active. Swaps the off-state message to explain why.
  locked: { type: Boolean, default: false },
  // True when showing a remote host's probe rather than this Mac's; readings render identically either way.
  remote: { type: Boolean, default: false },
  // AG-only multi-account view (unused for Claude Code)
  accounts: { type: Array, default: () => [] },
  viewingEmail: { default: null },
  activeEmail: { default: null },
  activeEmails: { type: Object, default: () => new Set() },
  popupPosition: { type: String, default: 'popup-pos-tl' }
});

// Single source of truth for which body view to render. Priority: error > loading > off
// (manual or locked) > empty (no data yet) > data. Off is checked before data on purpose  - 
// otherwise flipping a source off leaves the last-fetched bars on screen until relaunch.
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

const agDisplayName = computed(() => {
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
    (props.activeEmails && (props.activeEmails.has(key) || props.activeEmails.has(email)));
  if (!inActive) return false;
  if (acc.fetchedAt && (agoNow.value - acc.fetchedAt) > 600) return false;
  return true;
}

// Design lock: the header shows a truncated email (12 chars wide, 7 at the narrow breakpoint) to
// keep width stable when the active/cached account changes; the full email is shown in the
// dropdown rows untouched.
const isNarrow = ref(typeof window !== 'undefined' && window.innerWidth <= 700);
function updateIsNarrow() { isNarrow.value = window.innerWidth <= 700; }
function truncEmail(email) {
  const max = isNarrow.value ? 7 : 12;
  return email.length > max ? email.slice(0, max) + '…' : email;
}
onMounted(() => {
  window.addEventListener('resize', updateIsNarrow);
});
onUnmounted(() => {
  window.removeEventListener('resize', updateIsNarrow);
});

// Antigravity 2.1.1+ Groups & Buckets detection
const quotaSummaryGroups = computed(() => {
  if (props.agentId !== 'antigravity' || !props.data || !props.data.quotaSummary) return null;
  return props.data.quotaSummary.groups || null;
});

const geminiGroup = computed(() => {
  if (!quotaSummaryGroups.value) return null;
  return quotaSummaryGroups.value.find(g => g.displayName.toLowerCase().includes('gemini')) || null;
});

const claudeGroup = computed(() => {
  if (!quotaSummaryGroups.value) return null;
  return quotaSummaryGroups.value.find(g => g.displayName.toLowerCase().includes('claude') || g.displayName.toLowerCase().includes('3p')) || null;
});

const gemini5hBucket = computed(() => {
  if (!geminiGroup.value?.buckets) return null;
  return geminiGroup.value.buckets.find(b => b.window === '5h' || b.bucketId.includes('5h')) || null;
});

const geminiWeeklyBucket = computed(() => {
  if (!geminiGroup.value?.buckets) return null;
  return geminiGroup.value.buckets.find(b => b.window === 'weekly' || b.bucketId.includes('weekly')) || null;
});

const claude5hBucket = computed(() => {
  if (!claudeGroup.value?.buckets) return null;
  return claudeGroup.value.buckets.find(b => b.window === '5h' || b.bucketId.includes('5h')) || null;
});

const claudeWeeklyBucket = computed(() => {
  if (!claudeGroup.value?.buckets) return null;
  return claudeGroup.value.buckets.find(b => b.window === 'weekly' || b.bucketId.includes('weekly')) || null;
});

// ── A pool's own full 7d dims that same pool's 5h ────────────────────────────
// Once a pool's weekly quota reads 100% the pool is exhausted whatever its 5-hour figure says,
// so the 5h reading is noise competing for attention. Each flag is derived STRICTLY from its own
// group's weekly bucket and is handed only to that group's own 5H circle, so a full Gemini week
// can never dim the Claude/OSS pool (or the reverse) - the multi-entity blast-radius rule in
// CLAUDE.md. A null/absent weekly reading (old `models`-shaped payload, bucket missing) dims
// nothing: `null >= 100` must never be treated as "full".
const geminiWeeklyPct = computed(() =>
  geminiWeeklyBucket.value?.remainingFraction !== undefined
    ? Math.round((1 - geminiWeeklyBucket.value.remainingFraction) * 100)
    : null);
const claudeWeeklyPct = computed(() =>
  claudeWeeklyBucket.value?.remainingFraction !== undefined
    ? Math.round((1 - claudeWeeklyBucket.value.remainingFraction) * 100)
    : null);

const gemini5hMutedByWeekly = computed(() => geminiWeeklyPct.value !== null && geminiWeeklyPct.value >= 100);
const claude5hMutedByWeekly = computed(() => claudeWeeklyPct.value !== null && claudeWeeklyPct.value >= 100);

const MUTED_BY_WEEKLY_REASON = '7D pool full';

// Backward compatibility fallbacks
const geminiPool = computed(() => {
  if (props.agentId !== 'antigravity' || !props.data || !props.data.models) return null;
  return props.data.models.find(m => m.label.toLowerCase().includes('gemini'));
});

const claudeOssPool = computed(() => {
  if (props.agentId !== 'antigravity' || !props.data || !props.data.models) return null;
  return props.data.models.find(m => !m.label.toLowerCase().includes('gemini')) || null;
});

const gemini5hData = computed(() => {
  const bucket = gemini5hBucket.value;
  if (bucket) {
    return {
      percentage: bucket.remainingFraction !== undefined ? Math.round((1 - bucket.remainingFraction) * 100) : null,
      resetsAt: bucket.resetTime ? Math.floor(new Date(bucket.resetTime).getTime() / 1000) : null
    };
  }
  const oldPool = geminiPool.value;
  if (oldPool) {
    return {
      percentage: oldPool.remainingPercentage !== undefined ? Math.round((1 - oldPool.remainingPercentage) * 100) : null,
      resetsAt: oldPool.resetTime ? Math.floor(new Date(oldPool.resetTime).getTime() / 1000) : null
    };
  }
  return null;
});

const claude5hData = computed(() => {
  const bucket = claude5hBucket.value;
  if (bucket) {
    return {
      percentage: bucket.remainingFraction !== undefined ? Math.round((1 - bucket.remainingFraction) * 100) : null,
      resetsAt: bucket.resetTime ? Math.floor(new Date(bucket.resetTime).getTime() / 1000) : null
    };
  }
  const oldPool = claudeOssPool.value;
  if (oldPool) {
    return {
      percentage: oldPool.remainingPercentage !== undefined ? Math.round((1 - oldPool.remainingPercentage) * 100) : null,
      resetsAt: oldPool.resetTime ? Math.floor(new Date(oldPool.resetTime).getTime() / 1000) : null
    };
  }
  return null;
});

// CC bar helpers
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

const ccNow = ref(Math.floor(Date.now() / 1000));
let ccClockTimer = null;
onUnmounted(() => { if (ccClockTimer) clearInterval(ccClockTimer); });

// The clock for every relative age this card renders - AG's cached note and account dropdown, Claude Code's data age, and the derived `stale` below. Shared across all mounted cards (module scope above); deliberately separate from `ccNow`, which is a 60s countdown for the reset lines and carries a refetch side effect.
onMounted(retainAgoClock);
onUnmounted(releaseAgoClock);

// Derived, never stored. `stale` used to be a ref written ONLY on a successful fetch, so past a
// 5-hour boundary with no new Claude Code turn it stayed false forever while `dataAt` aged
// correctly: the card showed neither the age nor the badge, and drew a pre-reset percentage
// labelled "ready". Deriving it from the same clock the age label already ticks means no code path
// can forget to update it.
//
// The rule itself is unchanged (docs/arch/usage-claudecode.md §4): older than 10 minutes, or past
// the FIVE_HOUR reset - `five_hour` only, because that is the bucket
// `scripts/get-claudecode-usage.sh` writes its STALE_RESET contract against, and a weekly bucket
// rolls over far too rarely to be a freshness signal. Antigravity payloads carry no `rate_limits`
// at all, so that clause is simply never true for AG - no agent branch needed.
const stale = computed(() => {
  if (!props.data) return false;
  // No mtime reported: the age is unknown, which is what the pre-derivation code treated as infinitely old. The badge (not the age label) is the honest rendering of that.
  if (!props.dataAt) return true;
  if (agoNow.value - props.dataAt > 600) return true;
  const fh = props.data?.rate_limits?.five_hour;
  return !!(fh && fh.resets_at > 0 && agoNow.value > fh.resets_at);
});

// The one relative-age formatter in this card: `<1m` / `Nm` / `NhNm` / `Nh`, reactive via agoNow. Every age shown anywhere in the header goes through it, so AG and Claude Code cannot drift into two dialects of the same string.
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

// Absolute HH:MM for the tooltips - precision belongs there, not in the visible string.
function formatAbsTime(sec) {
  if (!sec) return '';
  const d = new Date(sec * 1000);
  const hh = String(d.getHours()).padStart(2, '0');
  const mm = String(d.getMinutes()).padStart(2, '0');
  return `${hh}:${mm}`;
}

const cachedAgo = computed(() => (props.cachedAt ? `${formatAgo(props.cachedAt)} ago` : ''));
const cachedAbsTime = computed(() => formatAbsTime(props.cachedAt));

// Claude Code header age. Renders only where the "Stale" badge used to: the badge was a yes/no with nothing actionable in it, and the age it was hiding is already known exactly (`dataAt`, the very mtime the stale rule compares).
const dataAgo = computed(() => (props.dataAt ? `${formatAgo(props.dataAt)} ago` : ''));
const dataAbsTime = computed(() => formatAbsTime(props.dataAt));

// The one age note in the header, for whichever agent this card is. AG announces WHEN a cached
// reading was taken (it is showing an offline account's last state); Claude Code announces HOW OLD
// the live reading is, and only once that is worth saying. Same slot, same styling, one element.
const ageNote = computed(() => {
  if (props.agentId === 'antigravity') {
    return props.isCached ? { text: cachedAgo.value, title: `Data cached at ${cachedAbsTime.value}` } : null;
  }
  return (stale.value && dataAgo.value) ? { text: dataAgo.value, title: `Data from ${dataAbsTime.value}` } : null;
});

// ── Claude Code buckets: rendered generically ────────────────────────────────
// `rate_limits` is an OPEN map, not a fixed pair. The whole pipeline below the UI (statusline hook →
// get-claudecode-usage.sh → agent_usage.rs → usageMonitor) already carries every key through
// untouched; only this component used to hardcode five_hour + seven_day. Adding a bucket must never
// require a code change here again.
//   Order: five_hour, seven_day, then the known model-scoped weeklies in the order below, then any
//   key we have never seen, alphabetically (so an unknown one lands at the bottom deterministically).
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

// Unknown keys still get a readable label instead of a raw snake_case key: a `seven_day_*` prefix is rewritten to the same "7-Day X" shape the known weeklies use, anything else is Title Cased.
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
    // A null/absent bucket means "not applicable to this plan", not an error - skip it silently rather than drawing an N/A bar for a limit the account does not have.
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
      resetLine: formatResetLine(resetsAt, ccNow.value),
    };
  });
});

const cc5hResetsAt = computed(() => {
  const r5 = props.data?.rate_limits?.five_hour?.resets_at ?? null;
  // Claude reports 5h.resets_at == 7d.resets_at when the 5h window sits idle at 0% with no
  // fresh API traffic to establish a real boundary. Drawing that far-future "5-day" reset is
  // misleading, so treat it as unknown → the reset line falls into its existing N/A state.
  const r7 = props.data?.rate_limits?.seven_day?.resets_at ?? null;
  if (r5 && r7 && r5 === r7) return null;
  return r5;
});
// Same rule as AG's per-pool dimming above, for Claude Code's SHARED pool: the shared `seven_day`
// at 100% makes the `five_hour` reading noise. Read the AG block for the full rationale. Scope is
// deliberate and stays narrow as buckets multiply: only the shared weekly dims, and only the shared
// session bar. A model-scoped weekly (seven_day_opus/_fable/…) neither dims another bar nor is
// dimmed by one - it is a separate pool, so a full Opus week says nothing about the 5h window.
// A null/absent 7d still dims nothing: `null >= 100` must never read as "full".
const cc5hMutedBy7d = computed(() => cc7dPct.value !== null && cc7dPct.value >= 100);

// P4 boundary trigger: CC had no client-side equivalent of AG's UsageCircle @timeout - the
// 5-hour bar could sit stale at "ready" past its reset with nothing prompting a refetch until
// the next STALE_RESET poll noticed server-side. Same wasPast/nowPast edge-detect pattern as
// UsageCircle.vue, wired to the existing @retry → refresh handler (AgentUsageSlot.vue).
// Deliberately still keyed to `five_hour` only after the generic-bucket refactor: the 5-hour window
// is the one that turns over often enough for a client-side boundary refetch to be worth anything,
// and it is the same bucket the script's STALE_RESET contract is written against. A weekly bucket
// rolling over is caught by the ordinary poll.
//
// A plain setInterval, NOT hostInterval: `ccNow` also drives the visible reset countdown, which must
// keep counting on a companion (seam P exempts cosmetic UI clocks). The producer half - the `retry`
// emit - is gated where every other producer path is, at `checkUsage()` itself, so on a phone this
// timer only repaints.
onMounted(() => {
  if (props.agentId === 'claudecode') {
    let wasPast = cc5hResetsAt.value > 0 && ccNow.value > cc5hResetsAt.value;
    ccClockTimer = setInterval(() => {
      ccNow.value = Math.floor(Date.now() / 1000);
      const nowPast = cc5hResetsAt.value > 0 && ccNow.value > cc5hResetsAt.value;
      if (nowPast && !wasPast) emit('retry');
      wasPast = nowPast;
    }, 60000);
  }
});

// Kept as a named computed (not folded into ccBuckets) because it is the input to the dimming rule above, which is about the SHARED weekly pool specifically - not "whatever weekly buckets exist".
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

.agent-info {
  display: flex;
  align-items: center;
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

/* AG account-switch dropdown (anchored under the email) */
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
  box-shadow: 0 6px 18px rgba(0, 0, 0, 0.5);
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
  background: rgba(255, 255, 255, 0.06);
}
.ag-account-item.is-current {
  background: rgba(0, 210, 255, 0.1);
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
  background: #22c55e;
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
  color: #a5f3fc;
  padding: 1px 5px;
  border-radius: 4px;
  font-size: 9px;
  font-weight: 600;
  letter-spacing: 0.5px;
  line-height: 1.2;
}

.agent-plan-badge.claude {
  background: rgba(217, 119, 87, 0.1);
  color: #D97757;
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

.badge-stale {
  font-size: 9px;
  font-weight: 600;
  text-transform: uppercase;
  background: var(--bg-tertiary);
  color: var(--text-darker);
  padding: 2px 6px;
  border-radius: 4px;
}

/* Data-age note, shared by AG's cached reading and Claude Code's stale reading: plain amber text, not a badge box (keeps the header narrow - no padding, border or background to pay for) */
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

.circles-row {
  display: flex;
  gap: 4px;
  /* Tight gap */
  justify-content: space-between;
  align-items: stretch;
  width: 100%;
  padding: 2px 0;
}

/* fieldset for grouping Antigravity */
.zone-fieldset {
  flex: 1;
  border: 1px dashed rgba(255, 255, 255, 0.18);
  /* Brighter dashed line */
  border-radius: 6px;
  padding: 4px 2px 4px 2px;
  /* Super compact padding */
  margin: 0;
  min-width: 0;
  box-sizing: border-box;
  transition: border-color 0.2s ease;
}

.zone-fieldset.zone-gemini {
  border-color: rgba(96, 165, 250, 0.35);
}

.zone-fieldset.zone-gemini:hover {
  border-color: rgba(96, 165, 250, 0.55);
}

.zone-fieldset.zone-gemini .zone-legend {
  color: #93c5fd;
}

.zone-fieldset.zone-claude {
  border-color: rgba(251, 146, 60, 0.35);
}

.zone-fieldset.zone-claude:hover {
  border-color: rgba(251, 146, 60, 0.55);
}

.zone-fieldset.zone-claude .zone-legend {
  color: #fdba74;
}

.zone-fieldset:hover {
  border-color: rgba(255, 255, 255, 0.25);
}

.zone-legend {
  font-size: 8px;
  font-weight: 800;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.5px;
  padding: 0 4px;
  line-height: 1;
  margin-left: 6px;
}

.zone-content {
  display: flex;
  justify-content: space-around;
  align-items: flex-start;
  gap: 2px;
}

/* Skeleton loader for circles */
.usage-circles-skeleton {
  display: flex;
  flex-direction: column;
  gap: 6px;
  width: 100%;
}

.skeleton-zone {
  border-color: rgba(255, 255, 255, 0.04) !important;
}

.skeleton-circle-wrapper {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
}

.skeleton-circle {
  width: 40px;
  height: 40px;
  border-radius: 50%;
  background: rgba(255, 255, 255, 0.05);
  animation: pulse 1.5s infinite ease-in-out;
}

.skeleton-text {
  border-radius: 2px;
  background: rgba(255, 255, 255, 0.05);
  animation: pulse 1.5s infinite ease-in-out;
}

.skeleton-text-15 {
  width: 15px;
}

.skeleton-text-25 {
  width: 25px;
  height: 6px;
}

.clickable-icon {
  cursor: pointer;
}

@keyframes pulse {
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
  background: rgba(239, 68, 68, 0.1);
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

/* 5h reading dimmed because this pool's own 7d is full - colour ladder dropped, opacity reduced,
   explanation carried by the bar's title tooltip. No extra row/label (extreme-narrow principle). */
.cc-bar-pct.color-muted {
  color: var(--text-darker);
}

.cc-usage-bar.is-muted {
  opacity: 0.45;
  transition: opacity 0.2s ease;
}

.cc-progress-track {
  height: 5px;
  background: rgba(255, 255, 255, 0.07);
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
  background: rgba(255, 255, 255, 0.08);
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
  background: rgba(255, 255, 255, 0.05);
  animation: pulse 1.5s infinite ease-in-out;
}

.skeleton-bar-track {
  height: 5px;
  width: 100%;
  border-radius: 3px;
  background: rgba(255, 255, 255, 0.05);
  animation: pulse 1.5s infinite ease-in-out;
}

.skeleton-bar-time {
  height: 9px;
  width: 130px;
  border-radius: 2px;
  background: rgba(255, 255, 255, 0.05);
  animation: pulse 1.5s infinite ease-in-out;
}

/* Reload button - circular, hosts the countdown ring */
.btn-reload {
  position: relative;
  overflow: visible;
  border-radius: 50% !important;
  border: 1px solid rgba(255, 255, 255, 0.08) !important;
}

.btn-reload:hover {
  border-color: rgba(255, 255, 255, 0.15) !important;
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

/* Narrow mode (<=700px): the LOCAL/REMOTE columns stay side-by-side (not stacked) - the fix is
   letting each card's content, including the progress bars and reset-line text, actually shrink
   to fit its half instead of the fixed 200px forcing horizontal overflow past the window edge. */
@container main-view (max-width: 700px) {
  .agent-usage-card {
    min-width: 0;
  }
}
</style>
