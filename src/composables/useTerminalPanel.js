// The terminal dock stack's collapse state, and the one gesture that expands it.
//
// SSOT-1 / SYNC-1 (docs/plan/done/remote-control.md §9): this ref deliberately lives in a composable
// and NOT in src/store/*.js. services/mirror.js auto-discovers every `isRef` export under
// src/store/ and mirrors it — which is exactly wrong here: which stack a screen is looking at is
// navigation, local to each device, and mirroring it would yank the Mac's dock around whenever the
// phone expanded/collapsed a stack. The same reasoning already applies to `editingProject` /
// `showConfigModal`, and to useLogs.js's `consoleRef` / `copied`. `isLogExpanded` IS in the store and
// IS mirrored (mirror.js's PER_SCREEN_KEYS — per-screen state is excluded from broadcast there, not
// exempted from mirroring altogether); the terminal stack's own collapse state below follows the
// same per-screen exclusion, just via a different mechanism (never a mirrored ref at all, rather
// than a mirrored-but-excluded one).
import { ref } from 'vue'

/** Terminal dock stack's collapse state. PER-SCREEN (see file header) — lives here, not in
 *  src/store/, so mirror.js never discovers it. */
export const terminalStackCollapsed = ref(true)

/** Expand the terminal dock stack. This module OWNS `terminalStackCollapsed`, so callers that need
 *  the stack open (useTerminalTabs.js's openScopeTerminal, on the project/global terminal buttons)
 *  ask for the behaviour rather than writing another module's ref by hand. */
export function expandTerminalStack() {
  terminalStackCollapsed.value = false
}
