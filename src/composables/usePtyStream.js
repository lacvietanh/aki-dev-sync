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
  // Stateful UTF-8 decoder with stream:true to reassemble multi-byte sequences split across PTY read boundaries.
  const textDecoder = new TextDecoder('utf-8', { fatal: false })

  let unsubscribeFrame = null
  let disposed = false

  // Holds frames arriving during scrollback hydration to drain in order without breaking parser statefulness.
  let liveQueue = []
  let feedingLive = false

  // ── Token application ───────────────────────────────────────────────────────
  // Token mutation handlers. buffer = uncommitted line; lines = committed lines capped at MAX_LINES.

  // In-place push + splice avoids O(n) array copy while preserving Vue 3 reactivity.
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
        // Force-commit pathological single lines to prevent unbounded buffer growth.
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

  // Frame filter: onFrame is a broadcast channel — every listener sees every tab's frames.

  function isForThisTab(frame) {
    return (frame.tab_id ?? 0) === tabId
  }

  // ── Handle a single live frame ──────────────────────────────────────────────

  function handleLiveFrame(frame) {
    if (frame.t === FRAME_PTY_OUTPUT) {
      if (!isForThisTab(frame)) return
      if (typeof frame.alive === 'boolean') alive.value = frame.alive
      if (frame.reset) {
        // reset:true wipes screen + scrollback (matches xterm term.reset()) so replay payloads do not append below stale lines.
        lines.value = []
        resetDisplay()
      }
      if (frame.data) feedBase64Chunk(frame.data)
    } else if (frame.t === FRAME_PTY_EXIT) {
      if (!isForThisTab(frame)) return
      alive.value = false
      // Bare notice without data/reset; clear tail + buffer so stale fragments do not corrupt respawn lines.
      resetDisplay()
    }
  }

  // ── sendRaw — keystroke funnel ──────────────────────────────────────────────
  // Encodes UTF-8 bytes to base64 for FRAME_PTY_INPUT; companion-only surface so no isHost branch.

  function sendRaw(str) {
    if (!str) return
    const data = encodeBytesToBase64(new TextEncoder().encode(str))
    send({ t: FRAME_PTY_INPUT, tab_id: tabId, data })
  }

  // ── start() — four-step ordering ────────────────────────────────────────────
  // Four-step start ordering: subscribe -> queue live frames -> hydrate scrollback -> drain queue in order.

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
