// SimpleView — plain-text PTY stream renderer without xterm.js.
//
// A companion screen that renders the PTY byte stream as a plain text line stream instead of mounting a full terminal emulator. It never learns cols/rows, never implements grid geometry, and honours only cursor-up, line-erase, and carriage-return — no cursor addressing, scroll regions, or column tracking.
//
// Contract: docs/plan/wish-terminal-split-simpleview.md
import { onBeforeUnmount, ref } from 'vue'
import { onFrame, send } from '../services/bridge'
import { invoke } from '../utils/tauri'
import { FRAME_PTY_INPUT, FRAME_PTY_OUTPUT, FRAME_PTY_EXIT } from '../constants/protocol'
import { createAnsiParser } from '../utils/ansiStrip'
import { decodeBase64ToBytes, encodeBytesToBase64 } from '../utils/ptyCodec'

const MAX_LINES = 2000
// Defensive cap on an in-progress `buffer` with no newline for a long time (e.g. a pathological single-line stream). Forces a wrap so buffer can't grow unbounded.
const MAX_LINE = 65536

export function usePtyStream(tabId = 0) {
  const lines = ref([])        // committed lines, capped at MAX_LINES (drop from front)
  const buffer = ref('')       // in-progress uncommitted line
  const alive = ref('unknown') // 'unknown' | true | false

  const parser = createAnsiParser()
  // Stateful UTF-8 decoder — survives PTY read() boundaries so a multi-byte sequence split across two chunks is correctly reassembled. The `stream: true` flag tells the decoder to buffer incomplete trailing bytes rather than emitting a replacement character.
  const textDecoder = new TextDecoder('utf-8', { fatal: false })

  let unsubscribeFrame = null
  let disposed = false

  // Live-frame queue: frames arriving during scrollback hydration (step 2 of start()) are held here and drained in arrival order after the scrollback is fully ingested, so the parser's statefulness is never fed out-of-order.
  let liveQueue = []
  let feedingLive = false

  // ── Token application ───────────────────────────────────────────────────────
  // The token → ref mutation table pinned in BRIEF.md. `buffer` is the in-progress uncommitted line; `lines` is committed lines capped at MAX_LINES.

  // Push a completed line onto `lines` and re-apply the MAX_LINES cap. Uses .push (in place) instead of spread-rebuild — Vue 3 ref arrays stay reactive under .push, and this avoids an O(n) copy per committed line.
  function commitLine(text) {
    lines.value.push(text)
    if (lines.value.length > MAX_LINES) {
      lines.value.splice(0, lines.value.length - MAX_LINES)
    }
  }

  function applyToken(token) {
    if (!token) return
    switch (token.t) {
      case 'text':
        buffer.value += token.v
        // Defensive cap: no newline for a long time (pathological single line) would otherwise grow `buffer` unbounded. Force-commit it as a line and keep going.
        if (buffer.value.length > MAX_LINE) {
          commitLine(buffer.value)
          buffer.value = ''
        }
        break
      case 'nl': {
        commitLine(buffer.value)
        buffer.value = ''
        break
      }
      case 'cr':
        buffer.value = ''
        break
      case 'up': {
        const n = token.n ?? 1
        if (n >= lines.value.length) {
          lines.value = []
        } else {
          lines.value = lines.value.slice(0, lines.value.length - n)
        }
        break
      }
      case 'eraseLine':
        buffer.value = ''
        break
    }
  }

  function feedTokens(tokens) {
    if (!tokens || !tokens.length) return
    for (const token of tokens) applyToken(token)
  }

  // ── Reset display tail ──────────────────────────────────────────────────────
  // Anything that resets `lines` must ALSO call parser.reset() and clear buffer, or a stale fragment corrupts the first line after the reset.

  function resetDisplay() {
    parser.reset()
    buffer.value = ''
  }

  // ── Base64 chunk → feed through parser ──────────────────────────────────────

  function feedBase64Chunk(base64) {
    if (!base64) return
    const bytes = decodeBase64ToBytes(base64)
    const text = textDecoder.decode(bytes, { stream: true })
    const tokens = parser.feed(text)
    feedTokens(tokens)
  }

  // ── Frame filter ────────────────────────────────────────────────────────────
  // onFrame is a broadcast channel — every listener sees every tab's frames.

  function isForThisTab(frame) {
    return (frame.tab_id ?? 0) === tabId
  }

  // ── Handle a single live frame ──────────────────────────────────────────────

  function handleLiveFrame(frame) {
    if (frame.t === FRAME_PTY_OUTPUT) {
      if (!isForThisTab(frame)) return
      if (typeof frame.alive === 'boolean') alive.value = frame.alive
      if (frame.reset) {
        // reset:true = "replace everything on screen" (src-tauri/src/pty.rs). Unlike EXIT (which keeps scrollback), a reset frame MUST also clear committed `lines`, or CLEAR/RESTART, companion-join replay and congestion resync would append their fresh payload BELOW the stale scrollback instead of replacing it — the xterm peer wipes both screen and scrollback here (usePtyTerminal.js's `term.reset()`), and this must match.
        lines.value = []
        resetDisplay()
      }
      if (frame.data) feedBase64Chunk(frame.data)
    } else if (frame.t === FRAME_PTY_EXIT) {
      if (!isForThisTab(frame)) return
      alive.value = false
      // EXIT is a bare notice with no `data` and no `reset` flag (src/services/ptyBridge.js:178-181). It must clear tail + buffer independently so a stale fragment does not corrupt the first line after the common respawn path (typing to restart, via idempotent pty_spawn, which emits no reset frame at all).
      resetDisplay()
    }
  }

  // ── sendRaw — keystroke funnel ──────────────────────────────────────────────
  // Mirrors usePtyTerminal's companion path: encodes the string as UTF-8 bytes, then base64, then sends as a raw FRAME_PTY_INPUT frame. No isHost branch — SimpleView is a companion surface by design and never reads the role marker.

  function sendRaw(str) {
    if (!str) return
    const data = encodeBytesToBase64(new TextEncoder().encode(str))
    send({ t: FRAME_PTY_INPUT, tab_id: tabId, data })
  }

  // ── start() — four-step ordering ────────────────────────────────────────────
  // 1. Subscribe FRAME_PTY_OUTPUT + FRAME_PTY_EXIT via onFrame() first, so no live frame is lost.
  // 2. Live frames arriving from here on go into a queue, not into the parser — the parser is stateful and order-sensitive.
  // 3. await invoke('pty_get_scrollback', { tabId }), decode, feed through parser.
  // 4. Only then drain the queue through the same parser, in arrival order, and switch to feeding live.

  async function start() {
    // Step 1 — subscribe first
    unsubscribeFrame = onFrame((frame) => {
      if (disposed || !frame) return
      if (frame.t !== FRAME_PTY_OUTPUT && frame.t !== FRAME_PTY_EXIT) return
      if (feedingLive) {
        handleLiveFrame(frame)
      } else {
        // Step 2 — queue while scrollback is being hydrated
        liveQueue.push(frame)
      }
    })

    // Step 3 — fetch + ingest scrollback
    try {
      const result = await invoke('pty_get_scrollback', { tabId })
      if (disposed) return
      if (result && typeof result.alive === 'boolean') alive.value = result.alive
      if (result && result.data) {
        resetDisplay()
        feedBase64Chunk(result.data)
      }
    } catch (e) {
      console.error('[usePtyStream] pty_get_scrollback failed', e)
    }

    // Step 4 — drain queue in arrival order, then switch to live
    if (!disposed) {
      for (const frame of liveQueue) handleLiveFrame(frame)
      liveQueue = []
      feedingLive = true
    }
  }

  // ── Teardown ────────────────────────────────────────────────────────────────

  onBeforeUnmount(() => {
    disposed = true
    if (unsubscribeFrame) unsubscribeFrame()
    liveQueue = []
    feedingLive = false
  })

  return { lines, buffer, alive, sendRaw, start }
}
