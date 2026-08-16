// @docs docs/arch/usage-claudecode.md
//
// What each usage monitor last READ, keyed by the monitor's identity (`agentId@host`).
// Store-based state mirrors to companion devices via mirror.js without needing SSH probe roundtrips on phone.
// MUST NOT be in mirror.js PER_SCREEN_KEYS: readings reflect measured session data, not screen-local view state.
import { ref } from 'vue'

/** Frozen empty reading blueprint shared by monitors before initial report. */
export const EMPTY_READING = Object.freeze({
  data: null,
  loading: false,
  error: null,
  // Unix epoch seconds when reading cache file was written (null = unknown mtime).
  dataAt: null,
  isCached: false,
  cachedAt: null,
  pollHalted: false,
  // AG-only multi-account view (harmless/unused for Claude Code).
  accounts: [],
  activeEmail: null,
  activeEmails: new Set(),
})

/** monitorId -> reading. One entry per monitor, never a single shared blob. */
export const usageReadings = ref({})

/** This monitor's reading, or the empty one. Never null, so callers do not branch. */
export function usageReading(id) {
  return usageReadings.value[id] || EMPTY_READING
}

/** Merge partial reading into a single monitorId, preserving last reading across remounts. */
export function patchUsageReading(id, patch) {
  const prev = usageReadings.value[id] || EMPTY_READING
  // Replace map object to trigger mirror watcher and downstream reactivity.
  usageReadings.value = { ...usageReadings.value, [id]: { ...prev, ...patch } }
}
