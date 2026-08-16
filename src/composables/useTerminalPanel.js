// SSOT-1 / SYNC-1 (docs/plan/done/remote-control.md §9): terminal dock collapse state lives in a composable, not src/store/*.js, to prevent services/mirror.js from auto-mirroring local per-screen navigation state across devices.
import { ref } from 'vue'

/** Terminal dock stack collapse state (per-screen, unmirrored). */
export const terminalStackCollapsed = ref(true)

/** Expands terminal dock stack; provides module-owned setter for external callers (e.g. useTerminalTabs.js). */
export function expandTerminalStack() {
  terminalStackCollapsed.value = false
}
