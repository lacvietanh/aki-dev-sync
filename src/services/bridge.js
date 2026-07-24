// Seam T — transport + role (docs/plan/remote-control.md §1, §7.1, §9, §13.1).
//
// The ONLY place `window.__AKI_ROLE__` is read (ENV-1, §9). Every other module that needs to
// know host-vs-companion imports `isHost` from here — never re-derives it.
//
// This module owns exactly one WebSocket to the Rust relay (`src-tauri/src/web_server.rs`,
// a separate lane — not touched here) and exposes four primitives everything else is built on:
//   - `send(frame)`   fire-and-forget frame out
//   - `request(frame)` frame out + Promise resolved by a matching `invoke_result` (§13.2)
//   - `onFrame(cb)`   subscribe to every inbound app frame (mirror.js / intents.js consume this)
//   - `connectionState` a ref other UI can read (Wave 2 pairing modal, connection dot, etc.)
import { ref } from 'vue'
import {
  REMOTE_PORT,
  FRAME_PING,
  FRAME_PONG,
  FRAME_INVOKE_RESULT,
  CLOSE_UNPAIRED,
  DEVICE_TOKEN_STORAGE_KEY,
} from '../constants/protocol'

// §9 / S-1: our own role marker, set by `src/boot/roleStamp.js` (first import in main.js) when the
// page is running inside the Tauri webview. Any other case — a phone browser served the same bundle
// by axum, or no marker at all — defaults to companion, the safe direction (mis-detecting a phone as
// host is the dangerous one). Nothing stamps the companion: absence of the marker IS the signal.
export const isHost = typeof window !== 'undefined' && window.__AKI_ROLE__ === 'host'

// 'idle' | 'connecting' | 'open' | 'closed' | 'unpaired' | 'error'
export const connectionState = ref('idle')

// Native asset scheme vs browser. Per ICON-1 (§7.0) icons now ride mirrored state
// (`projectStore.projectIcons`), so this is deliberately minimal — kept only for any
// native-window-only asset that still wants a base to prefix.
export const assetBase = isHost ? 'aki-devsync-icon://' : ''

let ws = null
let reconnectTimer = null
let reconnectDelayMs = 1000
const MAX_RECONNECT_DELAY_MS = 10000

let pingTimer = null
let pingTimeoutTimer = null
const PING_INTERVAL_MS = 15000
const PING_TIMEOUT_MS = 5000

const frameListeners = new Set()

let nextRequestId = 1
const pending = new Map() // id -> { resolve, reject }

// Companion-side invoke RPC watchdog. request() is used ONLY by the companion's invoke() (seam N):
// every genuinely long operation (run_sync, delete-preview) runs on the host via an `intent`, not
// as a companion invoke, so a companion invoke that has not been answered in this window is not
// "slow" — it means no `invoke_result` is coming back at all. Without this the promise hangs
// forever (silently), which is exactly why a broken invoke produces no console error to capture.
const INVOKE_RPC_TIMEOUT_MS = 20000

function getDeviceToken() {
  try {
    return localStorage.getItem(DEVICE_TOKEN_STORAGE_KEY) || ''
  } catch {
    return ''
  }
}

function setDeviceToken(token) {
  try {
    localStorage.setItem(DEVICE_TOKEN_STORAGE_KEY, token)
  } catch {
    /* storage unavailable — token just won't survive a reload */
  }
}

// Companion pairing flow (§7.1): true once this device has a stored token, whether or not it
// has actually been accepted by the host yet — used by Wave 2's pairing modal to decide
// whether to show "enter code" vs "connecting…" on first paint.
export function hasDeviceToken() {
  return !!getDeviceToken()
}

/** Drop the stored device token. Called when the host rejects it (close 4001): a rejected token
 *  only fails again on every reconnect, so clearing it lets the companion fall back to fresh code
 *  entry instead of looping on a dead credential. */
export function clearDeviceToken() {
  try {
    localStorage.removeItem(DEVICE_TOKEN_STORAGE_KEY)
  } catch {
    /* storage unavailable — nothing to clear */
  }
}

function wsUrl() {
  if (isHost) return `ws://127.0.0.1:${REMOTE_PORT}/ws?role=host`
  const token = getDeviceToken()
  // Origin-relative on purpose: the page, `/ws` and `/pair` are ALWAYS served by the SAME axum
  // origin, so derive the socket from `location` rather than hardcoding a port. This is what makes
  // an HTTPS entry point work — e.g. `tailscale serve` terminating TLS for `<host>.ts.net` on 443
  // and proxying to local :1421. There the page is https, so the socket MUST be
  // `wss://<host>.ts.net/ws` (same origin, no `:1421`): a plain `ws://…:1421` from an https page is
  // blocked as mixed content and would also miss the TLS terminator. Over plain http on a LAN IP,
  // `location.host` already carries `:1421`, so the normal path is byte-identical to before; a
  // port-forward/tunnel is handled too (it carries the whole server, `/ws` included). Secure-context
  // (https) is also what unlocks the installable/standalone PWA — see docs/feat/remote-control.md.
  const wsProto = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
  return `${wsProto}//${window.location.host}/ws?role=companion&token=${encodeURIComponent(token)}`
}

/** Subscribe to every inbound *app* frame (init/delta/intent — ping/pong/invoke_result are
 *  consumed internally by this module and never forwarded here). Returns an unsubscribe fn. */
export function onFrame(cb) {
  frameListeners.add(cb)
  return () => frameListeners.delete(cb)
}

/** Fire-and-forget. Returns false (and drops the frame, with a console.debug) if the socket
 *  isn't open right now — callers that need a durable resync rely on mirror.js's full
 *  rebroadcast on (re)connect rather than on this module buffering frames. */
export function send(frame) {
  if (!ws || ws.readyState !== WebSocket.OPEN) {
    console.debug('[bridge] send dropped, socket not open', frame && frame.t)
    return false
  }
  ws.send(JSON.stringify(frame))
  return true
}

/** RPC-with-reply (§4, §13.2 `invoke`/`invoke_result`). Adds a fresh `id`, resolves/rejects the
 *  returned Promise when the matching `invoke_result` frame arrives. Rejects immediately if the
 *  socket isn't open; rejects every in-flight request on socket drop (see `close` handler). */
export function request(frame) {
  return new Promise((resolve, reject) => {
    const id = nextRequestId++
    const ok = send({ ...frame, id })
    if (!ok) {
      console.error(`[bridge] invoke "${frame && frame.cmd}" NOT sent — socket not open`)
      reject(new Error('bridge: not connected'))
      return
    }
    // Watchdog: if the host never replies with a matching invoke_result, surface a concrete,
    // named error instead of hanging silently. clearTimeout on either settle path (and on a socket
    // drop, which calls reject via rejectAllPending) so a normal reply never trips it.
    const timer = setTimeout(() => {
      if (!pending.has(id)) return
      pending.delete(id)
      const msg =
        `[bridge] invoke "${frame && frame.cmd}" got NO invoke_result from the host within ` +
        `${INVOKE_RPC_TIMEOUT_MS}ms — the host is not answering {t:'invoke'} frames (seam N: the ` +
        `host-side invoke responder is not wired). args=${JSON.stringify(frame && frame.args)}`
      console.error(msg)
      reject(new Error(msg))
    }, INVOKE_RPC_TIMEOUT_MS)
    pending.set(id, {
      resolve: (v) => { clearTimeout(timer); resolve(v) },
      reject: (e) => { clearTimeout(timer); reject(e) },
    })
  })
}

function rejectAllPending(err) {
  for (const { reject } of pending.values()) reject(err)
  pending.clear()
}

function clearPingTimers() {
  if (pingTimer) clearInterval(pingTimer)
  if (pingTimeoutTimer) clearTimeout(pingTimeoutTimer)
  pingTimer = null
  pingTimeoutTimer = null
}

function startPingLoop() {
  clearPingTimers()
  pingTimer = setInterval(() => {
    send({ t: FRAME_PING })
    pingTimeoutTimer = setTimeout(() => {
      console.warn('[bridge] ping timed out — treating socket as dead')
      if (ws) ws.close()
    }, PING_TIMEOUT_MS)
  }, PING_INTERVAL_MS)
}

function handleMessage(evt) {
  let frame
  try {
    frame = JSON.parse(evt.data)
  } catch {
    return
  }

  // Liveness — consumed here, never forwarded to app-level listeners.
  if (frame.t === FRAME_PONG) {
    if (pingTimeoutTimer) {
      clearTimeout(pingTimeoutTimer)
      pingTimeoutTimer = null
    }
    return
  }
  if (frame.t === FRAME_PING) {
    send({ t: FRAME_PONG })
    return
  }

  // RPC replies — consumed here, resolve/reject the matching request() Promise.
  if (frame.t === FRAME_INVOKE_RESULT) {
    const p = pending.get(frame.id)
    if (!p) return
    pending.delete(frame.id)
    if (Object.prototype.hasOwnProperty.call(frame, 'err')) p.reject(new Error(frame.err))
    else p.resolve(frame.ok)
    return
  }

  // Everything else (init/delta/intent/invoke) is app-level — hand it to mirror.js / intents.js.
  for (const cb of frameListeners) {
    try {
      cb(frame)
    } catch (e) {
      console.error('[bridge] frame listener threw', e)
    }
  }
}

function scheduleReconnect() {
  if (reconnectTimer) return
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null
    connect()
  }, reconnectDelayMs)
  reconnectDelayMs = Math.min(reconnectDelayMs * 2, MAX_RECONNECT_DELAY_MS)
}

/** Open (or re-open) the one WS connection. Safe to call repeatedly — no-ops while already
 *  open/connecting. Auto-reconnects with backoff on an ordinary drop; does NOT auto-reconnect
 *  after a 4001 (unpaired) close — that needs a fresh token via `pairDevice()` first. */
export function connect() {
  if (typeof window === 'undefined') return
  if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) return

  connectionState.value = 'connecting'
  let socket
  try {
    socket = new WebSocket(wsUrl())
  } catch (e) {
    console.error('[bridge] failed to open socket', e)
    connectionState.value = 'error'
    scheduleReconnect()
    return
  }
  ws = socket

  socket.addEventListener('open', () => {
    reconnectDelayMs = 1000
    connectionState.value = 'open'
    console.info(`[bridge] socket OPEN (role=${isHost ? 'host' : 'companion'})`)
    startPingLoop()
  })
  socket.addEventListener('message', handleMessage)
  socket.addEventListener('close', (evt) => {
    clearPingTimers()
    rejectAllPending(new Error('bridge: connection closed'))
    if (evt.code === CLOSE_UNPAIRED && !isHost) {
      // §13.1: bad/absent token on a COMPANION. Surface "needs pairing" and do NOT auto-reconnect
      // with the same (rejected) token — the pairing gate reads this state.
      connectionState.value = 'unpaired'
      return
    }
    // A 4001 on the HOST is never "unpaired" — the host has no token/pairing at all. It can only
    // mean the host's own loopback WS was refused (e.g. the is_loopback_ip bug), which must
    // self-heal, not give up: fall through to reconnect. Otherwise host_tx stays null on the relay
    // and every companion connects to an empty, un-broadcast session.
    connectionState.value = 'closed'
    scheduleReconnect()
  })
  socket.addEventListener('error', () => {
    connectionState.value = 'error'
  })
}

export function disconnect() {
  if (reconnectTimer) {
    clearTimeout(reconnectTimer)
    reconnectTimer = null
  }
  clearPingTimers()
  if (ws) {
    ws.close()
    ws = null
  }
  connectionState.value = 'idle'
}

/** Companion-only pairing (§7.1): exchange the 6-digit code shown on the Mac for a persistent
 *  device token, store it, then (re)connect. Throws on a bad code (401) — Wave 2's pairing
 *  modal is the caller and owns the error UI. */
export async function pairDevice(code) {
  // Same origin-relative reasoning as wsUrl(): `/pair` is served by the same axum origin as the
  // page, so use `location.origin` — this pairs correctly whether reached over http on `:1421` or
  // over https via `tailscale serve` on 443 (where an explicit `:1421` would be wrong/insecure).
  const res = await fetch(`${window.location.origin}/pair`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ code }),
  })
  if (!res.ok) {
    throw new Error(res.status === 401 ? 'Invalid pairing code' : `Pairing failed (${res.status})`)
  }
  const { token } = await res.json()
  setDeviceToken(token)
  connect()
  return token
}
