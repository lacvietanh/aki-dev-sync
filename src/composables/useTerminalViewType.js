import { computed } from 'vue'
import TerminalView from '../components/TerminalView.vue'

// Resolution seam (docs/plan/done/wish-terminal-manual-resize-authority.md): host and companion both mount TerminalView; seam preserved to revive SimpleView.vue as opt-in low-bandwidth mode.
export function useTerminalViewType() {
  const ViewComponent = computed(() => TerminalView)
  return { ViewComponent }
}
