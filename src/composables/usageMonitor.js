// @docs docs/arch/usage-claudecode.md
// @docs docs/arch/usage-antigravity.md
// @docs docs/plan/done/usage-monitor-entity-refactor.md
// @docs docs/plan/done/1.16.1-ag-usage.md
// @docs docs/research/claudecode-usage-FINAL.md
// @docs docs/arch/logger.md
//
// One UsageMonitor watches ONE agent on ONE machine, and never changes which.
//
// This file was `useAgentUsage.js`, a composable taking a reactive `hostRef` that callers retargeted at runtime. That shape is what made two remote hosts unwatchable, and it dragged a whole family of guards along with it - `lastNonNullHost`, `realHostChange`, the retarget-and-wipe branch, and a `!hostRef.value` early return in five places. Making the host immutable and part of the identity deletes all of them: a monitor cannot be pointed somewhere else, so there is nothing to detect, nothing to reset, and nothing to discard. Whether it is currently polling is the ONLY thing that varies, and that is `enabled`.
//
// Instances are created through `usageMonitorRegistry.getMonitor()`, never directly - identity is the registry's job, and two slots naming the same (agent, host) must share one instance so a single SSH round trip serves both. A monitor outlives whichever component happened to ask for it first, so it is NOT torn down on that component's unmount; the registry refcounts holders instead and calls `stopWatching()` only when the last one lets go (`releaseMonitor`). The instance and its last reading survive that - only the poll timer and the wake subscription stop.
//
// A monitor's READING lives in `store/usageReadingStore.js`, not in refs here: only `src/store/*.js` is mirrored to companions (services/mirror.js), and a reading that cannot reach the phone is a phone with empty cards - which is what used to push it into running probes of its own.
import { ref, computed, watch } from 'vue';
import { invoke } from '../utils/tauri';
import { isHost } from '../services/bridge';
import { hostInterval } from '../utils/scheduler';
import { refreshSettings, manualRefreshCount } from '../store/refreshStore';
import { usageReading, patchUsageReading } from '../store/usageReadingStore';
import { persistAgAccount, loadAgAccount, listAgAccounts, lastActiveEmailFor, lastActiveKeyFor } from './agUsageCache';

// ─── Logger ──────────────────────────────────────────────────────────────────
// Three levels matching logger.rs contract:
//   error → always: console.error + backend (file + stderr)
//   info  → debug-only: console.info  + backend
//   debug → debug-only: console.log   + backend
//
// Frontend console is printed FIRST (preserves DevTools source-line links), then the line is forwarded to the Rust backend via fire-and-forget IPC so all events appear in the same usage.log and terminal stderr, interleaved in real chronological order with Rust log entries.
//
// Timestamp format: YYYYMMDD.HHMMSS.mmm - compact, matches Rust now_human().

let _isDebugMode = false;

// Compact timestamp matching Rust format YYYYMMDD.HHMMSS.mmm (local time for JS)
function fmtNow() {
  const d = new Date();
  const p2 = n => String(n).padStart(2, '0');
  const p3 = n => String(n).padStart(3, '0');
  return `${d.getFullYear()}${p2(d.getMonth()+1)}${p2(d.getDate())}.` +
         `${p2(d.getHours())}${p2(d.getMinutes())}${p2(d.getSeconds())}.` +
         `${p3(d.getMilliseconds())}`;
}

// Tagged with the monitor's full identity (`USAGE:claudecode@hostB`) rather than just the agent, so two hosts' lines are separable in one usage.log - the whole point of the entity split.
function makeLogger(id) {
  const tag = `USAGE:${id}`;
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

    // 2) Forward to Rust backend (file + stderr) - fire-and-forget
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
// WKWebView suspends/throttles setInterval when the window is fully occluded, minimized, or the machine sleeps - poll ticks stop silently, and every self-recovery layer built on top of them (the statusline hook writing fresh data) goes dormant too, since all of them only run when a poll tick actually fires. Two listeners, installed ONCE at module scope and shared by every monitor in the registry, drive recovery:
//   1. visibilitychange/focus - immediate refresh the moment the user looks back at the app.
//   2. watchdog heartbeat - catches suspends that never flip document.visibilityState (pure occlusion without a Space/window switch) or a resume that doesn't fire either DOM event.
// See docs/arch/usage-claudecode.md §4 (WKWebView suspend self-heal).
const WATCHDOG_INTERVAL_MS = 7000;
const _wakeSubscribers = new Set(); // Set<{ onWake: (reason) => void, lastTickAt: () => number }>
let _wakeListenersInstalled = false;

function installWakeListenersOnce() {
  if (_wakeListenersInstalled) return;
  // Host-only, like the watchdog's own `hostInterval` below and `useBackgroundRefresh`'s guard:
  // there is nothing to self-heal on a companion (it produces no cycles - seam P), and every
  // subscriber's onWake ultimately asks the HOST to do work. A phone regaining focus is not a
  // reason for the Mac to probe anything.
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
      // Threshold is per-subscriber, not a flat 2×interval: a subscriber that has backed off (unreachable host) legitimately has a much larger gap between ticks, and treating that as a suspend would have the watchdog re-firing probes every 7s - defeating the very backoff meant to stop hammering that host. A subscriber whose own cycle is switched off returns Infinity and is simply never woken.
      const threshold = sub.gapThresholdMs();
      if (threshold > 0 && Number.isFinite(threshold) && now - sub.lastTickAt() > threshold) sub.onWake('watchdog');
    }
  }, WATCHDOG_INTERVAL_MS);
}

/**
 * The ONE wake/self-heal mechanism in this app. Any cycle that can be silently frozen by a WKWebView
 * suspend (usage polls, background git/diff refresh) subscribes here rather than installing its own
 * listeners and its own heartbeat - two watchdogs would double the wake-up cost and mean two places
 * to reason about when recovery misbehaves. Decided in docs/plan/done/1.20.1-flow-audit-fixes.md §3.21.
 *
 * `gapThresholdMs()` returning 0 or a non-finite value means "my cycle is off right now" - the
 * watchdog skips that subscriber instead of the whole heartbeat, so one disabled cycle can never
 * silence another's recovery.
 */
export function subscribeWake(sub) {
  installWakeListenersOnce();
  _wakeSubscribers.add(sub);
  return () => _wakeSubscribers.delete(sub);
}

/**
 * Build one monitor. `agentId` and `host` are immutable identity; `enabled` and `locked` are reactive policy owned by the caller (the registry, reading `usageMonitorStore`).
 *
 * Call through `usageMonitorRegistry.getMonitor()`, not directly.
 */
export function createUsageMonitor({ id, agentId, host, enabled, locked, toggle }) {
  const ulog = makeLogger(id);
  logStartupInfo(); // one-time: resolves debug mode, enables console output
  const isAg = agentId === 'antigravity';

  // The reading is stored under this monitor's id in `usageReadingStore` (mirrored), and read back
  // here as computeds so the shape callers destructure is unchanged. Writes go through `set()`.
  // `stale` is deliberately NOT among them: it was duplicated state only a successful fetch ever
  // updated, so past a reset boundary with no new turn it stayed false forever while `dataAt` aged
  // correctly. It is now derived in the card from `dataAt` + the clock that card already ticks, and
  // no code path can forget to update it.
  const reading = computed(() => usageReading(id));
  // Host-only by construction. The reading is measured on the Mac and travels down the mirror; a
  // companion writing into its own copy could only produce a value the next delta contradicts (an
  // amber breaker icon cleared on the phone while the host is still halted, say). Making the ONE
  // write funnel refuse is cheaper to trust than a guard at each of the dozen call sites.
  const set = (patch) => { if (isHost) patchUsageReading(id, patch); };

  const data = computed(() => reading.value.data);
  const loading = computed(() => reading.value.loading);
  const error = computed(() => reading.value.error);
  // Unix seconds the currently-displayed reading was written (the cache file's mtime) - the clock staleness and the age label are measured against. Never a fetch/render time: for Claude Code the file is written by the statusline hook, and its mtime is the only honest answer to "how old is this number".
  const dataAt = computed(() => reading.value.dataAt);
  // AG-only: tracks whether current data is from cache (AG offline) and when it was cached
  const isCached = computed(() => reading.value.isCached);
  const cachedAt = computed(() => reading.value.cachedAt); // Unix seconds

  // AG-only: multi-account view state (unused for Claude Code - one account per machine).
  // A monitor reports what is LIVE; it deliberately holds no view selection. Which account a card is pinned to belongs to the SLOT displaying it (`AgentUsageSlot.vue`'s `slotViewingEmail`, persisted per slot since 1.18.0) - two slots share one monitor by design, so a single selection stored here could not express "slot C watches the IDE account, slot D watches the CLI account", which is the whole point of the per-slot pin. The monitor's own `viewingEmail`/`selectAccount` pair was the pre-slot shape of that idea; it was left unwired when the pin moved up to the slot and was removed in 1.20.0.
  // Every read below is scoped to THIS monitor's host, so two hosts signed into the same Google account keep separate dropdowns and separate readings (agUsageCache §4).
  const accounts = computed(() => reading.value.accounts);       // dropdown list [{ email, fetchedAt }] sorted newest-first
  const activeEmail = computed(() => reading.value.activeEmail); // email of the primary successful live fetch
  const activeEmails = computed(() => reading.value.activeEmails); // Set of emails of all currently live accounts
  const refreshAccounts = () => { set({ accounts: listAgAccounts(host) }); };
  // Seeded from THIS host's own cache. `set` is host-only, so on a companion this is a no-op and the
  // dropdown arrives mirrored instead of being rebuilt from the phone's (empty) localStorage.
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
  // Generation token. `epoch` is bumped on every probe ISSUE and on every `enabled` flip;
  // `inFlightEpoch` names the probe currently awaiting (0 = idle). A result whose epoch is no longer
  // current is discarded instead of written, which is what makes two overlapping probes land in
  // ISSUE order rather than completion order - and what stops a probe issued 29s ago from writing a
  // reading for a monitor the user has since switched off.
  let epoch = 0;
  let inFlightEpoch = 0;
  let pendingRecheck = false; // a poll/manual-reload arrived while a check was already in flight
  // Whether anything is still displaying this monitor (registry refcount > 0). A released monitor
  // keeps its identity and its last reading but must not poll: exploring ten hosts used to leave
  // twenty live 30s SSH loops behind cards nobody renders.
  let watching = true;
  let unsubscribeFromWake = null;
  // Circuit breaker for the poll loop itself - see restartPollTimer below.
  let consecutiveFailCount = 0;
  // Part of the mirrored reading, not a local ref: the slot's power icon renders amber off it
  // (contract C-3) on the phone as well as the Mac, and the icon is the only place the user can
  // see - or clear - a halted monitor.
  const pollHalted = computed(() => reading.value.pollHalted);
  const MAX_CONSECUTIVE_FAILS = 5;

  /**
   * One tick of the breaker. THE ONLY THING IT COUNTS IS REACHABILITY - never "there was no reading".
   * A host with no cache file yet, a Claude Code window past its reset boundary, an Antigravity IDE
   * that is not running: all of those answer, and all of them legitimately return no data. Counting
   * them would halt polling on a perfectly healthy machine; ignoring the unreachable case (what the
   * old shared `null` forced) is why the incident this breaker was written for could never trip it.
   */
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
      // Genuine failure (transport/host down). Allow a bounded number of retries on later ticks so a host coming back online gets provisioned - but never retry forever (that was the 30s retry storm when the script wrongly exited 1 on empty auth; the script now exits 0, so the only failures reaching here are real transport errors, which still deserve a cap).
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
    // THE producer gate (seam P, utils/scheduler.js). Every path that can ask for a probe - the poll
    // timer, the wake listeners, an `enabled` flip, a manual refresh, the card's boundary retry -
    // funnels through this one function, so gating it here is what makes "the companion never
    // produces" structural instead of a guard someone has to remember at each call site. It matters
    // far more than a wasted call: `utils/tauri.invoke` on a companion is an RPC the HOST executes,
    // so a phone merely regaining focus used to fire a 30s SSH probe on the Mac. The phone's numbers
    // arrive through the mirrored reading store instead.
    if (!isHost) return;
    // Nothing is displaying this monitor any more (registry refcount 0).
    if (!watching) return;
    if (!enabled.value) {
      // Monitoring off - leave any last-known data in place (the enabled watcher below already marked it isCached when the toggle flipped) instead of wiping it.
      set({ loading: false });
      return;
    }
    if (inFlightEpoch) {
      // Don't silently drop this request (e.g. a manual "Reload" click landing mid-poll, common right after relaunching AG/switching accounts) - run once more immediately after the in-flight check finishes instead of waiting up to a full poll interval.
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
      // Superseded while this probe was in flight (the monitor was toggled, or a newer probe was
      // issued). Writing now would land an older answer on top of a newer one - completion order,
      // not issue order - so the whole result is dropped, breaker included.
      if (myEpoch !== epoch) {
        ulog('result discarded - superseded', { issued: myEpoch, current: epoch }, 'debug');
        return;
      }
      // `host_answered` is the whole point of the result envelope (agent_usage.rs's AgentUsageResult):
      // "no reading" and "no host" used to arrive here as the same `null`, and this line reset the
      // breaker on both - so a host refusing TCP (ssh exit 255 → null) cleared the very counter that
      // was supposed to notice it, while the breaker still fired on a hang or a spawn failure.
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

          // ── Reset-window freshness (log-only, feeds reset_overdue_s/until_reset_s below) ──
          // file_modified_at (cache mtime), not fetched_at: for AG the two are identical (the script writes fresh data on every live poll), but for Claude Code fetched_at is always ≈0 right after Rust reads the file  - that blinded this badge to a cache frozen mid-window (statusLine/oauth both silent, resets_at still in the future) - the exact freshness blind spot behind Lỗi C. mtime is the data's true age either way.
          let resetIsPast = false;
          if (!isAg) {
            // five_hour only, on purpose - NOT "any bucket whose reset has passed". The staleness
            // contract is defined script-side: get-claudecode-usage.sh raises STALE_RESET off the
            // 5-hour window, so this client-side test has to name the same bucket or the two
            // disagree. The weekly buckets (shared or model-scoped) roll over far too rarely to be
            // a useful freshness signal, and a lapsed weekly would otherwise pin the card to
            // "stale" for hours while the 5h reading is perfectly current.
            const fh = parsed?.rate_limits?.five_hour;
            resetIsPast = fh?.resets_at > 0 && nowSec > fh.resets_at;
          }
          // The mtime the staleness rule is measured against - published so the UI can show the age rather than assert "Stale". 0 means the host could not report an mtime; that is "unknown", not "now", so it stays null and the card falls back to the badge.
          const nextDataAt = mtimeSec > 0 ? mtimeSec : null;

          if (isAg) {
            // Record this live fetch under its account email and refresh the dropdown. `data` always tracks the live/active account: a slot that wants a different one resolves it itself from `accounts` + `agUsageCache` (`AgentUsageSlot.vue`'s `slotAccountInfo`), so the monitor has no pinned-view case to skip.
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
          // Log every bucket the payload carries, not a hardcoded 5h/7d pair - `rate_limits` is an
          // open map and a bucket that never reaches the log is a bucket nobody can debug.
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

          // Re-provision existing hosts once per session (fire-and-forget). Hosts that already have a cache always land here (never the null path that used to call provision), so without this the upgraded statusline hook (aki-rlcache v2) would never reach them. provision() is idempotent and flips `provisioned` up front, so this runs at most once per host per session and does not block the read.
          if (!isAg && !provisioned) provision();
        } catch (e) {
          ulog('parse error', { err: String(e), content_preview: String(res.content).slice(0, 100) }, 'error');
          set({ error: 'Invalid usage data format.' });
        }
      } else {
        // null from server. The backend already says WHY (`miss_reason`: "no cache", "STALE_RESET",
        // "no live AG session", "probe exited N"…) - this line used to guess "STALE_RESET" from
        // `hadData` alone, which mislabelled every other miss the moment a reading existed.
        ulog('got null', { hadData, why: result?.miss_reason ?? 'unknown' }, 'info');

        // AG offline: the live fetch failed (IDE mid-restart - common right after an account switch). Show the LAST-ACTIVE account's cache deterministically (never an ambiguous global blob), so the display can't randomly flip old/new. A slot pinned to some other account overrides this for itself, from the same cache, without the monitor knowing.
        if (isAg) {
          refreshAccounts();
          // The full `email:sourceType` handle, not the email: one email can be signed into the IDE
          // and the desktop/CLI pair at once with two separate quotas, and this card must show the
          // session that was actually live - not whichever of the two the cache happened to list
          // first (agUsageCache: the entity is the triple).
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
          // STALE_RESET: past the reset boundary with no new CC turn yet. Keep the old reading on screen instead of blanking it - same cached-badge mechanism AG already uses. See docs/arch/usage-claudecode.md §4.
          set({ isCached: true, cachedAt: lastFetchedAt });
          ulog('cc STALE_RESET: keep cached', { cachedAt: lastFetchedAt }, 'info');
        } else {
          set({ data: null });
        }

        if (!isAg) provision();
      }
    } catch (e) {
      // A raised IPC error is a hang (30s script timeout) or a spawn failure - the host is not
      // answering either. Set `error` first: haltPolling, if this is the fifth in a row, replaces it
      // with the halt message, which is the one the user needs.
      ulog('IPC error', { err: String(e) }, 'error');
      if (myEpoch === epoch) {
        set({ error: e.toString() });
        noteUnreachable(String(e));
      }
    } finally {
      // Only the probe that is still the current one owns these flags. A superseded probe finishing
      // late must not clear the loading state of the one that replaced it, nor free the in-flight
      // slot it no longer holds.
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

  // AG-only: called right after a successful logout. logout_antigravity wipes AG's own auth state (SQLite rows, keychain item, session cookies) but this monitor's own cache/view-state is deliberately left untouched - see "Log Out behavior & cache retention" in docs/arch/usage-antigravity.md (PO decision, 2026-07-07): the header showing the just-logged-out account's last-known data until a new account goes live is the INTENDED behavior (the whole point of the per-account cache is to keep showing each account's last-known state), not a bug.
  //
  // Regression note: 1.9.3 (`a26b8f5`/`b082d0d`) treated that as a bug and cleared the account on logout - `clearAgStore()` ended up wiping the ENTIRE per-account history, not just the just-logged-out account, silently erasing every other cached account too. Fixed 2026-07-07 by removing the clearing behavior entirely, per the corrected product decision above.
  //
  // Named `recheckAfterLogout`, not `resetAccount`: it resets nothing. The old name promised exactly the blast radius 1.9.3 actually shipped, which is the naming failure CLAUDE.md's Regression Guard exists to stop (name a function by its real scope).
  const recheckAfterLogout = () => {
    if (!isAg) return;
    ulog('ag logout: recheck', {}, 'info');
    checkUsage(); // just an immediate poll to pick up a new login sooner - no state is cleared
  };

  // Circuit breaker. The log from the 2026-07-20 incident shows 12 consecutive failures spaced at exactly 30.0s - the poll kept probing at full rate for 24 minutes after the host had stopped accepting TCP entirely. A host that has failed this many times in a row is down, not slow, and no amount of further probing will change that; only a human fixing it will.
  //
  // Deliberately a hard stop, not exponential backoff: backoff is for a host expected to recover on its own, which is not this case, and the evenly-spaced log proves probes were already serialized (they never piled up), so there is nothing for a graduated delay to relieve. Stopping outright is both simpler and the honest signal to the user.
  //
  // Only an explicit user action resumes: manual refresh, or switching this monitor back on. Notably NOT the wake listeners - visibilitychange/focus fire constantly as the user moves between windows, and resuming on those would rebuild the same relentless loop through the back door.
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

  /** Trips the breaker: stops the timer and tells the user why, in the one place that shows errors. */
  function haltPolling() {
    if (pollTimer) clearInterval(pollTimer);
    pollTimer = null;
    // Names the host: a slot can be pointed anywhere, so "unreachable" without a name leaves the
    // user guessing which machine is down. The slot's power icon shows this same text as its tooltip.
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

  /**
   * The amber power icon's click (contract C-3): clear the breaker and probe once, right now.
   *
   * Named for what it does rather than `resume`/`reset`: it clears THIS monitor's breaker and
   * nothing else - no other monitor, no stored preference, no cached reading.
   */
  function retryAfterHalt() {
    ulog('retry after halt', { host }, 'info');
    resumePolling();
    set({ error: null });
    restartPollTimer();
    checkUsage();
  }

  // P1 wake self-heal: triggered by visibilitychange/focus or the watchdog heartbeat (module scope, see installWakeListenersOnce above) after a suspected WKWebView suspend. Re-checks immediately and restarts the interval - a suspended setInterval does not reliably resume ticking on its own even once the page is visible/focused again.
  function onWake(reason) {
    if (!enabled.value) {
      // Bump the clock even though there is nothing to recover: visibilitychange/focus wake every
      // subscriber unconditionally, and leaving `lastTickAt` frozen here meant the 7s watchdog then
      // saw an ever-growing gap and re-fired (and re-logged) this same no-op on every heartbeat,
      // forever, for a monitor that is switched off.
      lastTickAt = Date.now();
      return;
    }
    // A halted monitor stays halted. `restartPollTimer` already refuses to rebuild the interval, but
    // `checkUsage()` below is unconditional, so a halted monitor still probed the dead host once per
    // wake - and the watchdog manufactures a wake every `gapThresholdMs`, so "stopped polling" was in
    // practice one probe per ~60s, forever. That is the relentless loop the breaker exists to end,
    // reached through the back door the breaker's own comment says it must not be. `lastTickAt` is
    // bumped so the watchdog does not re-fire (and re-log) every 7s heartbeat.
    // Only an explicit user action resumes: the amber power icon (retryAfterHalt) or a manual refresh.
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
    // 0 when this monitor's own cycle is off - switched off, released, or the interval set to 0.
    // The watchdog then skips THIS monitor (not the whole heartbeat, which other cycles share), the
    // contract subscribeWake documents. It used to report a live threshold for a disabled monitor,
    // which is the case its own comment says returns 0.
    gapThresholdMs: () => (watching && enabled.value ? 2 * refreshSettings.value.usage_interval_s * 1000 : 0),
  };
  unsubscribeFromWake = subscribeWake(wakeSubscription);

  /**
   * Called by `usageMonitorRegistry.releaseMonitor()` when the LAST holder stops displaying this
   * monitor. Stops this monitor's poll timer and drops its wake subscription - and nothing else: the
   * instance stays in the registry and its reading stays in `usageReadingStore`, so pointing a slot
   * back at this host paints the last-known numbers immediately instead of an empty card.
   *
   * Named for its true blast radius (CLAUDE.md, Regression Guard): it stops ONE monitor watching. It
   * is not a dispose, not a reset, and touches no other monitor's state.
   */
  function stopWatching() {
    if (!watching) return;
    watching = false;
    // Any probe still in flight is orphaned deliberately: bumping the epoch makes its result land on
    // the floor rather than in a reading nobody is displaying.
    epoch++;
    inFlightEpoch = 0;
    pendingRecheck = false;
    if (pollTimer) clearInterval(pollTimer);
    pollTimer = null;
    if (unsubscribeFromWake) { unsubscribeFromWake(); unsubscribeFromWake = null; }
    ulog('stopped watching - no holders', { host }, 'info');
  }

  /** The reverse: a slot pointed back at this monitor. Re-arms the timer and probes once. */
  function startWatching() {
    if (watching) return;
    watching = true;
    unsubscribeFromWake = subscribeWake(wakeSubscription);
    lastTickAt = Date.now();
    ulog('watching again', { host }, 'info');
    restartPollTimer();
    checkUsage();
  }

  // The ONLY thing that varies about a monitor. Its predecessor watched a mutable host ref and had to work out whether a change meant "toggled off", "toggled back on" or "now pointing at a different machine" - the last of which discarded the reading. A monitor's machine is now fixed, so switching off keeps the last reading on screen as cached, and switching on resumes it.
  watch(enabled, (on) => {
    ulog('enabled', { on, host }, 'info');
    provisioned = false;
    provisionFailCount = 0;
    // A probe may be in flight right now. Bumping the epoch is what abandons it: its result is
    // discarded on arrival and its `finally` no longer owns the in-flight slot, so the fresh probe
    // below cannot be overwritten by an answer about the state the user just left. This used to
    // clear `isChecking` unconditionally instead, which let two probes race and write in COMPLETION
    // order.
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
      // Unconditional on both roles: `checkUsage` is host-gated at its own entry (seam P), so this
      // is a no-op on a companion, whose numbers arrive through the mirrored reading store. The
      // guard that used to sit here claimed the same thing about state that was never mirrored.
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
    // Breaker state, for the power icon (contract C-3): amber while halted, and clicking it retries
    // instead of toggling the monitor off.
    pollHalted,
    retryAfterHalt,
    // Lifecycle, driven by the registry's refcount - never by a component directly.
    stopWatching,
    startWatching,
    // Readings (mirrored, keyed by `id`, in store/usageReadingStore.js). No `stale`: the card
    // derives it from `dataAt` (finding 3) so no path can leave it behind.
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
    recheckAfterLogout
  };
}
