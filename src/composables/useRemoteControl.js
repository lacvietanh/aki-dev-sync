// Host-side companion relay entry point: manages pairing credentials, reachable URLs, and host-only lifecycle (docs/plan/done/remote-control.md §7.1).
// Module-scope singleton refs preserve state across dropdown toggle; invoke uses Seam-N Tauri IPC wrapper.
import { ref } from 'vue'
import { invoke } from '../utils/tauri'
import { isHost } from '../services/bridge'

const running = ref(false)
const pairingCode = ref('')
const port = ref(0)
const urls = ref([]) // [{ kind: 'lan' | 'tailscale', url: 'http://…:1421' }]
const busy = ref(false)
const error = ref(null)

// HTTPS-over-Tailscale provides secure origin for standalone PWA installation; disabled automatically on stop.
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

// On enable failure (e.g. HTTPS certs disabled for tailnet), Tailscale returns error message with admin URL.
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

// start_companion_server is idempotent (mints fresh code if running); re-reads URLs in case network changed.
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
    // Best-effort HTTPS serve shutdown to avoid background proxying; failure does not block stopping relay.
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
    // Clear code and URLs immediately so UI never surfaces stale credentials after stop.
    running.value = false
    pairingCode.value = ''
    urls.value = []
    busy.value = false
  }
}

// Re-syncs active relay state on webview reload (e.g. HMR) since Rust companion server outlives frontend refs.
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

// Re-enumerates reachable addresses (e.g. Tailscale activated post-start) without mutating server state.
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
  // available alias isolates isHost token from UI components per ENV-1 / DoD §12 boundary rules.
  return {
    available: isHost, running, pairingCode, port, urls, busy, error, start, stop, refreshUrls,
    httpsAvailable, httpsEnabled, httpsUrl, httpsBusy, toggleHttps,
  }
}
