// Live external-`Terminal.app` count — the producer behind the `TERM` cell's bottom (slate) badge.
// Derived live count via host process-table scan (`list_terminal_sessions` in system.rs) instead of an ever-growing session counter.
// Attribution is decided in `utils/terminalOwnership.js`; this file only owns the poll (docs/plan/done/terminal-ownership-model.md §10 S2).
// HOST ONLY: scan needs Mac process tree. Companion receives `externalTermCounts` through state mirror.
import { ref } from 'vue'
import { hostInterval } from '../utils/scheduler'
import { isHost } from '../services/bridge'
import { invoke } from '../utils/tauri'
import { attributeTerminalSessions } from '../utils/terminalOwnership'
import { projects, externalTermCounts, externalTermGlobalCount } from '../store/projectStore'

// Short enough that closing a window feels immediate, long enough that three tiny local subprocesses per tick stay invisible. Deliberately fixed, not user-configurable: this is not a status check.
const POLL_MS = 5000

// After the app opens a Terminal window, the shell needs a moment to exist and land in the project directory — scanning immediately would read the tree before `cd` happened and see nothing.
const RESCAN_DELAY_MS = 800

let timer = null

// Single snapshot pass: per-project and global badges derive from one attribution result and never disagree.
export async function refreshExternalTermCounts() {
  if (!isHost) return
  try {
    const sessions = await invoke('list_terminal_sessions')
    const { byProjectId, globalCount } = attributeTerminalSessions(sessions, projects.value)
    externalTermCounts.value = byProjectId
    externalTermGlobalCount.value = globalCount
  } catch (e) {
    // Leave the previous snapshot standing rather than zeroing the badges: a failed scan means "we don't know right now", not "every window closed".
    console.error('[externalTerminals] scan failed', e)
  }
}

// Rescan immediately after opening a Terminal window so the badge does not wait for the next poll tick.
export function scheduleExternalTermRescan() {
  if (!isHost) return
  setTimeout(refreshExternalTermCounts, RESCAN_DELAY_MS)
}

// Start polling loop. Idempotent: subsequent calls preserve existing timer.
export function startExternalTerminalWatch() {
  if (!isHost || timer) return
  refreshExternalTermCounts()
  timer = hostInterval(refreshExternalTermCounts, POLL_MS)
}

// ── Detail view (1.22.0) ────────────────────────────────────────────────────────────────────────
// Detail inspection (`describe_terminal_sessions`): fetched on demand when modal opens, never polled on cadence.

// Host-only capability: process tree scan requires local Mac access and is omitted from companion allowlist.
export const externalTerminalsSupported = isHost

export const showExternalTermModal = ref(false)
export const externalTermSessions = ref([])
export const externalTermLoading = ref(false)
// Separate from externalTermSessions so failed rescans preserve previous session list alongside error.
export const externalTermError = ref('')

export async function refreshExternalTermSessions() {
  if (!isHost) return
  externalTermLoading.value = true
  externalTermError.value = ''
  try {
    // Every project's path, so each session can name the project it is standing in. Sessions elsewhere are still returned — "what else do I have open" is half the reason to look.
    const paths = [...new Set(projects.value.filter(p => p.local_path).map(p => p.local_path))]
    externalTermSessions.value = await invoke('describe_terminal_sessions', { paths })
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
