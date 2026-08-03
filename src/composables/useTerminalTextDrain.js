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
//   ANY keypress        vetoed unconditionally -> xterm sends nothing and does NOT preventDefault ->
//                        the browser inserts -> the drain sends it once. This row used to read
//                        "`_keyPress` sends it and cancels -> textarea never mutates", which was
//                        FALSE and is what produced the jul31 double-space blocker: `_keyPress`
//                        cancels without `force` (Terminal.ts:1133) and `cancel()` is a no-op unless
//                        `cancelEvents` is on (Terminal.ts:1308) — so for space (keyCode 32, fails
//                        Keyboard.ts:381) and uppercase A-Z (caps-lock HACK, Terminal.ts:1052) xterm
//                        sent the char AND the browser inserted it, and the drain sent it again.
//                        See docs/research/terminal-input-jul31.md §5.2 and the council verdict.
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

/** Hex codepoints of a drained chunk, for the ring only. Vietnamese arrives either precomposed
 *  (U+1EA1 etc.) or decomposed (base + combining mark), and the two render nearly alike \u2014 a report
 *  that says "the accent looks off" is unanswerable without this. Capped: the ring is a debug buffer,
 *  not a transcript. */
function codepoints(s) {
  return Array.from(s.slice(0, 16), c => c.codePointAt(0).toString(16).padStart(4, '0')).join(' ')
}

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
    const inputType = ev.inputType || ''
    // xterm already sent the paste (bracketed wrapping intact) but clears the textarea BEFORE
    // the browser's own insertion, not after — so the pasted text is still sitting there.
    // Drain it without sending, or the next uncancelled keystroke (SPACE) re-sends it.
    // Ordered above the composing guard: xterm's paste handler runs regardless of composition
    // state, and clearing here is safe because the composition helper re-reads a live textarea.
    if (inputType === 'insertFromPaste') {
      textarea.value = ''
      record('input-paste-cleared', { inputType })
      return
    }
    if (composing || ev.isComposing) return
    // Composition is xterm's own, correctly-handled path. Drop is not: xterm 5.5.0 registers no
    // drop listener, so dropped text lingers until a later keystroke drags it along — a known
    // pre-existing defect left alone, since draining it would discard text nothing else sends.
    if (
      inputType.startsWith('insertComposition') ||
      inputType === 'insertFromComposition' ||
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
    // `cp` records the codepoints, not just the glyphs: a decomposed "á" (a + U+0301) and a
    // precomposed one (U+00E1) are indistinguishable in a copy-pasted bug report but behave
    // differently in the terminal, so the ring has to say which one actually arrived.
    record('drained', { inputType, text, length: text.length, cp: codepoints(text) })
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

  /** Veto xterm's `keypress` handling for EVERY keypress, so text has exactly one route to the PTY:
   *  the drain. Returning `false` skips xterm's `_keyPress` WITHOUT preventDefault, so the browser's
   *  own insertion still lands in the textarea and the drain claims it. (Guard v1 got this backwards
   *  and vetoed WITH preventDefault, destroying the insertion: "ăn gì" -> "ăn ".)
   *
   *  Why ALL of them, not just the multi-character carrier this used to match: xterm's `cancel()` is
   *  a no-op unless `cancelEvents` is on (Terminal.ts:1308, OptionsService.ts:56 — we don't set it),
   *  and `_keyPress` cancels WITHOUT `force` (Terminal.ts:1133). So for any key that `_keyDown` lets
   *  through without a forced cancel, `_keyPress` sends the char AND the browser still mutates the
   *  textarea — the drain then sends it a second time. That set is exactly {space, uppercase A-Z}:
   *  space is keyCode 32 and fails Keyboard.ts:381's `keyCode >= 48` test, and A-Z exits early via
   *  the caps-lock HACK at Terminal.ts:1052-1056. Everything else (arrows, F-keys, Ctrl/Alt combos)
   *  is force-cancelled in `_keyDown`, so no keypress ever reaches here. xterm's own anti-double
   *  guard `_keyPressHandled` cannot save us: it lives in `_inputEvent`, bound to the textarea
   *  itself (Terminal.ts:384), and our capture-phase `stopPropagation()` on the ancestor keeps that
   *  handler from ever running. Confirmed on hardware 2026-07-31: "TEST" arrived as "TTEÉTT".
   *
   *  NOTE: this claims the terminal's single custom key handler slot. Nothing else in the app uses
   *  it — window-level ⌘ shortcuts live in dock/TerminalStack.vue. */
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
      // Hand the single handler slot back rather than leaving a closure over a disposed terminal.
      term.attachCustomKeyEventHandler(() => true)
      instances--
      record('detached', { instances })
    },
  }
}
