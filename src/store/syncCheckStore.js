import { ref } from 'vue'
import { projectRuntime, bumpEpoch } from './projectStore'

// Kill switch for project sync/diff: SSH-based project sync (pull/push/select/open-remote)
// and background remote-diff checks. Claude Code remote usage monitoring has its own
// independent switch — see AgentUsageSection.vue's `ccRemote` (aki-src-ccremote-enabled).

// Migration: this switch used to be `aki-remote-mode-enabled`, a single flag that also governed
// Claude Code remote usage monitoring. That half now lives in `aki-src-ccremote-enabled`
// (AgentUsageSection). Seed both new keys from the old value so an existing user's setup keeps
// behaving exactly as before the split, then let them diverge.
const LEGACY_KEY = 'aki-remote-mode-enabled'
const KEY = 'aki-sync-check-enabled'

function initialEnabled() {
  const current = localStorage.getItem(KEY)
  if (current !== null) return current !== 'false'
  const legacy = localStorage.getItem(LEGACY_KEY)
  return legacy === null ? true : legacy !== 'false'
}

export const syncCheckEnabled = ref(initialEnabled())

export function toggleSyncCheck() {
  syncCheckEnabled.value = !syncCheckEnabled.value
  localStorage.setItem(KEY, String(syncCheckEnabled.value))
  if (!syncCheckEnabled.value) {
    // Turning off: any remote-diff check still in flight for any project must not land after
    // this point, and its busy indicator must clear immediately rather than wait for that
    // now-irrelevant check to eventually resolve — same cancellation primitive as an individual
    // project's host/path edit (see bumpEpoch in projectStore.js). This never touches an actual
    // rsync push/pull in progress, only the read-only diff check.
    for (const id of Object.keys(projectRuntime.value)) bumpEpoch(id)
  }
}
