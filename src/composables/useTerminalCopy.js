// Terminal ⌘C copy path: xterm's own copy route (a native `copy` DOM event) never fires in this
// WKWebView (F1), and a mouse-mode TUI's redundant protocol re-arm clears the selection (F2).
// Also drives select-to-copy without holding Option, and a native OSC 52 handler so remote tools
// can write the local clipboard directly.
// Reference: docs/plan/terminal-copy-selection.md, docs/research/terminal-copy-selection-root-cause.md,
// docs/plan/terminal-select-to-copy-final.md, docs/research/terminal-select-to-copy-deep-dive.md.

import { copyText } from '../utils/clipboard.js'

let instances = 0
let available = false
let protocolChanges = 0
let rearmSuppressed = 0
let stashLength = 0
let copiesOnSelect = 0
let osc52Copies = 0

if (typeof window !== 'undefined' && !window.__akiTermCopy) {
  window.__akiTermCopy = {
    status() {
      return { instances, available, protocolChanges, rearmSuppressed, stashLength, copiesOnSelect, osc52Copies }
    },
    help() {
      return [
        '__akiTermCopy.status() — instance count, protocol-suppress availability, protocol/rearm counters, last stash length, copy-on-select and OSC 52 counters',
      ]
    },
  }
}

// Swallowing the redundant assignment is the whole point: xterm answers EVERY activeProtocol write with disable() -> clearSelection().
function suppressRedundantRearm(term) {
  const svc = term._core?.coreMouseService
  if (!svc) return null
  const proto = Object.getPrototypeOf(svc)
  const descriptor = proto && Object.getOwnPropertyDescriptor(proto, 'activeProtocol')
  if (!descriptor?.get || !descriptor?.set) return null

  Object.defineProperty(svc, 'activeProtocol', {
    configurable: true,
    get() {
      return descriptor.get.call(svc)
    },
    set(value) {
      protocolChanges++
      if (value === descriptor.get.call(svc)) {
        rearmSuppressed++
        return
      }
      descriptor.set.call(svc, value)
    },
  })

  return () => {
    delete svc.activeProtocol
  }
}

const DRAG_THRESHOLD_PX = 4

// xterm only ever starts a DOM selection on mousedown when `_selectionService.shouldForceSelection`
// is true (natively: Option held on macOS). This composable never lets a real mousedown reach that
// check at all (see attachDragSelection) — it only replays a *synthetic* mousedown once a drag is
// confirmed — so forcing this permanently true has no effect on a plain, un-intercepted click.
function forceSelectionAlways(term) {
  const svc = term._core?._selectionService
  if (!svc || typeof svc.shouldForceSelection !== 'function') return null
  const original = svc.shouldForceSelection
  svc.shouldForceSelection = () => true
  return () => {
    svc.shouldForceSelection = original
  }
}

// Replays a swallowed click as the mouse-report escape code the TUI would have received natively,
// so clicking inside a mouse-aware app (menus, htop, Claude Code) keeps working. A no-op when no
// mouse-tracking protocol is active — `coreMouseService.triggerMouseEvent` restricts internally.
function replayClick(term, downEvent, upEvent) {
  const core = term._core
  const mouseService = core?._mouseService
  const coreMouse = core?.coreMouseService
  if (!mouseService || !coreMouse || !core.screenElement) return
  const downCoords = mouseService.getMouseReportCoords(downEvent, core.screenElement)
  const upCoords = mouseService.getMouseReportCoords(upEvent, core.screenElement)
  if (!downCoords || !upCoords) return
  const base = { button: 0, ctrl: upEvent.ctrlKey, alt: upEvent.altKey, shift: upEvent.shiftKey }
  coreMouse.triggerMouseEvent({ ...downCoords, ...base, action: 1 })
  coreMouse.triggerMouseEvent({ ...upCoords, ...base, action: 0 })
}

// Owns the mousedown -> mouseup gesture from document capture, which always runs before xterm's own
// listeners on term.element (capture on an ancestor precedes the target phase; capture on the same
// node registered later, which is what attaching after term.open() would be, cannot preempt a
// listener xterm already added). A real mousedown is always swallowed and undecided at first. If the
// pointer moves past the threshold before mouseup, it is classified as a drag: a synthetic mousedown
// (at the true start point) plus the current mousemove are replayed so xterm's own SelectionService
// — now always "force selected" — takes over the rest of the drag itself, rather than this composable
// re-deriving row/column math by hand. Below the threshold it is a click, replayed to the app instead.
function attachDragSelection(term, onCopy) {
  const root = term.element
  if (!root) return () => {}

  let down = null
  let dragging = false

  const isSynthetic = (ev) => ev.__akiSynthetic === true

  function relay(type, sourceEvent, x, y) {
    const synthetic = new MouseEvent(type, {
      bubbles: true,
      cancelable: true,
      view: window,
      clientX: x,
      clientY: y,
      button: 0,
      buttons: 1,
      ctrlKey: sourceEvent.ctrlKey,
      altKey: sourceEvent.altKey,
      shiftKey: sourceEvent.shiftKey,
    })
    synthetic.__akiSynthetic = true
    root.dispatchEvent(synthetic)
  }

  function onMouseDown(ev) {
    if (isSynthetic(ev) || ev.button !== 0 || !root.contains(ev.target)) return
    down = { x: ev.clientX, y: ev.clientY, event: ev }
    dragging = false
    // Replicates xterm's own first statement in its mousedown handler, which this swallow skips.
    term.focus()
    ev.preventDefault()
    ev.stopImmediatePropagation()
  }

  function onMouseMove(ev) {
    if (isSynthetic(ev) || !down || dragging) return
    const dx = ev.clientX - down.x
    const dy = ev.clientY - down.y
    if (Math.hypot(dx, dy) <= DRAG_THRESHOLD_PX) {
      ev.stopImmediatePropagation()
      return
    }
    dragging = true
    relay('mousedown', down.event, down.x, down.y)
    relay('mousemove', ev, ev.clientX, ev.clientY)
    // From here on real events are left alone: xterm's own SelectionService (now tracking, from the
    // synthetic mousedown above) extends and ends the selection itself, the same path an Option-drag
    // already exercises today.
  }

  function onMouseUp(ev) {
    if (isSynthetic(ev) || !down) return
    if (!dragging) {
      ev.preventDefault()
      ev.stopImmediatePropagation()
      replayClick(term, down.event, ev)
    } else {
      const text = term.getSelection()
      if (text && text.trim().length > 0) {
        copiesOnSelect++
        onCopy(text)
      }
    }
    down = null
    dragging = false
  }

  document.addEventListener('mousedown', onMouseDown, true)
  document.addEventListener('mousemove', onMouseMove, true)
  document.addEventListener('mouseup', onMouseUp, true)

  return () => {
    document.removeEventListener('mousedown', onMouseDown, true)
    document.removeEventListener('mousemove', onMouseMove, true)
    document.removeEventListener('mouseup', onMouseUp, true)
  }
}

// Decodes an OSC 52 clipboard-set payload (`ESC ] 52 ; c ; <base64> BEL`) and writes it locally, so
// remote tools (tmux, neovim, custom scripts) can copy to the Mac clipboard over SSH the same way
// they would to a local terminal's clipboard. atob() is Latin1-only (RULE-coding.md C5), so the
// decoded bytes are re-read as UTF-8 rather than used as a string directly.
function registerOsc52(term) {
  return term.parser.registerOscHandler(52, (data) => {
    const idx = data.indexOf(';')
    const payload = idx >= 0 ? data.slice(idx + 1) : data
    if (!payload || payload === '?') return false
    try {
      const bytes = Uint8Array.from(atob(payload), (c) => c.charCodeAt(0))
      const decoded = new TextDecoder().decode(bytes)
      osc52Copies++
      copyText(decoded)
      return true
    } catch (err) {
      console.error('[useTerminalCopy] OSC 52 base64 decode failed', err)
      return false
    }
  })
}

// Attaches ⌘C copy claim, select-to-copy without Option, and OSC 52 to an already-open()ed xterm
// Terminal instance.
export function useTerminalCopy(term) {
  const root = term.element
  if (!root) return { dispose: () => {} }
  instances++

  // Survives the selection being wiped moments after it was made; never overwritten with empty.
  let stashed = ''
  const selectionSub = term.onSelectionChange(() => {
    const s = term.getSelection()
    if (s) {
      stashed = s
      stashLength = s.length
    }
  })

  const restoreProtocol = suppressRedundantRearm(term)
  available = !!restoreProtocol

  const restoreForceSelection = forceSelectionAlways(term)
  const detachDrag = attachDragSelection(term, (text) => copyText(text))
  const oscDisposable = registerOsc52(term)

  function onKeydownCapture(ev) {
    if (!(ev.metaKey && !ev.ctrlKey && !ev.altKey && ev.key === 'c')) return
    const text = term.getSelection() || stashed
    if (!text) return
    ev.preventDefault()
    ev.stopPropagation()
    copyText(text)
  }
  root.addEventListener('keydown', onKeydownCapture, true)

  return {
    dispose() {
      root.removeEventListener('keydown', onKeydownCapture, true)
      selectionSub.dispose()
      if (restoreProtocol) restoreProtocol()
      if (restoreForceSelection) restoreForceSelection()
      detachDrag()
      oscDisposable?.dispose()
      instances--
    },
  }
}
