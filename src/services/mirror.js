// Seam S — state mirror (docs/plan/remote-control.md §2, §13.3, §13.4).
//
// Zero per-key wiring: every `isRef` export of every `src/store/*.js` module is discovered by
// `import.meta.glob` and registered under `"<storeFile>.<exportName>"`. Both host and companion
// run this exact code and get identical keys pointing at their OWN local refs — a delta from the
// host lands directly in the companion's real ref and Vue re-renders, no adapter, no parallel
// data model. Adding a feature never touches this file (SSOT-1).
import { isRef, isReadonly, watch } from 'vue'
import { isHost, onFrame, send, connectionState } from './bridge'
import { FRAME_INIT, FRAME_DELTA, FRAME_COMPANION_CONNECTED } from '../constants/protocol'

// Build-time glob (Vite-only — this literal call is what Vite's plugin rewrites; do not make it
// dynamic). §2.2 / §13.4.
const mods = import.meta.glob('../store/*.js', { eager: true })

function basename(path) {
  return path.split('/').pop().replace(/\.js$/, '')
}

const STATE = new Map() // "<store>.<ref>" -> Ref

for (const [path, mod] of Object.entries(mods)) {
  const store = basename(path)
  for (const [name, val] of Object.entries(mod)) {
    if (isRef(val)) STATE.set(`${store}.${name}`, val)
  }
}

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

function flushDirty() {
  flushScheduled = false
  if (dirty.size === 0) return
  const v = {}
  for (const key of dirty) {
    const { ok, value } = encodeKeyed(key, STATE.get(key).value)
    if (ok) v[key] = value
  }
  dirty.clear()
  if (Object.keys(v).length > 0) send({ t: FRAME_DELTA, v })
}

function broadcastFull() {
  const v = {}
  for (const [key, ref] of STATE.entries()) {
    const { ok, value } = encodeKeyed(key, ref.value)
    if (ok) v[key] = value
  }
  send({ t: FRAME_INIT, v })
}

// ── COMPANION: apply into the SAME local refs ───────────────────────────────────────────────
let applying = false

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
