// Seam N — HOST half (docs/plan/done/remote-control.md §4, §13.2 `invoke`/`invoke_result`).
// Answers companion invoke() frames over the bridge with invoke_result (ok or err), preventing 20s watchdog timeouts.
// Symmetrical with Seam A (intents.js) but request-response (awaits value and emits invoke_result for frame.id) instead of fire-and-forget.
// Calls invoke from utils/tauri.js (R-3 single import point) rather than @tauri-apps/api/core directly.
import { isHost, onFrame, send } from './bridge'
import { invoke } from '../utils/tauri'
import { FRAME_INVOKE, FRAME_INVOKE_RESULT } from '../constants/protocol'

/**
 * Default-deny allowlist of Tauri commands callable by a companion.
 * Must only contain safe reads/queries or companion-initiated UI gestures (terminal tabs, SSH editor, Git modal, usage slots).
 * Destructive/privileged commands route through host-side intents (services/action.js) to enforce UI guards (e.g. run_sync delete barrier).
 * Excluded: privileged config writes, companion server admin (isHost-only), sync/drop internals, and direct pty streaming.
 */
export const COMPANION_ALLOWED_COMMANDS = new Set([
  // Reads / status queries — no side effect on the Mac beyond a log line.
  'check_for_updates',
  'check_ide_availability',
  'check_project_stack',
  'check_statusline_status',
  'check_sync_status',
  'find_in_downloads',
  'get_claude_mode',
  'get_git_info',
  'get_log_path',
  'get_project_icons_map',
  'get_ssh_history_status',
  'get_ssh_hosts',
  'is_debug_mode',
  'load_projects',
  'log_frontend',
  'read_global_note',
  'read_project_changelog',
  'read_ssh_config',
  'resolve_remote_path',
  'resolve_report_html',
  'build_remote_ssh_command',
  // Agent usage slots — the phone runs its own slots (usageSlotStore targets are per-screen).
  'get_agent_usage',
  'provision_agent_usage',
  // In-app terminal (docs/plan/done/1.20.0-terminal-and-remote-sync.md §4.4): the phone drives real tabs.
  'pty_clear',
  'pty_close_tab',
  'pty_cwd',
  'pty_get_scrollback',
  'pty_kill',
  'pty_list_tabs',
  'pty_restart',
  'pty_spawn',
  // OPEN popup / header links — "do it on the Mac" is the whole point of these buttons on a phone.
  'macos_open',
  'open_local_terminal',
  'open_remote_subprocess',
  // Git modal — fetch/pull/push/commit from the phone.
  'run_git_command',
  // SSH config editor: composables/useSsh.js documents these writes as running from the clicker, with only the reactive reconcile routed host-side through an action.
  'save_ssh_config',
  'undo_ssh_config',
  'redo_ssh_config',
  // Header one-shot installers — idempotent, user-initiated, no data loss.
  'install_akiclaudedoc',
  'install_ssh_terminal_color',
])

/** Executes inbound companion invoke on host; always replies with invoke_result (ok or err) to prevent watchdog timeout. */
async function respondToInvoke(frame) {
  const { id, cmd, args } = frame
  // Echo frame.from as 'to' so replies route strictly to requesting connection (request IDs are per-page counters starting at 1).
  const to = frame.from
  if (!COMPANION_ALLOWED_COMMANDS.has(cmd)) {
    // Reply with explicit error instead of dropping frame to avoid companion watchdog timeout.
    console.error(`[hostInvoke] refused command "${cmd}" — not in COMPANION_ALLOWED_COMMANDS`)
    send({ t: FRAME_INVOKE_RESULT, id, to, err: `command "${cmd}" is not allowed from a companion` })
    return
  }
  try {
    const ok = await invoke(cmd, args)
    // Void commands serialize without ok/err (JSON.stringify drops undefined), resolving as undefined on companion.
    send({ t: FRAME_INVOKE_RESULT, id, to, ok })
  } catch (e) {
    // Forward real error message to companion console.
    const err = e && e.message ? e.message : String(e)
    console.error(`[hostInvoke] command "${cmd}" failed`, e)
    send({ t: FRAME_INVOKE_RESULT, id, to, err })
  }
}

/** Host-only boot: wires incoming FRAME_INVOKE frames to respondToInvoke. */
export function initHostInvoke() {
  if (!isHost) return
  onFrame((frame) => {
    if (frame && frame.t === FRAME_INVOKE) respondToInvoke(frame)
  })
}
