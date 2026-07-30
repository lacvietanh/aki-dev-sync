# Terminal Input Layer Separation — Design Plan

Status: **Implemented — shipped in commit `7ce5804` ("feat(terminal): app-owned textarea input layer, replaces WkImeGuard"): `src/composables/useTerminalInput.js` created, `TerminalView.vue` updated. `useWkImeGuard.js` kept for the `legacy` escape hatch. Runtime verification on a Mac with OpenKey is still pending.** Moved to `done/` on 2026-07-30 by the plan-consolidation pass, verified against the commit rather than against this line.

Two stale references outside this doc still point at the pre-move path `docs/plan/terminal-ime-input-layer-separation.md` and could not be corrected here — source comments are out of scope for a docs pass, and `docs/research/` is immutable: `src/components/TerminalView.vue:332` and `:468`, `src/composables/useTerminalInput.js:4`, and `docs/research/terminal-vietnamese-ime-root-cause-2.md:1`. Fix the three source comments in the next code-touching task; leave the research line alone (`docs.B2` — a research doc is never edited after the fact).

Chain: follows `docs/research/terminal-vietnamese-ime-root-cause-2.md` (guard v2), supersedes `useWkImeGuard.js`.

## 1. Rationale

The current `useWkImeGuard.js` works by intercepting xterm.js 5.5.0's internal event pipeline — it classifies each keydown, vetoes xterm's handlers via `attachCustomKeyEventHandler`, and sends bytes through `term.input()` itself. This is inherently fragile: it depends on xterm's `_keyDownSeen`, `_keyPressHandled`, textarea-diff scheduling, and `beforeinput` behavior, all of which are internal and change across xterm versions.

The fix is to stop fighting xterm for control of its own hidden textarea and instead give it no textarea to fight over. Set `disableStdin: true` so xterm never fires `onData` from its own keyboard pipeline, then overlay an app-owned `<textarea>` that handles all keyboard input and sends committed text to `term.input(text, true)` — which bypasses the `disableStdin` gate by going through `paste()`.

**What VS Code actually does** (corrected from the task brief): VS Code's integrated terminal does NOT separate input from xterm. It uses xterm's default keyboard pipeline without modification. VS Code works with Vietnamese typing because Electron's Chromium does not tag synthetic keys as `keyCode 229`. The separation pattern described in the task brief applies to VS Code's **editor** (`textAreaInput.ts`), which renders content locally and can host preedit — fundamentally different from a terminal whose content is remote PTY with no local echo.

## 2. Architecture

### 2.1 New composable: `useTerminalInput.js`

Replaces `useWkImeGuard.js`. Owns a `<textarea>` element, manages its lifecycle, and exposes:
- `mount(rootEl)` — creates textarea, appends to root, wires listeners
- `dispose()` — removes textarea and listeners
- Diagnostic API: `window.__akiTermInput` (same pull-based pattern as `__akiIme`)

### 2.2 Textarea overlay

```
┌─ TerminalView.pty-terminal ─────────────────────┐
│ ┌─ pty-terminal-mount (xterm renders here) ────┐│
│ │                                               ││
│ │  PTY output only — xterm.js is renderer       ││
│ │                                               ││
│ └───────────────────────────────────────────────┘│
│ ┌─ aki-term-input-overlay ─────────────────────┐│
│ │  <textarea> (app-owned)                       ││
│ │  - position: absolute, full coverage          ││
│ │  - opacity: 0, but NOT display:none           ││
│ │  - caret-color: transparent                   ││
│ │  - receives ALL focus and keyboard events     ││
│ └───────────────────────────────────────────────┘│
│ ┌─ pty-key-row (unchanged) ────────────────────┐│
│ ┌─ pty-compose-row (unchanged) ────────────────┘│
└──────────────────────────────────────────────────┘
```

The textarea covers the entire mount area. It is visually transparent but still focusable and interactive — the browser's IME and keyboard pipeline treat it as a real input element. The user sees only the PTY-rendered content underneath; the textarea is invisible but present.

### 2.3 Input flow

**Ordinary typing (English, no IME):**
```
keydown on textarea → preventDefault (never mutate textarea)
                   → classify: printable? → send char via term.input(char, true)
                   → classify: Backspace/Enter/Esc → send via term.input(seq, true)
```

**Control keys (Ctrl+C, arrow keys, etc.):**
```
keydown on textarea → detect ctrlKey/metaKey
                   → preventDefault
                   → send escape sequence via term.input(seq, true)
```

**IME composition (macOS built-in Vietnamese, CJK):**
```
compositionstart → mark composing=true
compositionupdate → browser draws preedit IN the textarea (it's a real <input>)
compositionend → read textarea.value delta → term.input(text, true)
              → clear textarea.value → composing=false
```

**OpenKey/EVKey (synthetic keyCode 229):**
```
keydown with keyCode 229, isComposing=false
  → preventDefault (keep textarea pristine)
  → wait for beforeinput/input
  → read inserted text via ev.data or textarea value delta
  → term.input(text, true)
  → clear textarea
```

Because the textarea is app-owned and the browser handles IME natively on it, the multi-char carrier event ("gì" as one keydown) lands in `beforeinput.data` or `input.data` as the full string — exactly what the current guard's most complex code path (`multiCarrier → beforeinput → input fallback`) handles, but now handled by the browser's own input event pipeline without per-shape classification.

### 2.4 xterm.js configuration changes

```js
new Terminal({
  disableStdin: true,  // NEW — xterm's own onData never fires
  // ... all other options unchanged
})
```

`term.input(data, true)` still works: it calls `paste()` which goes through `CoreService.triggerDataEvent` via a path that does NOT check `disableStdin` (the gate is only in `triggerDataEvent`'s regular code path; `paste()` writes to the textarea and lets the `input` event fire, which triggers `_inputEvent` → `triggerDataEvent`... actually, let me re-verify this).

**CORRECTION after deeper source read:** `paste()` (`Clipboard.ts` L51) does `textarea.value = text`, then `textarea.dispatchEvent(new Event('input', ...))` — which triggers `_inputEvent`. But `_inputEvent` calls `coreService.triggerDataEvent`, which IS gated by `disableStdin`. So `term.input()` with `disableStdin: true` would be silently suppressed.

**Fix:** Use `term.paste(data)` instead of `term.input(data)`. `paste()` (Terminal.ts L890) calls the same `paste()` function in Clipboard.ts which calls `coreService.triggerDataEvent` — actually, same problem.

**Real fix — verified from source:** Looking at `paste()` in Clipboard.ts L51-64:
```ts
export function paste(text, textarea, coreService, optionsService) {
  // ... bracket paste wrapping ...
  textarea.value = text;
  textarea.dispatchEvent(new Event('input', { bubbles: true, cancelable: true }));
}
```

And `_inputEvent` (Terminal.ts L1172):
```ts
protected _inputEvent(ev) {
  if (ev.data && ev.inputType === 'insertText' && ...) {
    this.coreService.triggerDataEvent(text, true);
    ...
  }
}
```

The dispatched synthetic `input` event has no `.data` property (we created it without `InputEvent` init), so `_inputEvent`'s guard `ev.data && ev.inputType === 'insertText'` fails and it returns false. So `paste()` with `disableStdin` actually drops.

**Actual solution:** Bypass xterm's keyboard pipeline entirely. Instead of `term.input()` or `term.paste()`, send data directly via the existing `sendRaw()` in `usePtyTerminal.js`. The textarea composable receives a callback `onData(text)` that the caller wires to their PTY send path.

This means `useTerminalInput.js` does NOT import xterm at all. It only manages the textarea and calls a data callback. `TerminalView.vue` wires `(text) => ptyApi.value.sendRaw(text)` as the callback.

### 2.5 Separation of concerns

| Concern | Owner | Mechanism |
|---------|-------|-----------|
| Keyboard capture | `useTerminalInput.js` | App-owned textarea overlay |
| IME composition | Browser | Native textarea composition events |
| PTY send | `usePtyTerminal.js` | `sendRaw(text)` — unchanged |
| Terminal rendering | xterm.js | `term.write(bytes)` from PTY output — unchanged |
| Key row (Esc/Tab/arrows) | `TerminalView.vue` | Existing key row buttons — unchanged |
| Compose row | `TerminalView.vue` | Existing compose input — kept for true IME fallback |
| App shortcuts (⌘T/⌘W/⌘+/⌘-) | `TerminalStack.vue` | Existing keydown handler — unchanged (capture phase, fires before textarea) |

### 2.6 What the textarea handles vs what it passes through

**Handles (sends to PTY):**
- All printable characters
- Backspace, Enter, Escape
- Tab (when not captured by app)
- Ctrl+C, Ctrl+D, Ctrl+Z, Ctrl+L, etc.
- Arrow keys, Home, End, PageUp, PageDown
- IME-composed text (via compositionend → read value delta)
- OpenKey-style injected text (via beforeinput/input → read data)

**Passes through (does not preventDefault):**
- Meta/Cmd key combinations — `TerminalStack.vue`'s window-level capture handles these
- These are identified by `ev.metaKey` at keydown

**Never reaches textarea (handled by TerminalStack.vue's window capture):**
- ⌘T, ⌘W, ⌘⇧[, ⌘⇧], ⌘+, ⌘-, ⌘0 — these are captured at window level, capture phase, before the textarea ever sees them

## 3. Detailed design

### 3.1 `src/composables/useTerminalInput.js`

```js
// Terminal input layer — app-owned textarea overlay that separates keyboard
// capture from xterm.js, so xterm is a pure renderer.
//
// Architecture: docs/plan/terminal-ime-input-layer-separation.md
//
// Replaces useWkImeGuard.js. The problem that guard solved (WKWebView's
// keyCode 229 tagging of synthetic keystrokes, which xterm 5.5.0's IME
// fallback paths mishandle) is solved here by removing xterm from the
// keyboard path entirely: disableStdin prevents xterm from firing onData
// from its own textarea, and this app-owned textarea captures all input
// with the browser's own (correct) event pipeline.
//
// DIAGNOSTICS — same pull-based pattern as the old guard (record always,
// read on demand). window.__akiTermInput replaces window.__akiIme.

export function useTerminalInput(onData) {
  // onData: (text: string) => void — called when text is committed and
  // should be sent to the PTY. The caller wires this to sendRaw().
  //
  // Returns: { mount(rootEl), dispose(), focus(), blur(), textarea }
}
```

### 3.2 Event handling matrix

| Event | Action | Notes |
|-------|--------|-------|
| `keydown` (printable, no modifier) | `preventDefault()`, `onData(key)` | Single char, no IME involved |
| `keydown` (Backspace) | `preventDefault()`, `onData('\x7f')` | |
| `keydown` (Enter) | `preventDefault()`, `onData('\r')` | |
| `keydown` (Escape) | `preventDefault()`, `onData('\x1b')` | |
| `keydown` (Tab) | `preventDefault()`, `onData('\t')` | |
| `keydown` (ArrowUp/Down/Left/Right) | `preventDefault()`, `onData(CSI seq)` | Respects cursor key mode? No — app cursor mode is a terminal state. Use standard ANSI sequences; the PTY/shell handles mode. |
| `keydown` (Ctrl+letter) | `preventDefault()`, `onData(ctrl byte)` | Standard terminal Ctrl mapping |
| `keydown` (Meta/Cmd held) | Pass through | TerminalStack.vue handles these |
| `keydown` (Dead key) | Pass through | Browser needs to see it for composition |
| `keydown` (isComposing or keyCode 229, and composing) | Pass through | IME owns the textarea |
| `keydown` (keyCode 229, NOT composing) | `preventDefault()` | OpenKey synthetic key — wait for beforeinput |
| `compositionstart` | Mark `composing = true` | |
| `compositionupdate` | Nothing | Browser handles preedit display in textarea |
| `compositionend` | Read textarea value delta, `onData(text)`, clear value, `composing = false` | |
| `beforeinput` (insertText, not composing, no keydown claim) | `preventDefault()`, `onData(ev.data)` | Catches OpenKey multi-char payload |
| `beforeinput` (insertCompositionText) | Pass through | IME owns it |
| `input` (insertText, not composing, not yet claimed) | Read delta, `onData(text)`, clear value | Last-resort fallback |
| `paste` | `preventDefault()`, read clipboard, `onData(text)` | |
| `focus` | Record diagnostic | |
| `blur` | Record diagnostic | |

### 3.3 Key classification at keydown

```js
function classifyKey(ev) {
  // Modifier-only keys: pass through
  if (['Shift','Control','Alt','Meta','CapsLock','Fn','Hyper','Super','OS','Symbol','AltGraph'].includes(ev.key)) {
    return 'modifier'
  }
  // Meta/Cmd combinations: pass through for TerminalStack
  if (ev.metaKey) return 'meta'
  // Alt combinations (Option on Mac): pass through — may be composing
  if (ev.altKey && !ev.ctrlKey) return 'pass'
  // Dead keys: pass through for composition
  if (ev.key === 'Dead' || ev.key === 'Unidentified') return 'pass'
  // IME composing
  if (ev.isComposing) return 'composing'
  // Backspace/Enter/Escape/Tab: handle directly
  if (['Backspace','Enter','Escape','Tab'].includes(ev.key)) return 'control'
  // Arrow keys
  if (ev.key.startsWith('Arrow')) return 'arrow'
  // Home/End/PageUp/PageDown/Insert/Delete
  if (['Home','End','PageUp','PageDown','Insert','Delete'].includes(ev.key)) return 'nav'
  // Function keys
  if (/^F\d+$/.test(ev.key)) return 'function'
  // Ctrl held: handle directly (Ctrl+letter → control byte)
  if (ev.ctrlKey && !ev.altKey) return 'ctrl'
  // Printable: handle at keydown if single char, otherwise wait for beforeinput
  if (ev.key && ev.key.length === 1) return 'printable'
  // Multi-char key (OpenKey carrier) — wait for beforeinput
  return 'pass'
}
```

### 3.4 Edge cases

**Paste:** `paste` event on textarea → `preventDefault()`, read `ev.clipboardData.getData('text/plain')`, bracket-wrap, send via `onData()`.

**Selection:** Text selection inside the terminal must still work. The textarea does NOT intercept mouse events — only keyboard. xterm.js's built-in selection (mouse-based) is unaffected because `disableStdin` does not touch rendering or mouse handling.

**Copy:** `⌘C` without selection → should send SIGINT (Ctrl+C) to the PTY. With selection → should copy to clipboard. This is already handled by xterm.js's selection + copy handler (`document.execCommand('copy')` on the selection), which is independent of keyboard. We need to handle the `⌘C` keydown case: if xterm has a selection, pass through (let the browser copy); if no selection, send `\x03`.

**Scroll:** Unchanged — xterm.js viewport handles scroll events independently of stdin.

**Font zoom:** `⌘+`/`⌘-`/`⌘0` already captured by `TerminalStack.vue`'s window-level handler — never reaches the textarea.

**Focus management:** `TerminalView.vue` currently calls `term.focus()` after mount and on tab switch. This must now call `textarea.focus()` instead (on the overlay textarea). xterm's own textarea is unreachable with `disableStdin` (but still exists — we hide it via CSS).

### 3.5 CSS for xterm's textarea

```css
/* Hide xterm's internal textarea — keyboard input goes through our overlay */
.pty-terminal-mount :deep(.xterm-helper-textarea) {
  display: none !important;
}
```

This is simpler and less fragile than trying to blur/refocus it programmatically.

### 3.6 Escape hatch

```js
// In TerminalView.vue's onMounted:
const inputMode = localStorage.getItem('aki-input-mode')
if (inputMode === 'legacy') {
  // Old flow: xterm native keyboard + useWkImeGuard.js
  term = new Terminal({ disableStdin: false, ... })
  imeGuard = useWkImeGuard(term)
} else {
  // New flow (default): disableStdin + app-owned textarea
  term = new Terminal({ disableStdin: true, ... })
  termInput = useTerminalInput((text) => ptyApi.value?.sendRaw(text))
  termInput.mount(mountEl.value)
}
```

`localStorage['aki-input-mode'] = 'legacy'` + reopening the terminal tab reverts to the old guard. The flag is checked at mount time.

## 4. File changes

### 4.1 New file: `src/composables/useTerminalInput.js`
- Creates/manages the overlay textarea
- Full keyboard event handling (classification matrix §3.2)
- Diagnostic ring: `window.__akiTermInput` with `status()`, `tail(n)`, `dump()`, `debug(on)`, `clear()`, `help()`
- Export: `useTerminalInput(onData) → { mount(el), dispose(), focus(), blur() }`

### 4.2 Modified: `src/components/TerminalView.vue`
- Import `useTerminalInput` instead of `useWkImeGuard`
- `disableStdin: true` in Terminal constructor
- Wire overlay textarea lifecycle (mount after `term.open()`, dispose before `term.dispose()`)
- CSS: hide `.xterm-helper-textarea`
- Escape hatch: check `localStorage['aki-input-mode']` at mount
- `term.focus()` calls replaced with `termInput.focus()`
- Keep compose row, key row, font zoom, title handling — all unchanged

### 4.3 Deleted: `src/composables/useWkImeGuard.js`
- Removed from the codebase
- Import removed from TerminalView.vue

### 4.4 Modified: `docs/feat/in-app-terminal.md`
- Update §Vietnamese input: replace guard description with input layer separation
- Update `__akiIme` references to `__akiTermInput`

### 4.5 Modified: `docs/research/terminal-vietnamese-ime-root-cause-2.md`
- Add status line: "Superseded by docs/plan/terminal-ime-input-layer-separation.md"

## 5. What stays unchanged

- `usePtyTerminal.js` — no changes. `sendRaw()` is the same funnel.
- `TerminalStack.vue` — no changes. Window-level keydown handler for ⌘ shortcuts unchanged.
- `TerminalTabStrip.vue` — no changes.
- `terminalTabsStore.js` — no changes.
- `ptyBridge.js` — no changes.
- `src-tauri/src/pty.rs` — no changes.
- Key row, compose row, font zoom — all unchanged.
- CSS theme, fit handling, resize observer — all unchanged.

## 6. Risks and mitigations

| Risk | Mitigation |
|------|-----------|
| Browser IME behavior on hidden textarea differs from visible one | Textarea is not `display:none` — it's `opacity:0` with `caret-color:transparent`, positioned absolute over the mount area. Browser treats it as a real, visible, focused input. |
| `disableStdin` behavior changes in xterm.js upgrade | Escape hatch (`legacy` mode) provides fallback. Single point of failure (one option), easy to verify in changelog before upgrading. |
| App keyboard shortcuts (⌘C copy) conflict with terminal Ctrl+C | Selection-aware: if xterm has selected text, pass ⌘C through; otherwise send `\x03`. |
| Mac host loses Vietnamese typing during Phase 3 dev | Guard stays in tree until the new input layer is verified. Removed as the last commit. |
| True IME composition (macOS built-in Vietnamese) textarea cursor visible | `caret-color: transparent` on the textarea. The browser's IME preedit window appears near the cursor position even with transparent caret. |
| Multiple TerminalView instances (tabs) each have their own textarea | Each `useTerminalInput` instance creates its own textarea scoped to its mount element. `v-show` hidden tabs' textareas are not focusable. |

## 7. Implementation order

1. **Create `useTerminalInput.js`** — the full composable with all event handlers, diagnostic API, and classification matrix.
2. **Integrate in `TerminalView.vue`** — behind the `aki-input-mode` flag, defaulting to new mode. Wire the overlay.
3. **Add CSS for xterm textarea and overlay** — hide xterm's textarea, style the overlay.
4. **Test locally (browser mode)** — verify English typing, arrow keys, Ctrl+C, paste, basic IME.
5. **Remove `useWkImeGuard.js`** — after the new layer is confirmed on Mac with OpenKey.
6. **Update docs** — `docs/feat/in-app-terminal.md`, research doc status lines.
7. **Git commit** — single commit with all changes.

## 8. Verification checklist

- [ ] English typing: fast and slow, no dropped or doubled characters
- [ ] Backspace, Enter, Escape, Tab: correct behavior
- [ ] Ctrl+C (SIGINT): interrupts running process
- [ ] Ctrl+D (EOF): exits shell
- [ ] Arrow keys: navigate history and cursor
- [ ] Home/End: line navigation
- [ ] Paste (⌘V): text pasted correctly, bracketed
- [ ] Copy (⌘C with selection): copies to clipboard
- [ ] Vietnamese with OpenKey: `tieengs vieejt as` → correct output, fast and slow
- [ ] Vietnamese with macOS built-in IME: compose works, commit correct
- [ ] `localStorage['aki-input-mode']='legacy'` → old guard still works
- [ ] `window.__akiTermInput.status()` → diagnostic API functional
- [ ] Multiple tabs: typing in tab A doesn't affect tab B
- [ ] App shortcuts (⌘T, ⌘W, ⌘+, ⌘-): work while terminal focused
- [ ] Companion (phone) typing: unaffected (key row + compose row unchanged)
- [ ] Regression: font zoom, panel resize, tab switch, stack collapse
