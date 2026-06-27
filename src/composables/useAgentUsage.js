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

// ─── AG localStorage cache key ───────────────────────────────────────────────
const AG_CACHE_KEY = 'aki-antigravity-usage-cache';

function persistAgCache(dataObj, fetchedAt) {
  try {
    localStorage.setItem(AG_CACHE_KEY, JSON.stringify({ data: dataObj, fetchedAt }));
  } catch (_) {}
}

function loadAgCache() {
  try {
    const raw = localStorage.getItem(AG_CACHE_KEY);
    if (!raw) return null;
    return JSON.parse(raw);
  } catch (_) { return null; }
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
          data.value = JSON.parse(res.content);
          staleResetSyncDone = false;
          // When fresh data arrives, clear cached state
          isCached.value = false;
          cachedAt.value = null;

          const fetchedAt = parseInt(res.fetched_at, 10);
          const nowSec = Date.now() / 1000;

          // ── Stale detection ──────────────────────────────────────────────
          // Use fetched_at age as the universal stale signal (works for both
          // AG and Claude Code — rate_limits only exists on Claude Code).
          const dataAge = fetchedAt > 0 ? (nowSec - fetchedAt) : Infinity;
          let resetIsPast = false;
          if (agentName === 'claudecode') {
            const fiveHour = data.value?.rate_limits?.five_hour;
            resetIsPast = fiveHour?.resets_at > 0 && nowSec > fiveHour.resets_at;
          }
          stale.value = resetIsPast || dataAge > 600;

          // AG: persist live data to localStorage for offline cache
          if (agentName === 'antigravity') {
            persistAgCache(data.value, fetchedAt);
            ulog('ag cache persisted', { fetchedAt }, 'debug');
          }

          const fiveHour = data.value?.rate_limits?.five_hour;
          const sevenDay  = data.value?.rate_limits?.seven_day;
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
            stale:                stale.value,
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

        // AG offline: load last-known cache from localStorage instead of showing empty state
        if (agentName === 'antigravity') {
          const cached = loadAgCache();
          if (cached) {
            data.value = cached.data;
            isCached.value = true;
            cachedAt.value = cached.fetchedAt;
            stale.value = true;
            ulog('ag offline — loaded cache', { fetchedAt: cached.fetchedAt }, 'info');
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
    forceSync
  };
}
