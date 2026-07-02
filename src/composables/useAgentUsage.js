// @docs docs/arch/usage-claudecode.md
// @docs docs/arch/usage-antigravity.md
// @docs docs/research/claude-usage-1.2.x-analyze.md
// @docs docs/arch/logger.md
import { ref, watch, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { refreshSettings, manualRefreshCount } from '../store/refreshStore';

// ─── Logger ──────────────────────────────────────────────────────────────────
// Three levels matching logger.rs contract:
//   error → always: console.error + backend (file + stderr)
//   info  → debug-only: console.info  + backend
//   debug → debug-only: console.log   + backend
//
// Frontend console is printed FIRST (preserves DevTools source-line links),
// then the line is forwarded to the Rust backend via fire-and-forget IPC so
// all events appear in the same usage.log and terminal stderr, interleaved in
// real chronological order with Rust log entries.
//
// Timestamp format: YYYYMMDD.HHMMSS.mmm — compact, matches Rust now_human().

let _isDebugMode = false;
let _debugFetched = false;

// Compact timestamp matching Rust format YYYYMMDD.HHMMSS.mmm (local time for JS)
function fmtNow() {
  const d = new Date();
  const p2 = n => String(n).padStart(2, '0');
  const p3 = n => String(n).padStart(3, '0');
  return `${d.getFullYear()}${p2(d.getMonth()+1)}${p2(d.getDate())}.` +
         `${p2(d.getHours())}${p2(d.getMinutes())}${p2(d.getSeconds())}.` +
         `${p3(d.getMilliseconds())}`;
}

function makeLogger(agentName) {
  const tag = `USAGE:${agentName}`;
  return function ulog(event, fields = {}, level = 'debug') {
    const pairs = Object.entries(fields).map(([k, v]) => `${k}=${JSON.stringify(v)}`).join(' ');
    const msg   = `${event}${pairs ? ' ' + pairs : ''}`;
    const line  = `[${fmtNow()}][${tag}] ${msg}`;

    // 1) Print to Webview DevTools console immediately:
    //    - error → always (surface real failures regardless of debug mode)
    //    - info/debug → only when debug mode is confirmed active
    if (level === 'error') {
      console.error(line);
    } else if (_isDebugMode) {
      if (level === 'info') console.info(line);
      else                  console.log(line);
    }

    // 2) Forward to Rust backend (file + stderr) — fire-and-forget
    invoke('log_frontend', { level, tag, msg }).catch(() => {});
  };
}

// One-time startup: fetch debug mode + log path from Rust, enable console output.
let _startupLogged = false;
async function logStartupInfo() {
  if (_startupLogged) return;
  _startupLogged = true;
  try {
    const [isDebug, logPath] = await Promise.all([
      invoke('is_debug_mode'),
      invoke('get_log_path'),
    ]);
    _isDebugMode  = !!isDebug;
    _debugFetched = true;
    if (_isDebugMode) {
      console.info(`[${fmtNow()}][USAGE:init] debug_mode=true log_file=${logPath}`);
      console.info(`[${fmtNow()}][USAGE:init] Frontend logs → console + backend pipeline.`);
    }
  } catch (_) {
    _debugFetched = true; // don't block forever on IPC error
  }
}

// ─── AG localStorage cache (per-account) ─────────────────────────────────────
// Antigravity can switch logged-in accounts on the same machine, so usage is cached
// PER EMAIL. A dropdown (AgentUsage.vue header) lets the user inspect a previous
// account's cached usage while another account is live.
//
// Store shape (v2): { accounts: { "<email>": { data, fetchedAt }, ... }, lastActiveEmail }
//
// NOTE: Claude Code deliberately has NO equivalent — exactly one account per remote host
// by design (see docs/arch/usage-claudecode.md). Only Antigravity uses this store; the
// switch to last-active-account on a null fetch also removes the old single-blob bug where
// the display randomly flipped between accounts during an IDE restart.
const AG_CACHE_KEY_V1 = 'aki-antigravity-usage-cache';    // legacy single-blob key
const AG_CACHE_KEY = 'aki-antigravity-usage-cache-v2';    // per-account store

function saveAgStore(store) {
  try { localStorage.setItem(AG_CACHE_KEY, JSON.stringify(store)); } catch (_) {}
}

function loadAgStore() {
  try {
    const raw = localStorage.getItem(AG_CACHE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw);
      if (parsed && parsed.accounts) return parsed;
    }
    // One-time migration: v1 single-blob → v2 per-account map, keyed by the blob's email.
    const v1raw = localStorage.getItem(AG_CACHE_KEY_V1);
    if (v1raw) {
      const v1 = JSON.parse(v1raw);
      const email = v1?.data?.email;
      const store = { accounts: {}, lastActiveEmail: null };
      if (email) {
        store.accounts[email] = { data: v1.data, fetchedAt: v1.fetchedAt };
        store.lastActiveEmail = email;
      }
      saveAgStore(store);
      localStorage.removeItem(AG_CACHE_KEY_V1);
      return store;
    }
  } catch (_) {}
  return { accounts: {}, lastActiveEmail: null };
}

function persistAgAccount(dataObj, fetchedAt) {
  // AG always authenticates via Google, so a live payload always carries an email.
  const email = dataObj.email;
  const store = loadAgStore();
  store.accounts[email] = { data: dataObj, fetchedAt };
  store.lastActiveEmail = email;
  saveAgStore(store);
}

function loadAgAccount(email) {
  if (!email) return null;
  const store = loadAgStore();
  return store.accounts[email] || null;
}

function listAgAccounts() {
  const store = loadAgStore();
  return Object.entries(store.accounts)
    .map(([email, v]) => ({ email, fetchedAt: v.fetchedAt }))
    .sort((a, b) => (b.fetchedAt || 0) - (a.fetchedAt || 0));
}

export function useAgentUsage(agentName, hostRef) {
  const ulog = makeLogger(agentName);
  logStartupInfo(); // one-time: resolves debug mode, enables console output

  const data = ref(null);
  const loading = ref(false);
  const error = ref(null);
  const stale = ref(false);
  // AG-only: tracks whether current data is from cache (AG offline) and when it was cached
  const isCached = ref(false);
  const cachedAt = ref(null); // Unix seconds

  // AG-only: multi-account view state (unused for Claude Code — one account per remote).
  // Design lock: viewingEmail is intentionally NOT persisted — pinning to a previous account is a
  // transient inspection; every reload returns to the follow-live view of the active account.
  const accounts = ref([]);       // dropdown list [{ email, fetchedAt }] sorted newest-first
  const viewingEmail = ref(null); // null = follow live/active account; else a pinned email
  const activeEmail = ref(null);  // email of the last successful live fetch
  let latestLive = null;          // last successful live parse (for returning to live view)
  let latestLiveStale = false;
  const refreshAccounts = () => { accounts.value = listAgAccounts(); };
  if (agentName === 'antigravity') {
    const store = loadAgStore();
    accounts.value = listAgAccounts();
    activeEmail.value = store.lastActiveEmail;
  }

  let pollTimer = null;
  let pollCount = 0;
  let provisioned = false;
  let initialSyncDone = false;
  let staleResetSyncDone = false;
  let isSyncing = false;
  let isChecking = false;
  // Layer 4 (retry/backoff): if a force-sync fails, allow the next poll tick to retry by
  // clearing the one-shot guards — but cap consecutive auto-retries so a genuinely broken
  // remote (claude missing, network down) doesn't spawn probe sessions forever.
  let forceSyncFailCount = 0;
  const MAX_FORCESYNC_RETRIES = 3;

  const provision = async () => {
    if (!hostRef.value || provisioned) return;
    provisioned = true;
    ulog('provision start', { host: hostRef.value }, 'info');
    try {
      await invoke('provision_agent_usage', { agentName, host: hostRef.value });
      ulog('provision ok', {}, 'info');
    } catch (e) {
      provisioned = false;
      ulog('provision error', { err: String(e) }, 'error');
    }
  };

  const checkUsage = async () => {
    if (!hostRef.value) {
      data.value = null;
      error.value = null;
      return;
    }
    if (isChecking) {
      ulog('checkUsage skip (isChecking=true)', {}, 'debug');
      return;
    }
    isChecking = true;
    pollCount++;

    ulog('checkUsage start', {
      host: hostRef.value,
      poll: pollCount,
      hadData: data.value !== null,
      initialSyncDone,
      staleResetSyncDone,
      isSyncing,
    }, 'debug');

    loading.value = true;
    ulog('loading=true', {}, 'debug');
    error.value = null;

    try {
      const hadData = data.value !== null;
      ulog('invoking get_agent_usage', { host: hostRef.value }, 'debug');
      const res = await invoke('get_agent_usage', { agentName, host: hostRef.value });
      ulog('get_agent_usage returned', { hasResult: res !== null }, 'debug');

      if (res) {
        try {
          const parsed = JSON.parse(res.content);
          staleResetSyncDone = false;

          const fetchedAt = parseInt(res.fetched_at, 10);
          const nowSec = Date.now() / 1000;

          // ── Stale detection ──────────────────────────────────────────────
          // Use fetched_at age as the universal stale signal (works for both
          // AG and Claude Code — rate_limits only exists on Claude Code).
          const dataAge = fetchedAt > 0 ? (nowSec - fetchedAt) : Infinity;
          let resetIsPast = false;
          if (agentName === 'claudecode') {
            const fh = parsed?.rate_limits?.five_hour;
            resetIsPast = fh?.resets_at > 0 && nowSec > fh.resets_at;
          }
          const liveStale = resetIsPast || dataAge > 600;

          if (agentName === 'antigravity') {
            // Record this live fetch under its account email and refresh the dropdown.
            // Only update the VISIBLE data when the user is viewing the live/active account
            // (not pinned to a previous account's cache).
            latestLive = parsed;
            latestLiveStale = liveStale;
            activeEmail.value = parsed?.email || activeEmail.value;
            persistAgAccount(parsed, fetchedAt);
            refreshAccounts();
            if (viewingEmail.value === null || viewingEmail.value === activeEmail.value) {
              data.value = parsed;
              isCached.value = false;
              cachedAt.value = null;
              stale.value = liveStale;
            }
            ulog('ag live fetched', { email: activeEmail.value, viewing: viewingEmail.value, fetchedAt }, 'debug');
          } else {
            data.value = parsed;
            isCached.value = false;
            cachedAt.value = null;
            stale.value = liveStale;
          }

          const fiveHour = parsed?.rate_limits?.five_hour;
          const sevenDay  = parsed?.rate_limits?.seven_day;
          const mtime = parseInt(res.file_modified_at, 10);
          ulog('got data', {
            'five_hour.pct':      fiveHour?.used_percentage ?? null,
            'five_hour.resets_at': fiveHour?.resets_at ?? null,
            'five_hour.state':    fiveHour?.resets_at > 0
                                    ? (nowSec > fiveHour.resets_at ? 'PAST' : 'future')
                                    : 'no_reset',
            'seven_day.pct':      sevenDay?.used_percentage ?? null,
            mtime,
            file_age_s:           mtime > 0 ? Math.round(nowSec - mtime) : null,
            stale:                liveStale,
            stale_reason:         resetIsPast ? 'resetIsPast' : dataAge > 600 ? 'dataAgeStale' : 'none',
            reset_overdue_s:      resetIsPast ? Math.round(nowSec - fiveHour.resets_at) : null,
            until_reset_s:        (!resetIsPast && fiveHour?.resets_at > 0)
                                    ? Math.round(fiveHour.resets_at - nowSec) : null,
          }, 'info');

          // Data was read successfully — normal path, no forceSync needed.
          // resets_at=0 means no active session in the 5h window, but the cache file
          // IS readable. forceSync purpose is strictly "cannot read from cache because
          // no session has written to it" — that is the null-result path below.
          if (agentName === 'claudecode' && !initialSyncDone) {
            initialSyncDone = true;
            ulog('first load ok (data present, no forceSync)', { 'resets_at': fiveHour?.resets_at ?? null }, 'info');
          }
          // Re-provision existing hosts once per session (fire-and-forget). Hosts that already
          // have a cache always land here (never the null path that used to call provision), so
          // without this the upgraded statusline hook (aki-rlcache v2) would never reach them.
          // provision() is idempotent and flips `provisioned` up front, so this runs at most once
          // per host per session and does not block the read.
          if (agentName === 'claudecode' && !provisioned) provision();
        } catch (e) {
          ulog('parse error', { err: String(e), content_preview: String(res.content).slice(0, 100) }, 'error');
          error.value = "Invalid usage data format.";
        }
      } else {
        // null from server: either no cache file (first load) or STALE_RESET (had data → null)
        ulog('got null', {
          hadData,
          initialSyncDone,
          staleResetSyncDone,
          null_reason: !hadData
            ? (initialSyncDone ? 'repeated_null' : 'first_load_no_cache')
            : 'transition_had_data_now_null (STALE_RESET)',
        }, 'info');

        // AG offline: the live fetch failed (IDE mid-restart — common right after an account
        // switch). Show the LAST-ACTIVE account's cache deterministically (never an ambiguous
        // global blob), so the display can't randomly flip old/new. If the user pinned the view
        // to a specific account, keep showing that one.
        if (agentName === 'antigravity') {
          latestLive = null;
          refreshAccounts();
          const store = loadAgStore();
          if (!activeEmail.value) activeEmail.value = store.lastActiveEmail;
          const targetEmail = viewingEmail.value || store.lastActiveEmail;
          const cached = loadAgAccount(targetEmail);
          if (cached) {
            data.value = cached.data;
            isCached.value = true;
            cachedAt.value = cached.fetchedAt;
            stale.value = true;
            ulog('ag offline — showing cached account', { email: targetEmail, fetchedAt: cached.fetchedAt }, 'info');
          } else {
            data.value = null;
            isCached.value = false;
            cachedAt.value = null;
            ulog('ag offline — no cache available', {}, 'info');
          }
        } else {
          data.value = null;
        }

        if (agentName === 'claudecode' && !initialSyncDone) {
          initialSyncDone = true;
          ulog('first load no cache → provision then forceSync', {}, 'info');
          await provision();
          forceSync();
        } else if (agentName === 'claudecode' && hadData && !staleResetSyncDone) {
          // Transition: had data → now null = STALE_RESET. The cache was readable until
          // this poll, so the hook is already installed — no provision needed. Recover
          // once with a single force-sync.
          staleResetSyncDone = true;
          ulog('STALE_RESET (had data → null) → forceSync (provision not needed)', {}, 'info');
          forceSync();
        } else {
          ulog('null received but no auto-forceSync triggered', {
            reason: !hadData
              ? 'hadData=false (already null from prev poll)'
              : 'staleResetSyncDone=true (already triggered once)',
          }, 'debug');
        }
      }
    } catch (e) {
      ulog('IPC error', { err: String(e) }, 'error');
      error.value = e.toString();
    } finally {
      loading.value = false;
      isChecking = false;
      ulog('checkUsage done', { loading: false, isChecking: false, hasData: data.value !== null, hasError: !!error.value }, 'debug');
    }
  };

  const forceSync = async () => {
    if (!hostRef.value || isSyncing) {
      ulog('forceSync skip', { reason: !hostRef.value ? 'no host' : 'already syncing' }, 'debug');
      return;
    }
    isSyncing = true;
    loading.value = true;
    ulog('loading=true (forceSync)', {}, 'debug');
    error.value = null;
    ulog('forceSync start', { host: hostRef.value, failCount: forceSyncFailCount }, 'info');

    let succeeded = false;
    try {
      ulog('invoking force_sync_agent_usage', { host: hostRef.value }, 'debug');
      // Rust now returns Err (rejects) when the remote script produced no output —
      // e.g. the shell died early. That lands in catch below and is treated as failure.
      const raw = await invoke('force_sync_agent_usage', { agentName, host: hostRef.value });
      ulog('force_sync_agent_usage returned', { raw_len: String(raw).length }, 'debug');
      let diag = null;
      try {
        diag = JSON.parse(raw);
        ulog('forceSync diagnostic', diag, 'debug');
      } catch (_) {
        ulog('forceSync raw (not JSON)', { raw_preview: String(raw).slice(0, 200) }, 'debug');
      }
      // claude ran but its output couldn't be parsed into usable data → soft failure.
      if (diag && diag.parsed === false) {
        throw new Error(`parser did not parse (parse_error=${diag.parse_error || 'unknown'})`);
      }
      ulog('forceSync done, calling checkUsage', {}, 'info');
      await checkUsage();
      succeeded = data.value !== null;
      ulog('forceSync complete', { data_loaded: succeeded }, 'info');
    } catch (e) {
      ulog('forceSync error', { err: String(e) }, 'error');
      error.value = e.toString();
    } finally {
      loading.value = false;
      isSyncing = false;
      if (succeeded) {
        forceSyncFailCount = 0;
        ulog('forceSync finally', { isSyncing: false, outcome: 'success' }, 'info');
      } else {
        forceSyncFailCount++;
        if (forceSyncFailCount < MAX_FORCESYNC_RETRIES) {
          // Clear the one-shot guards so the next poll tick auto-retries (poll interval = backoff).
          initialSyncDone = false;
          staleResetSyncDone = false;
          ulog('forceSync finally', {
            isSyncing: false, outcome: 'fail-will-retry', failCount: forceSyncFailCount,
          }, 'info');
        } else {
          // Give up auto-retrying; keep guards set. Manual refresh still forces a fresh attempt.
          if (!error.value) {
            error.value = `Force sync failed ${forceSyncFailCount}× — auto-retry stopped. Try manual refresh.`;
          }
          ulog('forceSync finally', {
            isSyncing: false, outcome: 'fail-giveup', failCount: forceSyncFailCount,
          }, 'error');
        }
      }
    }
  };

  // AG-only: switch which account's usage is displayed. Pass null (or the active email) to
  // return to the live view; pass another cached email to pin the view to that account's cache.
  // The background poll keeps running and updating caches regardless of the selection.
  const selectAccount = (email) => {
    viewingEmail.value = email;
    if (agentName !== 'antigravity') return;
    if (email === null || email === activeEmail.value) {
      if (latestLive) {
        data.value = latestLive;
        isCached.value = false;
        cachedAt.value = null;
        stale.value = latestLiveStale;
      } else {
        // No live data yet → fall back to the last-active account's cache.
        const cached = loadAgAccount(loadAgStore().lastActiveEmail);
        if (cached) {
          data.value = cached.data;
          isCached.value = true;
          cachedAt.value = cached.fetchedAt;
          stale.value = true;
        } else {
          data.value = null;
          isCached.value = false;
          cachedAt.value = null;
        }
      }
    } else {
      const cached = loadAgAccount(email);
      if (cached) {
        data.value = cached.data;
        isCached.value = true;
        cachedAt.value = cached.fetchedAt;
        stale.value = true;
      }
    }
    ulog('ag select account', { email, active: activeEmail.value }, 'info');
  };

  function restartPollTimer() {
    if (pollTimer) clearInterval(pollTimer);
    pollTimer = null;
    const s = refreshSettings.value.usage_interval_s;
    ulog('poll timer restart', { interval_s: s, host: hostRef.value }, 'debug');
    if (hostRef.value && s > 0) {
      pollTimer = setInterval(() => {
        ulog('poll tick', { poll: pollCount + 1, interval_s: s }, 'debug');
        checkUsage();
      }, s * 1000);
    }
  }

  watch(() => hostRef.value, (newHost) => {
    ulog('host changed', { newHost, resetting_all_flags: true }, 'info');
    provisioned = false;
    initialSyncDone = false;
    staleResetSyncDone = false;
    isSyncing = false;
    isChecking = false;
    forceSyncFailCount = 0;
    pollCount = 0;
    data.value = null;
    error.value = null;
    isCached.value = false;
    cachedAt.value = null;
    if (newHost) {
      checkUsage();
    }
    restartPollTimer();
  }, { immediate: true });

  watch(() => refreshSettings.value.usage_interval_s, (newVal) => {
    ulog('interval changed', { new_interval_s: newVal }, 'debug');
    restartPollTimer();
  });

  watch(() => manualRefreshCount.value, (count) => {
    if (!hostRef.value) return;
    ulog('manual refresh', { count, agent: agentName }, 'info');
    // Manual refresh (Reload / Refresh buttons) = normal fresh load for all agents.
    // forceSync is NOT called here — it auto-triggers inside checkUsage() only when
    // the result is genuinely null (no cache / STALE_RESET). Resetting the one-shot
    // guards lets checkUsage re-evaluate from scratch for this refresh cycle.
    if (agentName === 'claudecode') {
      initialSyncDone = false;
      staleResetSyncDone = false;
      forceSyncFailCount = 0;
    }
    checkUsage();
  });

  onUnmounted(() => {
    if (pollTimer) clearInterval(pollTimer);
    ulog('unmounted', {}, 'debug');
  });

  return {
    data,
    loading,
    error,
    stale,
    isCached,
    cachedAt,
    refresh: checkUsage,
    forceSync,
    // AG-only multi-account view (harmless/unused for Claude Code)
    accounts,
    viewingEmail,
    activeEmail,
    selectAccount
  };
}
