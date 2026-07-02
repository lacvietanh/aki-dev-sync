import { ref } from 'vue'

// Single global kill switch for everything that touches a remote host: SSH-based project
// sync (pull/push/select/open-remote), background remote-diff checks, and Claude Code
// remote usage monitoring. For users who don't work with remote hosts, this stops the app
// from running any SSH/remote check at all instead of silently failing per-project.
export const remoteModeEnabled = ref(localStorage.getItem('aki-remote-mode-enabled') !== 'false')

export function toggleRemoteMode() {
  remoteModeEnabled.value = !remoteModeEnabled.value
  localStorage.setItem('aki-remote-mode-enabled', String(remoteModeEnabled.value))
}
