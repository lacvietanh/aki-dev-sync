// @docs docs/plan/usage-monitor-entity-refactor.md §2, §5
//
// Which usage monitors are switched on. One entry per monitor identity, NOT four fixed flags.
//
// A monitor is `(agentId, host)` - "Claude Code on hostA" and "Claude Code on hostB" are two
// different entities that must be watchable at the same time, each with its own account. The four
// flags this replaces (`usageSourcesStore`, deleted) keyed on `(agent, local|remote)` instead, which
// made a second remote host unrepresentable: both remote flags described whichever single host the
// global picker happened to name.
//
// Mirrored (an `isRef` export of a `src/store/*.js` module) because monitoring on/off is one shared
// Mac setting, not a per-device flag - the 1.19.0 decision, carried over unchanged. `setMonitorEnabled`
// closes the companion→host direction: a phone's power-button click runs on the Mac and mirrors back.
import { ref, watch } from 'vue'
import { action } from '../services/action'
import { selectedSshHost } from './sshStore'

const STORAGE_KEY = 'aki-usage-monitor-enabled'

/** `local` for this machine, else the SSH host string. The one spelling of a monitor's identity. */
export const LOCAL_HOST = 'local'

/**
 * The host part of a monitor whose host is not chosen yet - a REMOTE slot on a machine with no SSH
 * host configured, which `usageSlotStore.slotTarget` reports as `''`.
 *
 * It MUST NOT collapse into `LOCAL_HOST`, and that is the whole reason this constant exists. While
 * `''` fell back to `local`, a not-yet-targeted REMOTE monitor and the LOCAL monitor were literally
 * the same entity: one registry instance (so the REMOTE tab rendered this Mac's numbers) and one
 * persisted flag (so the REMOTE tab's power button switched the LOCAL monitor off). Parentheses keep
 * it outside the space of real SSH host aliases, so it can never shadow a host actually named this.
 */
export const NO_HOST = '(none)'

/** The identity of a monitor. Used verbatim as the registry key, the store key and the log tag. */
export function monitorId(agentId, host) {
  return `${agentId}@${host || NO_HOST}`
}

/** The four flags `usageSourcesStore` used to own, resolved for one host (or local-only if none). */
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

// One-time seed from the four legacy flags, so an upgrade keeps every monitor exactly as the user
// left it. The two remote flags need a host; if none has resolved yet, the deferred pass below
// picks them up rather than dropping them.
let legacyRemotePending = false
function seed() {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw) return JSON.parse(raw) || {}
  } catch (_) {}
  legacyRemotePending = true
  return readLegacyFlags(selectedSshHost.value)
}

export const monitorEnabled = ref(seed())

// Deferred half of the legacy seed. `sshStore.sshHosts` is filled by an async IPC read that lands
// well after this module evaluates, so on an install that never pinned a host to localStorage
// `selectedSshHost` is still `''` at seed time and the two REMOTE legacy flags are unreadable. Left
// unmigrated they would simply be lost - and because an absent entry defaults to ON (see
// `isMonitorEnabled`), a user who had deliberately turned remote monitoring OFF would be switched
// back on silently and start an unwanted SSH poll loop.
//
// Scoped to the two ids of the ONE host that just resolved, and only fills ids the map does not
// already carry: no other monitor's flag is read, written or cleared (project Regression Guard for
// multi-entity state). Runs at most once, then stops itself.
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

/**
 * True when `id` should be polling. A monitor with no entry defaults to ON, matching every
 * pre-refactor source's default: pointing a slot at a new host starts watching it, which is the
 * whole point. An unreachable host is handled by the consecutive-failure breaker in
 * usageMonitor.js, not by making the default off.
 */
export function isMonitorEnabled(id) {
  const v = monitorEnabled.value[id]
  return v === undefined ? true : !!v
}

/**
 * Set ONE monitor's flag. Replaces the map object so the mirror's watcher fires.
 *
 * Scoped to the single id by construction - the project's Regression Guard for multi-entity state:
 * a function that touches a keyed store must name and affect exactly the entity it is about. There
 * is deliberately no "clear all monitors" counterpart.
 */
export const setMonitorEnabled = action('usageMonitorStore.setMonitorEnabled', (id, value) => {
  monitorEnabled.value = { ...monitorEnabled.value, [id]: !!value }
  try { localStorage.setItem(STORAGE_KEY, JSON.stringify(monitorEnabled.value)) } catch (_) {}
})
