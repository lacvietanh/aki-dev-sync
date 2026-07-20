<template>
  <div class="agent-usage-card">
    <!-- Claude Code Custom Header -->
    <div v-if="agentId === 'claudecode'" class="agent-header claudecode-custom-header">
      <div class="agent-title-group">
        <div class="agent-icon-wrapper">
          <img src="/claude-icon.png" class="agent-img-icon icon-glow" alt="Claude Code" />
        </div>
        <div class="agent-name-row">
          <span class="agent-name u-narrow-hide">{{ agentName }}</span>
          <span v-if="data && claudeTierDisplay" class="agent-plan-badge claude">
            {{ claudeTierDisplay }}
          </span>
          <span v-if="data?.email" class="agent-account" :class="{ 'email-blurred': !showEmail }">{{ truncEmail(data.email) }}</span>
          <button v-if="data?.email" class="btn-eye-inline" @click.stop="$emit('toggle-email')" :title="showEmail ? 'Hide email' : 'Show email'" :aria-label="showEmail ? 'Hide email' : 'Show email'">
            <i class="fa-regular" :class="showEmail ? 'fa-eye' : 'fa-eye-slash'"></i>
          </button>
          <span v-if="ccOrgName" class="agent-org" :class="{ 'email-blurred': !showEmail }">· {{ ccOrgName }}</span>
        </div>
      </div>
      <div class="agent-status-badges">
        <span v-if="stale" class="badge-stale" title="Data is older than 10 minutes">Stale</span>
        <button class="btn-ui-action btn-reload" :class="{ 'error-state': error, 'is-loading': loading }" @click="!loading && !sourceOff && $emit('retry')" :disabled="loading || sourceOff" :title="sourceOff ? (locked ? 'Monitor only for native Claude — Proxy mode active' : 'Monitoring off') : loading ? 'Loading data' : 'Refresh Data'" :aria-label="loading ? 'Loading data' : 'Refresh Data'">
          <RefreshRing :interval-s="sourceOff ? 0 : refreshSettings.usage_interval_s" :refresh-key="drainKey" :overlay="true" />
          <i class="fa-solid" :class="loading ? 'fa-circle-notch fa-spin' : 'fa-rotate-right'"></i>
        </button>
      </div>
    </div>

    <!-- Antigravity Header (Keep tiny logo + email) -->
    <div v-else class="agent-header">
      <div class="agent-title-group">
        <div class="agent-icon-wrapper">
          <img src="/antigravity-icon.png" class="agent-img-icon icon-glow" alt="Antigravity" @click="handleIconClick" style="cursor: pointer;" title="Open Antigravity App" />
        </div>
        <div class="agent-info">
          <div class="agent-name">
            <span class="u-narrow-hide">{{ agentName }}</span>
            <span v-if="data && data.userTier?.name" class="agent-plan-badge ag">
              {{ data.userTier.name.replace('Google', 'GG') }}
            </span>
            <!-- Email doubles as the account-switch trigger (no extra element — Extreme Narrow).
                 The handler is on the wrapper because a blurred email has pointer-events:none. -->
            <span
              v-if="data && data.email"
              class="ag-account-wrap"
              role="button"
              tabindex="0"
              title="Switch account view"
              @click.stop="toggleAccountMenu"
            >
              <span
                class="agent-account ag-account-trigger"
                :class="{ 'email-blurred': !showEmail }"
              >{{ truncEmail(data.email) }}</span>
              <div v-if="accountMenuOpen" class="ag-account-menu" @click.stop>
                <button
                  v-for="acc in accounts"
                  :key="acc.email"
                  class="ag-account-item"
                  :class="{ 'is-current': acc.email === (viewingEmail || activeEmail) }"
                  @click="pickAccount(acc.email)"
                >
                  <span class="ag-account-email" :class="{ 'email-blurred': !showEmail }">{{ acc.email }}</span>
                  <span class="ag-account-metacol">
                    <span v-if="acc.email === activeEmail" class="ag-live-dot" title="Live account"></span>
                    <span class="ag-account-time">{{ formatAgo(acc.fetchedAt) }}</span>
                  </span>
                </button>
                <button class="ag-account-item ag-logout-item" :disabled="loggingOut" @click="logoutAntigravity" title="Sign out — keeps settings/extensions/rules">
                  <i class="fa-solid" :class="loggingOut ? 'fa-circle-notch fa-spin' : 'fa-right-from-bracket'"></i>
                  <span>{{ loggingOut ? 'Logging out…' : 'Log Out' }}</span>
                </button>
              </div>
            </span>
            <button v-if="data && data.email" class="btn-eye-inline" @click.stop="$emit('toggle-email')" :title="showEmail ? 'Hide email' : 'Show email'" :aria-label="showEmail ? 'Hide email' : 'Show email'">
              <i class="fa-regular" :class="showEmail ? 'fa-eye' : 'fa-eye-slash'"></i>
            </button>
          </div>
        </div>
      </div>

      <div class="agent-status-badges">
        <!-- Show cached badge when AG is offline; stale badge otherwise -->
        <span v-if="isCached" class="cached-note" :title="'Data cached at ' + cachedAbsTime">{{ cachedAgo }}</span>
        <span v-else-if="stale" class="badge-stale" title="Data is older than 10 minutes">Stale</span>
        <button class="btn-ui-action btn-reload" :class="{ 'error-state': error, 'is-loading': loading }" @click="!loading && !sourceOff && $emit('retry')" :disabled="loading || sourceOff" :title="sourceOff ? (locked ? 'Monitor only for native Claude — Proxy mode active' : 'Monitoring off') : loading ? 'Loading data' : 'Refresh Data'" :aria-label="loading ? 'Loading data' : 'Refresh Data'">
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
                <div class="skeleton-text" style="width: 15px;"></div>
                <div class="skeleton-text" style="width: 25px; height: 6px;"></div>
              </div>
            </div>
          </fieldset>
          <fieldset class="zone-fieldset zone-claude skeleton-zone">
            <legend class="zone-legend">Claude/OSS</legend>
            <div class="zone-content">
              <div v-for="i in 2" :key="i" class="skeleton-circle-wrapper">
                <div class="skeleton-circle"></div>
                <div class="skeleton-text" style="width: 15px;"></div>
                <div class="skeleton-text" style="width: 25px; height: 6px;"></div>
              </div>
            </div>
          </fieldset>
        </div>
      </div>

      <!-- Off state (manual toggle OR locked-by-proxy) takes priority over stale cached bars —
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
            <div class="cc-usage-bar">
              <div class="cc-bar-header">
                <span class="cc-bar-label">5-Hour</span>
                <span class="cc-bar-pct" :class="cc5hColorClass">{{ cc5hPct !== null ? cc5hPct + '%' : 'N/A' }}</span>
              </div>
              <div class="cc-progress-track">
                <div class="cc-progress-fill" :class="cc5hColorClass" :style="{ width: (cc5hPct || 0) + '%' }"></div>
              </div>
              <div class="cc-reset-line" :class="{ 'is-na': !cc5hResetsAt }">
                <template v-if="cc5hResetLine.val">
                  <span class="time-label">Reset </span><span class="time-val">{{ cc5hResetLine.val }}</span>
                  <span v-if="cc5hResetLine.abs" class="time-abs"> ({{ cc5hResetLine.abs }})</span>
                </template>
                <span v-else class="time-label">{{ cc5hResetLine.label }}</span>
              </div>
            </div>
            <div v-if="data.rate_limits?.seven_day?.used_percentage != null" class="cc-usage-bar">
              <div class="cc-bar-header">
                <span class="cc-bar-label">7-Day</span>
                <span class="cc-bar-pct" :class="cc7dColorClass">{{ cc7dPct !== null ? cc7dPct + '%' : 'N/A' }}</span>
              </div>
              <div class="cc-progress-track">
                <div class="cc-progress-fill" :class="cc7dColorClass" :style="{ width: (cc7dPct || 0) + '%' }"></div>
              </div>
              <div class="cc-reset-line" :class="{ 'is-na': !cc7dResetsAt }">
                <template v-if="cc7dResetLine.val">
                  <span class="time-label">Reset </span><span class="time-val">{{ cc7dResetLine.val }}</span>
                  <span v-if="cc7dResetLine.abs" class="time-abs"> ({{ cc7dResetLine.abs }})</span>
                </template>
                <span v-else class="time-label">{{ cc7dResetLine.label }}</span>
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
                             @timeout="$emit('retry')" />
                <UsageCircle
                             label="Gemini Weekly Limit"
                             subLabel="7D"
                             :percentage="geminiWeeklyBucket?.remainingFraction !== undefined ? Math.round((1 - geminiWeeklyBucket.remainingFraction) * 100) : null"
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
                             @timeout="$emit('retry')" />
                <UsageCircle
                             label="Claude & GPT Weekly Limit"
                             subLabel="7D"
                             :percentage="claudeWeeklyBucket?.remainingFraction !== undefined ? Math.round((1 - claudeWeeklyBucket.remainingFraction) * 100) : null"
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

<script setup>
// @docs docs/arch/usage-claudecode.md
// @docs docs/arch/usage-antigravity.md
import { computed, ref, watch, onMounted, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import Swal from 'sweetalert2';
import UsageCircle from './UsageCircle.vue';
import RefreshRing from './RefreshRing.vue';
import { refreshSettings } from '../store/refreshStore';

const props = defineProps({
  agentId: String,
  agentName: String,
  data: Object,
  loading: Boolean,
  error: String,
  stale: Boolean,
  isCached: { type: Boolean, default: false },
  cachedAt: { type: Number, default: null },
  showEmail: { type: Boolean, default: true },
  sourceOff: { type: Boolean, default: false },
  // True when sourceOff is forced (not user-toggled) — e.g. Claude Code local monitoring
  // locked off while Proxy mode is active. Swaps the off-state message to explain why.
  locked: { type: Boolean, default: false },
  // AG-only multi-account view (unused for Claude Code)
  accounts: { type: Array, default: () => [] },
  viewingEmail: { default: null },
  activeEmail: { default: null }
});

// Single source of truth for which body view to render. Priority: error > loading > off
// (manual or locked) > empty (no data yet) > data. Off is checked before data on purpose —
// otherwise flipping a source off leaves the last-fetched bars on screen until relaunch.
const uiStatus = computed(() => {
  if (props.error) return { kind: 'error', text: props.error };
  if (props.sourceOff) {
    return {
      kind: 'off',
      icon: 'fa-power-off mb-1',
      text: props.locked ? 'Monitor only for native Claude — Proxy mode active' : 'Monitoring off',
    };
  }
  if (props.loading && !props.data) return { kind: 'loading' };
  if (!props.data) {
    return {
      kind: 'empty',
      icon: props.agentId === 'antigravity' ? 'fa-circle-info mb-1' : 'fa-hourglass-empty mb-1',
      text: props.agentId === 'antigravity' ? 'Not connected — open & sign in to Antigravity to monitor' : 'No data — waiting for next session',
    };
  }
  return { kind: 'data' };
});

const emit = defineEmits(['retry', 'select-account', 'toggle-email', 'logout-success']);

// AG account-switch dropdown
const accountMenuOpen = ref(false);
function toggleAccountMenu() {
  if (props.agentId !== 'antigravity') return;
  accountMenuOpen.value = !accountMenuOpen.value;
}
function pickAccount(email) {
  // Picking the live/active account returns to the follow-live view (viewingEmail = null).
  emit('select-account', email === props.activeEmail ? null : email);
  accountMenuOpen.value = false;
}
const loggingOut = ref(false);
async function logoutAntigravity() {
  if (loggingOut.value) return;
  accountMenuOpen.value = false;
  const { isConfirmed } = await Swal.fire({
    title: 'Đăng xuất Antigravity?',
    html: 'Ứng dụng sẽ tự đóng và xoá phiên đăng nhập hiện tại.<br>Settings, extension, rule và permission vẫn được giữ nguyên.',
    icon: 'warning',
    showCancelButton: true,
    confirmButtonColor: '#ef4444',
    cancelButtonColor: '#374151',
    confirmButtonText: 'Đăng xuất',
    cancelButtonText: 'Hủy',
    background: '#131317',
    color: '#F3F4F6',
  });
  if (!isConfirmed) return;
  loggingOut.value = true;
  try {
    await invoke('logout_antigravity');
    // AG's own auth state is now wiped, but this app's per-account cache/view-state isn't —
    // without this, the display would keep following the just-logged-out account until a live
    // fetch for the next login happens to succeed (can be several polls away).
    emit('logout-success');
  } finally {
    loggingOut.value = false;
  }
}
// Design lock: the header shows a truncated email (10 chars wide, 6 at the narrow breakpoint) to
// keep width stable when the active/cached account changes; the full email is shown in the
// dropdown rows untouched.
const isNarrow = ref(typeof window !== 'undefined' && window.innerWidth <= 700);
function updateIsNarrow() { isNarrow.value = window.innerWidth <= 700; }
function truncEmail(email) {
  const max = isNarrow.value ? 4 : 10;
  return email.length > max ? email.slice(0, max) + '…' : email;
}
function onDocClick() { accountMenuOpen.value = false; }
function onDocKey(e) { if (e.key === 'Escape') accountMenuOpen.value = false; }
onMounted(() => {
  document.addEventListener('click', onDocClick);
  document.addEventListener('keydown', onDocKey);
  window.addEventListener('resize', updateIsNarrow);
});
onUnmounted(() => {
  document.removeEventListener('click', onDocClick);
  document.removeEventListener('keydown', onDocKey);
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

// AG cached-at display — reactive relative time updated every 10s
const agCacheNow = ref(Math.floor(Date.now() / 1000));
let agCacheTimer = null;
onMounted(() => {
  if (props.agentId === 'antigravity') {
    agCacheTimer = setInterval(() => { agCacheNow.value = Math.floor(Date.now() / 1000); }, 10000);
  }
});
onUnmounted(() => { if (agCacheTimer) clearInterval(agCacheTimer); });

const cachedAgo = computed(() => {
  if (!props.cachedAt) return '';
  const diffS = agCacheNow.value - props.cachedAt;
  if (diffS < 60) return '<1m ago';
  const mins = Math.floor(diffS / 60);
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  const rem = mins % 60;
  return rem > 0 ? `${hrs}h${rem}m ago` : `${hrs}h ago`;
});

const cachedAbsTime = computed(() => {
  if (!props.cachedAt) return '';
  const d = new Date(props.cachedAt * 1000);
  const hh = String(d.getHours()).padStart(2, '0');
  const mm = String(d.getMinutes()).padStart(2, '0');
  return `${hh}:${mm}`;
});

// Relative "cached N ago" for a given Unix-seconds timestamp (used by the account dropdown).
// Reactive via agCacheNow (ticks every 10s).
function formatAgo(sec) {
  if (!sec) return '';
  const diffS = agCacheNow.value - sec;
  if (diffS < 60) return '<1m';
  const mins = Math.floor(diffS / 60);
  if (mins < 60) return `${mins}m`;
  const hrs = Math.floor(mins / 60);
  const rem = mins % 60;
  return rem > 0 ? `${hrs}h${rem}m` : `${hrs}h`;
}

const cc5hPct = computed(() => { const v = props.data?.rate_limits?.five_hour?.used_percentage; return v != null ? Math.round(v) : null; });
const cc5hResetsAt = computed(() => {
  const r5 = props.data?.rate_limits?.five_hour?.resets_at ?? null;
  // Claude reports 5h.resets_at == 7d.resets_at when the 5h window sits idle at 0% with no
  // fresh API traffic to establish a real boundary. Drawing that far-future "5-day" reset is
  // misleading, so treat it as unknown → the reset line falls into its existing N/A state.
  const r7 = props.data?.rate_limits?.seven_day?.resets_at ?? null;
  if (r5 && r7 && r5 === r7) return null;
  return r5;
});
const cc5hColorClass = computed(() => pctColorClass(cc5hPct.value));
const cc5hResetLine = computed(() => formatResetLine(cc5hResetsAt.value, ccNow.value));

// P4 boundary trigger: CC had no client-side equivalent of AG's UsageCircle @timeout — the
// 5-hour bar could sit stale at "ready" past its reset with nothing prompting a refetch until
// the next STALE_RESET poll noticed server-side. Same wasPast/nowPast edge-detect pattern as
// UsageCircle.vue, wired to the existing @retry → refresh handler (AgentUsageSlot.vue).
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

const cc7dPct = computed(() => { const v = props.data?.rate_limits?.seven_day?.used_percentage; return v != null ? Math.round(v) : null; });
const cc7dResetsAt = computed(() => props.data?.rate_limits?.seven_day?.resets_at ?? null);
const cc7dColorClass = computed(() => pctColorClass(cc7dPct.value));
const cc7dResetLine = computed(() => formatResetLine(cc7dResetsAt.value, ccNow.value));

// Org name: skip Anthropic's auto-generated default "email's Organization"
const ccOrgName = computed(() => {
  const org = props.data?.orgName;
  if (!org) return null;
  if (props.data?.email && org === `${props.data.email}'s Organization`) return null;
  return org;
});

// SVG ring — restarts on refresh complete or when interval setting changes
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
}

.agent-img-icon {
  width: 18px;
  height: 18px;
  border-radius: 4px;
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
  top: calc(100% + 4px);
  left: 0;
  z-index: 50;
  min-width: 180px;
  max-width: 280px;
  padding: 3px;
  background: #1a1d23; /* solid — --bg-tertiary is near-transparent and would show through */
  border: 1px solid var(--border-color);
  border-radius: 6px;
  box-shadow: 0 6px 18px rgba(0, 0, 0, 0.5);
  display: flex;
  flex-direction: column;
  gap: 1px;
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
.ag-account-email {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
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
  background: var(--accent-cyan);
  box-shadow: 0 0 5px var(--accent-cyan);
}
.ag-logout-item {
  justify-content: flex-start;
  gap: 6px;
  margin-top: 2px;
  padding-top: 5px;
  border-top: 1px solid rgba(255, 255, 255, 0.06);
  border-radius: 0 0 4px 4px;
  color: #f87171;
}
.ag-logout-item:hover {
  background: rgba(239, 68, 68, 0.12);
}
.ag-logout-item:disabled {
  opacity: 0.5;
  cursor: not-allowed;
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

/* Cached indicator: plain amber text, not a badge box (keeps the header narrow) */
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
  transition: all 0.2s ease;
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

/* Reload button — circular, hosts the countdown ring */
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

/* Narrow mode (<=700px): the LOCAL/REMOTE columns stay side-by-side (not stacked) — the fix is
   letting each card's content, including the progress bars and reset-line text, actually shrink
   to fit its half instead of the fixed 200px forcing horizontal overflow past the window edge. */
@media (max-width: 700px) {
  .agent-usage-card {
    min-width: 0;
  }
}
</style>
