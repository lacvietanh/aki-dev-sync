import { ref } from 'vue'
import { projectRuntime, bumpEpoch } from './projectStore'
import { action } from '../services/action'

// Kill switch for project sync/diff: SSH-based project sync (pull/push/select/open-remote)
// and background remote-diff checks. Usage monitoring has its own independent switches, one per
// `(agent, host)` monitor - see `usageMonitorStore.monitorEnabled`.

// Migration: this switch used to be `aki-remote-mode-enabled`, a single flag that also governed
// Claude Code remote usage monitoring. That half moved to the per-source flag
// `aki-src-ccremote-enabled`, which the 1.20.0 entity refactor has since folded into
// `usageMonitorStore`'s per-monitor map. Seed this key from the old value so an existing user's
// setup keeps behaving exactly as before the split, then let them diverge.
const LEGACY_KEY = 'aki-remote-mode-enabled'
const KEY = 'aki-sync-check-enabled'

function initialEnabled() {
  const current = localStorage.getItem(KEY)
  if (current !== null) return current !== 'false'
  const legacy = localStorage.getItem(LEGACY_KEY)
  return legacy === null ? true : legacy !== 'false'
}

export const syncCheckEnabled = ref(initialEnabled())

// Wrapped as a seam-A action (R-2): a companion's power-button click is relayed to the host and
// run there, so `syncCheckEnabled` and the epoch bumps only ever mutate on the host and mirror
// back. On the host `action(fn) === fn` — unchanged behaviour. localStorage here is the host's,
// which is correct: the toggle is a host setting, not a per-device one.
export const toggleSyncCheck = action('syncCheckStore.toggleSyncCheck', () => {
  syncCheckEnabled.value = !syncCheckEnabled.value
  localStorage.setItem(KEY, String(syncCheckEnabled.value))
  if (!syncCheckEnabled.value) {
    // Turning off: any remote-diff check still in flight for any project must not land after
    // this point, and its busy indicator must clear immediately rather than wait for that
    // now-irrelevant check to eventually resolve - same cancellation primitive as an individual
    // project's host/path edit (see bumpEpoch in projectStore.js). This never touches an actual
    // rsync push/pull in progress, only the read-only diff check.
    for (const id of Object.keys(projectRuntime.value)) bumpEpoch(id)
  }
})
