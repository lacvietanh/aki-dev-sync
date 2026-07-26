// In-app terminal — HOST-side relay (docs/plan/1.20.0-terminal-and-remote-sync.md §4.4 wire path).
//
// Symmetry with services/hostInvoke.js and services/intents.js: host-only, boot once from
// services/index.js's initRemote(), no-op on a companion. This module's whole job is bridging the
// PTY's Tauri-native event ('pty-output', emitted by the raw reader thread in
// src-tauri/src/pty.rs) onto the WS relay so companions see the same bytes the host's own
// TerminalView renders — and the reverse direction for companion keystrokes.
//
// Deliberately NOT routed through services/mirror.js (isRef diffing) or services/intents.js
// (action() dispatch): terminal bytes are a firehose, not diffable app state and not a discrete
// gesture. It rides raw top-level frames on the same bridge socket instead (FRAME_PTY_INPUT /
// FRAME_PTY_OUTPUT, src/constants/protocol.js).
//
// ENV-1 (docs/plan/done/remote-control.md §9): this is one of the two files allowed to branch on
// `isHost` for the in-app terminal (the other is usePtyTerminal.js's branch) — never
// TerminalView.vue.
import { listen } from '@tauri-apps/api/event'
import { isHost, onFrame, send } from './bridge'
import { invoke } from '../utils/tauri'
import { FRAME_PTY_INPUT, FRAME_PTY_OUTPUT, FRAME_PTY_EXIT, FRAME_COMPANION_CONNECTED } from '../constants/protocol'

let started = false

/** Push the current scrollback to every companion as one `reset` pty_output frame — used both
 *  right after a companion joins (§4.4 "Scrollback replay") and is safe to call even if no PTY
 *  has been spawned yet (an empty scrollback just produces an empty reset, harmless). */
async function pushScrollback() {
  try {
    const { data, cols, rows, alive } = await invoke('pty_get_scrollback')
    // Sent even when `data` is empty: a rejoining companion whose terminal is already mounted
    // still needs the authoritative size and liveness, and an empty reset is harmless.
    send({ t: FRAME_PTY_OUTPUT, data: data || '', reset: true, cols, rows, alive })
  } catch (e) {
    // No PTY session yet, or the command failed — nothing to replay. Not an error worth surfacing;
    // the companion's own TerminalView mount will retry via the same call once it opens the tab.
    console.debug('[ptyBridge] scrollback push skipped', e && e.message ? e.message : e)
  }
}

/** Boot this seam. Host-only — see module doc comment. Idempotent, matching the other seam
 *  init functions' contract even though nothing here calls it more than once today. */
export function initPtyBridge() {
  if (!isHost || started) return
  started = true

  // Lowest-latency path for the HOST's own TerminalView is a direct `listen('pty-output')` in
  // usePtyTerminal.js — this listener exists ONLY to relay the same event to companions over WS.
  listen('pty-output', (event) => {
    const payload = (event && event.payload) || {}
    // `reset` rides along so CLEAR / RESTART wipe the phone's screen in the same beat as the
    // Mac's — an append-only relay would leave the phone showing a scrollback the host has
    // already discarded, which then never self-corrects until the next reconnect.
    if (payload.data || payload.reset) {
      send({ t: FRAME_PTY_OUTPUT, data: payload.data || '', reset: !!payload.reset })
    }
  })

  // The shell ended. Companions cannot see the Tauri-native event, and must not try to infer this
  // from the output bytes — see FRAME_PTY_EXIT's doc comment in constants/protocol.js.
  listen('pty-exit', () => send({ t: FRAME_PTY_EXIT }))

  // Companion keystrokes arrive as raw pty_input frames (not a generic `invoke`, to avoid an
  // invoke_result round-trip per keystroke — see protocol.js's FRAME_PTY_INPUT doc comment).
  onFrame((frame) => {
    if (frame && frame.t === FRAME_PTY_INPUT && frame.data) {
      invoke('pty_write', { data: frame.data }).catch((e) => {
        console.error('[ptyBridge] pty_write failed', e)
      })
    } else if (frame && frame.t === FRAME_COMPANION_CONNECTED) {
      pushScrollback()
    }
  })
}
