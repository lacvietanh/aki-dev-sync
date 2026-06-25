// @docs docs/arch/usage-claudecode.md
// @docs docs/arch/usage-antigravity.md
// @docs docs/research/claude-usage-1.2.x-analyze.md
import { ref, watch, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { refreshSettings, manualRefreshCount } from '../store/refreshStore';

// Structured logger: always writes to console.warn (visible in DevTools F12).
// Format: [YYYY-MM-DD HH:MM:SS.mmm][USAGE:agent] event key=val ...
function fmtNow() {
  const d = new Date();
  const pad2 = n => String(n).padStart(2, '0');
  const pad3 = n => String(n).padStart(3, '0');
  return `${d.getFullYear()}-${pad2(d.getMonth()+1)}-${pad2(d.getDate())} ` +
         `${pad2(d.getHours())}:${pad2(d.getMinutes())}:${pad2(d.getSeconds())}.${pad3(d.getMilliseconds())}`;
}

function makeLogger(agentName) {
  return function ulog(event, fields = {}) {
    const pairs = Object.entries(fields).map(([k, v]) => `${k}=${JSON.stringify(v)}`).join(' ');
    const line = `[${fmtNow()}][USAGE:${agentName}] ${event}${pairs ? ' ' + pairs : ''}`;
    // Use console.warn so it appears even when console.log is filtered out.
    console.warn(line);
  };
}

// One-time startup info: print log file path so developer knows where to look.
let _startupLogged = false;
async function logStartupInfo() {
  if (_startupLogged) return;
  _startupLogged = true;
  try {
    const [isDebug, logPath] = await Promise.all([
      invoke('is_debug_mode'),
      invoke('get_log_path'),
    ]);
    console.warn(`[${fmtNow()}][USAGE:init] debug_mode=${isDebug} log_file=${logPath}`);
    console.warn(`[${fmtNow()}][USAGE:init] Open DevTools (F12) to trace frontend events. Rust events → log file.`);
  } catch (_) { /* not critical */ }
}

export function useAgentUsage(agentName, hostRef) {
  const ulog = makeLogger(agentName);
  logStartupInfo(); // one-time: prints log file path and debug mode to console

  const data = ref(null);
  const loading = ref(false);
  const error = ref(null);
  const stale = ref(false);

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
    ulog('provision start', { host: hostRef.value });
    try {
      await invoke('provision_agent_usage', { agentName, host: hostRef.value });
      ulog('provision ok');
    } catch (e) {
      provisioned = false;
      ulog('provision error', { err: String(e) });
      console.error(`Failed to provision ${agentName}:`, e);
    }
  };

  const checkUsage = async () => {
    if (!hostRef.value) {
      data.value = null;
      error.value = null;
      return;
    }
    if (isChecking) {
      ulog('checkUsage skip (isChecking=true)');
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
    });

    loading.value = true;
    ulog('loading=true');
    error.value = null;

    try {
      const hadData = data.value !== null;
      ulog('invoking get_agent_usage', { host: hostRef.value });
      const res = await invoke('get_agent_usage', { agentName, host: hostRef.value });
      ulog('get_agent_usage returned', { hasResult: res !== null });

      if (res) {
        try {
          data.value = JSON.parse(res.content);
          staleResetSyncDone = false;

          const mtime = parseInt(res.file_modified_at, 10);
          const fetchedAt = parseInt(res.fetched_at, 10);
          const nowSec = Date.now() / 1000;
          const fiveHour = data.value?.rate_limits?.five_hour;
          const sevenDay = data.value?.rate_limits?.seven_day;
          const resetIsPast = fiveHour?.resets_at > 0 && nowSec > fiveHour.resets_at;
          const mtimeStale = mtime > 0 && (nowSec - mtime) > 600;
          stale.value = resetIsPast || mtimeStale;

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
            stale_reason:         resetIsPast ? 'resetIsPast' : mtimeStale ? 'mtimeStale' : 'none',
            reset_overdue_s:      resetIsPast ? Math.round(nowSec - fiveHour.resets_at) : null,
            until_reset_s:        (!resetIsPast && fiveHour?.resets_at > 0)
                                    ? Math.round(fiveHour.resets_at - nowSec) : null,
          });

          // Data was read successfully — normal path, no forceSync needed.
          // resets_at=0 means no active session in the 5h window, but the cache file
          // IS readable. forceSync purpose is strictly "cannot read from cache because
          // no session has written to it" — that is the null-result path below.
          if (agentName === 'claudecode' && !initialSyncDone) {
            initialSyncDone = true;
            ulog('first load ok (data present, no forceSync)', { 'resets_at': fiveHour?.resets_at ?? null });
          }
        } catch (e) {
          ulog('parse error', { err: String(e), content_preview: String(res.content).slice(0, 100) });
          console.error(`Failed to parse ${agentName} usage JSON:`, e);
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
        });

        data.value = null;

        if (agentName === 'claudecode' && !initialSyncDone) {
          // First load with no readable cache: the remote may have never been
          // provisioned (statusLine hook absent). Provision is a prerequisite for
          // ongoing polling, so run it FIRST, then force-sync — one ordered flow, not
          // two concurrent SSH sessions racing to the same host (which interleaved the
          // logs and added load right when we were busiest). provision() swallows its
          // own errors; force-sync parses /usage directly and never needs the hook, so
          // a provision failure must not block recovery.
          // forceSync stays fire-and-forget on purpose: it ends by calling checkUsage(),
          // which the outer isChecking guard would skip if we were still awaiting here.
          initialSyncDone = true;
          ulog('first load no cache → provision then forceSync');
          await provision();
          forceSync();
        } else if (agentName === 'claudecode' && hadData && !staleResetSyncDone) {
          // Transition: had data → now null = STALE_RESET. The cache was readable until
          // this poll, so the hook is already installed — no provision needed. Recover
          // once with a single force-sync.
          staleResetSyncDone = true;
          ulog('STALE_RESET (had data → null) → forceSync (provision not needed)');
          forceSync();
        } else {
          ulog('null received but no auto-forceSync triggered', {
            reason: !hadData
              ? 'hadData=false (already null from prev poll)'
              : 'staleResetSyncDone=true (already triggered once)',
          });
        }
      }
    } catch (e) {
      ulog('IPC error', { err: String(e) });
      console.error(`Error fetching ${agentName} usage:`, e);
      error.value = e.toString();
    } finally {
      loading.value = false;
      isChecking = false;
      ulog('checkUsage done', { loading: false, isChecking: false, hasData: data.value !== null, hasError: !!error.value });
    }
  };

  const forceSync = async () => {
    if (!hostRef.value || isSyncing) {
      ulog('forceSync skip', { reason: !hostRef.value ? 'no host' : 'already syncing' });
      return;
    }
    isSyncing = true;
    loading.value = true;
    ulog('loading=true (forceSync)');
    error.value = null;
    ulog('forceSync start', { host: hostRef.value, failCount: forceSyncFailCount });

    let succeeded = false;
    try {
      ulog('invoking force_sync_agent_usage', { host: hostRef.value });
      // Rust now returns Err (rejects) when the remote script produced no output —
      // e.g. the shell died early. That lands in catch below and is treated as failure.
      const raw = await invoke('force_sync_agent_usage', { agentName, host: hostRef.value });
      ulog('force_sync_agent_usage returned', { raw_len: String(raw).length });
      let diag = null;
      try {
        diag = JSON.parse(raw);
        ulog('forceSync diagnostic', diag);
      } catch (_) {
        ulog('forceSync raw (not JSON)', { raw_preview: String(raw).slice(0, 200) });
      }
      // claude ran but its output couldn't be parsed into usable data → soft failure.
      if (diag && diag.parsed === false) {
        throw new Error(`parser did not parse (parse_error=${diag.parse_error || 'unknown'})`);
      }
      ulog('forceSync done, calling checkUsage');
      await checkUsage();
      succeeded = data.value !== null;
      ulog('forceSync complete', { data_loaded: succeeded });
    } catch (e) {
      ulog('forceSync error', { err: String(e) });
      console.error(`Error force syncing ${agentName}:`, e);
      error.value = e.toString();
    } finally {
      loading.value = false;
      isSyncing = false;
      if (succeeded) {
        forceSyncFailCount = 0;
        ulog('forceSync finally', { isSyncing: false, outcome: 'success' });
      } else {
        forceSyncFailCount++;
        if (forceSyncFailCount < MAX_FORCESYNC_RETRIES) {
          // Clear the one-shot guards so the next poll tick auto-retries (poll interval = backoff).
          initialSyncDone = false;
          staleResetSyncDone = false;
          ulog('forceSync finally', {
            isSyncing: false, outcome: 'fail-will-retry', failCount: forceSyncFailCount,
          });
        } else {
          // Give up auto-retrying; keep guards set. Manual refresh still forces a fresh attempt.
          if (!error.value) {
            error.value = `Force sync failed ${forceSyncFailCount}× — auto-retry stopped. Try manual refresh.`;
          }
          ulog('forceSync finally', {
            isSyncing: false, outcome: 'fail-giveup', failCount: forceSyncFailCount,
          });
        }
      }
    }
  };

  function restartPollTimer() {
    if (pollTimer) clearInterval(pollTimer);
    pollTimer = null;
    const s = refreshSettings.value.usage_interval_s;
    ulog('poll timer restart', { interval_s: s, host: hostRef.value });
    if (hostRef.value && s > 0) {
      pollTimer = setInterval(() => {
        ulog('poll tick', { poll: pollCount + 1, interval_s: s });
        checkUsage();
      }, s * 1000);
    }
  }

  watch(() => hostRef.value, (newHost) => {
    ulog('host changed', { newHost, resetting_all_flags: true });
    provisioned = false;
    initialSyncDone = false;
    staleResetSyncDone = false;
    isSyncing = false;
    isChecking = false;
    forceSyncFailCount = 0;
    pollCount = 0;
    data.value = null;
    error.value = null;
    if (newHost) {
      checkUsage();
    }
    restartPollTimer();
  }, { immediate: true });

  watch(() => refreshSettings.value.usage_interval_s, (newVal) => {
    ulog('interval changed', { new_interval_s: newVal });
    restartPollTimer();
  });

  watch(() => manualRefreshCount.value, (count) => {
    if (!hostRef.value) return;
    ulog('manual refresh', { count, agent: agentName });
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
    ulog('unmounted');
  });

  return {
    data,
    loading,
    error,
    stale,
    refresh: checkUsage,
    forceSync
  };
}
