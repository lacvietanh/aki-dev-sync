// @docs docs/plan/done/usage-monitor-entity-refactor.md §2, §5
// Monitor on/off state keyed by (agentId, host) entity rather than fixed local/remote flags.
// Mirrored across companion devices; toggle executes on Mac host and mirrors back.
import { ref, watch } from 'vue'
import { action } from '../services/action'
import { selectedSshHost } from './sshStore'

const STORAGE_KEY = 'aki-usage-monitor-enabled'

/** `local` for this machine, else the SSH host string. */
export const LOCAL_HOST = 'local'

/** Sentinel for unconfigured remote host slot; prevents collision with LOCAL_HOST or real SSH aliases. */
export const NO_HOST = '(none)'

/** Identity of a monitor formatted as "${agentId}@${host}". Used as registry and store key. */
export function monitorId(agentId, host) {
  return `${agentId}@${host || NO_HOST}`
}

/** Maps legacy localStorage boolean keys to monitor IDs for migration. */
function legacyKeysFor(host) {
  const legacy = {
    [monitorId('antigravity', LOCAL_HOST)]: 'aki-src-ag-enabled',
    [monitorId('claudecode', LOCAL_HOST)]: 'aki-src-cclocal-enabled',
  }
  if (host) {
    legacy[monitorId('antigravity', host)] = 'aki-src-agremote-enabled'
    legacy[monitorId('claudecode', host)] = 'aki-src-ccremote-enabled'
  }
  return legacy
}

function readLegacyFlags(host) {
  const out = {}
  for (const [id, key] of Object.entries(legacyKeysFor(host))) {
    const v = localStorage.getItem(key)
    if (v !== null) out[id] = v === 'true'
  }
  return out
}

// Seeds state from storage or legacy flags; defers remote flags if SSH host is not yet resolved.
let legacyRemotePending = false
function seed() {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw) return JSON.parse(raw) || {}
  } catch (_) {}
  legacyRemotePending = true
  const migrated = readLegacyFlags(selectedSshHost.value)
  // Persist migrated local flags immediately so state reaches disk even if no remote host exists.
  try { localStorage.setItem(STORAGE_KEY, JSON.stringify(migrated)) } catch (_) {}
  return migrated
}

export const monitorEnabled = ref(seed())

// Deferred migration: watches selectedSshHost once to migrate remote legacy flags after async SSH host resolution.
if (legacyRemotePending) {
  const stopLegacyWatch = watch(selectedSshHost, (host) => {
    if (!host) return
    legacyRemotePending = false
    stopLegacyWatch()
    const patch = {}
    for (const [id, value] of Object.entries(readLegacyFlags(host))) {
      if (monitorEnabled.value[id] === undefined) patch[id] = value
    }
    if (Object.keys(patch).length === 0) return
    monitorEnabled.value = { ...monitorEnabled.value, ...patch }
    try { localStorage.setItem(STORAGE_KEY, JSON.stringify(monitorEnabled.value)) } catch (_) {}
  })
}

/** True if monitor id is enabled; unconfigured entries default to true (active). */
export function isMonitorEnabled(id) {
  const v = monitorEnabled.value[id]
  return v === undefined ? true : !!v
}

/** Sets enabled state for a single monitor id and triggers mirrored updates. */
export const setMonitorEnabled = action('usageMonitorStore.setMonitorEnabled', (id, value) => {
  monitorEnabled.value = { ...monitorEnabled.value, [id]: !!value }
  try { localStorage.setItem(STORAGE_KEY, JSON.stringify(monitorEnabled.value)) } catch (_) {}
})
