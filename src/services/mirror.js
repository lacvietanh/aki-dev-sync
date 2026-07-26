// Seam S — state mirror (docs/plan/done/remote-control.md §2, §13.3, §13.4).
//
// Zero per-key wiring: every `isRef` export of every `src/store/*.js` module is discovered by
// `import.meta.glob` and registered under `"<storeFile>.<exportName>"`. Both host and companion
// run this exact code and get identical keys pointing at their OWN local refs — a delta from the
// host lands directly in the companion's real ref and Vue re-renders, no adapter, no parallel
// data model. Adding a feature never touches this file (SSOT-1).
import { isRef, isReadonly, watch } from 'vue'
import { isHost, onFrame, send, connectionState } from './bridge'
import { FRAME_INIT, FRAME_DELTA, FRAME_COMPANION_CONNECTED } from '../constants/protocol'
import {
  LOG_CAP,
  globalLogs,
  projectLogs,
  logAppendCounts,
  appendGlobalLogLines,
  appendProjectLogLines,
} from '../store/logStore'

// Build-time glob (Vite-only — this literal call is what Vite's plugin rewrites; do not make it
// dynamic). §2.2 / §13.4.
const mods = import.meta.glob('../store/*.js', { eager: true })

function basename(path) {
  return path.split('/').pop().replace(/\.js$/, '')
}

// ── PER-SCREEN state: the ONE exclusion list (docs/plan/done/1.20.1-flow-audit-fixes.md §3.12) ────
//
// Everything else in `src/store/*.js` is shared by construction. These three are not: they record
// what THIS screen is looking at, not what the session is doing. Mirroring them meant a phone
// joining (which triggers a `broadcastFull()` to *every* companion) shut the Mac's log panel and
// re-pointed its usage slots — the app fighting the user. Two people looking at one session are
// looking for different things; the genuinely shared parts (project data, logs, dialogs, the PTY)
// stay mirrored.
//
// Excluded at REGISTRATION, so these refs are never watched, never broadcast and never applied —
// the one place the decision lives (SSOT-1), instead of a filter at each of the three send sites.
// The gesture side of the same decision is `PER_SCREEN_ACTION_KEYS` in services/action.js: a
// per-screen ref whose setter is an `action()` must also run locally, or the phone would ship its
// choice to the Mac and change nothing of its own.
const PER_SCREEN_KEYS = new Set([
  'logStore.activeLogProjectId',
  'logStore.isLogExpanded',
  'usageSlotStore.slotTargets',
  // WP-C: which terminal TAB a screen is looking at is that screen's own navigation, exactly like
  // activeLogProjectId — a phone switching tabs must not yank the Mac's terminal focus and vice
  // versa. `terminalTabsStore.terminalTabs` (the tab LIST itself) is deliberately NOT here: which
  // tabs exist is genuinely shared session state and stays mirrored.
  'terminalTabsStore.activeTerminalTabId',
  // Which tab GROUP a screen is in — same per-screen reasoning as activeTerminalTabId.
  'terminalTabsStore.activeTerminalScope',
])

const STATE = new Map() // "<store>.<ref>" -> Ref

for (const [path, mod] of Object.entries(mods)) {
  const store = basename(path)
  for (const [name, val] of Object.entries(mod)) {
    const key = `${store}.${name}`
    if (isRef(val) && !PER_SCREEN_KEYS.has(key)) STATE.set(key, val)
  }
}

// The two log arrays are mirrored, but NOT by re-encoding the whole value on every change: a
// 5,000-line rsync dirties `projectLogs` 5,000 times, and the generic path made the nth delta carry
// n lines (quadratic, §3.14). They travel as APPENDS instead — see buildLogPayload().
const LOG_GLOBAL_KEY = 'logStore.globalLogs'
const LOG_PROJECT_KEY = 'logStore.projectLogs'

// ── SER-1: JSON-safe encode/decode ──────────────────────────────────────────────────────────
// Set -> {__t:'Set', v:[...]}; Map -> {__t:'Map', v:[[k,v],...]}. A value that cannot encode
// (DOM node, function, symbol, bigint, circular ref) throws; the caller drops that ONE key from
// the outgoing payload, once, with a console.warn — no hand-maintained exclusion list.
function encode(value, seen) {
  if (value instanceof Set) return { __t: 'Set', v: Array.from(value, (x) => encode(x, seen)) }
  if (value instanceof Map) {
    return { __t: 'Map', v: Array.from(value.entries(), ([k, v]) => [encode(k, seen), encode(v, seen)]) }
  }
  if (typeof value === 'function') throw new Error('function cannot be mirrored')
  if (typeof value === 'symbol' || typeof value === 'bigint') {
    throw new Error(`${typeof value} cannot be mirrored`)
  }
  if (typeof Node !== 'undefined' && value instanceof Node) throw new Error('DOM node cannot be mirrored')
  if (Array.isArray(value)) return value.map((x) => encode(x, seen))
  if (value && typeof value === 'object') {
    if (seen.has(value)) throw new Error('circular reference cannot be mirrored')
    seen.add(value)
    const out = {}
    for (const [k, v] of Object.entries(value)) out[k] = encode(v, seen)
    return out
  }
  return value // string | number | boolean | null | undefined
}

const warnedDrop = new Set()

function encodeKeyed(key, value) {
  try {
    return { ok: true, value: encode(value, new WeakSet()) }
  } catch (e) {
    if (!warnedDrop.has(key)) {
      warnedDrop.add(key)
      console.warn(`[mirror] "${key}" failed to encode — dropped from the mirror`, e)
    }
    return { ok: false }
  }
}

function decode(value) {
  if (value && typeof value === 'object') {
    if (value.__t === 'Set') return new Set(value.v.map(decode))
    if (value.__t === 'Map') return new Map(value.v.map(([k, v]) => [decode(k), decode(v)]))
    if (Array.isArray(value)) return value.map(decode)
    const out = {}
    for (const [k, v] of Object.entries(value)) out[k] = decode(v)
    return out
  }
  return value
}

// ── HOST: watch + coalesce + broadcast ──────────────────────────────────────────────────────
const dirty = new Set()
let flushScheduled = false

function scheduleFlush() {
  if (flushScheduled) return
  flushScheduled = true
  queueMicrotask(flushDirty)
}

// What every companion has already been told about the logs: `n` = the host's append cursor at that
// moment, `len` = how many lines were retained then. Both are needed — the cursor alone cannot tell
// "20 new lines" from "cleared, then 20 new lines", and the cap makes length alone meaningless.
let sentGlobalLog = { n: 0, len: 0 }
let sentProjectLogs = new Map() // projectId -> { n, len }

function currentProjectBaselines(counts) {
  const next = new Map()
  for (const [pid, arr] of Object.entries(projectLogs.value)) {
    next.set(pid, { n: counts.projects[pid] || 0, len: arr.length })
  }
  return next
}

/**
 * Turn "the logs changed" into the smallest correct payload.
 *
 * Appends whenever the change is explainable as appends (the common case: rsync lines arriving one
 * at a time); falls back to the full value for that key when it is not — a `clearLog()`, a project
 * whose array was replaced wholesale, a project key that disappeared. Never guesses: an unexplained
 * shape is always resolved by sending the truth, so the companion cannot silently diverge.
 *
 * Writes any full values into `v` and returns the append payload (or null).
 */
function buildLogPayload(v, counts, wantGlobal, wantProjects) {
  const a = {}

  if (wantGlobal) {
    const arr = globalLogs.value
    const added = counts.global - sentGlobalLog.n
    const expectedLen = Math.min(LOG_CAP, sentGlobalLog.len + added)
    if (added > 0 && added <= arr.length && arr.length === expectedLen) {
      a.global = { from: sentGlobalLog.n, lines: arr.slice(arr.length - added) }
    } else if (added !== 0 || arr.length !== sentGlobalLog.len) {
      const { ok, value } = encodeKeyed(LOG_GLOBAL_KEY, arr)
      if (ok) v[LOG_GLOBAL_KEY] = value
    }
  }

  if (wantProjects) {
    const map = projectLogs.value
    const out = {}
    // A project whose log map entry is gone cannot be expressed as an append — send the map.
    let full = false
    for (const pid of sentProjectLogs.keys()) {
      if (!Object.prototype.hasOwnProperty.call(map, pid)) { full = true; break }
    }
    if (!full) {
      for (const [pid, arr] of Object.entries(map)) {
        const base = sentProjectLogs.get(pid) || { n: 0, len: 0 }
        const added = (counts.projects[pid] || 0) - base.n
        if (added === 0 && arr.length === base.len) continue // untouched this flush
        const expectedLen = Math.min(LOG_CAP, base.len + added)
        if (added > 0 && added <= arr.length && arr.length === expectedLen) {
          out[pid] = { from: base.n, lines: arr.slice(arr.length - added) }
        } else {
          full = true
          break
        }
      }
    }
    if (full) {
      const { ok, value } = encodeKeyed(LOG_PROJECT_KEY, map)
      if (ok) v[LOG_PROJECT_KEY] = value
    } else if (Object.keys(out).length > 0) {
      a.projects = out
    }
  }

  return Object.keys(a).length > 0 ? a : null
}

/** Advance the log baselines. Only ever called after the frame actually went out. */
function commitLogBaselines(counts) {
  sentGlobalLog = { n: counts.global, len: globalLogs.value.length }
  sentProjectLogs = currentProjectBaselines(counts)
}

function flushDirty() {
  flushScheduled = false
  if (dirty.size === 0) return

  const wantGlobal = dirty.has(LOG_GLOBAL_KEY)
  const wantProjects = dirty.has(LOG_PROJECT_KEY)
  const logsDirty = wantGlobal || wantProjects

  const v = {}
  for (const key of dirty) {
    if (key === LOG_GLOBAL_KEY || key === LOG_PROJECT_KEY) continue // handled below
    const { ok, value } = encodeKeyed(key, STATE.get(key).value)
    if (ok) v[key] = value
  }
  dirty.clear()

  const counts = logsDirty ? logAppendCounts() : null
  const a = logsDirty ? buildLogPayload(v, counts, wantGlobal, wantProjects) : null

  if (Object.keys(v).length === 0 && !a) return

  const frame = { t: FRAME_DELTA, v }
  if (a) frame.a = a
  // `s` travels with every log-bearing frame: it is the host's cursor AFTER this frame, so a
  // companion that applied it is exactly in step for the next append — and a companion that had to
  // skip one re-syncs its cursor here instead of rejecting every append from then on.
  if (logsDirty) frame.s = counts

  if (send(frame) && logsDirty) commitLogBaselines(counts)
}

function broadcastFull() {
  const v = {}
  for (const [key, ref] of STATE.entries()) {
    const { ok, value } = encodeKeyed(key, ref.value)
    if (ok) v[key] = value
  }
  const counts = logAppendCounts()
  if (send({ t: FRAME_INIT, v, s: counts })) commitLogBaselines(counts)
}

// ── COMPANION: apply into the SAME local refs ───────────────────────────────────────────────
let applying = false

// The HOST's append cursor as of the last frame this screen applied. Deliberately not read back out
// of `logStore.logAppendCounts()`: a companion appends log lines of its own too (its own
// `appendGlobalLog` calls), and mixing the two counters would make every host frame after a local
// line look out of order.
let appliedLogCursor = { global: 0, projects: {} }

/**
 * Apply an append payload into the SAME capped store helpers the host uses, so the companion's
 * 2,000-line ceiling is enforced by the same code (contract C-2) instead of a second copy of it.
 *
 * The `from` cursor is a continuity check, not a guess: if it does not match what this screen has
 * applied, a frame was lost and blindly appending would splice unrelated lines into the tail. Skip
 * it, say so once, and let the host's next full snapshot (every reconnect, and every time another
 * companion joins) repair the gap.
 */
function applyLogAppends(a) {
  if (a.global) {
    if (a.global.from === appliedLogCursor.global) appendGlobalLogLines(a.global.lines)
    else console.warn(`[mirror] global log append out of order (have ${appliedLogCursor.global}, got ${a.global.from}) — skipped`)
  }
  if (a.projects) {
    for (const [pid, d] of Object.entries(a.projects)) {
      const mine = appliedLogCursor.projects[pid] || 0
      if (d.from === mine) appendProjectLogLines(pid, d.lines)
      else console.warn(`[mirror] log append for "${pid}" out of order (have ${mine}, got ${d.from}) — skipped`)
    }
  }
}

function applyFrame(frame) {
  if (!frame || !frame.v) return
  applying = true
  try {
    for (const [key, encoded] of Object.entries(frame.v)) {
      const target = STATE.get(key)
      if (!target) continue
      // A computed ref without a setter is a pure function of OTHER mirrored refs (its own
      // dependencies are separately mirrored keys) — it recomputes correctly on its own once
      // those land, so writing to it would both fail silently (Vue readonly warning) and fight
      // the companion's own live recomputation. Generic check, no per-key list.
      if (isReadonly(target)) continue
      target.value = decode(encoded)
    }
    // Order matters: full log values in `v` land first, appends on top of them, and only then the
    // cursor — which is the host's state after both, so this screen is in step for the next frame.
    if (frame.a) applyLogAppends(frame.a)
    if (frame.s) appliedLogCursor = { global: frame.s.global || 0, projects: { ...(frame.s.projects || {}) } }
  } finally {
    applying = false
  }
}

/** Boot this seam. Idempotent-ish (call once from services/index.js's initRemote()). */
export function initMirror() {
  if (isHost) {
    for (const [key, ref] of STATE.entries()) {
      watch(
        ref,
        () => {
          if (applying) return
          dirty.add(key)
          scheduleFlush()
        },
        { deep: true }
      )
    }

    // Full resync whenever our own socket to the relay (re)opens — covers both "we just
    // reconnected after a drop" and the app's very first connection.
    watch(connectionState, (s) => {
      if (s === 'open') broadcastFull()
    })

    // A companion that joins while the host's socket is already open needs the SAME full
    // resync, but the host's own connectionState never changes for that. The relay is
    // content-blind (§13.6) and cannot construct an `init` payload itself (it holds no state),
    // so on each companion WS connect it emits a `companion-connected` frame to the host — a
    // connection event, not app state, so content-blindness holds. We ignore its `id` here (we
    // re-broadcast to everyone); Terminal View next round will use it for per-companion routing.
    onFrame((frame) => {
      if (frame.t === FRAME_COMPANION_CONNECTED) broadcastFull()
    })
  } else {
    onFrame((frame) => {
      if (frame.t === FRAME_INIT || frame.t === FRAME_DELTA) {
        // Diagnostic: proves the mirror is (or is not) delivering host state to the phone. If you
        // see "init N keys" the dashboard's data arrived and any remaining problem is in an
        // invoke/action, not the mirror. If you never see it, the host never broadcast a snapshot.
        const n = frame.v ? Object.keys(frame.v).length : 0
        console.info(`[mirror] applied ${frame.t} — ${n} key(s)`)
        applyFrame(frame)
      }
    })
  }
}
