// Host-side control of the companion (remote-control) server — docs/plan/done/remote-control.md §7.1.
//
// This is the HOST entry point: it turns the relay's `enabled` gate on/off and surfaces the
// pairing code + every reachable address (LAN / Tailscale, §7.1a) so the user knows where to point
// a phone and how to pair it. It is meaningless on a companion (a phone cannot start the Mac's
// server) — AppHeader only renders the section when `isHost`, so `start()`/`stop()` are host-only
// in practice.
//
// Module-scope singleton refs (not created per-call) so the state survives the dropdown being
// closed and reopened, and so any future caller shares the one source of truth. `invoke` comes
// from the Seam-N wrapper (utils/tauri.js): on the host it is the real Tauri IPC call.
import { ref } from 'vue'
import { invoke } from '../utils/tauri'
import { isHost } from '../services/bridge'

const running = ref(false)
const pairingCode = ref('')
const port = ref(0)
const urls = ref([]) // [{ kind: 'lan' | 'tailscale', url: 'http://…:1421' }]
const busy = ref(false)
const error = ref(null)

// HTTPS-over-Tailscale (`tailscale serve`) — the in-app on/off for the secure origin that lets a
// phone install the companion as a standalone PWA. `available` gates the whole row (no tailscale =
// no row). Kept as an explicit toggle, and turned OFF when remote control stops, so nothing is left
// serving silently in the background.
const httpsAvailable = ref(false)
const httpsEnabled = ref(false)
const httpsUrl = ref('') // https://<magicdns>/
const httpsBusy = ref(false)

function msg(e) {
  return String(e && e.message ? e.message : e)
}

async function refreshHttps() {
  try {
    const s = await invoke('get_tailscale_https')
    httpsAvailable.value = !!s.available
    httpsEnabled.value = !!s.enabled
    httpsUrl.value = s.url || ''
  } catch (e) {
    httpsAvailable.value = false
    httpsEnabled.value = false
  }
}

// Toggle serve on/off. On an enable failure (usually HTTPS certs not yet enabled for the tailnet) tailscale's own message — which includes the admin URL to click — comes back as the error string.
async function toggleHttps() {
  if (httpsBusy.value) return
  httpsBusy.value = true
  error.value = null
  try {
    const s = await invoke('set_tailscale_https', { enable: !httpsEnabled.value })
    httpsAvailable.value = !!s.available
    httpsEnabled.value = !!s.enabled
    httpsUrl.value = s.url || ''
  } catch (e) {
    error.value = msg(e)
  } finally {
    httpsBusy.value = false
  }
}

// Idempotent on the Rust side — calling again while running just mints a fresh code (see
// `start_companion_server`'s doc comment). We always re-read the URLs too, in case the tailnet
// came up (or an interface changed) since the last start.
async function start() {
  if (busy.value) return
  busy.value = true
  error.value = null
  try {
    const info = await invoke('start_companion_server')
    pairingCode.value = info.pairing_code
    port.value = info.port
    urls.value = await invoke('get_companion_url')
    running.value = true
    refreshHttps()
  } catch (e) {
    error.value = msg(e)
    running.value = false
  } finally {
    busy.value = false
  }
}

async function stop() {
  if (busy.value) return
  busy.value = true
  error.value = null
  try {
    // Turn HTTPS serve off with it — "off means off", nothing left proxying in the background after the user stops remote control. Best-effort; a failure here shouldn't block stopping the relay.
    if (httpsEnabled.value) {
      try {
        await invoke('set_tailscale_https', { enable: false })
      } catch { /* leave httpsEnabled as-is; the relay stop below is what matters */ }
      httpsEnabled.value = false
    }
    await invoke('stop_companion_server')
  } catch (e) {
    error.value = msg(e)
  } finally {
    // Whatever the RPC did, from the UI's point of view remote control is now off — clear the code/URLs so a stale code can't be read off the menu after stopping.
    running.value = false
    pairingCode.value = ''
    urls.value = []
    busy.value = false
  }
}

// The Rust relay OUTLIVES the webview: an HMR reload in dev (or any webview reload) resets these
// refs while the relay is still on, paired phones still connected. Without this re-sync the menu
// would show "Off" while the LAN is actually being served — the one thing this menu exists to tell
// the truth about. Runs once per page load, host only.
let synced = false
async function syncFromHost() {
  if (synced || !isHost) return
  synced = true
  try {
    const status = await invoke('get_companion_status')
    if (!status || !status.enabled) return
    pairingCode.value = status.pairing_code
    port.value = status.port
    running.value = true
    urls.value = await invoke('get_companion_url')
    refreshHttps()
  } catch (e) {
    // A relay that never bound (port taken, etc.) is a real "off" — leave the UI off, surface why.
    error.value = msg(e)
  }
}

// Re-enumerate reachable addresses without touching the enabled gate or the code — e.g. the user brought Tailscale up after starting. No-op when not running.
async function refreshUrls() {
  if (!running.value) return
  try {
    urls.value = await invoke('get_companion_url')
  } catch (e) {
    error.value = msg(e)
  }
}

export function useRemoteControl() {
  syncFromHost()
  // `available` = isHost, re-exposed under a neutral name (already derived in the Seam-T boundary,
  // services/bridge.js — this does not re-read the raw marker) so AppHeader can gate the host-only
  // section without any `isHost` token leaking into a component. ENV-1 / DoD §12 whitelists this one
  // feature module.
  return {
    available: isHost, running, pairingCode, port, urls, busy, error, start, stop, refreshUrls,
    httpsAvailable, httpsEnabled, httpsUrl, httpsBusy, toggleHttps,
  }
}
