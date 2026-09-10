// Host-side companion relay entry point: manages pairing credentials, reachable URLs, and host-only lifecycle (docs/plan/done/remote-control.md §7.1).
// Module-scope singleton refs preserve state across dropdown toggle; invoke uses Seam-N Tauri IPC wrapper.
import { ref, computed } from 'vue'
import { invoke } from '../utils/tauri'
import { isHost } from '../services/bridge'

const running = ref(false)
const pairingCode = ref('')
const port = ref(0)
const urls = ref([]) // [{ kind: 'lan' | 'tailscale' | 'public', url: 'http://…:1421' }]
const busy = ref(false)
const error = ref(null)

// HTTPS-over-Tailscale provides secure origin for standalone PWA installation; disabled automatically on stop.
const httpsAvailable = ref(false)
const httpsEnabled = ref(false)
const httpsUrl = ref('') // https://<magicdns>/
const httpsBusy = ref(false)
// Set when another process owns the tailnet 443 "/" mount; the backend refuses to enable HTTPS while it is, and names it here so the UI can say why (docs/research/remote-ingress-tailscale-conflict.md F1).
const foreignTarget = ref(null)

// Ingress mode (docs/plan/remote-ingress-rework.md §5): one stored origin, either Tailscale-managed or owner-run.
const ingressMode = ref('tailscale')
const ingressOrigin = ref('')
const suggestedOrigin = ref('')
const ingressBusy = ref(false)

// Long link secret minted per enable: pairs a phone by opening a URL instead of typing 6 digits into an endpoint the whole internet can also reach.
const pairLinkToken = ref('')
const devices = ref([])

function msg(e) {
  return String(e && e.message ? e.message : e)
}

function applyHttps(s) {
  httpsAvailable.value = !!s.available
  httpsEnabled.value = !!s.enabled
  httpsUrl.value = s.url || ''
  foreignTarget.value = s.foreignTarget || null
}

async function refreshHttps() {
  // Public mode means the owner runs the edge, so a Tailscale probe there would report a failure that is not one.
  if (ingressMode.value === 'public') {
    httpsAvailable.value = false
    httpsEnabled.value = false
    httpsUrl.value = ''
    foreignTarget.value = null
    return
  }
  try {
    applyHttps(await invoke('get_tailscale_https'))
  } catch (e) {
    httpsAvailable.value = false
    httpsEnabled.value = false
  }
}

function applyIngress(s) {
  ingressMode.value = s.mode || 'tailscale'
  ingressOrigin.value = s.origin || ''
  suggestedOrigin.value = s.suggestedOrigin || ''
}

async function refreshIngress() {
  try {
    applyIngress(await invoke('get_remote_ingress'))
  } catch (e) {
    error.value = msg(e)
  }
}

async function saveIngress(mode, origin) {
  if (ingressBusy.value) return false
  ingressBusy.value = true
  error.value = null
  try {
    applyIngress(await invoke('set_remote_ingress', { mode, origin }))
    await refreshHttps()
    await refreshUrls()
    return true
  } catch (e) {
    error.value = msg(e)
    return false
  } finally {
    ingressBusy.value = false
  }
}

// The token is not persisted, so it is read back from the live server rather than kept across a reload.
async function refreshPairLink() {
  try {
    const status = await invoke('get_companion_status')
    pairLinkToken.value = (status && status.pairLinkToken) || ''
  } catch (e) {
    pairLinkToken.value = ''
  }
}

async function refreshDevices() {
  try {
    devices.value = await invoke('list_paired_devices')
  } catch (e) {
    error.value = msg(e)
  }
}

// Revokes exactly the one device asked for; every other paired device is left intact (CLAUDE.md multi-entity guard).
async function revokeDevice(id) {
  try {
    await invoke('revoke_device', { id })
    await refreshDevices()
  } catch (e) {
    error.value = msg(e)
  }
}

function trimOrigin(u) {
  return String(u || '').trim().replace(/\/+$/, '')
}

// The origin a phone should actually open, whichever edge produced it.
const activeOrigin = computed(() => {
  if (ingressMode.value === 'public') return trimOrigin(ingressOrigin.value)
  if (httpsEnabled.value && httpsUrl.value) return trimOrigin(httpsUrl.value)
  const pick = urls.value.find((u) => u.kind === 'public')
    || urls.value.find((u) => u.kind === 'tailscale')
    || urls.value[0]
  return pick ? trimOrigin(pick.url) : ''
})

const pairLink = computed(
  () => (activeOrigin.value && pairLinkToken.value ? `${activeOrigin.value}/?pair=${pairLinkToken.value}` : '')
)

// On enable failure (e.g. HTTPS certs disabled for tailnet), Tailscale returns error message with admin URL.
async function toggleHttps() {
  if (httpsBusy.value) return
  httpsBusy.value = true
  error.value = null
  try {
    applyHttps(await invoke('set_tailscale_https', { enable: !httpsEnabled.value }))
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
    await refreshIngress()
    refreshHttps()
    refreshPairLink()
    refreshDevices()
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
    pairLinkToken.value = ''
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
    pairLinkToken.value = status.pairLinkToken || ''
    await refreshIngress()
    refreshHttps()
    refreshDevices()
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
    httpsAvailable, httpsEnabled, httpsUrl, httpsBusy, toggleHttps, foreignTarget,
    ingressMode, ingressOrigin, suggestedOrigin, ingressBusy, saveIngress, refreshIngress,
    activeOrigin, pairLink, devices, refreshDevices, revokeDevice,
  }
}
