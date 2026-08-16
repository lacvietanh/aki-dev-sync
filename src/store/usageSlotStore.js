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
import { allSlotIds } from './usageTierStore'

const STORAGE_KEY = 'aki-usage-slot-targets'

// What a slot shows before the user has ever touched it.
//
// A..D are a compatibility table, not the ceiling: those four are the only slots that existed when
// the panel was fixed at two rows, and reproducing their exact opening view is what keeps an
// upgraded install looking unchanged. Any slot beyond them (row 3+, which is new surface nobody has
// a habit about yet) gets BASE_DEFAULT - local Claude Code, the one target that always resolves.
//
// Deliberately NOT `remote` for new slots: a remote default with no host picked yet resolves to an empty host and opens on an error card.
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

// One-time lift of the per-slot keys AgentUsageSlot.vue used to write directly. Only A..D were ever
// written under that scheme, so for every later slot the three reads simply miss and the default
// stands - which is why this needs no slot list of its own.
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
        // Stored map wins per field, defaults fill the rest - so a record written when only A..D existed gains the new slots without losing a single choice already made in the old four.
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

/**
 * The `(agentId, host)` a slot currently resolves to - the single place the slot's three stored
 * choices are turned into one monitor identity, so no component has to branch on scope.
 *
 * An empty `remoteHost` means "follow the app-wide picker", which is what every slot did before it
 * could hold a host of its own; that keeps an upgraded install behaving exactly as it did.
 */
export function slotTarget(slotId) {
  const t = slotTargets.value[slotId] || defaultTarget(slotId)
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
  const current = slotTargets.value[slotId] || defaultTarget(slotId)
  slotTargets.value = { ...slotTargets.value, [slotId]: { ...current, ...patch } }
  try { localStorage.setItem(STORAGE_KEY, JSON.stringify(slotTargets.value)) } catch (_) {}
})
