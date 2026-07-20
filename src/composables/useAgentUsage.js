// @docs docs/arch/usage-claudecode.md
// @docs docs/arch/usage-antigravity.md
// @docs docs/research/claude-usage-1.2.x-analyze.md
// @docs docs/arch/logger.md
import { ref, watch, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import Swal from 'sweetalert2';
import { refreshSettings, manualRefreshCount } from '../store/refreshStore';

// The actual command run on the remote by force-sync-claudecode.sh — shown verbatim in the
// give-up debug alert so a dev can immediately try it by hand over SSH.
const FORCE_SYNC_PROMPT = 'claude --model haiku -p /usage';

function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

// Fires once, only when force-sync gives up after MAX_FORCESYNC_RETRIES attempts. Every user of
// this app is a dev — the small inline red text doesn't say what was actually run or what came
// back, so this surfaces the prompt + raw output/parse diagnostics for immediate debugging.
function showForceSyncDebugAlert({ host, error, diag }) {
  const rows = [
    ['Host', host],
    ['Prompt run', FORCE_SYNC_PROMPT],
    ['Error', error],
  ];
  if (diag) {
    rows.push(['parse_error', diag.parse_error ?? '(none)']);
    rows.push(['raw output (preview)', diag.raw_preview || '(empty)']);
    rows.push(['raw_len', diag.raw_len]);
  }
  const html = rows.map(([k, v]) => `<div style="text-align:left;margin-bottom:6px"><b>${escapeHtml(k)}:</b> <code style="white-space:pre-wrap">${escapeHtml(v)}</code></div>`).join('');
  Swal.fire({
    icon: 'error',
    title: 'Force sync gave up',
    html,
    confirmButtonText: 'OK',
    width: 560,
  });
}

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
  const email = dataObj?.email;
  if (!email) return; // malformed/partial RPC response mid-transition — never write an undefined-keyed entry
  const store = loadAgStore();
  const existing = store.accounts[email];
  // A live payload can succeed (GetUserStatus ok, exit 0) while quotaSummary is still null —
  // e.g. RetrieveUserQuotaSummary rejected independently (Promise.allSettled) right after an
  // account switch, while the language server is still re-establishing session state. Don't
  // let that partial snapshot overwrite a previously-good cached quotaSummary for this email;
  // otherwise the offline-fallback path can resurface a permanent N/A long after the real data
  // was available. Still track lastActiveEmail so the header/fallback follow the right account.
  if (dataObj.quotaSummary || !existing?.data?.quotaSummary) {
    store.accounts[email] = { data: dataObj, fetchedAt };
  }
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

// ─── Wake self-heal (P1) ─────────────────────────────────────────────────────
// WKWebView suspends/throttles setInterval when the window is fully occluded, minimized, or
// the machine sleeps — poll ticks stop silently, and every self-recovery layer built on top of
// them (STALE_RESET forceSync, OAuth poll, statusline hook) goes dormant too, since all of them
// only run when a poll tick actually fires. Two listeners, installed ONCE at module scope and
// shared by every useAgentUsage() instance (there are exactly 3: ag/ccLocal/ccRemote — see
// AgentUsageSection.vue), drive recovery:
//   1. visibilitychange/focus — immediate refresh the moment the user looks back at the app.
//   2. watchdog heartbeat — catches suspends that never flip document.visibilityState (pure
//      occlusion without a Space/window switch) or a resume that doesn't fire either DOM event.
// See docs/plan/fix-usage-monitor-freeze.md P1 for the investigation this implements.
const WATCHDOG_INTERVAL_MS = 7000;
const _wakeSubscribers = new Set(); // Set<{ onWake: (reason) => void, lastTickAt: () => number }>
let _wakeListenersInstalled = false;

function installWakeListenersOnce() {
  if (_wakeListenersInstalled) return;
  _wakeListenersInstalled = true;

  const fireWake = (reason) => {
    for (const sub of _wakeSubscribers) sub.onWake(reason);
  };

  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'visible') fireWake('visibilitychange');
  });
  window.addEventListener('focus', () => fireWake('focus'));

  setInterval(() => {
    const s = refreshSettings.value.usage_interval_s;
    if (!(s > 0)) return;
    const now = Date.now();
    for (const sub of _wakeSubscribers) {
      // Threshold is per-subscriber, not a flat 2×interval: a subscriber that has backed off
      // (unreachable host) legitimately has a much larger gap between ticks, and treating that
      // as a suspend would have the watchdog re-firing probes every 7s — defeating the very
      // backoff meant to stop hammering that host.
      if (now - sub.lastTickAt() > sub.gapThresholdMs()) sub.onWake('watchdog');
    }
  }, WATCHDOG_INTERVAL_MS);
}

export function useAgentUsage(agentName, hostRef) {
  const ulog = makeLogger(agentName);
  logStartupInfo(); // one-time: resolves debug mode, enables console output
  const a = agentName === 'antigravity' ? 'AG' : 'CC'; // abbreviation for log messages

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
  let lastTickAt = Date.now();  // ms of the last checkUsage() that actually ran — watchdog gap-detection (P1)
  let lastFetchedAt = null;     // Unix seconds of the last successful live fetch
  let lastNonNullHost = null;   // for distinguishing "toggled off" from "switched to a different host"
  let provisioned = false;
  let provisionFailCount = 0;       // bound provision retries (a down host must not retry forever)
  const MAX_PROVISION_RETRIES = 3;
  let initialSyncDone = false;
  let staleResetSyncDone = false;
  let isSyncing = false;
  let isChecking = false;
  let pendingRecheck = false; // a poll/manual-reload arrived while a check was already in flight
  // Layer 4 (retry/backoff): if a force-sync fails, allow the next poll tick to retry by
  // clearing the one-shot guards — but cap consecutive auto-retries so a genuinely broken
  // remote (claude missing, network down) doesn't spawn probe sessions forever.
  let forceSyncFailCount = 0;
  const MAX_FORCESYNC_RETRIES = 3;
  // Circuit breaker for the poll loop itself — see restartPollTimer below.
  let consecutiveFailCount = 0;
  let pollGeneration = 0;
  const BACKOFF_AFTER_FAILS = 3;
  const MAX_BACKOFF_MS = 10 * 60 * 1000;

  const provision = async () => {
    if (!hostRef.value || provisioned) return;
    provisioned = true;
    ulog('provision start', { host: hostRef.value }, 'info');
    try {
      await invoke('provision_agent_usage', { agentName, host: hostRef.value });
      ulog('provision ok', {}, 'info');
      provisionFailCount = 0;
    } catch (e) {
      // Genuine failure (transport/host down). Allow a bounded number of retries on later ticks so
      // a host coming back online gets provisioned — but never retry forever (that was the 30s
      // retry storm when the script wrongly exited 1 on empty auth; the script now exits 0, so the
      // only failures reaching here are real transport errors, which still deserve a cap).
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
    if (!hostRef.value) {
      // Source disabled — leave any last-known data in place (the host watcher below
      // already marked it isCached when the toggle flipped off) instead of wiping it.
      loading.value = false;
      return;
    }
    if (isChecking) {
      // Don't silently drop this request (e.g. a manual "Reload" click landing mid-poll,
      // common right after relaunching AG/switching accounts) — run once more immediately
      // after the in-flight check finishes instead of waiting up to a full poll interval.
      pendingRecheck = true;
      ulog('queued', {}, 'debug');
      return;
    }
    isChecking = true;
    pollCount++;
    lastTickAt = Date.now();

    ulog('check start', {
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
      ulog('invoke get', { host: hostRef.value }, 'debug');
      const res = await invoke('get_agent_usage', { agentName, host: hostRef.value });
      ulog('get ok', { hasResult: res !== null }, 'debug');
      consecutiveFailCount = 0; // host answered — drop any backoff

      if (res) {
        try {
          const parsed = JSON.parse(res.content);
          staleResetSyncDone = false;

          const fetchedAt = parseInt(res.fetched_at, 10);
          lastFetchedAt = fetchedAt;
          const nowSec = Date.now() / 1000;
          const mtimeSec = parseInt(res.file_modified_at, 10);

          // ── Stale detection ──────────────────────────────────────────────
          // file_modified_at (cache mtime), not fetched_at: for AG the two are
          // identical (the script writes fresh data on every live poll), but for
          // Claude Code fetched_at is always ≈0 right after Rust reads the file —
          // that blinded this badge to a cache frozen mid-window (statusLine/oauth
          // both silent, resets_at still in the future) — the exact freshness
          // blind spot behind Lỗi C. mtime is the data's true age either way.
          const dataAge = mtimeSec > 0 ? (nowSec - mtimeSec) : Infinity;
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
            const prevActive = activeEmail.value;
            activeEmail.value = parsed?.email || activeEmail.value;
            persistAgAccount(parsed, fetchedAt);
            refreshAccounts();
            // Auto-reset pin when the live account changes: if the user had pinned account X
            // but the live account is now Z (different email), holding the pin traps the UI
            // on X's stale cache forever — the gate below blocks every new live fetch of Z.
            // Clear viewingEmail so we follow the new live account automatically.
            if (viewingEmail.value !== null && activeEmail.value !== prevActive && viewingEmail.value !== activeEmail.value) {
              viewingEmail.value = null;
            }
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
          ulog('got data', {
            'five_hour.pct':      fiveHour?.used_percentage ?? null,
            'five_hour.resets_at': fiveHour?.resets_at ?? null,
            'five_hour.state':    fiveHour?.resets_at > 0
                                    ? (nowSec > fiveHour.resets_at ? 'PAST' : 'future')
                                    : 'no_reset',
            'seven_day.pct':      sevenDay?.used_percentage ?? null,
            mtime: mtimeSec,
            file_age_s:           mtimeSec > 0 ? Math.round(nowSec - mtimeSec) : null,
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
            ulog('cc first ok', { 'resets_at': fiveHour?.resets_at ?? null }, 'info');
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
          why: !hadData
            ? (initialSyncDone ? 'repeat' : 'no_cache')
            : 'STALE_RESET',
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
            ulog('ag offline cached', { email: targetEmail, fetchedAt: cached.fetchedAt }, 'info');
          } else {
            data.value = null;
            isCached.value = false;
            cachedAt.value = null;
            ulog('ag offline no cache', {}, 'info');
          }
        } else {
          data.value = null;
        }

        if (agentName === 'claudecode' && !initialSyncDone) {
          initialSyncDone = true;
          ulog('cc null: provision+fs', {}, 'info');
          await provision();
          forceSync();
        } else if (agentName === 'claudecode' && hadData && !staleResetSyncDone) {
          staleResetSyncDone = true;
          ulog('cc STALE_RESET: fs', {}, 'info');
          forceSync();
        } else {
          ulog('null: skip fs', {
            reason: !hadData ? 'prev null' : 'stale done',
          }, 'debug');
        }
      }
    } catch (e) {
      consecutiveFailCount++;
      ulog('IPC error', { err: String(e), fails: consecutiveFailCount }, 'error');
      error.value = e.toString();
    } finally {
      loading.value = false;
      isChecking = false;
      ulog('check done', { hasData: data.value !== null, hasError: !!error.value }, 'debug');
      if (pendingRecheck) {
        pendingRecheck = false;
        checkUsage();
      }
    }
  };

  const forceSync = async () => {
    if (!hostRef.value || isSyncing) {
      ulog('fs skip', { reason: !hostRef.value ? 'no host' : 'syncing' }, 'debug');
      return;
    }
    isSyncing = true;
    loading.value = true;
    ulog('loading=true (fs)', {}, 'debug');
    error.value = null;
    ulog('fs start', { host: hostRef.value, failCount: forceSyncFailCount }, 'info');

    let succeeded = false;
    let diag = null;
    try {
      ulog('invoke fs', { host: hostRef.value }, 'debug');
      // Rust now returns Err (rejects) when the remote script produced no output —
      // e.g. the shell died early. That lands in catch below and is treated as failure.
      const raw = await invoke('force_sync_agent_usage', { agentName, host: hostRef.value });
      ulog('fs invoke ok', { raw_len: String(raw).length }, 'debug');
      try {
        diag = JSON.parse(raw);
        ulog('fs diag', diag, 'debug');
      } catch (_) {
        ulog('fs raw (not JSON)', { raw_preview: String(raw).slice(0, 200) }, 'debug');
      }
      // claude ran but its output couldn't be parsed into usable data → soft failure.
      if (diag && diag.parsed === false) {
        throw new Error(`parser did not parse (parse_error=${diag.parse_error || 'unknown'})`);
      }
      ulog('fs done: checkUsage', {}, 'info');
      await checkUsage();
      succeeded = data.value !== null;
      ulog('fs complete', { data_loaded: succeeded }, 'info');
    } catch (e) {
      ulog('fs err', { err: String(e) }, 'error');
      error.value = e.toString();
    } finally {
      loading.value = false;
      isSyncing = false;
      if (succeeded) {
        forceSyncFailCount = 0;
        ulog('fs finally', { outcome: 'ok' }, 'info');
      } else {
        forceSyncFailCount++;
        if (forceSyncFailCount < MAX_FORCESYNC_RETRIES) {
          // Clear the one-shot guards so the next poll tick auto-retries (poll interval = backoff).
          initialSyncDone = false;
          staleResetSyncDone = false;
          ulog('fs finally', { outcome: 'retry', n: forceSyncFailCount }, 'info');
        } else {
          // Give up auto-retrying; keep guards set. Manual refresh still forces a fresh attempt.
          if (!error.value) {
            error.value = `Force sync failed ${forceSyncFailCount}× — auto-retry stopped. Try manual refresh.`;
          }
          ulog('fs finally', { outcome: 'giveup', n: forceSyncFailCount }, 'error');
          // The raw prompt + stderr dump is a DEV diagnostic, not a user-facing error. A normal
          // user must never see it (a first-run/reset-boundary give-up reads as a crash) — the
          // inline red text above already conveys the failure. Surface the modal only under
          // --debug, where the operator explicitly asked for diagnostics.
          if (_isDebugMode) {
            showForceSyncDebugAlert({ host: hostRef.value, error: error.value, diag });
          }
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
    ulog('ag select', { email, active: activeEmail.value }, 'info');
  };

  // AG-only: called right after a successful logout. logout_antigravity wipes AG's own auth state
  // (SQLite rows, keychain item, session cookies) but this composable's own cache/view-state is
  // deliberately left untouched — see "Log Out behavior & cache retention" in
  // docs/arch/usage-antigravity.md (PO decision, 2026-07-07): the header showing the just-logged-out
  // account's last-known data until a new account goes live is the INTENDED behavior (the whole
  // point of the per-account cache is to keep showing each account's last-known state), not a bug.
  //
  // Regression note: 1.9.3 (`a26b8f5`/`b082d0d`) treated that as a bug and cleared the account on
  // logout — `clearAgStore()` ended up wiping the ENTIRE per-account history, not just the
  // just-logged-out account, silently erasing every other cached account too. Fixed 2026-07-07 by
  // removing the clearing behavior entirely, per the corrected product decision above.
  const resetAccount = () => {
    if (agentName !== 'antigravity') return;
    ulog('ag logout: recheck', {}, 'info');
    checkUsage(); // just an immediate poll to pick up a new login sooner — no state is cleared
  };

  // Circuit breaker (P0). A fixed `setInterval` kept firing a fresh SSH every 30s at a host that
  // had already stopped answering — and since the script timeout is also 30s, each tick landed
  // exactly as the previous one was killed, so the remote never got an idle moment to recover.
  // Every failed probe can strand child processes remotely, so the old loop fed the very
  // overload it was failing on. Two structural changes, together:
  //   1. chained setTimeout instead of setInterval — the next probe is scheduled only after the
  //      previous one has finished, so probes can never overlap however slow the host is;
  //   2. exponential backoff after BACKOFF_AFTER_FAILS consecutive failures, capped at
  //      MAX_BACKOFF_MS, cleared the instant the host answers again.
  // See docs/research/ssh-process-leak-remote-ram-overflow.md §7 P0/P4.
  function nextPollDelayMs() {
    const base = refreshSettings.value.usage_interval_s * 1000;
    if (consecutiveFailCount < BACKOFF_AFTER_FAILS) return base;
    const steps = Math.min(consecutiveFailCount - BACKOFF_AFTER_FAILS + 1, 6);
    return Math.min(base * 2 ** steps, MAX_BACKOFF_MS);
  }

  function restartPollTimer() {
    if (pollTimer) clearTimeout(pollTimer);
    pollTimer = null;
    // A restart landing while a probe is mid-flight must retire that probe's chain, otherwise
    // the old chain reschedules itself on completion and we end up with two loops polling.
    const gen = ++pollGeneration;
    const s = refreshSettings.value.usage_interval_s;
    ulog('poll timer restart', { interval_s: s }, 'debug');
    if (!hostRef.value || s <= 0) return;

    const schedule = () => {
      if (gen !== pollGeneration) return;
      const delay = nextPollDelayMs();
      if (delay !== s * 1000) {
        ulog('poll backoff', { delay_ms: delay, fails: consecutiveFailCount }, 'info');
      }
      pollTimer = setTimeout(async () => {
        ulog('poll tick', { poll: pollCount + 1 }, 'debug');
        try {
          await checkUsage();
        } finally {
          schedule();
        }
      }, delay);
    };
    schedule();
  }

  // P1 wake self-heal: triggered by visibilitychange/focus or the watchdog heartbeat (module
  // scope, see installWakeListenersOnce above) after a suspected WKWebView suspend. Re-checks
  // immediately and restarts the interval — a suspended setInterval does not reliably resume
  // ticking on its own even once the page is visible/focused again.
  function onWake(reason) {
    if (!hostRef.value) return; // source disabled — nothing to recover
    ulog('wake', { reason, gap_ms: Date.now() - lastTickAt }, 'info');
    lastTickAt = Date.now(); // prevent the watchdog re-firing every heartbeat while this check is in flight
    checkUsage();
    restartPollTimer();
  }
  installWakeListenersOnce();
  const _wakeSub = {
    onWake,
    lastTickAt: () => lastTickAt,
    gapThresholdMs: () => 2 * nextPollDelayMs(),
  };
  _wakeSubscribers.add(_wakeSub);

  watch(() => hostRef.value, (newHost) => {
    // A source can be toggled off/on (host -> null -> same host again) without ever
    // actually changing which machine it points at. Only wipe data on a REAL host
    // change (e.g. switching the selected SSH remote) — toggling off should keep the
    // last-known reading on screen, marked as cached, not blank the card.
    const realHostChange = !!newHost && !!lastNonNullHost && newHost !== lastNonNullHost;
    ulog('host change', { newHost, lastNonNullHost, realHostChange }, 'info');
    provisioned = false;
    provisionFailCount = 0;
    initialSyncDone = false;
    staleResetSyncDone = false;
    isSyncing = false;
    isChecking = false;
    forceSyncFailCount = 0;
    consecutiveFailCount = 0;
    pollCount = 0;
    lastTickAt = Date.now();
    error.value = null;

    if (realHostChange) {
      data.value = null;
      isCached.value = false;
      cachedAt.value = null;
    } else if (!newHost && data.value !== null) {
      isCached.value = true;
      cachedAt.value = lastFetchedAt;
    }

    if (newHost) {
      lastNonNullHost = newHost;
      checkUsage();
    }
    restartPollTimer();
  }, { immediate: true });

  watch(() => refreshSettings.value.usage_interval_s, (newVal) => {
    ulog('interval changed', { interval_s: newVal }, 'debug');
    restartPollTimer();
  });

  watch(() => manualRefreshCount.value, (count) => {
    if (!hostRef.value) return;
    ulog('refresh', { count, a }, 'info');
    // An explicit user action always overrides the circuit breaker — they may well have just
    // fixed the host, and making them wait out a 10-minute backoff would be absurd.
    consecutiveFailCount = 0;
    restartPollTimer();
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
    pollGeneration++; // retire any in-flight probe's chain so it can't reschedule after unmount
    if (pollTimer) clearTimeout(pollTimer);
    pollTimer = null;
    _wakeSubscribers.delete(_wakeSub);
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
    selectAccount,
    resetAccount
  };
}
