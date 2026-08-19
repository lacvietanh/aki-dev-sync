# Terminal copy & selection — why nothing the app shipped could have fixed it

**Start time:** 2026-08-20

## Initial purpose

Owner report, 2026-08-20, two symptoms in the in-app terminal on macOS (Tauri/WKWebView), most visible against `claude` running over SSH:

1. Select text normally, press `⌘C` → **the clipboard is not touched at all**. This is why the owner reaches for Option-drag in the first place.
2. Hold `⌥` and drag to select under a mouse-mode TUI → the selection disappears the moment the mouse is released; roughly 2-3 attempts in 100 keep it.

Context at the time: app version 1.28.0. 1.23 shipped exactly one line for this area — `macOptionClickForcesSelection: true` (`src/components/TerminalView.vue:327`). A second investigation on 2026-08-19 recorded backlog item B10 (`docs/plan/backlog.md`), concluded *"not fixable in this app's code — reproduces identically in Terminal.app"*, proposed a mouse-reporting lock toggle, and **wrote no code**. The owner's question — "why does nothing change after it was supposedly fixed" — therefore has a trivial first half (nothing was built) and a real second half (was B10's verdict even right?).

## Strategy

Every previous round reasoned from the app's own source and from behavioural comparison with Terminal.app. Both were exhausted. This round reads the **installed runtime** instead — `node_modules/@xterm/xterm/lib/xterm.js` (5.5.0), which is the source of truth for what actually executes (`coding.A3`) — for the three mechanisms the symptoms touch: the copy path, the selection lifecycle, and the mouse-protocol lifecycle.

## Checklist

1. `grep` the bundle for the `copy` listener registration and `copyHandler`.
2. `grep` for every write of selection text into the textarea (the only bridge between xterm's own selection model and a native clipboard event).
3. `grep` `_keyDown` to check whether xterm cancels `⌘C` before the browser can act on it.
4. `grep` every caller of `clearSelection()` and `SelectionService.disable()`.
5. `grep` `set activeProtocol` and `bindMouse`'s `onProtocolChange` handler.
6. `grep` `_handleMouseUp` for the `altClickMovesCursor` branch.
7. Cross-read the app's own terminal surface: `TerminalView.vue`, `useTerminalTextDrain.js`, `dock/TerminalStack.vue`.

## Result

### F1 — the `⌘C` path is dead by construction in this webview, and it is the app's to own

xterm's only copy route is a **native `copy` DOM event**: `_initGlobal()` registers `addDisposableDomListener(this.element, "copy", e => this.hasSelection() && copyHandler(e, this._selectionService))`, and `copyHandler` does `e.clipboardData.setData("text/plain", t.selectionText)`. That event only exists if the browser decides a copy is happening.

The only place xterm ever puts selection text where a browser could copy it from is `onLinuxMouseSelection` — `this.textarea.value = e; this.textarea.focus(); this.textarea.select()` — which is the **Linux primary-selection emulation and fires nowhere else**. On macOS the hidden textarea stays empty and the DOM selection stays collapsed (the highlight is painted, not selected), so from WebKit's point of view `⌘C` has no content to copy. Result: no `copy` event, no clipboard write, silence. This matches the owner's report exactly — not "copies the wrong thing", but "the clipboard is not touched".

xterm is **not** in the way: in `_keyDown`, `evaluateKeyboardEvent` returns no `key` for `⌘`+letter on macOS (only `⌘A` is special-cased to SELECT_ALL), so the function returns without calling `cancel()`. No `preventDefault`, no data sent to the PTY. The key is free for the app to claim.

**Verification:** code read (bundle, 5.5.0), plus the owner's direct observation that the clipboard is untouched. **Not verified:** *why* WebKit raises no event (collapsed selection vs. empty editable). The fix does not depend on the answer — an app-owned handler reads `term.getSelection()` and writes through `src/utils/clipboard.js`'s `copyText()`, the same path every COPY button in this app already uses successfully in this same webview.

### F2 — the selection is wiped by xterm itself, and the most likely trigger is a redundant mouse-mode re-arm

Three separate mechanisms in the bundle clear a live selection. All three are reachable while a mouse-mode TUI is running:

| Mechanism | Code | Fires when |
|---|---|---|
| Protocol re-arm | `set activeProtocol(e){ … this._onProtocolChange.fire(…) }` → `bindMouse`'s handler → `this._selectionService.disable()` → `clearSelection()` | **every assignment** to `activeProtocol`, including re-arming the protocol already active. A TUI that re-emits `DECSET 1000/1002/1003` on redraw erases the selection each time |
| Any user input | `this._coreService.onUserInput(() => { this.hasSelection && this.clearSelection() })` | any keystroke, and any **forwarded mouse event** — under `DECSET 1003` a bare pointer move over the terminal is user input |
| Alt-click-moves-cursor | `_handleMouseUp`: `selectionText.length <= 1 && t < 500 && e.altKey && altClickMovesCursor` (default **on**) | an Option-drag that ended up selecting ≤ 1 character — sends a cursor-move sequence, which is itself user input |

The first is the strongest candidate for "gone the instant I release": it needs no pointer movement, no keystroke, and no cooperation from the user at all — only the TUI repainting.

**Verification:** the mechanisms are verified by code read. **Not verified:** which one actually fires under `claude`. That cannot be settled by reading, so the plan ships a counter (`window.__akiTermCopy.status()`) that records suppressed re-arms and protocol changes, readable on demand — one hand-off instead of a diagnostic round trip (method precedent: `docs/research/terminal-vietnamese-ime-root-cause-2.md`, pull-based diagnostics).

### F3 — B10's verdict does not survive F1

B10 reasoned: the symptom reproduces in Terminal.app, which does not use xterm.js, therefore the cause is outside this app and unfixable here. A shared symptom is not a shared mechanism — Terminal.app drops a selection when the text under it is repainted, which is a different cause with the same appearance. And regardless of what Terminal.app does, F1 is a defect **entirely inside this app's control**: `⌘C` has never worked in this terminal, on any target, mouse-mode or not. B10's proposed mouse-reporting toggle also addresses only the selection half, at the cost of breaking scroll and click inside the TUI, and needs UI in an app under an Extreme-Narrow rule. It is superseded, not scheduled.

### F4 — separate defect found while reading (owner-reported the same day)

Terminal tab-cycling (`⌘⇧[` / `⌘⇧]`) works once, then stops until the terminal is clicked again. `dock/TerminalStack.vue:150` gates every terminal shortcut on `hasTerminalFocus`, a flag maintained only by `@focusin`/`@focusout` on the mount wrapper (`:57`). On a tab switch, `TerminalView.vue`'s `watch(() => props.active)` calls `term.focus()` at the default `pre` flush — before `v-show` makes the incoming tab's root visible — so the focus call lands on a hidden element and is dropped, while the outgoing tab's textarea blurs and sets the flag `false`. Nothing sets it back. Every subsequent shortcut is refused. **Verified by code read**; the gate is the only guard on that path.

## Decision

**Action** → `docs/plan/terminal-copy-selection.md` (three items: app-owned copy path, protocol-re-arm suppression + counter, focus-gate fix).

**Cross-references**
- `docs/plan/backlog.md` B10 — superseded by this doc; entry updated to point here.
- `docs/plan/done/remaining-1.23.md` §4 (TERM-COPY) and `docs/plan/done/handtest-1.23.md` §54 — the 1.23 round: correctly ruled out the app's global `user-select: none` and `TerminalStack.vue`'s keydown handler, correctly added `macOptionClickForcesSelection`, and left the hand-test open. Its "fail means" condition is now answered: **fail**, for the reason in F1.
- `docs/arch/terminal-stack.md` § Keyboard input — states "keys stay entirely inside xterm's pipeline"; `⌘C` becomes the one documented exception once the plan ships.
