// In-app terminal — role wiring (docs/plan/done/1.20.0-terminal-and-remote-sync.md §4).
// TerminalView.vue owns xterm.js instance/DOM/key row; this composable owns role wiring, I/O routing, and resize authority (T-4).
// BINARY-SAFE TRANSPORT: PTY bytes ride base64 wire as raw byte codec (src-tauri/src/pty.rs, RULE-coding C5); stateful UTF-8 decoding handled by xterm.js Terminal.write(Uint8Array).
// ENV-1 (docs/plan/done/remote-control.md §9): isHost read directly only here and services/ptyBridge.js; TerminalView.vue template stays neutral.
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { isHost, onFrame, send } from '../services/bridge'
import { invoke } from '../utils/tauri'
import { decodeBase64ToBytes, encodeBytesToBase64 } from '../utils/ptyCodec'
import {
  FRAME_PTY_INPUT,
  FRAME_PTY_OUTPUT,
  FRAME_PTY_RESIZE,
  FRAME_PTY_RESIZE_REQUEST,
  FRAME_PTY_EXIT,
} from '../constants/protocol'
import { consumeTabPendingCmd, terminalTabs, setResizeOwner, GLOBAL_SCOPE } from '../store/terminalTabsStore'

// Module-level tab liveness tracker: persists across TerminalView mount/unmount and confines isHost branching here (ENV-1).
/** `{ [tabId]: 'unknown' | true | false }` — tri-state liveness, re-exported by useTerminalTabs.js as tabAlive. */
export const tabLiveness = ref({})

function setTabLiveness(tabId, value) {
  const id = typeof tabId === 'number' ? tabId : 0
  if (tabLiveness.value[id] === value) return
  tabLiveness.value = { ...tabLiveness.value, [id]: value }
}

let livenessTrackingStarted = false

/** Idempotent — safe to call from every screen; registers module-lifetime listeners on first call. */
export function startTabLivenessTracking() {
  if (livenessTrackingStarted) return
  livenessTrackingStarted = true
  if (isHost) {
    // Module-lifetime global listeners: track liveness for all tabs without per-tab filtering.
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

/** HOST BOOT ONLY — seeds tracker from pty_list_tabs() so re-adopted tabs show correct initial state. */
export function seedTabLiveness(list) {
  if (!Array.isArray(list) || list.length === 0) return
  const next = { ...tabLiveness.value }
  for (const t of list) {
    if (t && typeof t.id === 'number' && typeof t.alive === 'boolean') next[t.id] = t.alive
  }
  tabLiveness.value = next
}

/**
 * Wires an already-created xterm.js Terminal instance to one tab's PTY.
 * @param {import('@xterm/xterm').Terminal} term
 * @param {number} [tabId=0] which terminal tab this surface drives.
 */
export function usePtyTerminal(term, tabId = 0) {
  let unlistenHostOutput = null
  let unlistenHostExit = null
  let unsubscribeFrame = null
  // Flagged on unmount so in-flight async listen() promises cancel immediately via adoptSubscription instead of leaking.
  let disposed = false

  /** Takes ownership of async subscription or cancels immediately if already unmounted. */
  function adoptSubscription(unlisten, assign) {
    if (disposed) {
      unlisten()
      return
    }
    assign(unlisten)
  }

  // Unified modifier latch consumed exclusively by emitKey (docs/plan/done/terminal-input-surface.md §4).
  const pendingModifiers = ref({ ctrl: false, shift: false })

  /** Toggle latch on/off without timeout; consumed or cleared by emitKey. */
  function toggleModifier(name) {
    pendingModifiers.value = { ...pendingModifiers.value, [name]: !pendingModifiers.value[name] }
  }

  // Tri-state: 'unknown' | true | false. Starts at 'unknown'; only explicit host signals set false to prevent premature respawn/red tabs.
  const alive = ref('unknown')
  // Remembered from `start()` so RESTART reopens in the same directory the tab was opened for.
  let bootCwd = null
  // Guards against two respawns racing (e.g. the user mashes keys into a dead terminal).
  let restarting = false

  /** Adopts boolean liveness from host; ignores non-boolean payloads to avoid false death reports. */
  function applyAlive(value) {
    if (typeof value === 'boolean') alive.value = value
  }

  /** Gated by readyForPendingCmd to prevent double-execution race between spawn and scrollback hydrate (docs/plan/done/dev-build-in-app-launch.md #7). */
  let readyForPendingCmd = false

  /** Host only: consumes and sends pending launch command for this tab if present. */
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
    // term.reset() clears SGR modes and cursor state so replay/restart does not inherit stale prompt styling.
    if (reset) term.reset()
    if (base64) term.write(decodeBase64ToBytes(base64))
  }

  async function ensureSpawned(cwd) {
    try {
      await invoke('pty_spawn', { tabId, cwd: cwd ?? null })
      // Optimistically marks alive on resolved spawn to avoid a 1-frame WS latency gap.
      alive.value = true
    } catch (e) {
      // Keep 'unknown' on failure: an IPC error is not proof that the shell exited.
      alive.value = 'unknown'
      console.error('[usePtyTerminal] pty_spawn failed', e)
    }
  }

  /** Destructive restart: kills shell, wipes shared scrollback, and respawns login shell. */
  async function restart() {
    if (restarting) return
    restarting = true
    try {
      await invoke('pty_restart', { tabId, cwd: bootCwd ?? null })
      alive.value = true
      // Fresh shell resets to default authority, not a stale companion claim (host-only mirrored state). docs/plan/done/wish-terminal-manual-resize-authority.md lifecycle item 6.
      if (isHost) setResizeOwner(tabId, 'host')
    } catch (e) {
      // 'unknown', never `false` — same reasoning as ensureSpawned.
      alive.value = 'unknown'
      console.error('[usePtyTerminal] pty_restart failed', e)
    } finally {
      restarting = false
    }
  }

  /** Safe respawn via idempotent pty_spawn; preserves scrollback context and avoids accidental kill. */
  async function respawn() {
    if (restarting) return
    restarting = true
    try {
      await ensureSpawned(bootCwd)
    } finally {
      restarting = false
    }
  }

  /** Wipes host ring buffer for this tab; other tabs' scrollbacks remain untouched. */
  async function clear() {
    try {
      await invoke('pty_clear', { tabId })
    } catch (e) {
      console.error('[usePtyTerminal] pty_clear failed', e)
    }
  }

  /** Terminates shell for this tab, leaving tab open in exited state. */
  async function kill() {
    try {
      await invoke('pty_kill', { tabId })
    } catch (e) {
      console.error('[usePtyTerminal] pty_kill failed', e)
    }
  }

  /** Closes tab permanently and frees host session resources; requires explicit tabId without default fallback. */
  async function close() {
    try {
      await invoke('pty_close_tab', { tabId })
    } catch (e) {
      console.error('[usePtyTerminal] pty_close_tab failed', e)
    }
  }

  /** Opens current shell directory in external terminal; falls back to $HOME if cwd is unavailable. */
  async function openExternal() {
    try {
      const cwd = await invoke('pty_cwd', { tabId })
      // Scope owner for this tab (projectId or GLOBAL_SCOPE; docs/plan/done/terminal-ownership-model.md).
      const owner = terminalTabs.value.find((t) => t.id === tabId)?.projectId ?? GLOBAL_SCOPE
      // Contract C-1: null path defaults to $HOME; literal '~' is unexpanded by host (docs/plan/done/1.20.1-flow-audit-fixes.md §1.1).
      const { registerExternalTerminalLaunch } = await import('../store/projectStore')
      await registerExternalTerminalLaunch({ owner, path: cwd || null })
    } catch (e) {
      console.error('[usePtyTerminal] openExternal failed', e)
    }
  }

  /** Jumps shell into directory using single-quoted POSIX-safe input string. */
  function cd(path) {
    if (!path) return
    sendRaw(`cd '${String(path).replace(/'/g, "'\\''")}'\r`)
  }

  /** Hydrates scrollback and initial geometry on mount. */
  async function hydrateScrollback() {
    try {
      const { data, cols, rows, alive: isAlive } = await invoke('pty_get_scrollback', { tabId })
      if (disposed) return
      if (cols && rows) term.resize(cols, rows)
      writeChunk(data, true)
      // Authoritative liveness from host session map during hydration.
      alive.value = !!isAlive
    } catch (e) {
      // IPC failure leaves previous belief unchanged instead of inventing false.
      console.error('[usePtyTerminal] pty_get_scrollback failed', e)
    }
  }

  /** Checks if broadcast message/frame belongs to this tab instance. */
  function isForThisTab(message) {
    return (message.tab_id ?? 0) === tabId
  }

  function wireOutput() {
    if (isHost) {
      // Host PTY reader thread emits pty-output directly for minimum local latency.
      listen('pty-output', (event) => {
        if (disposed) return
        const payload = (event && event.payload) || {}
        if (!isForThisTab(payload)) return
        if (payload.data || payload.reset) writeChunk(payload.data, !!payload.reset)
        // Apply liveness outside write condition: liveness-only payloads carry neither data nor reset.
        applyAlive(payload.alive)
      }).then((un) => adoptSubscription(un, (h) => { unlistenHostOutput = h }))
      listen('pty-exit', (event) => {
        if (disposed) return
        // Filter exit by tab_id so one dying shell does not mark all open tabs dead.
        if (!isForThisTab((event && event.payload) || {})) return
        alive.value = false
      }).then((un) => adoptSubscription(un, (h) => { unlistenHostExit = h }))
      // Sync host xterm to companion resize claim (docs/plan/done/wish-terminal-manual-resize-authority.md).
      unsubscribeFrame = onFrame((frame) => {
        if (disposed || !frame || frame.t !== FRAME_PTY_RESIZE_REQUEST) return
        if (!isForThisTab(frame)) return
        if (!frame.cols || !frame.rows) return
        term.resize(frame.cols, frame.rows)
        setResizeOwner(tabId, frame.from || 'host')
        hostResize(frame.cols, frame.rows)
      })
    } else {
      unsubscribeFrame = onFrame((frame) => {
        if (disposed || !frame) return
        if (frame.t === FRAME_PTY_OUTPUT) {
          if (!isForThisTab(frame)) return
          if (frame.data || frame.reset) writeChunk(frame.data, !!frame.reset)
          if (frame.reset && frame.cols && frame.rows) term.resize(frame.cols, frame.rows)
          // Apply liveness on every frame carrying it, avoiding desync when host restarts shell without reset flag.
          applyAlive(frame.alive)
        } else if (frame.t === FRAME_PTY_EXIT) {
          if (!isForThisTab(frame)) return
          alive.value = false
        } else if (frame.t === FRAME_PTY_RESIZE) {
          if (!isForThisTab(frame)) return
          // T-4: companion updates size exclusively via host broadcast.
          term.resize(frame.cols, frame.rows)
        }
      })
    }
  }

  const NBSP_LIKE = /[\u00a0\u202f]/g

  /** Funnel for all input sources; normalizes typographic space-lookalikes to standard ASCII space. */
  function sendRaw(str) {
    if (!str) return
    const data = encodeBytesToBase64(new TextEncoder().encode(str.replace(NBSP_LIKE, ' ')))
    if (isHost) {
      invoke('pty_write', { tabId, data }).catch((e) => console.error('[usePtyTerminal] pty_write failed', e))
    } else {
      // Raw FRAME_PTY_INPUT avoids per-keystroke ack latency (see protocol.js).
      send({ t: FRAME_PTY_INPUT, tab_id: tabId, data })
    }
  }

  /** Maps character to ASCII control byte (0-31) for Ctrl+key chords, or returns null. */
  function toCtrlByte(ch) {
    const code = ch.toUpperCase().charCodeAt(0)
    return code >= 64 && code <= 95 ? String.fromCharCode(code - 64) : null
  }

  /** Single funnel converting keypresses to bytes and consuming modifier latches (shapes: csi, seq/shiftSeq, char). */
  function emitKey({ seq, shiftSeq, csi, char } = {}) {
    const { ctrl, shift } = pendingModifiers.value
    let out = null
    let usedCtrl = false
    let usedShift = false

    if (csi) {
      // 1 = no modifier, +1 Shift, +4 Ctrl — standard xterm modifier parameter.
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
    // T-5: no local echo — keystrokes go straight to host PTY and render on output echo.
    term.onData((chunk) => {
      // Explicit alive.value === false check respawns dead shell on keypress; 'unknown' passes keys through.
      if (alive.value === false) {
        respawn()
        return
      }
      // Emits through emitKey funnel; multi-char IME chunks (e.g. Gboard text drain) preserve armed Ctrl for next single key.
      emitKey({ char: chunk })
    })
  }

  /** Reads current resize authority holder from store ('host' or companion client id). */
  function resizeOwnerFor() {
    return terminalTabs.value.find((t) => t.id === tabId)?.resizeOwner || 'host'
  }

  /** Reactive indicator for Mac reclaim button when companion holds resize authority. */
  const showReclaimPill = computed(() => isHost && resizeOwnerFor() !== 'host')

  /** Capability flag: true on touch/companion surfaces requiring synthetic key row. */
  const showKeyRow = !isHost

  /** T-4: resizes host PTY and broadcasts new dimensions to all companions. */
  async function hostResize(cols, rows) {
    if (!isHost || !cols || !rows) return
    // Floor guard: ignore momentary 0px/collapsed panel fits to prevent unrecoverable PTY re-wrap.
    if (cols < 8 || rows < 3) return
    try {
      await invoke('pty_resize', { tabId, cols, rows })
      send({ t: FRAME_PTY_RESIZE, tab_id: tabId, cols, rows })
    } catch (e) {
      console.error('[usePtyTerminal] pty_resize failed', e)
    }
  }

  /** Companion request to take resize authority and set dimensions (docs/plan/done/wish-terminal-manual-resize-authority.md). */
  function requestResize(cols, rows) {
    if (isHost || !cols || !rows) return
    send({ t: FRAME_PTY_RESIZE_REQUEST, tab_id: tabId, cols, rows })
  }

  /** Boot: wire I/O, ensure PTY spawned, hydrate scrollback, and flush pending command. */
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
    // Mark disposed first so in-flight async listen() calls unlisten immediately on resolution.
    disposed = true
    if (unlistenHostOutput) unlistenHostOutput()
    if (unlistenHostExit) unlistenHostExit()
    if (unsubscribeFrame) unsubscribeFrame()
  })

  return {
    start,
    /** T-4 capability: true when host currently holds active PTY resize authority. */
    get ownsPtySize() {
      return isHost && resizeOwnerFor() === 'host'
    },
    showReclaimPill,
    showKeyRow,
    hostResize,
    requestResize,
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
