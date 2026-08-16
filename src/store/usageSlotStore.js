// @docs docs/plan/done/usage-monitor-entity-refactor.md §2, §3
//
// What each display slot is pointed at. A slot is a VIEW; the thing it views is a UsageMonitor (`usageMonitorStore.monitorId`).
// Slots and monitors are deliberately separate: two slots may point at the same monitor, and a monitor keeps running without a view.
// Per-slot remoteHost allows side-by-side host monitoring; empty remoteHost falls back to selectedSshHost.
// Stored in a store so slot targets mirror to paired companion devices.
import { ref } from 'vue'
import { action } from '../services/action'
import { selectedSshHost } from './sshStore'
import { allSlotIds } from './usageTierStore'

const STORAGE_KEY = 'aki-usage-slot-targets'

// SEED_DEFAULTS preserves legacy A..D presets; row 3+ slots use BASE_DEFAULT (local CC) to avoid unconfigured remote error cards.
const BASE_DEFAULT = { scope: 'local', localAgent: 'cc', remoteAgent: 'cc', remoteHost: '' }
const SEED_DEFAULTS = {
  A: { scope: 'local', localAgent: 'ag', remoteAgent: 'cc', remoteHost: '' },
  B: { scope: 'local', localAgent: 'ag', remoteAgent: 'cc', remoteHost: '' },
  C: { scope: 'local', localAgent: 'cc', remoteAgent: 'cc', remoteHost: '' },
  D: { scope: 'remote', localAgent: 'cc', remoteAgent: 'cc', remoteHost: '' },
}

/** The opening target for one slot. Always a fresh object - callers spread and mutate it. */
export function defaultTarget(slotId) {
  return { ...(SEED_DEFAULTS[slotId] || BASE_DEFAULT) }
}

// Migrates legacy per-slot localStorage keys (aki-usage-slot-*) into unified slot target structure.
function migrateSlot(slotId) {
  const dflt = defaultTarget(slotId)
  const top = localStorage.getItem(`aki-usage-slot-${slotId}-top`)
  const sub = localStorage.getItem(`aki-usage-slot-${slotId}-sub`)
  const remoteSub = localStorage.getItem(`aki-usage-slot-${slotId}-remote-sub`)
  return {
    scope: top === 'remote' || top === 'local' ? top : dflt.scope,
    localAgent: sub === 'ag' || sub === 'cc' ? sub : dflt.localAgent,
    remoteAgent: remoteSub === 'ag' || remoteSub === 'cc' ? remoteSub : dflt.remoteAgent,
    remoteHost: '', // never existed per-slot before; '' = follow selectedSshHost
  }
}

function seed() {
  const ids = allSlotIds()
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw) {
      const parsed = JSON.parse(raw)
      if (parsed && typeof parsed === 'object') {
        // Stored targets take precedence, filling unconfigured slots with defaults.
        const out = {}
        for (const id of ids) out[id] = { ...defaultTarget(id), ...parsed[id] }
        return out
      }
    }
  } catch (_) {}
  const out = {}
  for (const id of ids) out[id] = migrateSlot(id)
  return out
}

export const slotTargets = ref(seed())

/** Resolves a slot's stored target to (agentId, host). Empty remoteHost falls back to selectedSshHost. */
export function slotTarget(slotId) {
  const t = slotTargets.value[slotId] || defaultTarget(slotId)
  const agent = t.scope === 'remote' ? t.remoteAgent : t.localAgent
  const agentId = agent === 'ag' ? 'antigravity' : 'claudecode'
  const host = t.scope === 'remote' ? (t.remoteHost || selectedSshHost.value || '') : 'local'
  return { ...t, agentId, host }
}

/** Patch a single slot's target, replacing state map to trigger mirror watcher. */
export const setSlotTarget = action('usageSlotStore.setSlotTarget', (slotId, patch) => {
  const current = slotTargets.value[slotId] || defaultTarget(slotId)
  slotTargets.value = { ...slotTargets.value, [slotId]: { ...current, ...patch } }
  try { localStorage.setItem(STORAGE_KEY, JSON.stringify(slotTargets.value)) } catch (_) {}
})
