import { ref } from 'vue'
import { invoke } from '../utils/tauri'

// 'native' | 'proxy'. Local usage monitor is disabled in proxy mode as Anthropic account API stats do not reflect proxied traffic.
export const claudeMode = ref('native')

// Re-reads ~/.claude/settings.json since external edits or profile switches can change mode at any time.
export async function refreshClaudeMode() {
  try {
    claudeMode.value = await invoke('get_claude_mode')
  } catch {
    // Retain previous mode on error to avoid falsely unlocking usage monitor if proxy read fails.
  }
}

