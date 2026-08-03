import { computed } from 'vue'
import { isHost } from '../services/bridge'
import TerminalView from '../components/TerminalView.vue'
import SimpleView from '../components/SimpleView.vue'

// ENV-1 boundary module for terminal view resolution (host vs companion).
// Host mounts full xterm.js TerminalView; companion mounts line-stream SimpleView.
export function useTerminalViewType() {
  const ViewComponent = computed(() => (isHost ? TerminalView : SimpleView))
  return { ViewComponent }
}
