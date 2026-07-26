// Seam T — transport + role (docs/plan/done/remote-control.md §1, §7.1, §9, §13.1).
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
// Deliberate exception to "utils/tauri.js is the only module that imports @tauri-apps/api/core":
// that wrapper imports THIS module, so using it here would be a cycle at bootstrap — the exact
// hazard REGISTRY-1 exists to prevent. The transport seam has to be able to ask the host process
// one question (its relay host token) before the transport exists, so it takes the raw import and
// never uses it on a companion (`isHost` guards the only call site).
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

// §9 / S-1: our own role marker, set by `src/boot/roleStamp.js` (first import in main.js) when the
// page is running inside the Tauri webview. Any other case — a phone browser served the same bundle
// by axum, or no marker at all — defaults to companion, the safe direction (mis-detecting a phone as
// host is the dangerous one). Nothing stamps the companion: absence of the marker IS the signal.
export const isHost = typeof window !== 'undefined' && window.__AKI_ROLE__ === 'host'

// 'idle' | 'connecting' | 'open' | 'closed' | 'unpaired' | 'host-off' | 'error'
//
// 'unpaired'  — close 4001: this device's token was rejected. Drop it, ask for a code.
// 'host-off'  — close 4002: remote control is off on the Mac. The token is fine; keep it and wait.
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
// the genuinely unbounded operations (run_sync, delete-preview) run on the host via an `intent`,
// not as a companion invoke, so for most commands an unanswered call in this window is not "slow" —
// it means no `invoke_result` is coming back at all. Without this the promise hangs forever
// (silently), which is exactly why a broken invoke produces no console error to capture.
const INVOKE_RPC_TIMEOUT_MS = 20000

// …but the default is wrong for the commands that legitimately take longer than 20s, and firing on
// those invented a failure the host never had: `get_agent_usage` alone budgets 30s for its SSH
// round-trip, so the phone showed "the host is not answering" while the Mac was still working and
// about to reply. Per-command budget, consulted by request(): a value of 0 means NO client-side
// timeout at all (the socket drop is then the only failure signal — see rejectAllPending).
const INVOKE_TIMEOUT_MS_BY_CMD = {
  // SSH / network round-trips on the host, each well past the 20s default under a slow link.
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
  // No upper bound worth guessing: a git push/pull and a REPORT.html transfer are both "as long as
  // the repo/file and the network need".
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

// The relay's process-local host secret (src-tauri/src/web_server.rs `RelayState::host_token`),
// read once per page load from `get_companion_status`. `role=host` is refused without it, because
// the peer address alone proves nothing: `tailscale serve` proxies the whole tailnet in through
// 127.0.0.1, so a loopback test used to hand any tailnet peer the host role — letting it overwrite
// the relay's `host_tx`, cut the real Mac's mirror and forge init/delta frames to every phone.
// Minted per process, so it is never persisted and never valid after a restart.
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

// ── Outbound congestion (the JS half of the backpressure policy) ──────────────────────────────
//
// `ws.send()` never blocks and never fails: the browser accepts the frame and grows
// `bufferedAmount` until the socket drains. That is the same unbounded-queue shape the Rust relay
// had toward companions (src-tauri/src/web_server.rs, "Backpressure toward companions"), one hop
// earlier — so it gets the SAME policy rather than a second, differently-shaped one:
//
//   * only `pty_output` may be dropped, because only terminal bytes are re-derivable in full (the
//     host can replay its whole scrollback ring buffer on demand). Every other frame — deltas,
//     dialogs, intents, invoke results, liveness, pings — is queued to the browser as before,
//     however congested the socket is. Losing any of them is unrecoverable; losing a run of
//     terminal bytes is recoverable by one `reset` replay, which is what services/ptyBridge.js does
//     when this function turns it down.
//   * high/low water rather than one threshold, so a socket hovering at the limit does not
//     alternate between relaying and dropping every other frame — it drops until it has genuinely
//     caught up, then resumes with one clean re-hydrate.
//
// The frame-kind rule is mirrored in `is_coalescible()` in web_server.rs — same two-file mirroring
// (and same reason) as the close codes: `constants/protocol.js` is the protocol SSoT, and Rust
// cannot import it.
const SEND_BUFFER_HIGH_WATER = 1024 * 1024
const SEND_BUFFER_LOW_WATER = 128 * 1024
let congested = false

/** True while the socket's own outbound buffer is backed up. Latches at the high-water mark and
 *  only clears once the buffer has drained past the low-water mark. services/ptyBridge.js reads
 *  this to decide when it is worth re-hydrating the companions. */
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

/** Fire-and-forget. Returns false (and drops the frame) if the socket isn't open right now, or if
 *  the frame is terminal output and the socket is congested — callers that need a durable resync
 *  rely on mirror.js's full rebroadcast on (re)connect, or on ptyBridge's scrollback replay, rather
 *  than on this module buffering frames.
 *
 *  The return value is NOT advisory: a dropped `intent` is a tap that did nothing, so
 *  services/action.js turns `false` into a Toast (§3.13), and a dropped `pty_output` is a hole in a
 *  terminal stream, so services/ptyBridge.js turns `false` into a scrollback replay. */
export function send(frame) {
  if (!ws || ws.readyState !== WebSocket.OPEN) {
    console.warn('[bridge] send dropped, socket not open', frame && frame.t)
    return false
  }
  // Deliberately silent — a congested socket drops many frames in a row, and a warn per frame would
  // bury the console at exactly the moment someone is trying to read it. The caller reports it once.
  if (frame && frame.t === FRAME_PTY_OUTPUT && isSocketCongested()) return false
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
    // drop, which calls reject via rejectAllPending) so a normal reply never trips it. A budget of
    // 0 opts out entirely — those commands rely on the socket drop instead.
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

/** Open (or re-open) the one WS connection. Safe to call repeatedly — no-ops while already
 *  open/connecting. Auto-reconnects with backoff on an ordinary drop AND on a 4002 (the Mac's
 *  remote control is off — the token is still good, so waiting is the right move). The one close
 *  it does NOT retry is 4001 (unpaired): that token is dead and needs `pairDevice()` first. */
export function connect() {
  if (typeof window === 'undefined') return
  if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) return

  connectionState.value = 'connecting'

  // The host needs its relay token before it can dial. One extra IPC round-trip on the very first
  // connect only; every later reconnect reuses the cached value and takes the synchronous path, so
  // a relay drop still self-heals at backoff speed.
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
    // Identity check (same idea as pty.rs's generation token): a superseded socket can still fire
    // its close AFTER connect() has replaced it — a late close from the old one would then clear
    // the LIVE socket's ping timers and schedule a reconnect against a connection that is fine.
    if (ws !== socket) return
    clearPingTimers()
    rejectAllPending(new Error('bridge: connection closed'))
    if (!isHost) {
      if (evt.code === CLOSE_UNPAIRED) {
        // §13.1: this device's token was actually rejected (never paired, or revoked on the Mac).
        // Surface "needs pairing" and do NOT auto-reconnect with the same dead credential.
        connectionState.value = 'unpaired'
        return
      }
      if (evt.code === CLOSE_SERVER_DISABLED) {
        // Remote control is off on the Mac — an app restart, or the user flipping the toggle. The
        // token is untouched and will work again the moment the Mac is back, so keep it and keep
        // retrying. Treating this as "unpaired" is what used to strand every phone on every restart.
        connectionState.value = 'host-off'
        scheduleReconnect()
        return
      }
    } else if (evt.code === CLOSE_HOST_ROLE_REJECTED) {
      // Our host token was stale or missing (a relay that restarted under a reloaded webview mints
      // a new one). Drop the cached copy so the next connect() re-reads it instead of retrying the
      // same doomed URL forever — otherwise host_tx stays null and every companion sits empty.
      hostRelayToken = ''
    }
    // Anything else — including a 4001 on the HOST, which has no token/pairing concept at all —
    // must self-heal rather than give up, or host_tx stays null on the relay and every companion
    // connects to an empty, un-broadcast session.
    connectionState.value = 'closed'
    scheduleReconnect()
  })
  socket.addEventListener('error', () => {
    if (ws !== socket) return // same staleness guard as `close` above
    connectionState.value = 'error'
  })
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
