// Terminal ⌘C copy path: xterm's own copy route (a native `copy` DOM event) never fires in this
// WKWebView (F1), and a mouse-mode TUI's redundant protocol re-arm clears the selection (F2).
// Reference: docs/plan/terminal-copy-selection.md, docs/research/terminal-copy-selection-root-cause.md.

import { copyText } from '../utils/clipboard.js'

let instances = 0
let available = false
let protocolChanges = 0
let rearmSuppressed = 0
let stashLength = 0

if (typeof window !== 'undefined' && !window.__akiTermCopy) {
  window.__akiTermCopy = {
    status() {
      return { instances, available, protocolChanges, rearmSuppressed, stashLength }
    },
    help() {
      return [
        '__akiTermCopy.status() — instance count, protocol-suppress availability, protocol/rearm counters, last stash length',
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

// Attaches ⌘C copy claim and selection stash to an already-open()ed xterm Terminal instance.
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
      instances--
    },
  }
}
