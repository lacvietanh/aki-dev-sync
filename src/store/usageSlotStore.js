// @docs docs/plan/done/usage-monitor-entity-refactor.md §2, §3
//
// What each display slot is pointed at. A slot is a VIEW; the thing it views is a UsageMonitor
// (`usageMonitorStore.monitorId`). Slots and monitors are deliberately separate: two slots may point
// at the same monitor (one poll, two cards), and a monitor keeps running with no slot showing it.
//
// The per-slot `remoteHost` is the field that makes host A and host B watchable side by side. Before
// this, every slot's REMOTE tab read the one global `sshStore.selectedSshHost`, so retargeting it
// moved every remote card at once. `selectedSshHost` keeps its other jobs (the SSH-config modal,
// project sync/diff) and remains the DEFAULT for a slot that has never chosen a host of its own.
//
// Lives in a store rather than component-local localStorage - where the old `aki-usage-slot-*` keys
// sat - so a slot's target mirrors to a paired phone and can be retargeted from it.
import { ref } from 'vue'
import { action } from '../services/action'
import { selectedSshHost } from './sshStore'

const STORAGE_KEY = 'aki-usage-slot-targets'

// Slot A/B are the tier-1 pair, C/D the tier-2 pair (AgentUsageSection's row layout). The defaults
// reproduce what the four slots showed before they had independent targets.
const DEFAULTS = {
  A: { scope: 'local', localAgent: 'ag', remoteAgent: 'cc', remoteHost: '' },
  B: { scope: 'local', localAgent: 'ag', remoteAgent: 'cc', remoteHost: '' },
  C: { scope: 'local', localAgent: 'cc', remoteAgent: 'cc', remoteHost: '' },
  D: { scope: 'remote', localAgent: 'cc', remoteAgent: 'cc', remoteHost: '' },
}

// One-time lift of the four per-slot keys AgentUsageSlot.vue used to write directly.
function migrateSlot(slotId, dflt) {
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
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw) {
      const parsed = JSON.parse(raw)
      if (parsed && typeof parsed === 'object') {
        const out = {}
        for (const [id, dflt] of Object.entries(DEFAULTS)) out[id] = { ...dflt, ...parsed[id] }
        return out
      }
    }
  } catch (_) {}
  const out = {}
  for (const [id, dflt] of Object.entries(DEFAULTS)) out[id] = migrateSlot(id, dflt)
  return out
}

export const slotTargets = ref(seed())

/**
 * The `(agentId, host)` a slot currently resolves to - the single place the slot's three stored
 * choices are turned into one monitor identity, so no component has to branch on scope.
 *
 * An empty `remoteHost` means "follow the app-wide picker", which is what every slot did before it
 * could hold a host of its own; that keeps an upgraded install behaving exactly as it did.
 */
export function slotTarget(slotId) {
  const t = slotTargets.value[slotId] || DEFAULTS[slotId] || DEFAULTS.A
  const agent = t.scope === 'remote' ? t.remoteAgent : t.localAgent
  const agentId = agent === 'ag' ? 'antigravity' : 'claudecode'
  const host = t.scope === 'remote' ? (t.remoteHost || selectedSshHost.value || '') : 'local'
  return { ...t, agentId, host }
}

/**
 * Patch ONE slot's target. Replaces the map so the mirror's watcher fires.
 *
 * Scoped to the named slot (project Regression Guard for multi-entity state): retargeting slot C
 * cannot rewrite slot D's choice, which is exactly the failure the side-by-side two-host case would
 * otherwise hit on every host change.
 */
export const setSlotTarget = action('usageSlotStore.setSlotTarget', (slotId, patch) => {
  const current = slotTargets.value[slotId] || DEFAULTS[slotId] || DEFAULTS.A
  slotTargets.value = { ...slotTargets.value, [slotId]: { ...current, ...patch } }
  try { localStorage.setItem(STORAGE_KEY, JSON.stringify(slotTargets.value)) } catch (_) {}
})
