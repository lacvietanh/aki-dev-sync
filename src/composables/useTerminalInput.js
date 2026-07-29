// Terminal input layer — app-owned textarea overlay that separates keyboard capture from xterm.js,
// so xterm is a pure renderer. Replaces useWkImeGuard.js.
//
// Architecture: docs/plan/terminal-ime-input-layer-separation.md
//
// The problem that guard solved (WKWebView's keyCode 229 tagging of synthetic keystrokes, which
// xterm 5.5.0's IME fallback paths mishandle — upstream #5887/#5894) is solved here by removing
// xterm from the keyboard path entirely: disableStdin prevents xterm from firing onData from its
// own textarea, and this app-owned textarea captures all input with the browser's own (correct)
// event pipeline. The multi-char carrier event that was the hardest shape to handle ("gì" as one
// keydown with kVK 0) now lands in beforeinput.data or input.data as the full string, with no
// per-shape classification needed.
//
// DIAGNOSTICS: same pull-based pattern as the old guard — ring recorded always, read on demand.
// window.__akiTermInput (replaces window.__akiIme) provides status()/tail()/dump()/debug()/clear()/help().
//
// Escape hatch: localStorage['aki-input-mode'] = 'legacy' → old xterm native + useWkImeGuard flow
// (checked by TerminalView.vue at mount, not by this file — this file is the NEW path only).

const RING_MAX = 400
const ring = []
const counts = {
  keydownPrintable: 0,
  keydownControl: 0,
  keydownArrow: 0,
  keydownNav: 0,
  keydownCtrl: 0,
  keydownMeta: 0,
  keydownPass: 0,
  keydown229: 0,
  sentAtKeydown: 0,
  sentAtBeforeInput: 0,
  sentAtInput: 0,
  sentAtCompositionEnd: 0,
  sentAtPaste: 0,
  sentCopy: 0,
}
let instances = 0
let debugOn = false
try {
  debugOn = localStorage.getItem('aki-term-input-debug') === '1'
} catch { debugOn = false }

function record(type, detail) {
  const entry = { t: Math.round(performance.now()), type, ...detail }
  ring.push(entry)
  if (ring.length > RING_MAX) ring.shift()
  if (debugOn) console.log('[term-input]', type, detail)
}

if (typeof window !== 'undefined' && !window.__akiTermInput) {
  window.__akiTermInput = {
    status() {
      let flagDebug = null
      try { flagDebug = localStorage.getItem('aki-term-input-debug') } catch { /* */ }
      return {
        page: document.title,
        version: 1,
        instances,
        debugMirroring: debugOn,
        flags: { 'aki-term-input-debug': flagDebug },
        eventsRecorded: ring.length,
        counts: { ...counts },
      }
    },
    dump() { return ring.slice() },
    tail(n = 40) { return ring.slice(-n) },
    debug(on = true) { debugOn = !!on; return debugOn },
    clear() { ring.length = 0; for (const k of Object.keys(counts)) counts[k] = 0; return true },
    help() {
      return [
        '__akiTermInput.status()  — is the input layer attached',
        '__akiTermInput.tail(40)  — last 40 events',
        '__akiTermInput.dump()    — the whole ring',
        '__akiTermInput.debug(true) — mirror events to console',
        '__akiTermInput.clear()   — reset ring + counters',
        "localStorage['aki-input-mode']='legacy' then reopen tab — fallback to old guard",
      ]
    },
  }
}

// ── Key classification ──────────────────────────────────────────────────────────────────────────

const CTRL_KEYS = new Map([
  ['Backspace', '\x7f'],
  ['Enter', '\r'],
  ['Escape', '\x1b'],
  ['Tab', '\t'],
])

const ARROW_SEQ = { ArrowUp: 'A', ArrowDown: 'B', ArrowRight: 'C', ArrowLeft: 'D' }
const NAV_SEQ = {
  Home: 'H', End: 'F',
  PageUp: '5~', PageDown: '6~',
  Insert: '2~', Delete: '3~',
}

// Ctrl+letter → control byte (0x01-0x1A for A-Z, plus a few extras)
function ctrlByte(ch) {
  const code = ch.toUpperCase().charCodeAt(0)
  if (code >= 64 && code <= 95) return String.fromCharCode(code - 64)
  // @ is 0x00, [ \ ] ^ _ map to 0x1B-0x1F
  if (code === 64) return '\x00'
  if (code >= 91 && code <= 95) return String.fromCharCode(code - 64)
  return null
}

function classifyKey(ev) {
  if (['Shift', 'Control', 'Alt', 'Meta', 'CapsLock', 'Fn', 'Hyper', 'Super', 'OS', 'Symbol', 'AltGraph', 'NumLock'].includes(ev.key)) return 'modifier'
  if (ev.metaKey) return 'meta'
  if (ev.altKey && !ev.ctrlKey) return 'pass'
  if (ev.key === 'Dead' || ev.key === 'Unidentified' || ev.key === 'Process') return 'pass'
  if (ev.isComposing) return 'composing'
  if (ev.keyCode === 229) return 'key229'
  if (ev.ctrlKey && !ev.altKey) return 'ctrl'
  if (CTRL_KEYS.has(ev.key)) return 'control'
  if (ev.key.startsWith('Arrow')) return 'arrow'
  if (NAV_SEQ[ev.key] !== undefined) return 'nav'
  if (/^F\d+$/.test(ev.key)) return 'function'
  if (ev.key && ev.key.length === 1) return 'printable'
  return 'pass'
}

// ── Public API ──────────────────────────────────────────────────────────────────────────────────

/**
 * Creates an app-owned textarea overlay that captures all keyboard input for a terminal,
 * bypassing xterm.js's built-in keyboard pipeline entirely.
 *
 * @param {function} onData — (text: string) => void. Called when text is committed and should be
 *   sent to the PTY. The caller wires this to sendRaw() in usePtyTerminal.js.
 * @param {object} [opts]
 * @param {function} [opts.getSelection] — () => string | null. Returns xterm's current selection
 *   text, or null/empty if nothing selected. Used to decide ⌘C behavior (copy vs SIGINT).
 * @returns {{ mount, dispose, focus, blur, textarea, isComposing }}
 */
export function useTerminalInput(onData, opts = {}) {
  const log = record
  let textarea = null
  let composing = false
  let keyClass = null
  let claimed = false
  let disposed = false

  function onCompositionStart() {
    composing = true
    keyClass = null
    log('compositionstart', {})
  }

  function onCompositionUpdate(ev) {
    log('compositionupdate', { data: ev.data })
  }

  function onCompositionEnd(ev) {
    if (!composing) return
    composing = false
    // Read the committed text directly — compositionend fires AFTER the textarea has been updated
    // on most browsers, but the data property is unreliable. Read the textarea value instead.
    const value = textarea.value
    if (value) {
      counts.sentAtCompositionEnd++
      onData(value)
      textarea.value = ''
    }
    log('compositionend', { data: ev.data, sent: value || '(empty)' })
  }

  function onKeydown(ev) {
    const cls = classifyKey(ev)
    keyClass = cls
    claimed = false

    log('keydown', {
      key: ev.key, keyCode: ev.keyCode, code: ev.code,
      ctrlKey: ev.ctrlKey, metaKey: ev.metaKey, altKey: ev.altKey,
      isComposing: ev.isComposing, class: cls,
    })

    // Track counts
    switch (cls) {
      case 'printable': counts.keydownPrintable++; break
      case 'control': counts.keydownControl++; break
      case 'arrow': counts.keydownArrow++; break
      case 'nav': counts.keydownNav++; break
      case 'ctrl': counts.keydownCtrl++; break
      case 'meta': counts.keydownMeta++; break
      case 'key229': counts.keydown229++; break
      default: counts.keydownPass++; break
    }

    // Modifier-only: pass through
    if (cls === 'modifier') return
    // IME composing: browser owns the textarea
    if (cls === 'composing' || cls === 'pass') return

    // Meta/Cmd combinations: pass through (TerminalStack.vue handles app shortcuts at window level)
    if (cls === 'meta') {
      // ⌘C with selection → let browser copy; without selection → SIGINT
      if (ev.key === 'c' && opts.getSelection) {
        const sel = opts.getSelection()
        if (!sel) {
          claimed = true
          counts.sentAtKeydown++
          ev.preventDefault()
          onData('\x03')
        }
        // With selection: pass through → browser fires copy event → copy handler below fills clipboard
        return
      }
      // All other ⌘ combos: pass through
      return
    }

    // keyCode 229, not composing: OpenKey synthetic key. Don't handle at keydown — wait for
    // beforeinput/input where the actual text payload arrives.
    if (cls === 'key229') return

    // Ctrl+letter → control byte
    if (cls === 'ctrl') {
      const cb = ctrlByte(ev.key)
      if (cb !== null) {
        claimed = true
        counts.sentAtKeydown++
        ev.preventDefault()
        onData(cb)
      }
      return
    }

    // Printable single char — do NOT handle at keydown. The browser may need to compose
    // this key with a following key (dead key / combining diacritic), and preventing here
    // would prevent the composition. Let beforeinput deliver the committed char instead.
    // OpenKey/EVKey single-char injections also arrive at beforeinput as insertText.
    if (cls === 'printable') return

    // Control keys (Backspace, Enter, Escape, Tab)
    if (cls === 'control') {
      claimed = true
      counts.sentAtKeydown++
      ev.preventDefault()
      onData(CTRL_KEYS.get(ev.key))
      return
    }

    // Arrow keys
    if (cls === 'arrow') {
      claimed = true
      counts.sentAtKeydown++
      ev.preventDefault()
      const seqBase = ARROW_SEQ[ev.key]
      // Standard ANSI cursor sequences. Application cursor mode is terminal state and the
      // shell/PTY handles it — we send standard CSI sequences always.
      onData(`\x1b[${seqBase}`)
      return
    }

    // Navigation keys (Home, End, PageUp, PageDown, Insert, Delete)
    if (cls === 'nav') {
      claimed = true
      counts.sentAtKeydown++
      ev.preventDefault()
      onData(`\x1b[${NAV_SEQ[ev.key]}`)
      return
    }

    // Function keys
    if (cls === 'function') {
      claimed = true
      counts.sentAtKeydown++
      ev.preventDefault()
      const n = parseInt(ev.key.slice(1), 10)
      if (n <= 4) onData(`\x1b[${['P', 'Q', 'R', 'S'][n - 1]}`)
      else onData(`\x1b[${n < 10 ? n : 11 + (n - 11)}~`)
      return
    }
  }

  function onBeforeInput(ev) {
    log('beforeinput', {
      inputType: ev.inputType,
      data: ev.data,
      dataTransfer: ev.dataTransfer ? ev.dataTransfer.types?.join(',') : null,
      isComposing: ev.isComposing,
      class: keyClass,
      claimed,
    })

    // Already claimed at keydown or composition in progress — pass through
    if (claimed || composing || ev.isComposing) return

    // Only claim insertText and insertReplacementText for non-composing paths
    if (ev.inputType === 'insertText' && ev.data) {
      ev.preventDefault()
      ev.stopPropagation()
      counts.sentAtBeforeInput++
      onData(ev.data)
      return
    }

    if (ev.inputType === 'insertReplacementText') {
      const text = ev.data || (ev.dataTransfer?.getData?.('text/plain')) || null
      if (text) {
        ev.preventDefault()
        ev.stopPropagation()
        counts.sentAtBeforeInput++
        onData(text)
      }
      return
    }

    // deleteContentBackward / insertLineBreak during composition: pass through
    // (OpenKey backspace bursts are caught by key229 at keydown, not here)
  }

  function onInput(ev) {
    log('input', { inputType: ev.inputType, data: ev.data, class: keyClass, claimed })
    // Last-resort fallback: if the textarea was mutated without by our handlers, read the delta.
    // This catches OpenKey's multi-char carrier when WKWebView skips beforeinput entirely.
    if (claimed || composing || ev.isComposing) return
    if (ev.inputType === 'insertText' && ev.data) {
      counts.sentAtInput++
      onData(ev.data)
      textarea.value = ''
    }
  }

  function onPaste(ev) {
    const text = ev.clipboardData?.getData('text/plain')
    log('paste', { length: text ? text.length : 0 })
    if (text) {
      ev.preventDefault()
      counts.sentAtPaste++
      // Bracketed paste: the shell decides whether to enable bracketed mode (usually on).
      // Sending the paste as raw text is correct — the terminal emulator should bracket it.
      onData(text)
    }
  }

  function onCopy(ev) {
    if (!opts.getSelection) return
    const sel = opts.getSelection()
    log('copy', { hasSelection: !!sel, length: sel ? sel.length : 0 })
    if (sel) {
      ev.preventDefault()
      ev.clipboardData.setData('text/plain', sel)
      counts.sentCopy++
    }
  }

  function onFocus() { log('focus', {}) }
  function onBlur() {
    // Clear textarea on blur so accumulated preedit artifacts don't survive focus loss.
    // The terminal is PTY-echo'd, so the textarea should always be empty except during composition.
    if (textarea) textarea.value = ''
    log('blur', {})
  }

  function mount(rootEl) {
    if (!rootEl) { log('mount-failed', { reason: 'rootEl missing' }); return }
    if (textarea) { log('already-mounted', {}); return }

    textarea = document.createElement('textarea')
    textarea.classList.add('aki-term-input-overlay')
    textarea.setAttribute('aria-label', 'Terminal input')
    textarea.setAttribute('autocorrect', 'off')
    textarea.setAttribute('autocapitalize', 'off')
    textarea.setAttribute('autocomplete', 'off')
    textarea.setAttribute('spellcheck', 'false')
    textarea.setAttribute('rows', '1')

    // Insert BEFORE xterm's mount element (the overlay sits on top in the DOM stacking context)
    rootEl.style.position = 'relative'
    rootEl.appendChild(textarea)

    textarea.addEventListener('compositionstart', onCompositionStart)
    textarea.addEventListener('compositionupdate', onCompositionUpdate)
    textarea.addEventListener('compositionend', onCompositionEnd)
    textarea.addEventListener('keydown', onKeydown)
    textarea.addEventListener('beforeinput', onBeforeInput)
    textarea.addEventListener('input', onInput)
    textarea.addEventListener('paste', onPaste)
    textarea.addEventListener('copy', onCopy)
    textarea.addEventListener('focus', onFocus)
    textarea.addEventListener('blur', onBlur)

    instances++
    log('mounted', { instances })

    // Focus the textarea immediately so the user can start typing — same as term.focus() after mount
    textarea.focus({ preventScroll: true })
  }

  function dispose() {
    disposed = true
    if (!textarea) return
    textarea.removeEventListener('compositionstart', onCompositionStart)
    textarea.removeEventListener('compositionupdate', onCompositionUpdate)
    textarea.removeEventListener('compositionend', onCompositionEnd)
    textarea.removeEventListener('keydown', onKeydown)
    textarea.removeEventListener('beforeinput', onBeforeInput)
    textarea.removeEventListener('input', onInput)
    textarea.removeEventListener('paste', onPaste)
    textarea.removeEventListener('copy', onCopy)
    textarea.removeEventListener('focus', onFocus)
    textarea.removeEventListener('blur', onBlur)
    textarea.remove()
    textarea = null
    instances--
    log('disposed', { instances })
  }

  function focus() {
    textarea?.focus({ preventScroll: true })
  }

  function blur() {
    textarea?.blur()
  }

  return {
    mount, dispose, focus, blur,
    /** The textarea element — exposed so TerminalView can reference it for focus management. */
    textarea: () => textarea,
    /** Whether a composition is currently in progress (IME composing). */
    isComposing: () => composing,
  }
}
