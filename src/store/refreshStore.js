import { ref, watch } from 'vue'
import { action } from '../services/action'

const STORAGE_KEY = 'aki-refresh-settings'
const DEFAULTS = {
  git_interval_s: 60,
  remote_diff_interval_s: 60,
  usage_interval_s: 30,
}

function load() {
  try {
    return { ...DEFAULTS, ...JSON.parse(localStorage.getItem(STORAGE_KEY) || '{}') }
  } catch {
    return { ...DEFAULTS }
  }
}

export const refreshSettings = ref(load())

watch(refreshSettings, (v) => {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(v))
}, { deep: true })

// C→H action: persists refresh settings on host to drive local timers and mirror back to companions.
export const setRefreshSettings = action('refreshStore.setRefreshSettings', (settings) => {
  refreshSettings.value = { ...settings }
})

// Incrementing this triggers an immediate usage check in every registered UsageMonitor
export const manualRefreshCount = ref(0)
export function triggerManualRefresh() {
  manualRefreshCount.value++
}
