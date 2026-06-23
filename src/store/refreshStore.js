import { ref, watch } from 'vue'

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

// Incrementing this triggers an immediate usage check in all useAgentUsage instances
export const manualRefreshCount = ref(0)
export function triggerManualRefresh() {
  manualRefreshCount.value++
}
