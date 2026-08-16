import { ref } from 'vue'
import { projectRuntime, bumpEpoch } from './projectStore'
import { action } from '../services/action'

// Migration fallback from pre-1.20 unified key (aki-remote-mode-enabled).
const LEGACY_KEY = 'aki-remote-mode-enabled'
const KEY = 'aki-sync-check-enabled'

function initialEnabled() {
  const current = localStorage.getItem(KEY)
  if (current !== null) return current !== 'false'
  const legacy = localStorage.getItem(LEGACY_KEY)
  return legacy === null ? true : legacy !== 'false'
}

// Kill switch for SSH project sync & remote-diff (usage monitoring is separate in usageMonitorStore).
export const syncCheckEnabled = ref(initialEnabled())

// Seam-A action (R-2): companion toggle relays to host to mutate state and mirror back.
export const toggleSyncCheck = action('syncCheckStore.toggleSyncCheck', () => {
  syncCheckEnabled.value = !syncCheckEnabled.value
  localStorage.setItem(KEY, String(syncCheckEnabled.value))
  if (!syncCheckEnabled.value) {
    // Cancel in-flight diff checks & clear busy spinners immediately (see bumpEpoch in projectStore.js).
    for (const id of Object.keys(projectRuntime.value)) bumpEpoch(id)
  }
})
