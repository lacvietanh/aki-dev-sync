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
import { connectionState, isHost, isSocketCongested, onFrame, send } from './bridge'
import { invoke } from '../utils/tauri'
import { FRAME_PTY_INPUT, FRAME_PTY_OUTPUT, FRAME_PTY_EXIT, FRAME_COMPANION_CONNECTED } from '../constants/protocol'

let started = false

/** Push the current scrollback to every companion as one `reset` pty_output frame — used both
 *  right after a companion joins (§4.4 "Scrollback replay") and is safe to call even if no PTY
 *  has been spawned yet (an empty scrollback just produces an empty reset, harmless).
 *
 *  Returns whether the replay actually went out, so the congestion recovery below can tell "the
 *  companions are back in a known-good state" from "still nothing got through". */
async function pushScrollback() {
  try {
    const { data, cols, rows, alive } = await invoke('pty_get_scrollback')
    // Sent even when `data` is empty: a rejoining companion whose terminal is already mounted
    // still needs the authoritative size and liveness, and an empty reset is harmless.
    return send({ t: FRAME_PTY_OUTPUT, data: data || '', reset: true, cols, rows, alive })
  } catch (e) {
    // No PTY session yet, or the command failed — nothing to replay. Not an error worth surfacing;
    // the companion's own TerminalView mount will retry via the same call once it opens the tab.
    console.debug('[ptyBridge] scrollback push skipped', e && e.message ? e.message : e)
    return false
  }
}

// ── Congestion recovery: drop the tail, replay the whole buffer ───────────────────────────────
//
// A runaway shell (`yes`) produces a `pty_output` frame every ~20ms forever. If the host's own
// socket cannot drain that fast, bridge.js turns those frames down (see its congestion section)
// rather than let the browser's send buffer grow without bound. That leaves the companions missing
// a run of bytes — and a terminal stream with a hole in it is not stale, it is corrupt, because
// xterm renders whatever the surviving half of an escape sequence implies.
//
// So a refusal is not treated as a lost frame to retry; it flips this seam into "the companions owe
// a re-hydrate" and, the moment the socket has genuinely caught up, it replays the ENTIRE scrollback
// as one `reset` — the same frame a joining companion gets, on the same code path. What the phone
// sees is a screen that jumps forward, never one that is subtly wrong.
//
// Note the timer only exists because output can stop while congested: recovery cannot be driven by
// the next `pty-output` event when the whole problem may be that there is no next event.
const RESYNC_RETRY_MS = 250
// The handle IS the "a replay is owed" state — there is no second flag to drift out of step with it.
// It is deliberately not cleared until the attempt has finished, so a burst of refused frames during
// an in-flight replay cannot start a second one.
let resyncTimer = null

function scheduleResync() {
  if (resyncTimer) return
  resyncTimer = setTimeout(async () => {
    let owed = true
    try {
      // Not connected, or still backed up — nothing to gain from replaying 256 KB into a full
      // buffer, and the check costs nothing until the socket is actually usable again.
      owed = connectionState.value !== 'open' || isSocketCongested() || !(await pushScrollback())
    } finally {
      resyncTimer = null
      if (owed) scheduleResync()
    }
  }, RESYNC_RETRY_MS)
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
    //
    // `alive` rides along for the same reason one step further: dropping it here was the desync
    // (plan §2.4). The host emits a liveness-only payload after every spawn — no bytes, no reset —
    // and a relay that forwarded only byte-bearing payloads swallowed exactly the "the shell came
    // back" news the phone needed. The field is forwarded ONLY when the host actually stated it, so
    // a companion can keep distinguishing "no news" from "dead".
    const hasAlive = typeof payload.alive === 'boolean'
    if (payload.data || payload.reset || hasAlive) {
      const frame = { t: FRAME_PTY_OUTPUT, data: payload.data || '', reset: !!payload.reset }
      if (hasAlive) frame.alive = payload.alive
      // A refused frame leaves a hole in the companions' byte stream — heal it with a full replay
      // rather than pretending the next chunk continues from where the last one left off.
      if (!send(frame)) scheduleResync()
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
