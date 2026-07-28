// Live external-`Terminal.app` count — the producer behind the `TERM` cell's bottom (slate) badge.
//
// The badge used to be a session counter: the app can watch itself open a window, so the number only
// ever grew, and it kept claiming "3 windows" long after all three were closed. It is now derived
// instead of remembered — the host re-scans the process table on a fixed cadence
// (`count_external_terminals` in src-tauri/src/system.rs) and publishes a snapshot, so closing a
// window drops the badge within one tick.
//
// HOST ONLY (seam P, utils/scheduler.js): the scan needs `Terminal.app`'s process tree, which exists
// on the Mac and nowhere else. The companion never polls — its copy of `externalTermCounts` arrives
// through the state mirror like every other piece of shared state.
import { ref } from 'vue'
import { hostInterval } from '../utils/scheduler'
import { isHost } from '../services/bridge'
import { invoke } from '../utils/tauri'
import { projects, externalTermCounts, externalTermGlobalCount } from '../store/projectStore'

// Short enough that closing a window feels immediate, long enough that three tiny local subprocesses
// per tick stay invisible. Deliberately fixed, not user-configurable: this is not a status check.
const POLL_MS = 5000

// After the app opens a Terminal window, the shell needs a moment to exist and land in the project
// directory — scanning immediately would read the tree before `cd` happened and see nothing.
const RESCAN_DELAY_MS = 800

let timer = null

/** One scan → one whole-map snapshot. The map is keyed by project id (the store's currency); the
 *  path→id resolution happens here, on the host, so nothing downstream has to know a path at all. */
export async function refreshExternalTermCounts() {
  if (!isHost) return
  const withPaths = projects.value.filter(p => p.local_path)
  // De-duplicated: two projects may legitimately point at the same directory, and the backend
  // should not scan it twice — they then both read the same count, which is the truth.
  const paths = [...new Set(withPaths.map(p => p.local_path))]
  if (withPaths.length === 0) {
    if (Object.keys(externalTermCounts.value).length > 0) externalTermCounts.value = {}
  } else {
    try {
      const byPath = await invoke('count_external_terminals', { paths })
      const next = {}
      for (const p of withPaths) next[p.id] = byPath[p.local_path] || 0
      externalTermCounts.value = next
    } catch (e) {
      // Leave the previous snapshot standing rather than zeroing the badges: a failed scan means "we
      // don't know right now", not "every window closed".
      console.error('[externalTerminals] scan failed', e)
    }
  }
  // Global complement: sessions matching none of `paths` — the header terminal button's own badge.
  // Runs even with zero projects, since every open window would then be unowned.
  try {
    externalTermGlobalCount.value = await invoke('count_external_terminals_global', { paths })
  } catch (e) {
    console.error('[externalTerminals] global scan failed', e)
  }
}

/** Poke a scan just after an external Terminal was opened, so the badge doesn't wait out the tick. */
export function scheduleExternalTermRescan() {
  if (!isHost) return
  setTimeout(refreshExternalTermCounts, RESCAN_DELAY_MS)
}

/** Boot the cycle. Idempotent — a second call keeps the existing timer. */
export function startExternalTerminalWatch() {
  if (!isHost || timer) return
  refreshExternalTermCounts()
  timer = hostInterval(refreshExternalTermCounts, POLL_MS)
}

// ── Detail view (1.22.0) ────────────────────────────────────────────────────────────────────────
//
// The badge above answers "how many"; this answers "which, and what is running in them". Same scan,
// same subtree-root rule on the Rust side (`scan_terminal_tree`), so the modal can never disagree
// with the number the badge shows.
//
// ON DEMAND ONLY, never on the badge's 5s cadence: `list_external_terminals` returns a command line
// for every process in Terminal.app's tree, which is far more than a count and has no business
// being polled in the background.

/** Is the detail view even meaningful on this screen? HOST ONLY, and published as a CAPABILITY so
 *  dock/TerminalStack.vue asks what it can DO rather than who it is: the scan reads the Mac's
 *  process table, and `list_external_terminals` is deliberately absent from
 *  services/hostInvoke.js's companion allowlist, so on a phone the button would open a modal that
 *  can only ever show an error. */
export const externalTerminalsSupported = isHost

export const showExternalTermModal = ref(false)
export const externalTermSessions = ref([])
export const externalTermLoading = ref(false)
/** Non-empty means the LAST refresh failed. Kept separate from `externalTermSessions` so a failed
 *  re-scan shows the error *next to* the previous list rather than blanking it — an empty list and
 *  a failed scan are different facts and must not render the same. */
export const externalTermError = ref('')

export async function refreshExternalTermSessions() {
  if (!isHost) return
  externalTermLoading.value = true
  externalTermError.value = ''
  try {
    // Every project's path, so each session can name the project it is standing in. Sessions
    // elsewhere are still returned — "what else do I have open" is half the reason to look.
    const paths = [...new Set(projects.value.filter(p => p.local_path).map(p => p.local_path))]
    externalTermSessions.value = await invoke('list_external_terminals', { paths })
  } catch (e) {
    externalTermError.value = String(e?.message || e)
    console.error('[externalTerminals] detail scan failed', e)
  } finally {
    externalTermLoading.value = false
  }
}

export function openExternalTermModal() {
  showExternalTermModal.value = true
  refreshExternalTermSessions()
}

export function closeExternalTermModal() {
  showExternalTermModal.value = false
}
