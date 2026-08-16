// Seam T — transport + role (docs/plan/done/remote-control.md §1, §7.1, §9, §13.1).
// Owns single WebSocket to Rust relay (src-tauri/src/web_server.rs) and exposes transport primitives.
import { ref } from 'vue'
// Direct @tauri-apps/api/core import avoids bootstrap cycle with utils/tauri.js (guarded by isHost).
import { invoke as tauriInvoke } from '@tauri-apps/api/core'
import {
  REMOTE_PORT,
  FRAME_PING,
  FRAME_PONG,
  FRAME_INVOKE_RESULT,
  FRAME_PTY_OUTPUT,
  CLOSE_UNPAIRED,
  CLOSE_SERVER_DISABLED,
  CLOSE_HOST_ROLE_REJECTED,
  DEVICE_TOKEN_STORAGE_KEY,
} from '../constants/protocol'

// §9/S-1: Role marker set by roleStamp.js in Tauri webview; absence defaults safely to companion.
export const isHost = typeof window !== 'undefined' && window.__AKI_ROLE__ === 'host'

// Connection state: 'idle' | 'connecting' | 'open' | 'closed' | 'unpaired' (4001) | 'host-off' (4002) | 'error'
export const connectionState = ref('idle')

// Base prefix for native-window assets (icons ride mirrored projectStore.projectIcons per ICON-1).
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

// Per-connection request ID counter; sound because relay addresses invoke_result to the originating socket.
let nextRequestId = 1
const pending = new Map() // id -> { resolve, reject }

// Default timeout watchdog for companion invoke RPC promises to prevent silent hangs.
const INVOKE_RPC_TIMEOUT_MS = 20000

// Per-command timeouts: 120s for network/SSH round-trips; 0 disables timeout for unbounded streaming/git operations.
const INVOKE_TIMEOUT_MS_BY_CMD = {
  get_agent_usage: 120000,
  provision_agent_usage: 120000,
  resolve_remote_path: 120000,
  check_sync_status: 120000,
  check_statusline_status: 120000,
  check_for_updates: 120000,
  get_git_info: 120000,
  open_remote_subprocess: 120000,
  install_akiclaudedoc: 120000,
  install_ssh_terminal_color: 120000,
  run_git_command: 0,
  resolve_report_html: 0,
}

function invokeTimeoutMs(cmd) {
  const v = INVOKE_TIMEOUT_MS_BY_CMD[cmd]
  return v === undefined ? INVOKE_RPC_TIMEOUT_MS : v
}

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

// True if this device has a stored token (used by pairing modal for first paint state).
export function hasDeviceToken() {
  return !!getDeviceToken()
}

/** Drop stored device token on rejection (close 4001) so companion can fall back to code entry. */
export function clearDeviceToken() {
  try {
    localStorage.removeItem(DEVICE_TOKEN_STORAGE_KEY)
  } catch {
    /* storage unavailable — nothing to clear */
  }
}

// Process-local host secret required for role=host (prevents proxied loopback peers from hijacking host role).
let hostRelayToken = ''
let hostTokenPromise = null

function fetchHostRelayToken() {
  if (!hostTokenPromise) {
    hostTokenPromise = tauriInvoke('get_companion_status')
      .then((s) => {
        hostRelayToken = (s && s.hostToken) || ''
        if (!hostRelayToken) console.error('[bridge] host relay token missing from get_companion_status')
      })
      .catch((e) => {
        console.error('[bridge] could not read the host relay token', e)
      })
      .finally(() => {
        hostTokenPromise = null
      })
  }
  return hostTokenPromise
}

function wsUrl() {
  if (isHost) {
    return `ws://127.0.0.1:${REMOTE_PORT}/ws?role=host&token=${encodeURIComponent(hostRelayToken)}`
  }
  const token = getDeviceToken()
  // Origin-relative WS URL to support HTTPS/WSS (e.g. Tailscale serve TLS termination on 443) without mixed-content errors.
  const wsProto = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
  return `${wsProto}//${window.location.host}/ws?role=companion&token=${encodeURIComponent(token)}`
}

/** Subscribe to inbound app frames (init/delta/intent). Returns unsubscribe function. */
export function onFrame(cb) {
  frameListeners.add(cb)
  return () => frameListeners.delete(cb)
}

// Backpressure high/low water marks: drops only recoverable pty_output when send buffer backs up.
const SEND_BUFFER_HIGH_WATER = 1024 * 1024
const SEND_BUFFER_LOW_WATER = 128 * 1024
let congested = false

/** True while socket outbound buffer is backed up (latches at high-water, clears at low-water). */
export function isSocketCongested() {
  if (!ws || ws.readyState !== WebSocket.OPEN) {
    congested = false
    return false
  }
  if (congested) {
    if (ws.bufferedAmount <= SEND_BUFFER_LOW_WATER) congested = false
  } else if (ws.bufferedAmount > SEND_BUFFER_HIGH_WATER) {
    congested = true
  }
  return congested
}

/** Fire-and-forget frame send. Returns false if closed or congested pty_output (triggers replay/resync). */
export function send(frame) {
  if (!ws || ws.readyState !== WebSocket.OPEN) {
    console.warn('[bridge] send dropped, socket not open', frame && frame.t)
    return false
  }
  // Silently drop congested pty_output; caller handles replay without flooding logs.
  if (frame && frame.t === FRAME_PTY_OUTPUT && isSocketCongested()) return false
  ws.send(JSON.stringify(frame))
  return true
}

/** RPC-with-reply: attaches request ID and returns Promise settled by matching invoke_result frame. */
export function request(frame) {
  return new Promise((resolve, reject) => {
    const id = nextRequestId++
    const ok = send({ ...frame, id })
    if (!ok) {
      console.error(`[bridge] invoke "${frame && frame.cmd}" NOT sent — socket not open`)
      reject(new Error('bridge: not connected'))
      return
    }
    // RPC watchdog timeout: rejects with error if host doesn't reply within budget (budget 0 opts out).
    const budget = invokeTimeoutMs(frame && frame.cmd)
    const timer = budget
      ? setTimeout(() => {
          if (!pending.has(id)) return
          pending.delete(id)
          const msg =
            `[bridge] invoke "${frame && frame.cmd}" got NO invoke_result from the host within ` +
            `${budget}ms — the host is not answering {t:'invoke'} frames (seam N: the host-side ` +
            `invoke responder is not wired). args=${JSON.stringify(frame && frame.args)}`
          console.error(msg)
          reject(new Error(msg))
        }, budget)
      : null
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

/** Open/re-open WS connection with exponential backoff. Does not retry close 4001 (unpaired). */
export function connect() {
  if (typeof window === 'undefined') return
  if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) return

  connectionState.value = 'connecting'

  // Host fetches relay token via IPC on first connect; subsequent reconnects reuse cached token.
  if (isHost && !hostRelayToken) {
    fetchHostRelayToken().then(() => {
      if (hostRelayToken) openSocket()
      else scheduleReconnect() // retry the read with backoff rather than dialing a doomed URL
    })
    return
  }
  openSocket()
}

function openSocket() {
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
    // Ignore close events from superseded socket instances to prevent clobbering active connection.
    if (ws !== socket) return
    clearPingTimers()
    rejectAllPending(new Error('bridge: connection closed'))
    if (!isHost) {
      if (evt.code === CLOSE_UNPAIRED) {
        // Token rejected (close 4001): surface unpaired state without auto-reconnecting.
        connectionState.value = 'unpaired'
        return
      }
      if (evt.code === CLOSE_SERVER_DISABLED) {
        // Server disabled (close 4002): preserve token and retry until remote control is enabled.
        connectionState.value = 'host-off'
        scheduleReconnect()
        return
      }
    } else if (evt.code === CLOSE_HOST_ROLE_REJECTED) {
      // Stale host token (close 4003): clear cache so next connect re-fetches fresh token.
      hostRelayToken = ''
    }
    // Auto-reconnect on socket drop or host close to maintain relay host_tx broadcast session.
    connectionState.value = 'closed'
    scheduleReconnect()
  })
  socket.addEventListener('error', () => {
    if (ws !== socket) return // same staleness guard as `close` above
    connectionState.value = 'error'
  })
}

/** Companion pairing (§7.1): exchange 6-digit code for persistent token, store, and connect. */
export async function pairDevice(code) {
  // Origin-relative endpoint works over both HTTP and TLS-terminated reverse proxy.
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
