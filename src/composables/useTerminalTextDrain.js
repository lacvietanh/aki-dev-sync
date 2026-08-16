// Terminal input text drain: handles text insertion and IME composition (keys owned by xterm).
// Reference architecture: docs/research/terminal-vietnamese-ime-root-cause-4.md §7 & terminal-gboard-double-insert.md.

const RING_MAX = 400
const ring = []
const counts = {
  keypressVetoed: 0,
  drained: 0,
  drainedChars: 0,
  claimed229: 0,
  compositions: 0,
  enterAfterComposition: 0,
  deleteBackwardClaimed: 0,
}
let instances = 0
let debugOn = false
try {
  debugOn = localStorage.getItem('aki-term-input-debug') === '1'
} catch {
  debugOn = false
}

function record(type, detail) {
  const entry = { t: Math.round(performance.now()), type, ...detail }
  ring.push(entry)
  if (ring.length > RING_MAX) ring.shift()
  if (debugOn) console.log('[term-text-drain]', type, detail)
}

if (typeof window !== 'undefined' && !window.__akiTermInput) {
  window.__akiTermInput = {
    status() {
      let flagDebug = null
      try {
        flagDebug = localStorage.getItem('aki-term-input-debug')
      } catch {
        /* localStorage access fallback */
      }
      return {
        page: document.title,
        version: 3,
        instances,
        debugMirroring: debugOn,
        flags: { 'aki-term-input-debug': flagDebug },
        eventsRecorded: ring.length,
        counts: { ...counts },
      }
    },
    dump() {
      return ring.slice()
    },
    tail(n = 40) {
      return ring.slice(-n)
    },
    debug(on = true) {
      debugOn = !!on
      return debugOn
    },
    clear() {
      ring.length = 0
      for (const k of Object.keys(counts)) counts[k] = 0
      return true
    },
    help() {
      return [
        '__akiTermInput.status()    — drain status, active page, and claimed event counts',
        '__akiTermInput.tail(40)    — last 40 events as data',
        '__akiTermInput.dump()      — full debug ring',
        '__akiTermInput.debug(true) — mirror events to console',
        '__akiTermInput.clear()     — reset ring and counters',
      ]
    },
  }
}

// OpenKey invisible edit sentinels (U+202F narrow no-break space, U+200C zero-width non-joiner).
const SENTINELS = /[\u202F\u200C]/g

// Format hex codepoints of a drained chunk for debug ring inspection.
function codepoints(s) {
  return Array.from(s.slice(0, 16), c => c.codePointAt(0).toString(16).padStart(4, '0')).join(' ')
}

// WebKit compositionend-before-keydown ordering grace window (ms).
export const POST_COMPOSITION_MS = 100

// Attaches text drain listener to an opened xterm Terminal instance.
export function useTerminalTextDrain(term) {
  const root = term.element
  const textarea = term.textarea
  if (!root || !textarea) {
    record('attach-failed', { hasElement: !!root, hasTextarea: !!textarea })
    return { dispose: () => {} }
  }
  instances++

  // True while a native composing IME owns the input; deferred release prevents drain race.
  let composing = false
  let compositionEndedAt = -Infinity

  function onCompositionStart() {
    composing = true
    counts.compositions++
    record('compositionstart', {})
  }

  function onCompositionEnd(ev) {
    compositionEndedAt = performance.now()
    setTimeout(() => {
      composing = false
    }, 0)
    record('compositionend', { data: ev.data })
  }

  // Drain handler: reads inserted text, clears textarea synchronously, and sends to xterm input.
  function onInputCapture(ev) {
    if (ev.target !== textarea) return
    const inputType = ev.inputType || ''
    // Clear residual paste buffer before browser insertion to prevent double-send.
    if (inputType === 'insertFromPaste') {
      textarea.value = ''
      record('input-paste-cleared', { inputType })
      return
    }
    if (composing || ev.isComposing) return
    // Allow native composition and drop input types to pass through unhindered.
    if (
      inputType.startsWith('insertComposition') ||
      inputType === 'insertFromComposition' ||
      inputType === 'insertFromDrop'
    ) {
      record('input-passed', { inputType })
      return
    }

    const raw = textarea.value
    if (!raw) {
      // Gboard plain-delete correction on emptied textarea (docs/research/terminal-gboard-double-insert.md).
      if (inputType === 'deleteContentBackward') {
        ev.stopPropagation()
        counts.deleteBackwardClaimed++
        record('drain-backspace', { inputType })
        term.input('\x7f', true)
      }
      return
    }
    textarea.value = ''

    // Stop propagation so xterm's own unneeded textarea listener does not duplicate input.
    ev.stopPropagation()

    const text = raw.replace(SENTINELS, '').replace(/\r\n|\n|\r/g, '\r')
    if (!text) {
      record('input-sentinels-only', { raw: raw.length })
      return
    }
    counts.drained++
    counts.drainedChars += text.length
    record('drained', { inputType, text, length: text.length, cp: codepoints(text) })
    term.input(text, true)
  }

  // Claim Enter/Backspace for keyCode 229 committed edit keys on an empty textarea.
  function onKeydownCapture(ev) {
    if (ev.target !== textarea) return
    if (ev.keyCode !== 229 || ev.isComposing || composing) return
    if (ev.key === 'Backspace') {
      counts.claimed229++
      record('claim229', { key: ev.key })
      ev.preventDefault()
      term.input('\x7f', true)
      return
    }
    if (ev.key === 'Enter') {
      // Suppress Enter landing within the post-composition grace window.
      if (performance.now() - compositionEndedAt < POST_COMPOSITION_MS) {
        counts.enterAfterComposition++
        record('enter-suppressed-post-composition', {})
        return
      }
      counts.claimed229++
      record('claim229', { key: ev.key })
      ev.preventDefault()
      term.input('\r', true)
    }
  }

  // Veto xterm keypress without preventDefault so browser inserts into textarea for drain.
  function customKeyEventHandler(ev) {
    if (ev.type !== 'keypress') return true
    counts.keypressVetoed++
    record('keypress-vetoed', { key: ev.key, charCode: ev.charCode })
    return false
  }
  term.attachCustomKeyEventHandler(customKeyEventHandler)

  root.addEventListener('compositionstart', onCompositionStart, true)
  root.addEventListener('compositionend', onCompositionEnd, true)
  root.addEventListener('keydown', onKeydownCapture, true)
  root.addEventListener('input', onInputCapture, true)
  record('attached', { instances })

  return {
    dispose() {
      root.removeEventListener('compositionstart', onCompositionStart, true)
      root.removeEventListener('compositionend', onCompositionEnd, true)
      root.removeEventListener('keydown', onKeydownCapture, true)
      root.removeEventListener('input', onInputCapture, true)
      // Release custom key handler slot back on teardown.
      term.attachCustomKeyEventHandler(() => true)
      instances--
      record('detached', { instances })
    },
  }
}
