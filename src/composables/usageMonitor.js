// @docs docs/arch/usage-claudecode.md
// @docs docs/arch/usage-antigravity.md
// @docs docs/plan/done/usage-monitor-entity-refactor.md
// @docs docs/plan/done/1.16.1-ag-usage.md
// @docs docs/research/claudecode-usage-FINAL.md
// @docs docs/arch/logger.md
//
// Single UsageMonitor watching ONE agent on ONE machine with immutable identity.
// Created via usageMonitorRegistry.getMonitor() to share instances across slots and deduplicate SSH round trips.
// Readings live in store/usageReadingStore.js (mirrored to companions).
import { ref, computed, watch } from 'vue';
import { invoke } from '../utils/tauri';
import { isHost } from '../services/bridge';
import { hostInterval } from '../utils/scheduler';
import { refreshSettings, manualRefreshCount } from '../store/refreshStore';
import { usageReading, patchUsageReading } from '../store/usageReadingStore';
import { persistAgAccount, loadAgAccount, listAgAccounts, lastActiveEmailFor, lastActiveKeyFor } from './agUsageCache';

// ─── Logger ──────────────────────────────────────────────────────────────────
// Logs to frontend console first, then forwards to Rust backend (file + stderr) via fire-and-forget IPC.

let _isDebugMode = false;

// Compact timestamp matching Rust format YYYYMMDD.HHMMSS.mmm (local time for JS).
function fmtNow() {
  const d = new Date();
  const p2 = n => String(n).padStart(2, '0');
  const p3 = n => String(n).padStart(3, '0');
  return `${d.getFullYear()}${p2(d.getMonth()+1)}${p2(d.getDate())}.` +
         `${p2(d.getHours())}${p2(d.getMinutes())}${p2(d.getSeconds())}.` +
         `${p3(d.getMilliseconds())}`;
}

// Tagged with monitor identity (`USAGE:claudecode@hostB`) for log separability in multi-host setups.
function makeLogger(id) {
  const tag = `USAGE:${id}`;
  return function ulog(event, fields = {}, level = 'debug') {
    const pairs = Object.entries(fields).map(([k, v]) => `${k}=${JSON.stringify(v)}`).join(' ');
    const msg   = `${event}${pairs ? ' ' + pairs : ''}`;
    const line  = `[${fmtNow()}][${tag}] ${msg}`;

    // DevTools console: error always logged; info/debug only when debug mode is active.
    if (level === 'error') {
      console.error(line);
    } else if (_isDebugMode) {
      if (level === 'info') console.info(line);
      else                  console.log(line);
    }

    // Forward to Rust backend (file + stderr) via fire-and-forget IPC.
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
    if (_isDebugMode) {
      console.info(`[${fmtNow()}][USAGE:init] debug_mode=true log_file=${logPath}`);
      console.info(`[${fmtNow()}][USAGE:init] Frontend logs → console + backend pipeline.`);
    }
  } catch (_) {
    // Debug mode stays off if the IPC read fails - logging simply keeps quiet.
  }
}

// ─── Wake self-heal (P1) ─────────────────────────────────────────────────────
// Shared visibilitychange/focus and watchdog heartbeat listeners to recover from WKWebView throttling/suspends.
const WATCHDOG_INTERVAL_MS = 7000;
const _wakeSubscribers = new Set(); // Set<{ onWake: (reason) => void, lastTickAt: () => number }>
let _wakeListenersInstalled = false;

function installWakeListenersOnce() {
  if (_wakeListenersInstalled) return;
  // Host-only: companions do not run probe cycles (seam P).
  if (!isHost) return;
  _wakeListenersInstalled = true;

  const fireWake = (reason) => {
    for (const sub of _wakeSubscribers) sub.onWake(reason);
  };

  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'visible') fireWake('visibilitychange');
  });
  window.addEventListener('focus', () => fireWake('focus'));

  hostInterval(() => {
    const now = Date.now();
    for (const sub of _wakeSubscribers) {
      // Per-subscriber threshold prevents false wakeups when a subscriber is backed off or disabled.
      const threshold = sub.gapThresholdMs();
      if (threshold > 0 && Number.isFinite(threshold) && now - sub.lastTickAt() > threshold) sub.onWake('watchdog');
    }
  }, WATCHDOG_INTERVAL_MS);
}

/**
 * Single wake/self-heal mechanism: subscribers notify on WKWebView suspend recovery.
 * gapThresholdMs() returning <= 0 or non-finite skips watchdog heartbeat for that subscriber.
 */
export function subscribeWake(sub) {
  installWakeListenersOnce();
  _wakeSubscribers.add(sub);
  return () => _wakeSubscribers.delete(sub);
}

/**
 * Build one monitor with immutable identity (agentId, host) and reactive policy (enabled, locked).
 * Call through usageMonitorRegistry.getMonitor(), not directly.
 */
export function createUsageMonitor({ id, agentId, host, enabled, locked, toggle }) {
  const ulog = makeLogger(id);
  logStartupInfo(); // one-time: resolves debug mode, enables console output
  const isAg = agentId === 'antigravity';

  // Readings are mirrored in usageReadingStore; stale state is derived from dataAt in the card.
  const reading = computed(() => usageReading(id));
  // Host-only write funnel: companions receive readings via mirror and must not mutate store directly.
  const set = (patch) => { if (isHost) patchUsageReading(id, patch); };

  const data = computed(() => reading.value.data);
  const loading = computed(() => reading.value.loading);
  const error = computed(() => reading.value.error);
  // Cache file mtime in Unix seconds (clock staleness baseline; for Claude Code, written by statusline hook).
  const dataAt = computed(() => reading.value.dataAt);
  // AG-only: tracks whether current data is from cache (AG offline) and when it was cached
  const isCached = computed(() => reading.value.isCached);
  const cachedAt = computed(() => reading.value.cachedAt); // Unix seconds

  // AG-only multi-account state scoped to this host; account pin selection is owned per-slot.
  const accounts = computed(() => reading.value.accounts);       // dropdown list [{ email, fetchedAt }] sorted newest-first
  const activeEmail = computed(() => reading.value.activeEmail); // email of the primary successful live fetch
  const activeEmails = computed(() => reading.value.activeEmails); // Set of emails of all currently live accounts
  const refreshAccounts = () => { set({ accounts: listAgAccounts(host) }); };
  // Seeded from THIS host's own cache. set is host-only so companion receives mirrored data.
  if (isAg && isHost) {
    set({ accounts: listAgAccounts(host), activeEmail: lastActiveEmailFor(host) });
  }

  let pollTimer = null;
  let pollCount = 0;
  let lastTickAt = Date.now();  // ms of the last checkUsage() that actually ran - watchdog gap-detection (P1)
  let lastFetchedAt = null;     // Unix seconds of the last successful live fetch
  let provisioned = false;
  let provisionFailCount = 0;       // bound provision retries (a down host must not retry forever)
  const MAX_PROVISION_RETRIES = 3;
  // Epoch generation tokens: guarantees results commit in issue order and discards obsolete probe responses.
  let epoch = 0;
  let inFlightEpoch = 0;
  let pendingRecheck = false; // a poll/manual-reload arrived while a check was already in flight
  // Active holder flag (registry refcount > 0): prevents polling when no mounted view is displaying this monitor.
  let watching = true;
  let unsubscribeFromWake = null;
  // Circuit breaker for the poll loop itself - see restartPollTimer below.
  let consecutiveFailCount = 0;
  // Mirrored breaker state (contract C-3): controls the slot's amber halt icon across host and companions.
  const pollHalted = computed(() => reading.value.pollHalted);
  const MAX_CONSECUTIVE_FAILS = 5;

  /** Reachability failure counter for circuit breaker: counts transport/host failures, not empty readings. */
  function noteUnreachable(reason) {
    consecutiveFailCount++;
    ulog('host unreachable', { reason, fails: consecutiveFailCount }, 'error');
    if (consecutiveFailCount >= MAX_CONSECUTIVE_FAILS) haltPolling();
  }

  const provision = async () => {
    if (!enabled.value || provisioned) return;
    provisioned = true;
    ulog('provision start', { host }, 'info');
    try {
      await invoke('provision_agent_usage', { agentName: agentId, host });
      ulog('provision ok', {}, 'info');
      provisionFailCount = 0;
    } catch (e) {
      // Bounded retries for transport/host failure; prevents perpetual retry loops if host remains down.
      provisionFailCount += 1;
      if (provisionFailCount < MAX_PROVISION_RETRIES) {
        provisioned = false;
      } else {
        ulog('provision giveup', { n: provisionFailCount }, 'error');
      }
      ulog('provision err', { err: String(e), n: provisionFailCount }, 'error');
    }
  };

  const checkUsage = async () => {
    // Producer gate (seam P): companion never dispatches probes; all probe paths funnel here.
    if (!isHost) return;
    // Nothing is displaying this monitor any more (registry refcount 0).
    if (!watching) return;
    if (!enabled.value) {
      // Monitoring off - leave any last-known data in place instead of wiping it.
      set({ loading: false });
      return;
    }
    if (inFlightEpoch) {
      // Queue recheck to run immediately after in-flight check completes (e.g. manual reload during poll).
      pendingRecheck = true;
      ulog('queued', {}, 'debug');
      return;
    }
    const myEpoch = ++epoch;
    inFlightEpoch = myEpoch;
    pollCount++;
    lastTickAt = Date.now();

    ulog('check start', {
      host,
      poll: pollCount,
      hadData: data.value !== null,
    }, 'debug');

    set({ loading: true, error: null });
    ulog('loading=true', {}, 'debug');

    try {
      const hadData = data.value !== null;
      ulog('invoke get', { host }, 'debug');
      const result = await invoke('get_agent_usage', { agentName: agentId, host });
      // Drop results if superseded while in-flight (maintains issue-order consistency).
      if (myEpoch !== epoch) {
        ulog('result discarded - superseded', { issued: myEpoch, current: epoch }, 'debug');
        return;
      }
      // Reset breaker only if host actually answered; note unreachable if connection failed.
      const res = result?.data ?? null;
      ulog('get ok', { answered: !!result?.host_answered, hasData: res !== null, miss: result?.miss_reason ?? null }, 'debug');
      if (result?.host_answered) consecutiveFailCount = 0;
      else noteUnreachable(result?.miss_reason || 'host did not answer');

      if (res) {
        try {
          const parsed = JSON.parse(res.content);

          const fetchedAt = parseInt(res.fetched_at, 10);
          lastFetchedAt = fetchedAt;
          const nowSec = Date.now() / 1000;
          const mtimeSec = parseInt(res.file_modified_at, 10);

          // Freshness evaluation uses cache mtime (file_modified_at), reflecting true data age.
          let resetIsPast = false;
          if (!isAg) {
            // Script-side staleness contract evaluates 5-hour rate limit window specifically.
            const fh = parsed?.rate_limits?.five_hour;
            resetIsPast = fh?.resets_at > 0 && nowSec > fh.resets_at;
          }
          // UI staleness baseline: valid mtime in Unix seconds, or null if unknown.
          const nextDataAt = mtimeSec > 0 ? mtimeSec : null;

          if (isAg) {
            // Record live fetch under account email and refresh accounts list; slot resolves pinned views.
            const nextActiveEmail = parsed?.email || activeEmail.value;

            const liveList = [];
            if (Array.isArray(parsed?.allAccounts) && parsed.allAccounts.length > 0) {
              for (const a of parsed.allAccounts) {
                if (a.email) liveList.push(a.email);
              }
            } else if (parsed?.email) {
              liveList.push(parsed.email);
            }
            persistAgAccount(parsed, fetchedAt, host);
            set({
              dataAt: nextDataAt,
              activeEmail: nextActiveEmail,
              activeEmails: new Set(liveList),
              accounts: listAgAccounts(host),
              data: parsed,
              isCached: false,
              cachedAt: null,
            });
            ulog('ag live fetched', { email: nextActiveEmail, liveCount: liveList.length, fetchedAt }, 'debug');
          } else {
            set({ dataAt: nextDataAt, data: parsed, isCached: false, cachedAt: null });
          }

          const fiveHour = parsed?.rate_limits?.five_hour;
          // Log all rate limit buckets carried in the payload for observability.
          const bucketFields = {};
          for (const [k, b] of Object.entries(parsed?.rate_limits || {})) {
            if (!b || typeof b !== 'object') continue;
            bucketFields[`${k}.pct`] = b.used_percentage ?? null;
            bucketFields[`${k}.resets_at`] = b.resets_at ?? null;
            bucketFields[`${k}.state`] = b.resets_at > 0
              ? (nowSec > b.resets_at ? 'PAST' : 'future')
              : 'no_reset';
          }
          ulog('got data', {
            ...bucketFields,
            mtime: mtimeSec,
            file_age_s:           mtimeSec > 0 ? Math.round(nowSec - mtimeSec) : null,
            reset_overdue_s:      resetIsPast ? Math.round(nowSec - fiveHour.resets_at) : null,
            until_reset_s:        (!resetIsPast && fiveHour?.resets_at > 0)
                                    ? Math.round(fiveHour.resets_at - nowSec) : null,
          }, 'info');

          // Idempotent re-provision for active hosts once per session to ensure hook updates.
          if (!isAg && !provisioned) provision();
        } catch (e) {
          ulog('parse error', { err: String(e), content_preview: String(res.content).slice(0, 100) }, 'error');
          set({ error: 'Invalid usage data format.' });
        }
      } else {
        // Server returned null data; reason logged from backend miss_reason.
        ulog('got null', { hadData, why: result?.miss_reason ?? 'unknown' }, 'info');

        // AG offline: fallback to last-active account cache deterministically.
        if (isAg) {
          refreshAccounts();
          // Handle key includes sourceType (email:sourceType) to disambiguate IDE vs CLI quotas.
          const lastActiveKey = lastActiveKeyFor(host);
          if (!activeEmail.value) set({ activeEmail: lastActiveEmailFor(host) });
          const cached = loadAgAccount(lastActiveKey, host);
          if (cached) {
            set({ data: cached.data, isCached: true, cachedAt: cached.fetchedAt });
            ulog('ag offline cached', { account: lastActiveKey, fetchedAt: cached.fetchedAt }, 'info');
          } else {
            set({ data: null, isCached: false, cachedAt: null });
            ulog('ag offline no cache', {}, 'info');
          }
        } else if (hadData) {
          // STALE_RESET: past reset boundary without new CC turn; preserve last reading as cached.
          set({ isCached: true, cachedAt: lastFetchedAt });
          ulog('cc STALE_RESET: keep cached', { cachedAt: lastFetchedAt }, 'info');
        } else {
          set({ data: null });
        }

        if (!isAg) provision();
      }
    } catch (e) {
      // IPC error (timeout or spawn failure): record error and increment unreachable counter.
      ulog('IPC error', { err: String(e) }, 'error');
      if (myEpoch === epoch) {
        set({ error: e.toString() });
        noteUnreachable(String(e));
      }
    } finally {
      // Reset loading and in-flight state only if this probe remains the active epoch.
      if (inFlightEpoch === myEpoch) {
        inFlightEpoch = 0;
        set({ loading: false });
        ulog('check done', { hasData: data.value !== null, hasError: !!error.value }, 'debug');
        if (pendingRecheck) {
          pendingRecheck = false;
          checkUsage();
        }
      }
    }
  };

  // Circuit breaker: hard stop after MAX_CONSECUTIVE_FAILS unreachable probes; resumes only on user action.
  function restartPollTimer() {
    if (pollTimer) clearInterval(pollTimer);
    pollTimer = null;
    const s = refreshSettings.value.usage_interval_s;
    ulog('poll timer restart', { interval_s: s }, 'debug');
    if (!watching || !enabled.value || !(s > 0)) return;
    if (pollHalted.value) {
      ulog('poll halted - not restarting', { fails: consecutiveFailCount }, 'info');
      return;
    }
    pollTimer = hostInterval(() => {
      ulog('poll tick', { poll: pollCount + 1 }, 'debug');
      checkUsage();
    }, s * 1000);
  }

  /** Trips the breaker: stops the timer and sets error message with host name. */
  function haltPolling() {
    if (pollTimer) clearInterval(pollTimer);
    pollTimer = null;
    // Host name in error message clarifies target for user and power icon tooltip.
    set({
      pollHalted: true,
      error: `Host "${host}" unreachable ${consecutiveFailCount}× in a row - polling stopped. Click the power icon to retry.`,
    });
    ulog('poll halted', { host, fails: consecutiveFailCount }, 'error');
  }

  /** Clears the breaker after an explicit user action (refresh / switched back on). */
  function resumePolling() {
    consecutiveFailCount = 0;
    if (pollHalted.value) set({ pollHalted: false });
  }

  /** Amber power icon click (contract C-3): clears breaker and retries probe immediately. */
  function retryAfterHalt() {
    ulog('retry after halt', { host }, 'info');
    resumePolling();
    set({ error: null });
    restartPollTimer();
    checkUsage();
  }

  // Wake self-heal handler: re-checks immediately and restarts interval upon visibilitychange/focus/watchdog.
  function onWake(reason) {
    if (!enabled.value) {
      // Bump lastTickAt to prevent watchdog repeatedly firing gap checks on disabled monitors.
      lastTickAt = Date.now();
      return;
    }
    // Halted monitors stay stopped on wake events; only explicit user retry/refresh resumes probing.
    if (pollHalted.value) {
      lastTickAt = Date.now();
      ulog('wake ignored - polling halted', { reason }, 'debug');
      return;
    }
    ulog('wake', { reason, gap_ms: Date.now() - lastTickAt }, 'info');
    lastTickAt = Date.now(); // prevent the watchdog re-firing every heartbeat while this check is in flight
    checkUsage();
    restartPollTimer();
  }
  const wakeSubscription = {
    onWake,
    lastTickAt: () => lastTickAt,
    // Watchdog skips disabled or inactive monitors when threshold is 0.
    gapThresholdMs: () => (watching && enabled.value ? 2 * refreshSettings.value.usage_interval_s * 1000 : 0),
  };
  unsubscribeFromWake = subscribeWake(wakeSubscription);

  /** Releases hold: stops poll timer and wake subscription when no views are active. */
  function stopWatching() {
    if (!watching) return;
    watching = false;
    // Discard any in-flight probe by incrementing epoch.
    epoch++;
    inFlightEpoch = 0;
    pendingRecheck = false;
    if (pollTimer) clearInterval(pollTimer);
    pollTimer = null;
    if (unsubscribeFromWake) { unsubscribeFromWake(); unsubscribeFromWake = null; }
    ulog('stopped watching - no holders', { host }, 'info');
  }

  /** Re-arms poll timer and probes once when a view targets this monitor again. */
  function startWatching() {
    if (watching) return;
    watching = true;
    unsubscribeFromWake = subscribeWake(wakeSubscription);
    lastTickAt = Date.now();
    ulog('watching again', { host }, 'info');
    restartPollTimer();
    checkUsage();
  }

  // Reactive policy watcher: toggling off preserves cached reading; toggling on initiates fresh check.
  watch(enabled, (on) => {
    ulog('enabled', { on, host }, 'info');
    provisioned = false;
    provisionFailCount = 0;
    // Bump epoch to invalidate in-flight probes from previous state.
    epoch++;
    inFlightEpoch = 0;
    pendingRecheck = false;
    resumePolling();
    pollCount = 0;
    lastTickAt = Date.now();
    set({ error: null });

    if (!on) {
      if (data.value !== null) set({ isCached: true, cachedAt: lastFetchedAt });
    } else {
      // Host-gated inside checkUsage (seam P); companion receives mirrored state.
      checkUsage();
    }
    restartPollTimer();
  }, { immediate: true });

  watch(() => refreshSettings.value.usage_interval_s, (newVal) => {
    ulog('interval changed', { interval_s: newVal }, 'debug');
    restartPollTimer();
  });

  watch(() => manualRefreshCount.value, (count) => {
    if (!enabled.value) return;
    ulog('refresh', { count }, 'info');
    // An explicit user action always clears the breaker - they may well have just fixed the host.
    resumePolling();
    restartPollTimer();
    // Manual refresh (Reload / Refresh buttons) = normal fresh load for all agents.
    checkUsage();
  });

  return {
    // Identity - immutable, and the same string the store, the registry and usage.log use.
    id,
    agentId,
    host,
    // Policy, owned by the registry/store.
    enabled,
    locked,
    toggle,
    // Breaker state, for the power icon (contract C-3): amber while halted, and clicking it retries instead of toggling the monitor off.
    pollHalted,
    retryAfterHalt,
    // Lifecycle, driven by the registry's refcount - never by a component directly.
    stopWatching,
    startWatching,
    // Readings (mirrored, keyed by `id`, in store/usageReadingStore.js).
    data,
    loading,
    error,
    dataAt,
    isCached,
    cachedAt,
    refresh: checkUsage,
    // AG-only: what is currently signed in on this machine, for the slot's account dropdown to render and choose from (harmless/unused for Claude Code).
    accounts,
    activeEmail,
    activeEmails,
  };
}
