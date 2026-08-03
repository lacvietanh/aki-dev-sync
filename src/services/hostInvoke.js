// Seam N — HOST half (docs/plan/done/remote-control.md §4, §13.2 `invoke`/`invoke_result`).
//
// The companion half lives in utils/tauri.js: on the companion, invoke() sends `{t:'invoke', cmd,
// args, id}` over the bridge and awaits a matching `invoke_result`. This module is the ONLY thing
// that answers those frames — without it a companion invoke() hangs until the 20s watchdog in
// bridge.request() fires (exactly the `get_agent_usage` / `log_frontend` timeouts seen on the phone).
//
// Symmetry with Seam A (intents.js): host-only, one onFrame listener, unknown/failed calls reply
// with a concrete error rather than going silent. Difference from an intent — an invoke expects a
// value back, so we always emit an `invoke_result` for the companion's `id` (ok on success, err on
// throw); an intent is fire-and-forget and emits nothing.
//
// We call `invoke` from utils/tauri.js, NOT `@tauri-apps/api/core` directly: on the host that
// export IS the raw tauriInvoke, so this respects R-3's single-import-point rule (utils/tauri.js is
// the only module allowed to touch the native core import).
import { isHost, onFrame, send } from './bridge'
import { invoke } from '../utils/tauri'
import { FRAME_INVOKE, FRAME_INVOKE_RESULT } from '../constants/protocol'

/**
 * The ONLY Tauri commands a companion may run on the Mac. Default-deny: anything absent is refused
 * here, before it reaches the IPC layer.
 *
 * Membership rule — a command belongs here only if it is (a) a read/query, or (b) a gesture the
 * companion UI is *designed* to perform on the Mac (the OPEN popup, the in-app terminal, the SSH
 * config editor, the Git modal, the usage slots). Everything destructive or privileged is reached
 * from a phone through an `action()`/intent instead (services/action.js), so the host runs the real
 * flow — including its guards. `run_sync` is the concrete case: its typed-project-name `--delete`
 * barrier lives in composables/useSync.js, which only runs host-side via
 * `remoteActions.requestSync`; forwarding a raw `run_sync` invoke would have skipped that barrier
 * entirely.
 *
 * Deliberately NOT here (each is either unreachable from companion code or must never be):
 *   run_sync, cancel_sync            -> remoteActions.requestSync / requestCancelSync (intents)
 *   save_projects                    -> PERSIST-1: a companion would persist ITS copy of `projects`
 *   write_global_note                -> noteStore.applyGlobalNoteEdit (intent); reads are allowed
 *   set_claude_profile,
 *   apply_statusline_config          -> privileged host/remote config writes, host-window only
 *   get_companion_status             -> its payload carries the relay's host token (bridge.js)
 *   start/stop_companion_server,
 *   get_companion_url, get_tailscale_https,
 *   set_tailscale_https,
 *   list_paired_devices, revoke_device -> remote-control admin; that UI is host-only (`isHost`)
 *   get_sync_delete_preview,
 *   get_file_conflict_info           -> only ever called from inside a host-side sync/drop flow
 *   count_external_terminals,
 *   list_external_terminals          -> the scan reads `Terminal.app`'s process tree, which only
 *                                       exists on the Mac; both callers are `isHost`-gated, and the
 *                                       detail modal's own button is hidden on a companion
 *   cleanup_legacy_baselines         -> host-boot / host-gated callers only
 *   pty_write, pty_resize            -> companion keystrokes/size ride their own pty frames
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
  // SSH config editor: composables/useSsh.js documents these writes as running from the clicker,
  // with only the reactive reconcile routed host-side through an action.
  'save_ssh_config',
  'undo_ssh_config',
  'redo_ssh_config',
  // Header one-shot installers — idempotent, user-initiated, no data loss.
  'install_akiclaudedoc',
  'install_ssh_terminal_color',
])

/** Run one inbound companion invoke on the host and reply with its result. Any throw becomes an
 *  `err` string on the reply (the companion's request() Promise rejects with it) — never an
 *  unanswered frame, which would strand the companion on the watchdog timeout. */
async function respondToInvoke(frame) {
  const { id, cmd, args } = frame
  // ADDRESSED BACK TO THE ONE CONNECTION THAT ASKED, on every reply path. `from` is stamped by the
  // relay from its own connection counter (src-tauri/src/web_server.rs), so it names the real sender
  // and cannot be forged. It is echoed as `to` because a request `id` comes from a PER-PAGE counter
  // starting at 1: broadcast, two pages each with an id-1 call in flight resolved each other's
  // answers — silent wrong data, which is worse than a failure because nothing reports it.
  //
  // PER CONNECTION IS THE UNIT THAT MAKES THAT TRUE. The counter is per page, and one paired phone can
  // have two pages open; a device-level reply would still cross those two pages' id-1 calls, which is
  // the same bug at a shorter range. Echoed opaquely — this module never inspects the value.
  // `undefined` on a frame from an older companion bundle simply serializes away, leaving a broadcast
  // exactly as before.
  const to = frame.from
  if (!COMPANION_ALLOWED_COMMANDS.has(cmd)) {
    // Answer with a concrete error rather than dropping the frame: silence would strand the
    // companion on bridge.request()'s watchdog and hide the refusal from both consoles. Addressed
    // like every other reply — a refusal delivered to the wrong phone is the same bug in a hat.
    console.error(`[hostInvoke] refused command "${cmd}" — not in COMPANION_ALLOWED_COMMANDS`)
    send({ t: FRAME_INVOKE_RESULT, id, to, err: `command "${cmd}" is not allowed from a companion` })
    return
  }
  try {
    const ok = await invoke(cmd, args)
    // JSON.stringify drops an `undefined` value, so a void command serializes to a frame with
    // neither `ok` nor `err` — the companion reads that as resolve(undefined), which is correct.
    send({ t: FRAME_INVOKE_RESULT, id, to, ok })
  } catch (e) {
    // Preserve the host's real error text so the phone console shows the actual Tauri failure,
    // not a generic "rejected". Tauri command errors are usually plain strings already.
    const err = e && e.message ? e.message : String(e)
    console.error(`[hostInvoke] command "${cmd}" failed`, e)
    send({ t: FRAME_INVOKE_RESULT, id, to, err })
  }
}

/** Boot this seam. Host-only: wires incoming `invoke` frames to the real Tauri IPC. No-op on the
 *  companion (it is the SENDER of invoke frames, via utils/tauri.js). */
export function initHostInvoke() {
  if (!isHost) return
  onFrame((frame) => {
    if (frame && frame.t === FRAME_INVOKE) respondToInvoke(frame)
  })
}
