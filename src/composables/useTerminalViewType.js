import { computed } from 'vue'
import TerminalView from '../components/TerminalView.vue'

// ENV-1 boundary for terminal view resolution. Both host and companion mount the same xterm.js
// TerminalView now (docs/plan/wish-terminal-manual-resize-authority.md) — resize safety comes from
// the explicit manual-authority handoff, not from the companion mounting a plain-text component.
// The seam is kept (rather than TerminalStack importing TerminalView directly) as the one place to
// revive SimpleView.vue later as an opt-in low-bandwidth mode; it stays in the tree, unreferenced.
export function useTerminalViewType() {
  const ViewComponent = computed(() => TerminalView)
  return { ViewComponent }
}
