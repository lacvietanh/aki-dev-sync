// Seam S — state mirror (docs/plan/done/remote-control.md §2, §13.3, §13.4).
// Zero per-key wiring: import.meta.glob discovers store refs into "<store>.<ref>" (SSOT-1).
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

// Build-time glob (Vite-only — this literal call is what Vite's plugin rewrites; do not make it dynamic). §2.2 / §13.4.
const mods = import.meta.glob('../store/*.js', { eager: true })

function basename(path) {
  return path.split('/').pop().replace(/\.js$/, '')
}

// Per-screen UI state excluded at registration (docs/plan/done/1.20.1-flow-audit-fixes.md §3.12, SSOT-1).
const PER_SCREEN_KEYS = new Set([
  'logStore.activeLogProjectId',
  'logStore.isLogExpanded',
  'usageSlotStore.slotTargets',
  // WP-C: tab navigation is per-screen; tab list itself (terminalTabs) remains mirrored.
  'terminalTabsStore.activeTerminalTabId',
  'terminalTabsStore.activeTerminalScope',
  // SSH modal & editor state is per-screen navigation.
  'sshStore.showSshModal',
  'sshStore.sshConfigText',
  'sshStore.hasSshUndo',
  'sshStore.hasSshRedo',
])

const STATE = new Map() // "<store>.<ref>" -> Ref

for (const [path, mod] of Object.entries(mods)) {
  const store = basename(path)
  for (const [name, val] of Object.entries(mod)) {
    const key = `${store}.${name}`
    if (isRef(val) && !isReadonly(val) && !PER_SCREEN_KEYS.has(key)) STATE.set(key, val)
  }
}

// Log arrays travel as append deltas to avoid quadratic re-serialization during rsync bursts (§3.14).
const LOG_GLOBAL_KEY = 'logStore.globalLogs'
const LOG_PROJECT_KEY = 'logStore.projectLogs'

// SER-1: JSON-safe encode/decode for Set/Map tagged payloads; un-encodable values drop with warning.
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

// Tracks host log baseline: n = append cursor, len = retained count (distinguishes appends vs capped resets).
let sentGlobalLog = { n: 0, len: 0 }
let sentProjectLogs = new Map() // projectId -> { n, len }

function currentProjectBaselines(counts) {
  const next = new Map()
  for (const [pid, arr] of Object.entries(projectLogs.value)) {
    next.set(pid, { n: counts.projects[pid] || 0, len: arr.length })
  }
  return next
}

/** Builds append delta payload when possible, falling back to full state on log resets/drops. */
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
  // s carries host cursor post-frame so companion re-syncs baseline even after a skipped frame.
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

// Host append cursor tracked separately from local logAppendCounts to avoid local log interleaving.
let appliedLogCursor = { global: 0, projects: {} }

/** Applies append payload via store helpers (contract C-2); drops and awaits snapshot on cursor mismatch. */
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
      // Skip readonly/computed refs; they recompute automatically from their mirrored dependencies.
      if (isReadonly(target)) continue
      target.value = decode(encoded)
    }
    // Order: full log state in v lands first, then appends (a), then host cursor (s).
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

    // Full resync whenever host socket to relay (re)opens (reconnection or fresh connection).
    watch(connectionState, (s) => {
      if (s === 'open') broadcastFull()
    })

    // Re-broadcast full state when a companion connects via content-blind relay (§13.6).
    onFrame((frame) => {
      if (frame.t === FRAME_COMPANION_CONNECTED) broadcastFull()
    })
  } else {
    onFrame((frame) => {
      if (frame.t === FRAME_INIT || frame.t === FRAME_DELTA) {
        // Diagnostic log: confirms host state delivery before dispatching to store refs.
        const n = frame.v ? Object.keys(frame.v).length : 0
        console.info(`[mirror] applied ${frame.t} — ${n} key(s)`)
        applyFrame(frame)
      }
    })
  }
}
