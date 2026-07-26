// @docs docs/arch/usage-claudecode.md
//
// What each usage monitor last READ, keyed by the monitor's identity (`agentId@host` - the same
// string `usageMonitorStore`, `usageMonitorRegistry` and usage.log use).
//
// WHY THIS IS A STORE AND NOT COMPOSABLE STATE. A monitor's reading used to live in refs inside
// `createUsageMonitor()`. `services/mirror.js` globs `src/store/*.js` only, so nothing in composable
// scope ever reaches a companion: the phone's cards had no numbers at all, and the only way they
// could show any was for the phone to run the probe itself - which means an SSH round trip issued
// from the Mac on the phone's behalf, the exact thing seam P (`utils/scheduler.js`) exists to
// prevent. Moving the reading here mirrors it for free, so `checkUsage()` can be host-only.
//
// It MUST NOT be listed in mirror.js's PER_SCREEN_KEYS: a reading is what the session measured, not
// what this screen is looking at. Excluding it would restore the very hole described above.
import { ref } from 'vue'

/**
 * The shape a card renders. Frozen and shared: a monitor that has never reported reads this exact
 * object rather than allocating an empty one per call.
 */
export const EMPTY_READING = Object.freeze({
  data: null,
  loading: false,
  error: null,
  // Unix seconds the displayed reading was written (the cache file's mtime), i.e. the clock the
  // card derives staleness and the age label from. Null means the host reported no mtime - that is
  // "unknown", never "now".
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

/**
 * Merge a partial reading into ONE monitor's entry.
 *
 * Scoped to the single id by construction - the project's Regression Guard for multi-entity state.
 * There is deliberately no "clear all readings" counterpart: a monitor that stops being displayed
 * keeps its last reading (that is what makes a remount instant), and one monitor's reset must never
 * be able to blank another's.
 */
export function patchUsageReading(id, patch) {
  const prev = usageReadings.value[id] || EMPTY_READING
  // Replace the map object rather than mutating in place, matching usageMonitorStore: the mirror's
  // watcher and every computed reading downstream key off the identity change.
  usageReadings.value = { ...usageReadings.value, [id]: { ...prev, ...patch } }
}
