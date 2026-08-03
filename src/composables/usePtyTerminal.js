// In-app terminal — role wiring (docs/plan/done/1.20.0-terminal-and-remote-sync.md §4).
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
import { onBeforeUnmount, ref, watch } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { isHost, onFrame, send } from '../services/bridge'
import { invoke } from '../utils/tauri'
import { decodeBase64ToBytes, encodeBytesToBase64 } from '../utils/ptyCodec'
import { FRAME_PTY_INPUT, FRAME_PTY_OUTPUT, FRAME_PTY_RESIZE, FRAME_PTY_EXIT } from '../constants/protocol'
import { consumeTabPendingCmd } from '../store/terminalTabsStore'

// ── Module-level tab liveness tracker (S3 fix) ──────────────────────────────────────────────────
//
// Per-tab liveness that SURVIVES every TerminalView mount/unmount, unlike each composable
// instance's own `alive` ref above. Before this, useTerminalTabs.js's `tabAlive` map (consumed by
// TerminalTabStrip.vue's per-chip tint and TerminalCell.vue's dot badge) had exactly one writer:
// dock/TerminalStack.vue's `watchEffect`, which aggregated every MOUNTED tab's own `alive`. That
// stopped updating the moment the dock stack collapsed (the whole stack body unmounts — see
// TerminalStack.vue's own doc comment) and never ran at all on a companion, which does not mount
// every tab up front (see `activatedTabs` below). So a shell dying while the stack was collapsed,
// or on a companion, never turned its chip/badge red.
//
// This tracker is a SEPARATE set of listeners, registered ONCE at module scope regardless of
// mount/unmount, and lives in THIS file (not useTerminalTabs.js) specifically so its isHost branch
// stays inside the two files ENV-1 allows (this one and services/ptyBridge.js) rather than leaking
// into a file that must stay role-neutral.
/** `{ [tabId]: 'unknown' | true | false }` — same tri-state semantics as each instance's own
 *  `alive`. Re-exported by useTerminalTabs.js as `tabAlive` so existing consumers
 *  (TerminalTabStrip.vue, TerminalCell.vue) need no changes. */
export const tabLiveness = ref({})

function setTabLiveness(tabId, value) {
  const id = typeof tabId === 'number' ? tabId : 0
  if (tabLiveness.value[id] === value) return
  tabLiveness.value = { ...tabLiveness.value, [id]: value }
}

let livenessTrackingStarted = false

/** Idempotent — safe to call from every screen (useTerminalTabs.js does, at module scope); only
 *  the first call registers anything. */
export function startTabLivenessTracking() {
  if (livenessTrackingStarted) return
  livenessTrackingStarted = true
  if (isHost) {
    // Mirrors usePtyTerminal's own per-instance `applyAlive`/exit handling, but unfiltered by
    // tabId (this tracker covers EVERY tab, not one) and never torn down (module lifetime, same as
    // services/ptyBridge.js's listeners).
    listen('pty-output', (event) => {
      const payload = (event && event.payload) || {}
      if (typeof payload.alive === 'boolean') setTabLiveness(payload.tab_id, payload.alive)
    })
    listen('pty-exit', (event) => {
      const payload = (event && event.payload) || {}
      setTabLiveness(payload.tab_id, false)
    })
  } else {
    onFrame((frame) => {
      if (!frame) return
      if (frame.t === FRAME_PTY_OUTPUT) {
        if (typeof frame.alive === 'boolean') setTabLiveness(frame.tab_id, frame.alive)
      } else if (frame.t === FRAME_PTY_EXIT) {
        setTabLiveness(frame.tab_id, false)
      }
    })
  }
}

/** HOST BOOT ONLY — called from useTerminalTabs.js's `initTerminalTabs`. Seeds this tracker from
 *  `pty_list_tabs()`'s per-tab `alive` so a re-adopted tab's chip/badge is correct immediately,
 *  instead of sitting on `'unknown'` until that tab's first `pty_output`/`pty_exit`. */
export function seedTabLiveness(list) {
  if (!Array.isArray(list) || list.length === 0) return
  const next = { ...tabLiveness.value }
  for (const t of list) {
    if (t && typeof t.id === 'number' && typeof t.alive === 'boolean') next[t.id] = t.alive
  }
  tabLiveness.value = next
}

/**
 * Wires an already-created xterm.js `Terminal` instance to ONE TAB's PTY.
 *
 * One instance of this composable per mounted TerminalView, i.e. one per tab. Everything it does is
 * scoped to `tabId`: every command carries it, every outgoing frame stamps it, and — the part that
 * is easiest to get wrong — every incoming event is FILTERED by it. Both the Tauri event path and
 * the companion frame path are broadcast channels: each listener sees EVERY tab's bytes and must
 * discard the ones that are not its own. A missing filter does not fail loudly; it writes another
 * shell's output into this terminal.
 *
 * @param {import('@xterm/xterm').Terminal} term
 * @param {number} [tabId=0] which terminal tab this surface drives. Defaults to 0, the tab every
 *   id-less caller has always driven (src-tauri/src/pty.rs), so existing single-terminal call sites
 *   keep working untouched.
 * @returns {{ start, ownsPtySize, showKeyRow, hostResize, sendRaw, emitKey, pendingModifiers,
 *             toggleModifier, ctrlByteFor, alive, restart, clear, kill, close, openExternal, cd }}
 */
export function usePtyTerminal(term, tabId = 0) {
  let unlistenHostOutput = null
  let unlistenHostExit = null
  let unsubscribeFrame = null
  // Set by onBeforeUnmount. `listen()` is async, so a tab switch (TERMINAL → LOG) that happens
  // before its promise resolves used to leave the listener alive forever with nowhere to store its
  // unlisten handle — and that orphan then wrote every subsequent PTY chunk into a disposed
  // Terminal for the rest of the session. Every async subscription is adopted through
  // `adoptSubscription`, which either stores the handle or immediately undoes it.
  let disposed = false

  /** Takes ownership of a subscription that arrived asynchronously, or cancels it outright if this
   *  composable has already been torn down while the subscribe was in flight. */
  function adoptSubscription(unlisten, assign) {
    if (disposed) {
      unlisten()
      return
    }
    assign(unlisten)
  }
  // ── Sticky modifiers: ONE latch, ONE emitter ───────────────────────────────────────────────────
  //
  // Design + the six defects this shape deletes: docs/plan/done/terminal-input-surface.md §4. The
  // previous code kept `ctrlArmed` and `shiftArmed` as two independent refs consumed in two
  // different places — Ctrl only in `term.onData` here, Shift only in TerminalView's `fireKey` —
  // so neither could see the other. The observable consequences were not cosmetic: an armed Ctrl
  // survived a key-row tap and then turned the NEXT typed letter into a control byte inside a live
  // shell, and Ctrl+arrow could not be produced on a phone at all.
  //
  // The fix is flow shape, not more branches (RULE-design-core Law 8): one latch object, and
  // `emitKey` below as the ONLY function allowed to turn a key press into bytes. Extending to Alt
  // later is a third key in this object plus one term in `emitKey`'s modifier parameter — no new
  // mechanism. Per-tab by construction: one composable instance per mounted TerminalView, so
  // switching tabs cannot carry a latch across.
  const pendingModifiers = ref({ ctrl: false, shift: false })

  /** Tap to arm, tap again to disarm — a real toggle, which is what the key row's buttons promise.
   *  No timeout and no auto-expiry: the latch lives until `emitKey` CONSUMES it or the user taps it
   *  off. `name` is 'ctrl' | 'shift'. */
  function toggleModifier(name) {
    pendingModifiers.value = { ...pendingModifiers.value, [name]: !pendingModifiers.value[name] }
  }

  // Is there a live shell behind this terminal? TRI-STATE: `'unknown'` | `true` | `false`.
  //
  // WHY 'unknown' EXISTS, AND WHY IT IS THE INITIAL VALUE: `false` is a CLAIM — the UI paints the
  // tab red on it, and a keystroke on a `false` terminal triggers a respawn instead of being typed.
  // The old boolean had no way to say "I have not heard yet": it started at `false`, so a terminal
  // asserted "the shell is dead" for the length of one round-trip on every mount. With one terminal
  // that was one flicker at startup; with a tab per project, mounted lazily, it would be a red chip
  // every time a tab is opened. `false` may now only be set by the host actually SAYING the shell is
  // dead (a liveness-bearing payload, a pty-exit, or a hydrate).
  //
  // A FAILED CALL MUST NEVER PRODUCE `false`. An invoke that throws tells us nothing about the
  // shell — the shell may be perfectly alive and the IPC merely unavailable — so every failure path
  // lands on `'unknown'`, which renders exactly like a normal live terminal and passes keystrokes
  // straight through. Erring toward "not sure" is the safe direction here: the failure mode of a
  // wrong `false` is a red tab that respawns shells nobody asked for.
  const alive = ref('unknown')
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

  /** #7 (docs/plan/done/dev-build-in-app-launch.md) — the single funnel that dispatches a DEV/BUILD
   *  command into a PTY, exactly once. Two triggers feed it and they are handled differently on
   *  purpose (this split is the fix for a real triple-echo bug, kept here so it is not
   *  reintroduced):
   *   - A BRAND-NEW TAB'S FIRST SPAWN is handled DETERMINISTICALLY at the tail of `start()`, once
   *     `hydrateScrollback()` has already taken its snapshot — see `sendPendingCmdIfAny` there.
   *   - A DEAD TAB'S RESPAWN (`openRunCommand`'s dead-tab branch calls `pty_spawn` directly on an
   *     already-mounted tab, never touching `start()`/hydrate again) is handled by the watcher
   *     below, armed only once `readyForPendingCmd` is true.
   *
   *  WHY THE FIRST SPAWN CANNOT ALSO GO THROUGH THIS WATCHER: `start()` calls `ensureSpawned()`
   *  BEFORE `hydrateScrollback()` — it has to, a brand-new tab has no session for
   *  `pty_get_scrollback` to read until it is spawned. `pty.rs`'s `append_scrollback` writes every
   *  read into the ring buffer IMMEDIATELY, decoupled from the coalesced `pty-output` broadcast
   *  (`flush_locked`/`flusher_loop`, delayed by `FLUSH_INTERVAL`/`FLUSH_BYTES`). A watcher that sent
   *  the pending command on the FIRST unknown/false -> true edge fired right after `ensureSpawned`'s
   *  optimistic `alive.value = true` — i.e. BEFORE `hydrateScrollback()` took its snapshot. The
   *  write reached the shell and got echoed into the ring buffer fast enough that the snapshot
   *  already contained it, so `term.reset()` + replay rendered it once — and then the SAME bytes'
   *  still-pending, still-coalesced `pty-output` broadcast landed moments later and got appended by
   *  the live listener a second (in practice, sometimes a third) time. Gating the reactive path
   *  behind `readyForPendingCmd` removes the window entirely: the command is never written until
   *  the snapshot the hydrate takes is already final, so the write's echo can only ever arrive
   *  through the live stream, once. */
  let readyForPendingCmd = false

  /** HOST ONLY, checked here rather than by every caller: `start()` calls this unconditionally on
   *  every mount (host and companion both mount a TerminalView for an activated tab), and only the
   *  host may ever consume+send — a companion doing so would fire `consumeTabPendingCmd`'s intent
   *  stub at the host from a screen whose own mount timing has nothing to do with the host's actual
   *  first-spawn/hydrate race this function exists to avoid. */
  function sendPendingCmdIfAny() {
    if (!isHost) return
    const cmd = consumeTabPendingCmd(tabId)
    if (cmd) sendRaw(`${cmd}\r`)
  }

  watch(alive, (isAlive, wasAlive) => {
    if (isAlive !== true || wasAlive === true || !readyForPendingCmd) return
    sendPendingCmdIfAny()
  })

  function writeChunk(base64, reset) {
    // `term.reset()`, not `term.clear()`: clear() keeps the current line and all SGR/cursor modes,
    // so a scrollback replay or a RESTART would inherit the dead shell's colour state and leave a
    // stray prompt fragment on the first line.
    if (reset) term.reset()
    if (base64) term.write(decodeBase64ToBytes(base64))
  }

  async function ensureSpawned(cwd) {
    try {
      await invoke('pty_spawn', { tabId, cwd: cwd ?? null })
      // Not an optimistic guess: the host emits the authoritative liveness for this same call and
      // it will overwrite this within a frame. Set here only so the local screen does not sit on a
      // stale value for one WS round-trip. Set ONLY on success — the resolved promise IS the host
      // confirming the tab has a shell.
      alive.value = true
    } catch (e) {
      // NOT `false`. The spawn may have failed because the IPC seam is down, the tab cap was hit,
      // or the companion's socket dropped mid-call — none of which is evidence about the shell. See
      // `alive`'s doc comment: a failed call may never paint the tab red.
      alive.value = 'unknown'
      console.error('[usePtyTerminal] pty_spawn failed', e)
    }
  }

  /** RESTART: kill THIS TAB's shell, wipe THIS TAB's shared scrollback, spawn a fresh login shell
   *  on it. The host broadcasts the reset itself (src-tauri/src/pty.rs), so this needs no local
   *  clearing and works identically when a companion triggers it through the invoke seam. Every
   *  other tab is untouched — the blast radius is one tab, and the `tabId` below is what bounds it.
   *
   *  DESTRUCTIVE, AND THEREFORE BOUND TO THE EXPLICIT BUTTON ONLY. It ends whatever is running in
   *  the shell and wipes that tab's shared scrollback, which is not something a stray tap on a phone
   *  in another room may ever cause — see `respawn` for what an ordinary keystroke does instead. */
  async function restart() {
    if (restarting) return
    restarting = true
    try {
      await invoke('pty_restart', { tabId, cwd: bootCwd ?? null })
      alive.value = true
    } catch (e) {
      // 'unknown', never `false` — same reasoning as `ensureSpawned`.
      alive.value = 'unknown'
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

  /** Wipes the HOST's ring buffer FOR THIS TAB, not just this screen — otherwise the output comes
   *  straight back on the next reconnect, which is what makes a purely local clear feel broken.
   *  Other tabs' buffers are untouched. */
  async function clear() {
    try {
      await invoke('pty_clear', { tabId })
    } catch (e) {
      console.error('[usePtyTerminal] pty_clear failed', e)
    }
  }

  /** Ends THIS TAB's shell, leaving the tab itself open showing `[process exited]`. */
  async function kill() {
    try {
      await invoke('pty_kill', { tabId })
    } catch (e) {
      console.error('[usePtyTerminal] pty_kill failed', e)
    }
  }

  /** Closes THIS TAB for good: the host kills its shell and forgets everything keyed under it.
   *  Distinct from `kill()`, which leaves the tab in place — the tab LIST is store state and its
   *  own owner removes the entry (src/store/terminalTabsStore.js); this is only the backend half.
   *
   *  `tabId` is passed explicitly and the host command REQUIRES it (unlike every other pty command,
   *  which defaults to tab 0). That is deliberate on both sides: a destructive operation must not
   *  have a default target, or a dropped argument anywhere along this path would quietly close the
   *  user's first tab. */
  async function close() {
    try {
      await invoke('pty_close_tab', { tabId })
    } catch (e) {
      console.error('[usePtyTerminal] pty_close_tab failed', e)
    }
  }

  /** "Open in Terminal.app" (VS Code's external-terminal button): hands off the shell's CURRENT
   *  directory, so `cd`-ing around in here and then jumping out lands in the right place. Falls
   *  back to the plain home-directory window if the cwd cannot be read. */
  async function openExternal() {
    try {
      const cwd = await invoke('pty_cwd', { tabId })
      // Contract C-1 (docs/plan/done/1.20.1-flow-audit-fixes.md §1.1): `null`, never the literal `'~'`.
      // The host side does no shell expansion, so `cd "~"` looks for a directory actually named
      // `~` and always fails; a null path means "no cd at all" and the shell opens in $HOME by
      // itself, which is the fallback that was intended all along.
      await invoke('open_local_terminal', { localPath: cwd || null })
      // Same poke the OPEN popup's Terminal item sends: the live scan would count this window on
      // its next tick anyway, this only stops the badge lagging ~5s behind the click. Dynamic
      // import because projectStore's poke reaches back into useExternalTerminals — see its own
      // comment on why that direction is lazily resolved.
      const { pokeExternalTermCounts } = await import('../store/projectStore')
      pokeExternalTermCounts()
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
      const { data, cols, rows, alive: isAlive } = await invoke('pty_get_scrollback', { tabId })
      // The component can be unmounted mid-call; the Terminal is disposed by then.
      if (disposed) return
      if (cols && rows) term.resize(cols, rows)
      writeChunk(data, true)
      // The one place a `false` is legitimately derived from a call's RESULT rather than from a
      // pushed liveness statement — the host read its own session map to answer this.
      alive.value = !!isAlive
    } catch (e) {
      // The hydrate failing says nothing about the shell — leave whatever belief we already hold
      // rather than inventing `false`. See `alive`'s doc comment.
      console.error('[usePtyTerminal] pty_get_scrollback failed', e)
    }
  }

  /** Is this event/frame addressed to THE TAB THIS COMPOSABLE DRIVES?
   *
   *  Both channels below are broadcasts: the Tauri `pty-output` event and the WS `pty_output` frame
   *  both reach every mounted TerminalView, not just the one whose tab produced them. With N tabs
   *  open there are N listeners on each channel and N-1 of them must ignore any given message. This
   *  test is the entire mechanism — there is no per-tab subscription to lean on instead.
   *
   *  `?? 0` is the backward-compatibility default, matching the host: a message from an older
   *  build carries no tab id and means tab 0. */
  function isForThisTab(message) {
    return (message.tab_id ?? 0) === tabId
  }

  function wireOutput() {
    if (isHost) {
      // Lowest latency for the host's own screen: the PTY reader thread (src-tauri/src/pty.rs)
      // emits this Tauri event directly — no WS round-trip. services/ptyBridge.js separately
      // relays the same event to companions.
      listen('pty-output', (event) => {
        // An event delivered in the gap between subscribing and unsubscribing would otherwise
        // write into a Terminal the component has already disposed.
        if (disposed) return
        const payload = (event && event.payload) || {}
        // Another tab's bytes — and another tab's liveness. Dropped before EITHER is applied: a
        // sibling tab's `alive: false` would be just as wrong as its bytes.
        if (!isForThisTab(payload)) return
        if (payload.data || payload.reset) writeChunk(payload.data, !!payload.reset)
        // A liveness-only payload carries neither bytes nor `reset` (that is how the host says
        // "the shell came back" without touching the screen), so this must sit OUTSIDE the write
        // condition above.
        applyAlive(payload.alive)
      }).then((un) => adoptSubscription(un, (h) => { unlistenHostOutput = h }))
      listen('pty-exit', (event) => {
        if (disposed) return
        // `pty-exit` now carries `{ tab_id }`. Without this filter, ONE tab's shell exiting would
        // mark every open tab dead — and typing into any of them would then respawn a shell over a
        // session that was never dead in the first place.
        if (!isForThisTab((event && event.payload) || {})) return
        alive.value = false
      }).then((un) => adoptSubscription(un, (h) => { unlistenHostExit = h }))
    } else {
      unsubscribeFrame = onFrame((frame) => {
        if (disposed || !frame) return
        if (frame.t === FRAME_PTY_OUTPUT) {
          if (!isForThisTab(frame)) return
          if (frame.data || frame.reset) writeChunk(frame.data, !!frame.reset)
          if (frame.reset && frame.cols && frame.rows) term.resize(frame.cols, frame.rows)
          // Applied for EVERY pty_output frame that states it, not only for `reset` frames. That
          // narrower test was the desync: the host's "shell restarted" news reaches a companion on
          // a plain frame, and a companion that ignored it went on believing a live shell was dead.
          applyAlive(frame.alive)
        } else if (frame.t === FRAME_PTY_EXIT) {
          if (!isForThisTab(frame)) return
          alive.value = false
        } else if (frame.t === FRAME_PTY_RESIZE) {
          if (!isForThisTab(frame)) return
          // T-4: the ONLY path a companion's xterm is ever resized through — it never calls
          // pty_resize itself, and never derives a size from its own container.
          term.resize(frame.cols, frame.rows)
        }
      })
    }
  }

  /** Space-lookalikes that arrive from a source outside this app's control and must not reach the
   *  shell as-is: macOS's `⌥+Space` types U+00A0 directly, Gboard's suggestion-chip insertion and
   *  text pasted from a formatted web page/doc often carry U+00A0 or U+202F instead of an ASCII
   *  space. zsh (and most shells) treat those as ordinary word characters, not IFS, so
   *  `echo "1 2 3"` parses as ONE token and fails with `command not found`. Unlike
   *  the drain's `SENTINELS` (useTerminalTextDrain.js), which DELETES OpenKey's own invisible edit
   *  markers, these are deliberately REPLACED with a real space — the user meant a word boundary,
   *  they just typed/pasted the typographic variant of one. */
  const NBSP_LIKE = /[\u00a0\u202f]/g

  /** Shared funnel for every keystroke source: the real (soft) keyboard via xterm's onData, AND
   *  the mobile key row's synthetic sequences (Esc/Tab/arrows/Enter) — both end up here so there
   *  is exactly one place that decides host-direct-invoke vs. raw-bridge-frame. Also the one place
   *  NBSP-family normalization needs to live, since every source (typed text, IME chunks, pasted
   *  text via xterm, the compose row) reaches the PTY through this function and nothing else does. */
  function sendRaw(str) {
    if (!str) return
    const data = encodeBytesToBase64(new TextEncoder().encode(str.replace(NBSP_LIKE, ' ')))
    if (isHost) {
      invoke('pty_write', { tabId, data }).catch((e) => console.error('[usePtyTerminal] pty_write failed', e))
    } else {
      // Raw frame, not the generic invoke/invoke_result seam — an ack round-trip per keystroke
      // would double traffic and add latency for no benefit (see protocol.js's FRAME_PTY_INPUT
      // doc comment). services/ptyBridge.js applies it host-side, to the tab named here.
      send({ t: FRAME_PTY_INPUT, tab_id: tabId, data })
    }
  }

  /** The Ctrl byte for one character, or `null` if Ctrl means nothing for it. Published (as
   *  `ctrlByteFor`) so a caller that must decide whether to `preventDefault()` BEFORE emitting —
   *  the compose input's latch handler — can ask this one domain rather than restate it. */
  function toCtrlByte(ch) {
    const code = ch.toUpperCase().charCodeAt(0)
    // '@'..'_' (64-95) map to control codes 0-31 — the standard terminal Ctrl+key convention
    // (e.g. 'C' -> 0x03, ETX / SIGINT).
    return code >= 64 && code <= 95 ? String.fromCharCode(code - 64) : null
  }

  /**
   * THE ONLY PLACE A KEY PRESS BECOMES BYTES. Every source funnels through here — the key row's
   * buttons, xterm's `onData` (the real/soft keyboard), and the compose input's latch handler — so
   * there is exactly one site that reads the latch and exactly one that clears it.
   *
   * A key is described by ONE of three shapes:
   *   - `csi`   — a CSI final byte ('A'..'D' for the arrows). Both modifiers are encoded into the
   *               standard parameter (`\x1b[1;<1+shift+4*ctrl><final>`), which is what makes
   *               Ctrl+arrow word-jump reachable from a phone at all.
   *   - `seq` / `shiftSeq` — a literal byte string, with its own shifted form where one exists
   *               (Tab -> backtab `\x1b[Z`). Shift swaps; Ctrl has no meaning for these.
   *   - `char`  — a typed character or an IME chunk. Ctrl maps a single character to its control
   *               byte; Shift is deliberately NOT applied (a soft keyboard already owns
   *               capitalisation, and upper-casing an IME chunk would corrupt it).
   *
   * CLEARING RULE — clear ONLY what was actually consumed. This is one rule, not a branch per
   * symptom, and it is what makes the plan's explicit ruling on a multi-character chunk fall out
   * for free: `toCtrlByte` does not apply to a chunk, so the Ctrl is PRESERVED for the next single
   * character instead of being silently eaten. The same rule keeps a latch alive across a key that
   * has no shifted form, rather than dropping it without feedback.
   */
  function emitKey({ seq, shiftSeq, csi, char } = {}) {
    const { ctrl, shift } = pendingModifiers.value
    let out = null
    let usedCtrl = false
    let usedShift = false

    if (csi) {
      // 1 = no modifier, +1 Shift, +4 Ctrl — the standard xterm modifier parameter.
      const mod = 1 + (shift ? 1 : 0) + (ctrl ? 4 : 0)
      out = mod === 1 ? `\x1b[${csi}` : `\x1b[1;${mod}${csi}`
      usedShift = shift
      usedCtrl = ctrl
    } else if (char != null && char !== '') {
      out = char
      if (ctrl && char.length === 1) {
        const ctrlByte = toCtrlByte(char)
        if (ctrlByte !== null) {
          out = ctrlByte
          usedCtrl = true
        }
      }
    } else {
      out = shift && shiftSeq ? shiftSeq : seq
      usedShift = !!(shift && shiftSeq)
    }

    if (usedCtrl || usedShift) {
      pendingModifiers.value = {
        ctrl: ctrl && !usedCtrl,
        shift: shift && !usedShift,
      }
    }
    if (out) sendRaw(out)
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
      //
      // `=== false`, NOT `!alive.value`. The required behaviour is that only a STATED death
      // respawns: `'unknown'` means "we have not heard from the host yet", and a keystroke typed
      // then must go through to the shell. `!alive.value` happens to do that too — but only by the
      // accident that `'unknown'` is a truthy string, so it would start swallowing keystrokes the
      // moment the sentinel was ever spelled `''`, `0` or `null`. The explicit test says what the
      // rule actually is instead of depending on the sentinel's truthiness.
      if (alive.value === false) {
        respawn()
        return
      }
      // Through the same funnel as every other source: the latch is applied and cleared in exactly
      // one place (see emitKey). An IME chunk (length > 1) leaves an armed Ctrl untouched, so the
      // next single character still receives it instead of the Ctrl vanishing unnoticed.
      emitKey({ char: chunk })
    })
  }

  /** T-4 as a CAPABILITY rather than a role: "does this screen decide the shared PTY's cols/rows?"
   *  Only the host does. TerminalView.vue reads this to choose between fitting the grid and scaling
   *  the font — it must never import `isHost` itself (ENV-1), so every role fact the component
   *  needs is published here in terms of what the component actually decides with it. */
  const ownsPtySize = isHost

  /** The SECOND published capability, in the same voice as `ownsPtySize`: "does this surface need
   *  the synthetic key row (Esc/Tab/arrows/Ctrl)?" — not "is this a phone".
   *
   *  Only a screen without a physical keyboard does, which today means every screen that is not the
   *  host. Published as a capability rather than the role it happens to be derived from so
   *  TerminalView.vue keeps asking what it needs to DECIDE rather than who it is (ENV-1: this file
   *  and services/ptyBridge.js remain the only two places that read `isHost`). If a future host
   *  surface ever wants the key row, the change lands on this line and nowhere else. */
  const showKeyRow = !isHost

  /** T-4: call ONLY from the host's own fit-on-resize handler, i.e. behind `ownsPtySize`. Resizes
   *  the real PTY, then echoes the authoritative size to every companion so their xterm matches
   *  without ever asking for it. (The `isHost` guard below stays as the backstop that makes a
   *  mistaken call harmless rather than a shared-shell re-wrap.) */
  async function hostResize(cols, rows) {
    if (!isHost || !cols || !rows) return
    // Refuse absurd sizes. FitAddon measures a container that is momentarily 0px — the panel is
    // collapsed, the tab is mid-transition, the window is being dragged — and returns a 1- or
    // 2-row fit; pushing that to the real PTY re-wraps the running shell's output permanently and
    // is unrecoverable without a restart. Below this floor the local render is simply left as-is.
    if (cols < 8 || rows < 3) return
    try {
      await invoke('pty_resize', { tabId, cols, rows })
      send({ t: FRAME_PTY_RESIZE, tab_id: tabId, cols, rows })
    } catch (e) {
      console.error('[usePtyTerminal] pty_resize failed', e)
    }
  }

  /** Boot: wire I/O, ensure the shared PTY exists (idempotent — T-3), hydrate scrollback + size,
   *  THEN — only now — send a pending DEV/BUILD command if this tab was created with one. See
   *  `sendPendingCmdIfAny`'s doc comment above for why this must run after the snapshot rather than
   *  reactively off the spawn's own `alive` transition. */
  async function start(cwd) {
    bootCwd = cwd ?? null
    wireOutput()
    wireInput()
    await ensureSpawned(cwd)
    await hydrateScrollback()
    readyForPendingCmd = true
    sendPendingCmdIfAny()
  }

  onBeforeUnmount(() => {
    // Set FIRST: a `listen()` still in flight resolves after this and unlistens itself via
    // `adoptSubscription`, which is the whole point — the handle no longer has to exist yet for the
    // teardown to be complete.
    disposed = true
    if (unlistenHostOutput) unlistenHostOutput()
    if (unlistenHostExit) unlistenHostExit()
    if (unsubscribeFrame) unsubscribeFrame()
  })

  return {
    start,
    ownsPtySize,
    showKeyRow,
    hostResize,
    sendRaw,
    emitKey,
    pendingModifiers,
    toggleModifier,
    ctrlByteFor: toCtrlByte,
    alive,
    restart,
    clear,
    kill,
    close,
    openExternal,
    cd,
  }
}
