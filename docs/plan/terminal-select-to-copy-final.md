# Plan: Terminal Select-to-Copy, Mouse Bypass & OSC 52 Final Integration

**Status: code written 2026-09-08, unverified on real hardware** — `src/composables/useTerminalCopy.js` now implements drag interception, copy-on-select, and OSC 52; `src/components/TerminalView.vue` needed no change (`useTerminalCopy(term)` was already wired immediately after `term.open()` and disposed in `onBeforeUnmount`, both from the earlier ⌘C work). It shares no file with the Remote Control ingress work in flight.

**Implementation deviates from the sketch in §2–§3 below** on the actual mechanism, discovered by reading `node_modules/@xterm/xterm/lib/xterm.js` (this version's real internals, not guessed): xterm decides at the *original* mousedown — synchronously, before any drag distance is known — whether to forward mouse-report escape codes to the app or start a selection, gated by `_selectionService.shouldForceSelection(event)` (natively: `event.altKey && macOptionClickForcesSelection`). A capture-phase listener added on `term.element` *after* `term.open()` cannot preempt xterm's own listeners already registered on that same node (same-node listeners run in registration order regardless of the capture flag) — only a capture listener on an ancestor (here, `document`) fires first. The shipped design:
- `document`-level capture listeners (not `term.element`) own mousedown/mousemove/mouseup so they always run before xterm's.
- Every real mousedown is swallowed unconditionally (undecided at that point); `term.focus()` is called manually since xterm's own handler — which normally does this first — never runs.
- Crossing the 4px threshold replays a *synthetic* `mousedown` (at the true start point) + the current `mousemove` at the drag point, with `_selectionService.shouldForceSelection` permanently patched to always return `true` (so the synthetic replay is treated as a forced/Option-equivalent selection). From there, real events are left alone — xterm's own SelectionService (now actively tracking, exactly as an Option-drag works today) drives the rest of the drag itself, rather than this composable re-deriving buffer row/column math by hand.
- Below the threshold (a click), the swallowed mousedown+mouseup pair is replayed to the app via `coreMouseService.triggerMouseEvent()`, using `_mouseService.getMouseReportCoords()` for the col/row — the same private-but-stable path xterm's own mouse-report encoder uses internally. This is a no-op when no mouse-tracking protocol is active (the encoder's own `restrict()` gate).

This uses `term._core`, `_selectionService`, `_mouseService`, `coreMouseService` — all underscore-prefixed internals, consistent with the existing `suppressRedundantRearm()` in the same file (already relies on `term._core.coreMouseService`). None of this is exercised by `npm run build` (JS, no typecheck) — it is unverifiable without a live WKWebView, xterm, and a mouse-mode TUI, which is exactly the runtime gap the test matrix in §4 exists to close.

Root cause & architectural research: `docs/research/terminal-select-to-copy-deep-dive.md`.
Supersedes the manual-only `⌘C` limitations in `docs/plan/done/terminal-copy-selection.md`.

## 1. Executive Summary & Problem Breakdown

In the in-app terminal (Aki-Dev-Sync, Tauri v2 / WKWebView / macOS), running interactive TUIs (`claude` CLI, `htop`, `tmux`, `vim`) over SSH forces users to hold `Option` (⌥) to select text. Even then, selection highlights disappear on mouse release, and normal left-click dragging forwards raw mouse events to remote shells without touching the macOS clipboard.

| # | Component / Symptom | Root Cause | Target Solution |
|---|---|---|---|
| 1 | Normal left-drag sends mouse escape codes instead of selecting text | `DECSET 1000/1002/1003` enables `coreMouseService`, causing xterm to disable `SelectionService` and cancel DOM `mousedown` unless `Option` is held | Capture-phase drag threshold detector (> 4px) intercepts intentional selection drags and routes them to xterm selection while letting clicks pass to TUI |
| 2 | Selected text not copied to macOS clipboard automatically | WKWebView has no X11-style primary buffer; xterm's native `copy` DOM event never fires | Copy-on-Select in `useTerminalCopy.js`: on `mouseup` after intentional drag, immediately write `term.getSelection()` via `copyText()` |
| 3 | Highlight wiped immediately by TUI redraw or mouse move | TUI output and xterm `onUserInput` invoke `clearSelection()` upon any subsequent input/redraw | Instant copy on mouse release ensures clipboard is populated before wipe occurs; keep selection stash for manual `⌘C` |
| 4 | Remote processes cannot copy to local Mac clipboard | Remote SSH environment cannot directly access macOS pasteboard | Register native OSC 52 handler (`term.parser.registerOscHandler(52)`) to decode base64 payloads and write via `copyText()` |

---

## 2. Solution Architecture

```
User Mouse Action on Terminal
 │
 ├── Mousedown ────────► Record start (x, y, timestamp)
 │
 ├── Mousemove ────────► Calculate Euclidean distance
 │     │
 │     ├── Distance <= 4px (Click Gesture) ──► Let xterm forward mouse event to TUI
 │     │
 │     └── Distance > 4px (Drag Selection) ──► Intercept event, suppress TUI reporting,
 │                                             engage xterm selection highlight
 └── Mouseup
       │
       ├── Was Drag Selection?
       │     │
       │     ├── Text Selected (> 0 chars) ──► copyText() to macOS clipboard + update stash
       │     │                                 + trigger visual feedback / counter
       │     └── Empty Selection ────────────► No-op (protect clipboard)
       │
       └── Was Simple Click? ────────────────► Forward button release to TUI
```

### 2.1 Drag Interception & Mouse Bypass (Smart Selection)
- Attach capture-phase `pointerdown`/`mousedown` listener on `term.element`.
- Track `(startX, startY, isDragging, isSelecting)`.
- On `mousemove`:
  - If mouse button 0 is held and `Math.hypot(ev.clientX - startX, ev.clientY - startY) > 4`:
    - Mark `isSelecting = true`.
    - Stop event propagation (`ev.stopImmediatePropagation()`) to prevent xterm from streaming `\x1b[<0;col;rowM` to the remote shell.
    - Drive text selection using xterm's coordinate mapping (`term._core._mouseService.getCoords`) and `term.select()` / `SelectionService`.
- If the user holds `Option` (⌥), xterm's built-in `macOptionClickForcesSelection` continues to work unconditionally.

### 2.2 Copy-on-Select with Accidental Drag Guard
- In `useTerminalCopy.js`, handle `mouseup` on `document`:
  - If `isSelecting` was active:
    - Retrieve selected text: `const text = term.getSelection() || stashed`.
    - Guard: If `text && text.trim().length > 0`:
      - Execute `await copyText(text)`.
      - Update persistent `stashed` buffer.
      - Increment copy counter for diagnostics (`__akiTermCopy.status().copiesPerformed`).
  - Reset drag tracking state.
- Guard guarantee: Pure clicks (< 4px displacement, < 300ms) pass through untouched, guaranteeing that clicking buttons in Claude Code or navigating menus in htop never clobbers existing clipboard contents.

### 2.3 Visual Feedback & Selection Persistence
- Provide instant visual feedback when text is auto-copied (e.g. status counter increment, subtle highlight confirmation).
- Retain the selection stash in memory so manual `⌘C` remains fully supported as an explicit copy chord.
- Keep redundant protocol re-arm suppression (`suppressRedundantRearm`) to prevent spurious `clearSelection()` calls during TUI redraw cycles.

### 2.4 Native OSC 52 Escape Sequence Handler
- Register OSC 52 handler on xterm instance:
  ```js
  term.parser.registerOscHandler(52, (data) => {
    // Format: [clipboard-target];[base64-payload]
    const idx = data.indexOf(';')
    const payload = idx >= 0 ? data.slice(idx + 1) : data
    if (!payload || payload === '?') return false
    try {
      const decoded = atob(payload)
      copyText(decoded)
      return true
    } catch (err) {
      console.error('[useTerminalCopy] OSC 52 base64 decode failed', err)
      return false
    }
  })
  ```
- Allows remote tools (`tmux`, `neovim`, `yazi`, custom scripts) to copy text directly to the local macOS clipboard over SSH.

---

## 3. Step-by-Step Implementation Breakdown

### Target 1: Extend `src/composables/useTerminalCopy.js`
**Exact file:** `src/composables/useTerminalCopy.js` (lines 1-92)
1. Add diagnostic counters: `copiesOnSelect`, `osc52Copies`.
2. Implement `attachMouseDragSelection(term, stashedGetter, onCopy)`:
   - Listen to `mousedown` on `term.element` (capture phase).
   - Listen to `mousemove` and `mouseup` on `document` (capture phase).
   - Implement 4px drag threshold detection.
   - On valid drag `mouseup`: read `term.getSelection()`, call `copyText()`, update `stashed`.
3. Register OSC 52 handler via `term.parser.registerOscHandler(52, ...)` and register its disposable in the return bundle.
4. Clean up all DOM event listeners and OSC disposables inside `dispose()`.

### Target 2: Coordinate with `src/components/TerminalView.vue`
**Exact file:** `src/components/TerminalView.vue` (lines 330-365)
1. Ensure `useTerminalCopy(term)` is initialized immediately after `term.open(mountEl.value)`.
2. Confirm `termCopy.dispose()` is invoked in `onBeforeUnmount`.

---

## 4. Verification and Test Checklist

### Static Verification
- [x] No new dependencies added — `git diff package.json` empty.
- [x] Uses existing `copyText()` in `src/utils/clipboard.js` as single source of truth for clipboard writes — `grep -n navigator.clipboard src/composables/useTerminalCopy.js` matches nothing, only `clipboard.js` itself calls it.
- [x] Strict null-checks for xterm internals (`term._core?.coreMouseService`, `_selectionService`/`_mouseService` guarded before use) and try/catch around base64 decode.
- [x] Diagnostics exposed on `window.__akiTermCopy.status()`: `copiesOnSelect`, `osc52Copies` added alongside the existing counters.
- [x] `npm run build` green (11.1s), no new warnings attributable to this file.

### Runtime Test Matrix (macOS + SSH)
- [ ] **Test 1 (SSH Claude Code — Normal Drag)**: Connect to remote host over SSH running `claude`. Drag mouse over terminal text without pressing `Option`. Verify selection highlights during drag, and on mouseup text is automatically in macOS clipboard (test by pressing `⌘V` in text editor).
- [ ] **Test 2 (SSH Claude Code — Click Passthrough)**: Click interactive elements in Claude Code without dragging. Verify click is sent to TUI and existing macOS clipboard contents are NOT altered.
- [ ] **Test 3 (Option-Drag Legacy Muscle Memory)**: Hold `Option` and drag. Verify selection and auto-copy work as expected.
- [ ] **Test 4 (Explicit ⌘C Chord)**: Select text and press `⌘C`. Verify stashed text is copied to clipboard.
- [ ] **Test 5 (OSC 52 Remote Copy)**: Run `printf "\033]52;c;%s\a" "$(echo -n 'hello from osc52' | base64)"` in remote shell. Verify text is copied to local macOS clipboard.
- [ ] **Test 6 (Local Zsh Shell)**: Test drag selection and Copy-on-Select in standard local terminal tab.
