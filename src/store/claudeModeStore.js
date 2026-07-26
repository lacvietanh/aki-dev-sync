import { ref } from 'vue'
import { invoke } from '../utils/tauri'

// Whether Claude Code (this machine) is in 'native' or 'proxy' mode. Native-account usage
// monitoring (rate-limit %, email/org, session cost) reads straight from Anthropic's own
// account API and pricing table - none of it reflects a proxy's actual traffic/billing once
// requests are routed elsewhere, so the local usage monitor gets locked off while proxy is
// active (see claudeModeStore usage in AgentUsageSection.vue) instead of showing numbers
// that look real but aren't.
export const claudeMode = ref('native')

// Called at boot, after a profile change, and on a wake self-heal (useBackgroundRefresh.js) - the
// mode can change under the app at any time, since ~/.claude/settings.json is a file the user (or
// another tool) may edit directly.
export async function refreshClaudeMode() {
  try {
    claudeMode.value = await invoke('get_claude_mode')
  } catch {
    // Deliberately NOT forced back to 'native': the ref already starts at 'native', so this is
    // identical at boot, while a later failed read no longer wipes a known-good 'proxy' reading
    // and unlocks a usage monitor whose numbers would then be meaningless.
  }
}
