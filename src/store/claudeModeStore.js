import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

// Whether Claude Code (this machine) is in 'native' or 'proxy' mode. Native-account usage
// monitoring (rate-limit %, email/org, session cost) reads straight from Anthropic's own
// account API and pricing table — none of it reflects a proxy's actual traffic/billing once
// requests are routed elsewhere, so the local usage monitor gets locked off while proxy is
// active (see claudeModeStore usage in AgentUsageSection.vue) instead of showing numbers
// that look real but aren't.
export const claudeMode = ref('native')

export async function refreshClaudeMode() {
  try {
    claudeMode.value = await invoke('get_claude_mode')
  } catch {
    claudeMode.value = 'native'
  }
}
