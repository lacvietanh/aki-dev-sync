// Terminal input — the TEXT half only. xterm owns keys; this file owns text.
//
// Decided architecture: docs/research/terminal-vietnamese-ime-root-cause-4.md §7 (the head of the
// IME chain). It replaces BOTH previous attempts — useWkImeGuard.js (a capture-phase guard) and
// useTerminalInput.js (an app-owned overlay textarea with `disableStdin`) — and is deliberately far
// smaller than either.
//
// WHY THE SPLIT IS WHERE IT IS. Keys are a solved, mode-dependent protocol problem: DECCKM,
// modifier-encoded CSI sequences, Alt-as-meta, F1-F24, Ctrl mappings, bracketed paste. xterm gets
// all of that right in ~450 lines and it must never be re-implemented here — the overlay this file
// replaces re-implemented it in about 40 and was wrong six ways (arrows ignored application cursor
// mode, F5 emitted PageUp, paste lost its bracketing, Option+word-motions sent nothing).
//
// Text is the one thing xterm gets wrong, and it is one line of 5.5.0: `_keyPress` does
// `String.fromCharCode(ev.charCode)` — a single UTF-16 unit — while WebKit hands OpenKey's
// corrected syllable through as an ordinary keydown whose `event.key` is the WHOLE string. That is
// the historical `"ăn gì" -> "ăn g"`. (The chain's older premise, that WKWebView tags these keys
// keyCode 229 and Chromium does not, is backwards — see §1 of the research doc. Nothing here
// depends on which way it goes: a non-229 carrier is claimed by the keypress veto plus the drain,
// and a 229-tagged one is claimed by the drain before xterm's textarea diff can run.)
//
// EXCLUSIVITY IS BY CONSTRUCTION, NOT BY CLASSIFICATION. The whole design rests on one DOM
// specification guarantee: preventDefault on a key event suppresses the textarea mutation, and
// therefore the `input` event. Against xterm 5.5.0:
//
//   physical printable   `_keyPress` sends it and cancels -> textarea never mutates -> no `input`
//   multi-char carrier   keypress vetoed -> xterm sends nothing, does not cancel -> browser inserts
//                        the full string -> the drain sends it once
//   229 key, not composing  xterm bails without cancelling and schedules a setTimeout(0) textarea
//                        diff; the drain reads and empties SYNCHRONOUSLY, so that diff then compares
//                        '' vs '' and is a structural no-op
//   any non-text key     xterm encodes and cancels -> textarea unmutated -> inert
//   composition          xterm owns it -> the drain stands down
//
// Public xterm API only (`term.element`, `term.textarea`, `term.input`,
// `attachCustomKeyEventHandler`) — no vendoring, no patching. The genuinely upstream-shaped fix is
// three lines in `_keyPress`; that belongs in an upstream issue, not in a fork here. An
// `@xterm/xterm` 6.x upgrade is explicitly NOT bundled with this: master still does
// `String.fromCharCode(ev.charCode)` in `_keyPress`, so it fixes nothing for OpenKey, and it removed
// `this.cancel(ev)` from `_keyPress`/`_inputEvent`, which invalidates the table above.
//
// DIAGNOSTICS: pull-based — the ring is recorded ALWAYS and read on demand via a RETURNED value.
// Never console.log behind a pre-armed flag: Safari's Web Inspector does not replay messages
// emitted before it attached, and the Develop menu lists the inspector's OWN UI as a target, so a
// flag can read back "1" in the wrong origin and be invisible to the app. `__akiTermInput.status()`
// reports `page`, which must read the app's own title.

const RING_MAX = 400
const ring = []
const counts = {
  keypressVetoed: 0,
  drained: 0,
  drainedChars: 0,
  claimed229: 0,
  compositions: 0,
  enterAfterComposition: 0,
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
    /** Is the drain attached, and is this even the right page? */
    status() {
      let flagDebug = null
      try {
        flagDebug = localStorage.getItem('aki-term-input-debug')
      } catch {
        /* localStorage can throw in exotic contexts; the rest of status is still useful */
      }
      return {
        page: document.title, // must be the app's own title — otherwise you are inspecting the inspector
        version: 3,
        instances,
        debugMirroring: debugOn,
        flags: { 'aki-term-input-debug': flagDebug },
        eventsRecorded: ring.length,
        counts: { ...counts },
      }
    },
    /** Returned, not logged, so console filters and late-attaching inspectors cannot hide it. */
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
        '__akiTermInput.status()    — is the drain attached, on the right page, what has it claimed',
        '__akiTermInput.tail(40)    — last 40 events (returned as data, not logged)',
        '__akiTermInput.dump()      — the whole ring',
        '__akiTermInput.debug(true) — also mirror events to the console from now on',
        '__akiTermInput.clear()     — reset ring + counters before a clean repro',
      ]
    },
  }
}

/** OpenKey's invisible edit sentinels (U+202F narrow no-break space, U+200C zero-width non-joiner).
 *  They mark where the engine rewrote text and are not part of what the user typed — iTerm2's
 *  over-deletion bug (OpenKey#95) is what failing to absorb them looks like. */
const SENTINELS = /[\u202F\u200C]/g

/** WebKit fires `compositionend` BEFORE `keydown`, the reverse of Chromium, so the usual
 *  `isComposing` guard does not cover the Enter that commits a composition. CodeMirror 6 (100 ms)
 *  and ProseMirror (500 ms) both solve it with exactly this timestamp window. 100 ms is CodeMirror's
 *  number and the shorter of the two. */
export const POST_COMPOSITION_MS = 100

/**
 * Attaches the text drain to an already-opened xterm `Terminal`.
 *
 * MUST be called after `term.open()` — `term.textarea` and `term.element` do not exist before it.
 *
 * @param {import('@xterm/xterm').Terminal} term
 * @returns {{ dispose: () => void }}
 */
export function useTerminalTextDrain(term) {
  const root = term.element
  const textarea = term.textarea
  if (!root || !textarea) {
    record('attach-failed', { hasElement: !!root, hasTextarea: !!textarea })
    return { dispose: () => {} }
  }
  instances++

  // True while a REAL composing IME owns the keyboard. Released on a setTimeout(0) after
  // compositionend so xterm's own `_finalizeComposition` (also deferred) wins the race — releasing
  // synchronously would let the drain claim the commit out from under it.
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

  /** THE DRAIN. A no-local-echo terminal's textarea has exactly one correct resting value: empty.
   *  So the entire algorithm is "read all of it and empty it" — no diffing, no per-shape
   *  classification, no interpretation of a key event's payload. */
  function onInputCapture(ev) {
    if (ev.target !== textarea) return
    if (composing || ev.isComposing) return
    const inputType = ev.inputType || ''
    // Composition, paste and drop are xterm's own, correctly-handled paths. Claiming paste here in
    // particular would lose bracketed paste, which is the defect the previous overlay shipped with.
    if (
      inputType.startsWith('insertComposition') ||
      inputType === 'insertFromComposition' ||
      inputType === 'insertFromPaste' ||
      inputType === 'insertFromDrop'
    ) {
      record('input-passed', { inputType })
      return
    }

    const raw = textarea.value
    if (!raw) return // nothing was inserted — leave xterm's own `_inputEvent` alone
    textarea.value = ''

    // stopPropagation, not preventDefault: `input` is not cancelable and the mutation already
    // happened. Capture phase on an ancestor means xterm's own listener on the textarea never runs,
    // so it cannot re-send what has just been sent.
    ev.stopPropagation()

    const text = raw.replace(SENTINELS, '').replace(/\r\n|\n|\r/g, '\r')
    if (!text) {
      record('input-sentinels-only', { raw: raw.length })
      return
    }
    counts.drained++
    counts.drainedChars += text.length
    record('drained', { inputType, text, length: text.length })
    // Through `term.input`, i.e. into xterm's own `onData` — which is where the sticky-modifier
    // funnel lives (usePtyTerminal.js `emitKey`). A multi-character chunk leaves an armed Ctrl
    // latched rather than eating it.
    term.input(text, true)
  }

  /** The ONE irreducible keydown claim. Against an EMPTY textarea, Backspace and Enter produce no
   *  `input` event at all, so nothing else can claim them and the drain would silently drop them.
   *  Scoped to `keyCode === 229 && !isComposing`, i.e. a real composing IME's committed edit keys —
   *  OpenKey never reaches this branch (see the header: its carrier is not 229-tagged in WebKit).
   *  `preventDefault` only, never `stopPropagation`: xterm's scheduled textarea diff must still run
   *  and find an unmutated textarea, which makes it a harmless no-op rather than a severed channel. */
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
      // WebKit's compositionend-before-keydown ordering: this Enter may be the one that COMMITTED
      // the syllable, in which case acting on it submits a half-finished line and swallows the
      // commit. `isComposing` is already false by then, which is exactly why the timestamp window
      // exists.
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

  /** Veto xterm's keypress for the multi-character carrier ONLY. `ev.key.length > 1` with
   *  `ev.charCode === ev.key.codePointAt(0)` is the carrier / dead-key signature — the same
   *  discriminator sotasan/piyo's Tauri-targeted addon uses. Returning `false` skips xterm's
   *  processing WITHOUT preventDefault, so the browser's own insertion proceeds and lands in the
   *  drain. (Guard v1 got this backwards and vetoed WITH preventDefault, destroying the insertion:
   *  "ăn gì" -> "ăn ".)
   *
   *  NOTE: this claims the terminal's single custom key handler slot. Nothing else in the app uses
   *  it — window-level ⌘ shortcuts live in dock/TerminalStack.vue. */
  function customKeyEventHandler(ev) {
    if (ev.type !== 'keypress') return true
    if (typeof ev.key === 'string' && ev.key.length > 1 && ev.charCode === ev.key.codePointAt(0)) {
      counts.keypressVetoed++
      record('keypress-vetoed', { key: ev.key, charCode: ev.charCode })
      return false
    }
    return true
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
      // Hand the single handler slot back rather than leaving a closure over a disposed terminal.
      term.attachCustomKeyEventHandler(() => true)
      instances--
      record('detached', { instances })
    },
  }
}
