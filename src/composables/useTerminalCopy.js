// Terminal ⌘C copy path: xterm's own copy route (a native `copy` DOM event) never fires in this
// WKWebView (F1), and a mouse-mode TUI's redundant protocol re-arm clears the selection (F2).
// OSC 52 lets remote tools write the local clipboard over SSH; the mouse stays with xterm.
// Reference: docs/plan/done/terminal-copy-selection.md, docs/research/terminal-copy-selection-root-cause.md.

import { copyText } from '../utils/clipboard.js'

let instances = 0
let available = false
let protocolChanges = 0
let rearmSuppressed = 0
let stashLength = 0
let osc52Copies = 0

if (typeof window !== 'undefined' && !window.__akiTermCopy) {
  window.__akiTermCopy = {
    status() {
      return { instances, available, protocolChanges, rearmSuppressed, stashLength, osc52Copies }
    },
    help() {
      return [
        '__akiTermCopy.status() — instance count, protocol-suppress availability, protocol/rearm counters, last stash length, OSC 52 counter',
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

// OSC 52 (`ESC ] 52 ; c ; <base64> BEL`) → local clipboard. atob() is Latin1 (coding.C5); re-decode as UTF-8.
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

// Attaches ⌘C copy claim, selection stash, and OSC 52 to an already-open()ed xterm Terminal instance.
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
      oscDisposable?.dispose()
      instances--
    },
  }
}
