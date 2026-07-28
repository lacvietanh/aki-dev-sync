// WebKit IME key guard v2 — makes DIRECT typing with Vietnamese key-injection engines (OpenKey,
// EVKey, VKey) survive xterm.js 5.5.0 inside Tauri v2's WKWebView.
//
// Root cause + v1 post-mortem: docs/research/terminal-vietnamese-ime-root-cause-2.md (v1's
// failure) and …-jul27.md (the original xterm/WKWebView analysis). Short form:
//
// OpenKey's tap suppresses the tone key and synchronously injects N backspace pairs (kVK 51)
// followed by ONE carrier keydown/keyup (kVK 0) whose CGEventKeyboardSetUnicodeString payload is
// the WHOLE retyped chunk ("gì" — multi-char), unless the user enabled "Gửi từng phím" (then one
// event per char). WKWebView routes these through NSTextInputContext, which is why they surface
// as keyCode 229 — and, for the multi-char carrier, may surface as a keydown whose `key` IS the
// whole string. xterm 5.5.0 loses both shapes (issues #5887/#5894: keyup-gated `_keyDownSeen`
// drops fast bursts, the textarea diff collapses a backspace run into one DEL, `_keyPress` emits
// only charCode = the first UTF-16 unit — the historical "ăn gì" → "ăn g").
//
// v1 failed ("ăn gì" → "ăn ") because it vetoed xterm's keypress unconditionally for multi-char
// keys while its only replacement channel was `beforeinput` with exactly three inputTypes — any
// other delivery shape (insertReplacementText with a dataTransfer payload, no beforeinput at
// all) left NO claimant. v2's rule, per the delivery matrix in the -2 research doc: every
// possible shape has EXACTLY ONE claimant, and the guard only severs an xterm channel when it
// has provably installed its own.
//
//   Key classes (decided per keydown, never sticky across keys):
//   - synthetic229: keyCode 229 while no composition is active → WKWebView mis-tagged a direct
//     key. Backspace/Enter are sent to the PTY right at keydown (preventDefault only — NOT
//     stopPropagation, so xterm still schedules its textarea diff, which finds an unmutated
//     textarea and no-ops: a harmless safety net instead of a severed channel). Printables are
//     claimed at `beforeinput` (preventDefault starves xterm's diff deterministically); if no
//     beforeinput ever fires, the guard stands down and xterm's own diff delivers exactly as it
//     did pre-guard — never worse than stock.
//   - multiCarrier: keydown whose `key` is a multi-char string (impossible for physical typing,
//     the unicode-string carrier signature) and NOT keyCode 229. xterm would emit only the first
//     UTF-16 unit via keypress ("ăn g"), so that keypress is vetoed and the payload claimed at
//     `beforeinput`, falling back to the `input` event (mutation already done — can't prevent,
//     so it stops propagation instead to keep xterm's `_inputEvent` out). No diff is ever
//     scheduled for a non-229 key, so the input-fallback cannot double-send.
//   - Real composition (compositionstart seen or isComposing): guard stands down entirely;
//     insertCompositionText/insertFromComposition are never claimed. The compose row remains the
//     supported path for true IMEs (macOS built-in Vietnamese).
//
// DIAGNOSTICS — evidence first, console last (see `window.__akiIme` below). Every event the guard
// sees is recorded into a bounded in-memory ring ALWAYS, whether or not anything is enabled, and
// read back on demand with `__akiIme.dump()`. That inversion is deliberate: the first attempt at
// this used `console.log` gated on a localStorage flag, and produced no evidence at all on the
// Mac, because that design has three independent single points of failure —
//   1. Safari's Web Inspector does not replay console messages emitted BEFORE it attached
//      (Chrome buffers them; Safari does not), so anything logged at mount is simply gone;
//   2. the flag has to be set in the right origin's localStorage, and Safari's Develop menu lists
//      the INSPECTOR'S OWN UI as a target named `Main.html` (`inspector-resource:///Main.html`)
//      whenever an inspector is open — this app appears as `localhost` under `tauri dev`, in a
//      window titled "Web Inspector — localhost". A flag set in the wrong one reads back "1" and
//      is invisible to the app; verify with `__akiIme.status().page === 'Aki Dev Sync'`;
//   3. the flag is read once at mount, so setting it after the terminal opened does nothing.
// A ring + a return-valued dump has none of those: the console prints an evaluated RETURN VALUE
// regardless of log-level filters or when the inspector attached.
//
// Escape hatches (Safari Web Inspector → console, on the app's own page):
//   localStorage['aki-ime-guard'] = 'off'  → guard fully disabled (stock xterm behaviour), on the
//                                            next terminal mount. This is the A/B that PROVES
//                                            whether the guard is what makes typing work.
//   __akiIme.debug(true)                   → additionally mirror every recorded event to the
//                                            console, live, no remount needed.
//   localStorage['aki-ime-debug'] = '1'    → same mirroring, from mount (kept for continuity).
//
// Known benign limit: a guard-sent 1-byte chunk can be remapped by the phone key row's sticky
// Ctrl (usePtyTerminal wireInput) if armed mid-burst — phone-only surface, Vietnamese bursts are
// a Mac scenario; recorded in the -2 research doc, not worth a coupling between the two modules.

// Bounded so a long session cannot grow memory: one Vietnamese burst is ~6 entries, so 400 keeps
// the last few dozen words — far more than any diagnosis needs, and nothing is ever read except
// on an explicit dump().
const RING_MAX = 400
const ring = []
// Claim accounting: which channel actually delivered, per class. This is the single number that
// answers "which part of the guard is doing the work" without a keystroke-by-keystroke read.
const counts = {
  keydown229: 0,
  keydownCarrier: 0,
  sentAtKeydown: 0,
  sentAtBeforeInput: 0,
  sentAtInput: 0,
  keypressVetoed: 0,
  stoodDown: 0,
}
let instances = 0
let debugOn = false
try {
  debugOn = localStorage.getItem('aki-ime-debug') === '1'
} catch {
  debugOn = false
}

function record(type, detail) {
  const entry = { t: Math.round(performance.now()), type, ...detail }
  ring.push(entry)
  if (ring.length > RING_MAX) ring.shift()
  if (debugOn) console.log('[ime-guard]', type, detail)
}

// Installed once per page, not per terminal — every tab's guard feeds the same ring, which is
// what you want when reproducing a bug across tabs. Attached to `window` so it is reachable from
// the Web Inspector console with no import.
if (typeof window !== 'undefined' && !window.__akiIme) {
  window.__akiIme = {
    /** Is the guard even running here, and is this the right page at all? */
    status() {
      let flagGuard = null
      let flagDebug = null
      try {
        flagGuard = localStorage.getItem('aki-ime-guard')
        flagDebug = localStorage.getItem('aki-ime-debug')
      } catch {
        /* localStorage can throw in exotic contexts; the rest of status is still useful */
      }
      return {
        page: document.title, // must read 'Aki Dev Sync' — otherwise you are inspecting another app
        guardVersion: 2,
        guardsAttached: instances,
        guardDisabledByFlag: flagGuard === 'off',
        debugMirroring: debugOn,
        flags: { 'aki-ime-guard': flagGuard, 'aki-ime-debug': flagDebug },
        eventsRecorded: ring.length,
        counts: { ...counts },
      }
    },
    /** Everything the guard saw, oldest first. Returned (not logged) so console filters cannot hide it. */
    dump() {
      return ring.slice()
    },
    /** The last n events — the usual call right after reproducing one bad word. */
    tail(n = 40) {
      return ring.slice(-n)
    },
    /** Live console mirroring, no remount required. */
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
        '__akiIme.status()  — is the guard attached, on the right page, what has it claimed',
        '__akiIme.tail(40)  — last 40 recorded events (returned as data, not logged)',
        '__akiIme.dump()    — the whole ring',
        '__akiIme.debug(true) — also mirror events to the console from now on',
        '__akiIme.clear()   — reset ring + counters before a clean repro',
        "localStorage['aki-ime-guard']='off' then reopen the tab — A/B the guard itself",
      ]
    },
  }
}

export function useWkImeGuard(term) {
  if (localStorage.getItem('aki-ime-guard') === 'off') {
    record('disabled-by-flag', {})
    return { dispose: () => {} }
  }
  const log = record

  const root = term.element
  if (!root) {
    record('attach-failed', { reason: 'term.element missing' })
    return { dispose: () => {} }
  }
  instances++

  // True between compositionstart and compositionend — a REAL IME owns the keyboard; stand down.
  let composing = false
  // Class of the most recent keydown: null | 'synthetic229' | 'multiCarrier'.
  let keyClass = null
  // Whether the guard already claimed the current key's payload (beforeinput claimed → the input
  // fallback must not fire again for the same key).
  let claimed = false

  function onCompositionStart() {
    composing = true
    keyClass = null
    log('compositionstart', {})
  }
  function onCompositionEnd() {
    composing = false
    log('compositionend', {})
  }

  function classify(ev) {
    if (composing || ev.isComposing) return null
    if (ev.keyCode === 229) return 'synthetic229'
    // Multi-char `key` = the unicode-string carrier (kVK 0 + CGEventKeyboardSetUnicodeString).
    // 'Dead' and named keys ('Enter', 'ArrowUp'…) are excluded by requiring a non-letter-only
    // check to stay cheap: named DOM key values are ASCII letters only, while a carrier payload
    // for this engine class always contains a non-ASCII Vietnamese char.
    if (ev.key && ev.key.length > 1 && !/^[A-Za-z]+$/.test(ev.key) && !ev.metaKey && !ev.ctrlKey) {
      return 'multiCarrier'
    }
    return null
  }

  function onKeydown(ev) {
    keyClass = classify(ev)
    claimed = false
    log('keydown', {
      key: ev.key,
      keyCode: ev.keyCode,
      isComposing: ev.isComposing,
      class: keyClass,
    })
    if (keyClass === 'synthetic229') counts.keydown229++
    else if (keyClass === 'multiCarrier') counts.keydownCarrier++
    else counts.stoodDown++
    if (keyClass !== 'synthetic229') return
    // Only keys that are UNAMBIGUOUSLY direct are handled at keydown: a real IME's in-preedit
    // Backspace/Enter always arrives with isComposing=true, which classify() excludes. On an
    // empty hidden textarea a backspace produces NO beforeinput/input at all, so waiting for
    // those would drop it — hence keydown-time handling for exactly these two.
    if (ev.key === 'Backspace') {
      claimed = true
      counts.sentAtKeydown++
      term.input('\x7f', true)
      ev.preventDefault() // no stopPropagation: xterm's scheduled diff no-ops on the unmutated textarea
    } else if (ev.key === 'Enter') {
      claimed = true
      counts.sentAtKeydown++
      term.input('\r', true)
      ev.preventDefault()
    }
    // Printables fall through to beforeinput/input, where inputType disambiguates.
  }

  function onBeforeInput(ev) {
    log('beforeinput', {
      inputType: ev.inputType,
      data: ev.data,
      dataTransfer: ev.dataTransfer ? ev.dataTransfer.types.join(',') : null,
      isComposing: ev.isComposing,
      class: keyClass,
      claimed,
    })
    if (!keyClass || claimed || composing || ev.isComposing) return
    let out = null
    if (ev.inputType === 'insertText' && ev.data) {
      out = ev.data
    } else if (ev.inputType === 'insertReplacementText') {
      // WebKit carries the replacement payload in dataTransfer, not data.
      out = ev.data || (ev.dataTransfer && ev.dataTransfer.getData('text/plain')) || null
    } else if (ev.inputType === 'deleteContentBackward') {
      out = '\x7f'
    } else if (ev.inputType === 'insertLineBreak') {
      out = '\r'
    } else {
      return // composition/paste/other types stay on xterm's own path untouched
    }
    if (out === null) return
    claimed = true
    counts.sentAtBeforeInput++
    term.input(out, true)
    // Textarea must not mutate: for a 229 key xterm already scheduled a textarea diff, and an
    // unchanged value turns that diff into a no-op instead of a duplicate or a collapsed DEL.
    ev.preventDefault()
    ev.stopPropagation()
  }

  // Fallback claimant when WKWebView skipped beforeinput and went straight to mutation+input.
  // ONLY for multiCarrier keys: a 229 key that reached this point still has xterm's textarea
  // diff scheduled (keydown was not stopped), and that diff WILL deliver the mutation — claiming
  // here too would double-send. A non-229 multiCarrier key never scheduled a diff, so this is
  // its last and only channel.
  function onInput(ev) {
    log('input', { inputType: ev.inputType, data: ev.data, class: keyClass, claimed })
    if (keyClass !== 'multiCarrier' || claimed || composing || ev.isComposing) return
    if (ev.inputType === 'insertText' && ev.data) {
      claimed = true
      counts.sentAtInput++
      term.input(ev.data, true)
      // Mutation already happened (input is not cancelable) — stop propagation so xterm's
      // _inputEvent cannot re-emit the same data. The stale textarea content is harmless: xterm
      // clears it on Enter/^C, and any later 229 diff snapshots old==new consistently.
      ev.stopPropagation()
    }
  }

  // Veto xterm's keypress re-emission ONLY for keys the guard has a claimant for. For a
  // multiCarrier key xterm's _keyPress would emit charCode = the FIRST UTF-16 unit of the
  // payload and preventDefault the real insertion — the historical "ăn gì" → "ăn g". Vetoing
  // (returning false) skips xterm's processing WITHOUT preventDefault, so the browser's own
  // insertion proceeds and lands in onBeforeInput/onInput above.
  // NOTE: this claims the terminal's single custom key handler slot; nothing else in the app
  // uses attachCustomKeyEventHandler (window-level shortcuts live in TerminalStack.vue).
  term.attachCustomKeyEventHandler((ev) => {
    if (ev.type !== 'keypress') return true
    if (keyClass !== null) {
      counts.keypressVetoed++
      log('keypress-vetoed', { key: ev.key, charCode: ev.charCode, class: keyClass })
      return false
    }
    return true
  })

  root.addEventListener('compositionstart', onCompositionStart, true)
  root.addEventListener('compositionend', onCompositionEnd, true)
  root.addEventListener('keydown', onKeydown, true)
  root.addEventListener('beforeinput', onBeforeInput, true)
  root.addEventListener('input', onInput, true)
  log('attached', { version: 2, instances })

  return {
    dispose() {
      root.removeEventListener('compositionstart', onCompositionStart, true)
      root.removeEventListener('compositionend', onCompositionEnd, true)
      root.removeEventListener('keydown', onKeydown, true)
      root.removeEventListener('beforeinput', onBeforeInput, true)
      root.removeEventListener('input', onInput, true)
      instances--
      record('detached', { instances })
      // The custom key handler dies with the Terminal instance (term.dispose in TerminalView).
    },
  }
}
