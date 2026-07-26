// In-app terminal — role wiring (docs/plan/1.20.0-terminal-and-remote-sync.md §4).
//
// TerminalView.vue owns the xterm.js instance (creation, DOM mount, the mobile key row) and hands
// it to this composable; this file owns everything role-specific: which event feeds the terminal,
// where keystrokes go, and who is allowed to resize the shared PTY (T-4).
//
// BINARY-SAFE TRANSPORT: PTY bytes ride the wire as base64 (see src-tauri/src/pty.rs module doc
// comment). `atob`/`btoa` here are used ONLY as a raw-byte codec (each char code 0-255 = one raw
// byte, fed straight into a Uint8Array) — NOT to decode/encode text, which is the documented
// mojibake trap (RULE-coding C5). Actual UTF-8 interpretation of that byte stream happens inside
// xterm.js's own `Terminal.write(Uint8Array)`, which keeps a stateful UTF-8 decoder across calls
// and so correctly reassembles a multi-byte sequence split across two PTY read()s — nothing here
// needs its own split-sequence buffering.
//
// ENV-1 (docs/plan/done/remote-control.md §9): this file's `isHost` branch is one of the two places in
// the terminal feature allowed to read it directly (the other is services/ptyBridge.js) —
// TerminalView.vue's template must stay neutral.
import { onBeforeUnmount, ref } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { isHost, onFrame, send } from '../services/bridge'
import { invoke } from '../utils/tauri'
import { FRAME_PTY_INPUT, FRAME_PTY_OUTPUT, FRAME_PTY_RESIZE, FRAME_PTY_EXIT } from '../constants/protocol'

function decodeBase64ToBytes(b64) {
  const bin = atob(b64)
  const bytes = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i)
  return bytes
}

function encodeBytesToBase64(bytes) {
  let bin = ''
  for (let i = 0; i < bytes.length; i++) bin += String.fromCharCode(bytes[i])
  return btoa(bin)
}

/**
 * Wires an already-created xterm.js `Terminal` instance to the shared PTY.
 *
 * @param {import('@xterm/xterm').Terminal} term
 * @returns {{ start, hostResize, sendRaw, armCtrl, ctrlArmed, alive, restart, clear, kill,
 *             openExternal, cd }}
 */
export function usePtyTerminal(term) {
  let unlistenHostOutput = null
  let unlistenHostExit = null
  let unsubscribeFrame = null
  // §4.5 mobile key row: "Ctrl (sticky modifier — tap Ctrl, then tap C)". Armed by TerminalView's
  // Ctrl button; consumed by the very next onData chunk (see wireInput), then auto-disarmed.
  // Exposed as a ref (not a plain bool) so the key row's active-state styling stays reactive.
  const ctrlArmed = ref(false)
  // Is there a live shell behind this terminal? Drives the header's RESTART affordance and the
  // "type anything to respawn" behaviour. Set from `pty_get_scrollback().alive` on mount, and from
  // then on ONLY from what the host says: every liveness-bearing pty-output payload/frame, plus the
  // exit event/frame. Never guessed from output text, and never left to drift — a screen believing
  // the wrong thing here is what let one screen's keystroke destroy the other screen's live shell.
  const alive = ref(false)
  // Remembered from `start()` so RESTART reopens in the same directory the tab was opened for.
  let bootCwd = null
  // Guards against two respawns racing (e.g. the user mashes keys into a dead terminal).
  let restarting = false

  /** Liveness is only ever adopted when the host actually stated it. The host omits the field
   *  entirely on ordinary output (see PtyOutputPayload in src-tauri/src/pty.rs), so anything that
   *  is not a boolean means "this payload says nothing about liveness" — treating a missing field
   *  as `false` would mark the terminal dead on the first chunk of normal output. */
  function applyAlive(value) {
    if (typeof value === 'boolean') alive.value = value
  }

  function writeChunk(base64, reset) {
    // `term.reset()`, not `term.clear()`: clear() keeps the current line and all SGR/cursor modes,
    // so a scrollback replay or a RESTART would inherit the dead shell's colour state and leave a
    // stray prompt fragment on the first line.
    if (reset) term.reset()
    if (base64) term.write(decodeBase64ToBytes(base64))
  }

  async function ensureSpawned(cwd) {
    try {
      await invoke('pty_spawn', { cwd: cwd ?? null })
      // Not an optimistic guess: the host emits the authoritative liveness for this same call and
      // it will overwrite this within a frame. Set here only so the local screen does not sit on a
      // stale `false` for one WS round-trip.
      alive.value = true
    } catch (e) {
      console.error('[usePtyTerminal] pty_spawn failed', e)
    }
  }

  /** RESTART: kill whatever is there, wipe the shared scrollback, spawn a fresh login shell. The
   *  host broadcasts the reset itself (src-tauri/src/pty.rs), so this needs no local clearing and
   *  works identically when a companion triggers it through the invoke seam.
   *
   *  DESTRUCTIVE, AND THEREFORE BOUND TO THE EXPLICIT BUTTON ONLY. It ends whatever is running in
   *  the shell and wipes the shared scrollback, which is not something a stray tap on a phone in
   *  another room may ever cause — see `respawn` for what an ordinary keystroke does instead. */
  async function restart() {
    if (restarting) return
    restarting = true
    try {
      await invoke('pty_restart', { cwd: bootCwd ?? null })
      alive.value = true
    } catch (e) {
      console.error('[usePtyTerminal] pty_restart failed', e)
    } finally {
      restarting = false
    }
  }

  /** The "press any key to start a new shell" path. Deliberately `pty_spawn`, NOT `pty_restart`:
   *  `pty_spawn` is idempotent (T-3), so if this screen's `alive` is wrong and a shell IS running,
   *  the worst outcome is a no-op — where `pty_restart` would kill the live shell, everything
   *  inside it, and the scrollback. The person typing is holding a phone away from the Mac and did
   *  not ask to destroy anything; the flow is shaped so that they cannot, rather than guarded by
   *  hoping the liveness flag is fresh. It also keeps the dead shell's output above the new prompt,
   *  which is the context you were reading when it died. */
  async function respawn() {
    if (restarting) return
    restarting = true
    try {
      await ensureSpawned(bootCwd)
    } finally {
      restarting = false
    }
  }

  /** Wipes the HOST's ring buffer, not just this screen — otherwise the output comes straight
   *  back on the next reconnect, which is what makes a purely local clear feel broken. */
  async function clear() {
    try {
      await invoke('pty_clear')
    } catch (e) {
      console.error('[usePtyTerminal] pty_clear failed', e)
    }
  }

  async function kill() {
    try {
      await invoke('pty_kill')
    } catch (e) {
      console.error('[usePtyTerminal] pty_kill failed', e)
    }
  }

  /** "Open in Terminal.app" (VS Code's external-terminal button): hands off the shell's CURRENT
   *  directory, so `cd`-ing around in here and then jumping out lands in the right place. Falls
   *  back to the plain home-directory window if the cwd cannot be read. */
  async function openExternal() {
    try {
      const cwd = await invoke('pty_cwd')
      // Contract C-1 (docs/plan/1.20.1-flow-audit-fixes.md §1.1): `null`, never the literal `'~'`.
      // The host side does no shell expansion, so `cd "~"` looks for a directory actually named
      // `~` and always fails; a null path means "no cd at all" and the shell opens in $HOME by
      // itself, which is the fallback that was intended all along.
      await invoke('open_local_terminal', { localPath: cwd || null })
    } catch (e) {
      console.error('[usePtyTerminal] openExternal failed', e)
    }
  }

  /** Jumps the shared shell into a directory — the in-app counterpart of the project popup's
   *  "Terminal" item. Sent as ordinary keystrokes rather than a dedicated command so it behaves
   *  exactly like typing it (lands in shell history, respects the shell's own cd hooks) and needs
   *  no new host-side surface. Single-quoted with the POSIX '\'' escape so paths containing
   *  spaces, quotes or `$` cannot break out into command execution. */
  function cd(path) {
    if (!path) return
    sendRaw(`cd '${String(path).replace(/'/g, "'\\''")}'\r`)
  }

  /** Hydrates scrollback + the PTY's CURRENT size on open/rejoin — see pty.rs's `PtyScrollback`
   *  doc comment for why size travels with this call instead of waiting on the next resize echo. */
  async function hydrateScrollback() {
    try {
      const { data, cols, rows, alive: isAlive } = await invoke('pty_get_scrollback')
      if (cols && rows) term.resize(cols, rows)
      writeChunk(data, true)
      alive.value = !!isAlive
    } catch (e) {
      console.error('[usePtyTerminal] pty_get_scrollback failed', e)
    }
  }

  function wireOutput() {
    if (isHost) {
      // Lowest latency for the host's own screen: the PTY reader thread (src-tauri/src/pty.rs)
      // emits this Tauri event directly — no WS round-trip. services/ptyBridge.js separately
      // relays the same event to companions.
      listen('pty-output', (event) => {
        const payload = (event && event.payload) || {}
        if (payload.data || payload.reset) writeChunk(payload.data, !!payload.reset)
        // A liveness-only payload carries neither bytes nor `reset` (that is how the host says
        // "the shell came back" without touching the screen), so this must sit OUTSIDE the write
        // condition above.
        applyAlive(payload.alive)
      }).then((un) => {
        unlistenHostOutput = un
      })
      listen('pty-exit', () => {
        alive.value = false
      }).then((un) => {
        unlistenHostExit = un
      })
    } else {
      unsubscribeFrame = onFrame((frame) => {
        if (!frame) return
        if (frame.t === FRAME_PTY_OUTPUT) {
          if (frame.data || frame.reset) writeChunk(frame.data, !!frame.reset)
          if (frame.reset && frame.cols && frame.rows) term.resize(frame.cols, frame.rows)
          // Applied for EVERY pty_output frame that states it, not only for `reset` frames. That
          // narrower test was the desync: the host's "shell restarted" news reaches a companion on
          // a plain frame, and a companion that ignored it went on believing a live shell was dead.
          applyAlive(frame.alive)
        } else if (frame.t === FRAME_PTY_EXIT) {
          alive.value = false
        } else if (frame.t === FRAME_PTY_RESIZE) {
          // T-4: the ONLY path a companion's xterm is ever resized through — it never calls
          // pty_resize itself, and never derives a size from its own container.
          term.resize(frame.cols, frame.rows)
        }
      })
    }
  }

  /** Shared funnel for every keystroke source: the real (soft) keyboard via xterm's onData, AND
   *  the mobile key row's synthetic sequences (Esc/Tab/arrows/Enter) — both end up here so there
   *  is exactly one place that decides host-direct-invoke vs. raw-bridge-frame. */
  function sendRaw(str) {
    if (!str) return
    const data = encodeBytesToBase64(new TextEncoder().encode(str))
    if (isHost) {
      invoke('pty_write', { data }).catch((e) => console.error('[usePtyTerminal] pty_write failed', e))
    } else {
      // Raw frame, not the generic invoke/invoke_result seam — an ack round-trip per keystroke
      // would double traffic and add latency for no benefit (see protocol.js's FRAME_PTY_INPUT
      // doc comment). services/ptyBridge.js applies it host-side.
      send({ t: FRAME_PTY_INPUT, data })
    }
  }

  function toCtrlByte(ch) {
    const code = ch.toUpperCase().charCodeAt(0)
    // '@'..'_' (64-95) map to control codes 0-31 — the standard terminal Ctrl+key convention
    // (e.g. 'C' -> 0x03, ETX / SIGINT).
    return code >= 64 && code <= 95 ? String.fromCharCode(code - 64) : null
  }

  function wireInput() {
    // T-5: no local echo — every keystroke goes straight to the PTY and the terminal shows only
    // what pty-output/pty_output later renders back, exactly like every other bit of mirrored
    // state in this app being SSOT'd on the host.
    term.onData((chunk) => {
      // Dead shell: swallow the keystroke and respawn instead of writing into a closed PTY (which
      // is what made `exit` look like a freeze — pty_write simply errored into the console and the
      // screen never moved again). Mirrors the "press any key to restart" convention.
      // `respawn()`, never `restart()` — see respawn's doc comment for why the difference is the
      // whole point.
      if (!alive.value) {
        respawn()
        return
      }
      let out = chunk
      if (ctrlArmed.value) {
        ctrlArmed.value = false
        if (chunk.length === 1) {
          const ctrlByte = toCtrlByte(chunk)
          if (ctrlByte !== null) out = ctrlByte
        }
      }
      sendRaw(out)
    })
  }

  function armCtrl() {
    ctrlArmed.value = true
  }

  /** T-4: call ONLY from the host's own fit-on-resize handler (TerminalView.vue calls this
   *  unconditionally on every fit; it is a no-op on a companion, so the component never needs to
   *  know which screen it is running on). Resizes the real PTY, then echoes the authoritative
   *  size to every companion so their xterm matches without ever asking for it. */
  async function hostResize(cols, rows) {
    if (!isHost || !cols || !rows) return
    // Refuse absurd sizes. FitAddon measures a container that is momentarily 0px — the panel is
    // collapsed, the tab is mid-transition, the window is being dragged — and returns a 1- or
    // 2-row fit; pushing that to the real PTY re-wraps the running shell's output permanently and
    // is unrecoverable without a restart. Below this floor the local render is simply left as-is.
    if (cols < 8 || rows < 3) return
    try {
      await invoke('pty_resize', { cols, rows })
      send({ t: FRAME_PTY_RESIZE, cols, rows })
    } catch (e) {
      console.error('[usePtyTerminal] pty_resize failed', e)
    }
  }

  /** Boot: wire I/O, ensure the shared PTY exists (idempotent — T-3), hydrate scrollback + size. */
  async function start(cwd) {
    bootCwd = cwd ?? null
    wireOutput()
    wireInput()
    await ensureSpawned(cwd)
    await hydrateScrollback()
  }

  onBeforeUnmount(() => {
    if (unlistenHostOutput) unlistenHostOutput()
    if (unlistenHostExit) unlistenHostExit()
    if (unsubscribeFrame) unsubscribeFrame()
  })

  return { start, hostResize, sendRaw, armCtrl, ctrlArmed, alive, restart, clear, kill, openExternal, cd }
}
