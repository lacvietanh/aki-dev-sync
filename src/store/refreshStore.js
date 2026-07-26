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

// `refreshSettings` already mirrors host→companion. This closes C→H: a companion saving the
// Background Refresh modal runs the write ON THE HOST, whose reactive `refreshSettings` change then
// re-drives the Mac's own refresh timers, persists to the Mac's localStorage (via the watch above),
// and mirrors the new intervals back to every screen. Before this the modal set `refreshSettings`
// on the phone's copy only (ACT-1 class), so the Mac kept its old cadence. On the host
// action(fn)===fn. Takes a plain object (intent args are JSON) — the caller spreads its reactive
// `local` before passing it in.
export const setRefreshSettings = action('refreshStore.setRefreshSettings', (settings) => {
  refreshSettings.value = { ...settings }
})

// Incrementing this triggers an immediate usage check in every registered UsageMonitor
export const manualRefreshCount = ref(0)
export function triggerManualRefresh() {
  manualRefreshCount.value++
}
