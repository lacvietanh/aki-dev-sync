# Terminal input surface — font declarations, Shift+Enter, sticky modifiers

Covers three of the four in-app-terminal input-layer defects from the jul30 batch: **#2** (font declarations), **#8** (Shift+Enter in the compose input), and **#3** (sticky modifier toggles). **#13** (Vietnamese IME) is researched and decided elsewhere — `docs/research/terminal-vietnamese-ime-root-cause-4.md`, the head of that chain. §5 here is the interface to that closure: what it changes in this doc's sections, and the revised order the three items must ship in.

Feature doc to sync on implementation: `docs/feat/in-app-terminal.md` — the user-facing description of the in-app terminal. Architecture: `docs/arch/terminal-stack.md` — how the terminal stack, tabs and PTY wiring fit together. Both listed in §6, neither edited here.

---

## 1. Three framing corrections — read these before deciding anything

A reader who skips this section will fix the wrong things. Each is verified by direct source read, not inferred.

**C-1. The Mac window has no key row at all.** The Esc/Tab/Shift/Ctrl/arrow button row exists only where `showKeyRow` is true, and `showKeyRow = !isHost` (`src/composables/usePtyTerminal.js:480` — the composable that owns every host/companion branch), consumed by `v-if="ptyApi?.showKeyRow"` (`src/components/TerminalView.vue:29` — the xterm mount component). Every button #3 describes is a **companion-browser (phone) surface**. This fact *resolves* which surface the report is about rather than raising a question about it — the buttons exist nowhere else, so that is where the report came from. It also bounds the fix: §4 changes nothing a physical Mac keyboard touches (§4.5).

**C-2. Sticky modifiers already ship.** `KEY_ROW` already carries `{ label: 'Shift', arms: 'shift' }` and `{ label: 'Ctrl', arms: 'ctrl' }`, with `is-armed` cyan styling bound to `ctrlArmed` / `shiftArmed` (`TerminalView.vue:36-39, 180-190`). #3 is therefore "the state machine is incomplete", **not** "the feature is missing". A plan written as though sticky modifiers need building would duplicate working code and re-introduce the same two-latch split that is the actual defect.

**C-3. The byte encodings are already correct in-tree.** Shift+Tab is `\x1b[Z` (CSI Z, backtab) at `TerminalView.vue:182`; Shift+arrows are `\x1b[1;2{A..D}` (CSI modifier parameter 2 = Shift) at L185-188; Ctrl+K is `\x0b`, produced by `toCtrlByte`'s `charCode − 64` mapping (`usePtyTerminal.js:416-421`) — `'K'` is 75, and 75 − 64 = 11 = `0x0b`. Nothing in #3 is an encoding bug. Do not "fix" these sequences.

---

## 2. #2 — font declarations

The owner's instruction was to remove every custom font declaration in the terminal so it renders like the plain task-note input. Executing that literally reverts a feature he asked for two days earlier. §2.1 and §2.2 split the instruction along the axis that resolves the contradiction; §2.3 records the contradiction explicitly rather than deciding it silently.

### 2.1 Font FAMILY — removable, and an SSoT breach in its own right

The same stack — `"SFMono-Regular", Consolas, "Liberation Mono", Menlo, Courier, monospace` — is declared **verbatim three times**:

| Site | What it styles | Ruling |
|---|---|---|
| `TerminalView.vue:323` | the xterm `Terminal` option `fontFamily` — the terminal canvas itself | see §2.2's Courier New consequence; do not simply delete |
| `TerminalView.vue:463` | `.pty-key-label` — the text on key-row buttons ("Esc", "Tab", "Ctrl") | **remove**; these are labels with no column-alignment requirement |
| `TerminalView.vue:492` | `.pty-compose-input` — the compose row's text field | **remove**; this is the element the owner's complaint is about |

Three verbatim copies of one value is a Single-Source-of-Truth breach (`RULE-design-core.md` Law 1), independent of whether the value itself is right. That framing matters for scope: this is a **declaration-hygiene fix**, not a rendering bug hunt. Nothing here claims to know what the owner sees on screen; it removes overrides he did not ask for and cannot have wanted on an input he compared unfavourably to a working one.

**The good case the owner himself named** is `src/components/tasks/NotesField.vue:77-91` — the project task-note field. Its full spec: `font-family: inherit`, `font-size: 12px`, `line-height: 1.5`, `field-sizing: content` (native auto-grow), `background: transparent`, `border: none`, `resize: none`, `padding: 0`. That is the target shape for `.pty-compose-input`, and it is also — see §3 — what makes #8 possible at all. **#2 and #8 are one edit to one element; they must not be planned or executed apart.**

Also drop the compose input's `font-size: 13px` (`TerminalView.vue:491`) to the reference's `12px`, so the two inputs agree.

### 2.2 Font SIZE / SCALE — survives untouched

**`src/composables/useTerminalFont.js` (the per-device zoom scale, localStorage-backed) and the ⌘+ / ⌘− / ⌘0 handlers in `src/components/dock/TerminalStack.vue:163-175` are OUT OF SCOPE and must not be touched.** So are the key row's `−` / `%` / `+` buttons (`TerminalView.vue:54-69`), which are a phone's only zoom control.

That feature shipped in commit `8cc2669` ("feat(terminal): font zoom, and native per-device text size on a companion"), two commits before this batch was filed, and it is **entirely a `fontSize` feature**: `terminalFontScale` × `BASE_FONT_SIZE` (=12) applied in `doFit` (`TerminalView.vue:288`, `:272`). It touches no `fontFamily` anywhere. The owner's wording — "đừng cố custom font … loại bỏ hết" — is typeface language, and reading it as "delete the zoom feature two commits after shipping it" is both unsupported by the words and the more destructive of the two readings.

`fontSize: BASE_FONT_SIZE` (`TerminalView.vue:324`) and `lineHeight: 1.4` (`:325`) also stay: the first is the input to the zoom calculation, the second is grid geometry rather than typeface.

### 2.3 The Courier New consequence — why the xterm option is not simply deleted

xterm.js's `fontFamily` option has a **built-in default of `'courier-new, courier, monospace'`**. Deleting `TerminalView.vue:323` therefore does not yield "the normal font"; it yields Courier New, which on macOS is worse than the current stack on both cell metrics and Vietnamese diacritic coverage. On a canvas/WebGL-rendered terminal there is no "no font" state — xterm must measure a cell against *some* family.

The defensible reading of "remove the custom declaration" on the canvas is therefore: **collapse the four-name stack to the one Apple system token that means "the system monospace"** — `ui-monospace` (fallback `Menlo`) — declared once, in one place. Nothing bespoke remains, the SSoT breach closes, and no rendering regresses.

**ASSUMPTION** — that xterm 5.x's `fontFamily` default is the Courier New string above. It cannot be confirmed from this box: `node_modules/@xterm` **is not installed here** (verified — `node_modules/` holds 95 packages and `@xterm` is not among them). Settling command, on a machine with a full install: `grep -rn "courier" node_modules/@xterm/xterm/src/common/services/OptionsService.ts`.

### 2.4 What #2 does NOT claim

It does not claim to have identified what the owner sees as "lỗi font". Two candidate causes were considered and neither is verified: the mono override on the compose input (addressed by §2.1), and Vietnamese combining marks rendering badly when OpenKey is configured for decomposed Unicode ("Unicode tổ hợp"), which is a glyph-composition issue that **no font declaration change will fix**. If the symptom survives §2.1, the second candidate is where to look next; that is a Mac observation, listed in §5.

---

## 3. #8 — Shift+Enter in the compose input

### 3.1 The blocker is the element type, not the handler

**State this first, because it is what makes #8 not a handler fix.** The compose row is `<input type="text">` (`TerminalView.vue:79-90`). A single-line input has **no representation for a newline character at all** — `\n` cannot exist in its value. No keydown handler, no modifier check, and no escape-sequence choice can make Shift+Enter insert a newline into it. The element must become a `<textarea>`.

Secondary, and only reachable once the first is fixed: `@keydown.enter="onComposeKeydown"` (`TerminalView.vue:89`) uses Vue's `.enter` key modifier **without `.exact`**, so it matches regardless of modifier state, and `onComposeKeydown` (L159-162) only bails on `isComposing` / `keyCode 229`. Shift+Enter therefore submits today.

### 3.2 Buffer-newline is not wire-newline — where a naive fix goes wrong

The obvious fix is: make it a textarea, let Shift+Enter insert a literal `\n`, keep `sendRaw(text + '\r')` (`TerminalView.vue:167`). **That is wrong, and the failure is worse than the bug.**

A `0x0a` sitting mid-buffer is delivered to the PTY as an ordinary byte, and a readline-style shell reads it as *accept-line*. A two-line compose therefore submits line 1 and strands line 2 at the prompt as a stray command — with a shell, that stray line is *executed*. The newline the user typed into a text box and the newline that travels down a PTY are different things and must be translated at the send boundary, not passed through.

### 3.3 Shape

- `.pty-compose-input` becomes a `<textarea>` carrying the `NotesField.vue:77-91` spec from §2.1 (this is the same edit as #2 — one change, not two).
- `@keydown.enter.exact` submits. Shift+Enter is then simply **not handled** and falls through to the textarea's native newline insertion — which is also the only form a real IME will not fight, and the reason not to intercept it.
- Single-line sends stay **bit-identical to today**: buffer with no newline → `sendRaw(text + '\r')`, unchanged.
- Multi-line sends need a wire form. See §3.4 — this is the one open decision in #8.

### 3.4 The multi-line wire form — conditioned, not unconditional

Recommended: wrap a multi-line buffer in **bracketed paste** — `\x1b[200~` … `\x1b[201~` — then `\r`. It is the only form *specified* to deliver embedded newlines as literal text rather than as accept-line, and it is how pasting multi-line text into Claude Code and agy already works.

**But it must be conditioned on the receiving program having bracketed-paste mode enabled.** Sending `\x1b[200~` to a program with the mode off lands the escape bytes as literal garbage at the prompt — a worse failure than the one being fixed, and one the user cannot easily undo. So: read the terminal's current bracketed-paste state and wrap **only** when it is on; when it is off, fall back (send the buffer's lines joined by `\r`, i.e. as separate submitted commands, which is at least what the user typed rather than corrupt bytes).

**ASSUMPTION — the xterm 5.x API surface for reading that mode is unverified, and confirming it is a PREREQUISITE of this section, not a detail.** `@xterm/xterm` is not installed on this box (§2.3), so the typings cannot be read. Settling command, after `npm install` on a machine with a full install: `grep -rn "bracketedPaste\|BracketedPaste" node_modules/@xterm/xterm/typings/xterm.d.ts node_modules/@xterm/xterm/src/common/InputHandler.ts`. If no public accessor exists, §3.4's condition cannot be implemented as written and the decision reopens — do not ship an unconditional wrap in its place.

The rejected alternative is `\x1b\r` (ESC CR / Meta-Enter), which Claude Code's `/terminal-setup` binds in iTerm2 and VS Code. It is plausible for the agent CLIs this terminal exists to run, but it is **ASSUMPTION** for a bare zsh prompt (it depends on `self-insert-unmeta` being bound, which is not a default), and it has no specification behind it the way bracketed paste does.

---

## 4. #3 — sticky modifier toggles

### 4.0 Scope — settled, not contingent

The key row renders **only on a companion** (C-1), therefore the report is about the companion. That is an inference the `showKeyRow = !isHost` finding already supplied; it should have resolved the question rather than raised it, and an earlier draft of this doc wrongly hedged it as a branch. **There is no Mac/physical-key branch. This section is unconditional.**

The requirement, fully specified by the owner and needing no further input:

1. **Shift and Ctrl are toggles** — tap to arm, tap again to disarm; the armed state persists until it is consumed by a keystroke or explicitly cleared.
2. **Armed-versus-not is visible.** A latch the user cannot see is a latch the user cannot trust. `is-armed` styling already exists, so the work is making the *state* correct and consistent, not inventing an affordance — see §4.4 for what the existing treatment needs.
3. **The combos must reach the shell**: Shift+Tab, Ctrl+K, and the general case "arm a modifier, then press any key". §4.2's funnel delivers this; it is decided, not proposed.

### 4.1 Root cause — flow shape, not a missing branch

There is no single funnel from "a key was pressed" to "bytes go to the PTY". There are **two latches that cannot see each other**:

- `shiftArmed` is consumed **only** in `fireKey` (`TerminalView.vue:199-219` — the key-row button handler);
- `ctrlArmed` is consumed **only** in `term.onData` (`usePtyTerminal.js:427-453` — the real-keystroke handler).

Six consequences follow, each traced to a line rather than inferred:

1. **Ctrl + any key-row key is unreachable.** `fireKey` never reads `ctrlArmed`; for a non-`arms` key it sends `k.seq` raw. Ctrl+Left / Ctrl+Right (word jump), Ctrl+Enter and Ctrl+Esc cannot be produced on a phone at all.
2. **An armed Ctrl leaks onto an unrelated later keystroke.** Because of (1), `fireKey` also never *clears* it — so arming Ctrl and then tapping an arrow leaves `ctrlArmed` true, and the **next soft-keyboard letter is silently turned into a control byte**. This is the one that corrupts bytes into a live shell: the user typed a letter and the shell received `0x03`/`0x04`/`0x1a`. It is also the most likely literal referent of "hành vi không đúng", and it is the reason #3 ranks highest on severity of the three defects in this doc.
3. **Sticky Shift is invisible to real keystrokes.** `shiftArmed` is read only in `fireKey`, so arming Shift and typing a letter on the soft keyboard yields lowercase while the button stays lit — the indicator states something that is not true.
4. **Sticky Shift is silently dropped on keys with no `shiftSeq`.** `fireKey:216-217` disarms unconditionally and sends the plain `seq`. Enter and Esc have no `shiftSeq`, so **Shift+Enter from the key row sends a bare `\r` today** — #8's problem appearing in a second place, which is why §4.4 requires one wire form shared by both rows.
5. **An armed Ctrl silently eats a multi-character chunk.** `usePtyTerminal.js:445-451`: if armed and `chunk.length !== 1`, it disarms and forwards the chunk unmodified — the Ctrl is lost with no feedback. This is reachable from the IME path: `src/composables/useWkImeGuard.js` (the WKWebView keystroke guard) emits through `term.input(str, true)` at L222/227/259/277, i.e. straight into `onData`, so an OpenKey multi-character carrier arriving while Ctrl is armed hits exactly this branch. Already recorded as a known limit in `docs/research/terminal-vietnamese-ime-root-cause-2.md`; it is the concrete coupling between #3 and WS-D.
6. **The compose input can never send a control byte.** Its keystrokes go to `v-model`, never to `term.onData`. Since `TerminalView.vue:172` refocuses the compose input after every Send, that is the *default* focus on a phone — so a user watching a runaway process has no route to Ctrl+C. **In scope and funded** — see §4.3: under requirement 1 a latch that dies in the box the user types in is not a working feature.

### 4.2 Fix — one funnel, not more branches

Reshape rather than guard (`METHOD-flow-audit.md` §B6, `RULE-design-core.md` Law 8):

- one latch object, `pending = { ctrl: false, shift: false }`, extensible to `alt` without a new mechanism;
- one emitter, `emitKey({ seq, shiftSeq, char })` in `usePtyTerminal.js`, which is the **only** thing allowed to turn a key press into bytes: it applies shift (`shiftSeq ?? seq`), then ctrl (`toCtrlByte(char)`), clears the latch after emission, and calls `sendRaw` exactly once;
- every source calls it — the key-row buttons, `term.onData`, and any future injected-command path;
- the `arms` buttons become **real toggles** (tap to arm, tap again to disarm), which is what the owner asked for and what a one-shot arm does not give;
- `is-armed` then reflects one truth instead of two half-truths.

**This deletes consequences 1–4 by construction rather than by handling them** — there is no longer a code path where one latch is unread or uncleared, because there is only one place that reads or clears anything. That is the test of the fix: if the diff adds a branch per symptom, it is the wrong shape.

Consequence 5 needs one explicit decision, which construction does not make for us: **preserve** the armed Ctrl across a multi-character chunk rather than silently clearing it, so the next single character still receives it.

**Toggle semantics and latch lifetime**, per requirement 1:

- tapping an `arms` button when that modifier is **not** latched arms it; tapping it again disarms it. No timeout, no auto-expiry — the state persists until a keystroke consumes it or the user taps it off.
- the latch is **per tab**, and already is: `ctrlArmed` / `shiftArmed` are created inside `usePtyTerminal` (`usePtyTerminal.js:146, 154`), one instance per mounted `TerminalView`, i.e. one per tab. Switching tabs therefore cannot carry a latch across, and nothing extra is needed to make that true.

### 4.3 The latch spans the compose input — decided

Consequence 6 is not an edge case: `TerminalView.vue:172` refocuses the compose input after **every** Send, so on a phone that box holds focus by default. A latched Ctrl that stops working the moment focus is where the user types is not a working feature under requirement 1, and the user's escape hatch from a runaway process is exactly Ctrl+C.

**Decision: a latched modifier applies wherever the user's next keystroke lands, including the compose input.** This is how OS-level sticky-keys behaves — a latch is a statement about the *next key*, not about one widget — and it is the only reading consistent with the focus model already in the file: the key-row buttons carry `@mousedown.prevent` / `@touchstart.prevent` precisely so tapping them does **not** steal focus (`TerminalView.vue:22-28`). Tapping Ctrl while the compose input is focused therefore leaves focus in the compose input by design. Option "move focus to xterm on arm" is rejected for fighting that design, and "add a dedicated Ctrl+C button" is rejected as a new control under Extreme Narrow when the existing latch can simply be made to work.

Implementation is one keydown handler on the compose textarea that early-returns when nothing is latched — **zero behaviour change in the common case, and no new DOM element**. Its rules, each with the reason it exists:

- **Ctrl only.** A latched Shift is ignored in the compose input and **preserved**, not consumed. Shift on a text field is already the soft keyboard's own job, and intercepting it would swallow ordinary capitalisation — a far worse failure than the gap being closed.
- **Single printable characters only** (`ev.key.length === 1`). This matches `toCtrlByte`'s domain (`usePtyTerminal.js:416-421`) and `wireInput`'s existing `chunk.length === 1` test. Enter, Backspace and arrows fall through to normal textarea behaviour and the latch is preserved.
- **Never while composing.** Bail on `ev.isComposing || ev.keyCode === 229`, the same guard `onComposeKeydown` already carries (`TerminalView.vue:160`). An armed latch must never eat an IME composition keystroke — that would reintroduce the exact class of bug the compose row exists to remove.
- On a match: `preventDefault()` so the character does not enter the buffer, emit through the same `emitKey` funnel, clear the latch.

### 4.4 Visible feedback — what the existing treatment needs

Requirement 2 is already half-built: `.pty-key.is-armed` inverts to a solid cyan fill (`TerminalView.vue:456-460` — `color: var(--bg-primary); background: var(--accent-cyan); border-color: var(--accent-cyan)`). **Keep it.** A solid fill inversion is the strongest single-property state signal available and reusing the existing accent token is what `RULE-ui-pattern.md` §A2 asks for; inventing a second affordance would also breach Extreme Narrow. The contrast is not the weak point. Three things around it are:

- **The binding is the real defect.** Today `is-armed` is bound to two independent refs (`TerminalView.vue:36-39`), which is why the indicator can be truthful about one modifier and lying about the other (consequence 3). Under §4.2's single latch it becomes one truth, and the feedback problem largely dissolves with the state problem — which is why this is a state fix, not a design fix.
- **A stuck `:hover` can read as armed.** `.pty-key:hover` sets `border-color: var(--accent-cyan)` (`TerminalView.vue:451-454`). On iOS Safari a `:hover` state persists on the last-tapped element until something else is tapped, so an **un-armed** button can sit displaying a cyan border — a false positive on the exact signal the armed state uses. Fix: scope the hover rule in `@media (hover: hover)` so it never applies on touch. Cheap, no new value, no new element.
- **Touch target, noted not funded.** `.pty-key` is `padding: 4px 8px` at `font-size: 10px` (`TerminalView.vue:439-449`), roughly 24px tall against Apple HIG's 44pt minimum. That is a legibility-of-*target* issue rather than of *state*, it applies to all nine buttons equally, and enlarging them fights Extreme Narrow. Recorded here so a later reader knows it was assessed and deliberately left; it is not part of #3.

If the fill still does not read at a glance once the state is correct, the next cheapest step — not taken now, because it should not be needed — is a persistent outline on the row itself rather than a brighter button.

### 4.5 Explicitly out of scope

**The physical keyboard is untouched, and that is safe rather than an omission.** On the Mac there is no key row, and a physical Shift/Ctrl is delivered by the OS to xterm's hidden textarea and encoded by xterm itself. Nothing in this app intercepts it: `src/components/dock/TerminalStack.vue:146` — the window-level keydown capture — early-returns on any event without `metaKey`, so only ⌘-combos (⌘T/⌘W/⌘⇧[/⌘⇧]/⌘+/⌘−/⌘0) are ever seen. The latch must not be extended to physical keys.

### 4.6 Sequencing — #3 first

**Do #3 before #2+#8.** Severity decides it: consequence 2 puts wrong control bytes into a live shell, which is the only defect in this doc that can destroy the user's work rather than annoy him, and the owner has called the toggles basic.

An earlier draft argued the reverse on merge-collision grounds. **That argument was overstated and is withdrawn**: #2+#8 lands in the compose-row markup (`TerminalView.vue:78-94`), its `<style>` rules (L487-497, L462-464) and `onComposeSend`/`onComposeKeydown` (L159-173); #3 lands in the key-row markup (L29-46), `KEY_ROW`/`fireKey` (L180-229) and `usePtyTerminal.js`. Those regions are adjacent, not overlapping — a real but ordinary rebase, not a reason to invert a severity ranking.

One cross-dependency survives in either order: the Shift+Enter wire form (§3.4) must be decided **once** and consumed by both the compose row and `fireKey`'s Enter key, because consequence 4 is the same defect in the other row.

**Superseded in part by §5.3.** This subsection's ranking of #3 above #2+#8 stands; what it could not know is that #13 now lands in the same markup and must sit between them. Read §5.3 for the full order.

---

## 5. #13 — closed by research; this section is now the interface to it

**#13 is no longer blocked and no longer belongs to this doc's scope.** `docs/research/terminal-vietnamese-ime-root-cause-4.md` — the head of the IME chain, which supersedes `-3.md` — closed it analytically, without the Mac dump `-3.md` was waiting on. Everything below is what that closure *does to this doc*; the reasoning itself lives there and is not restated.

An earlier draft of this section argued there was no VS Code mechanism to port, on the strength of a single issue number. That refusal was already withdrawn once, and `-4.md` has now replaced it with a positive answer, so it is gone rather than merely retracted. The method lesson survives it and is the durable part: a question the owner had raised across four documents was closed on **one** source. One source is not enough sources.

### 5.1 The premise rounds 1-3 shared was inverted

The whole chain rested on *"WKWebView tags OpenKey's synthetic keys `keyCode 229`; Chromium does not, and that is the VS Code difference."* Read from both engines' source, it is the reverse: WebKit sets 229 only when a real input method consumed the event and *un-sets* it for a modeless `insertText:`-only engine like OpenKey; Chromium sets 229 with no IME at all whenever the inserted text is longer than one character. **VS Code works because Chromium mis-tags the carrier and xterm's 229 path happens to be value-based.**

The actual defect is one line of xterm 5.5.0: `_keyPress` does `String.fromCharCode(ev.charCode)` — one UTF-16 unit — while WebKit hands OpenKey's corrected syllable through as an ordinary keydown whose `event.key` is the whole string. `"ăn gì" → "ăn g"`. Two of the three citations the chain leaned on do not say what they were used for (one is an Android report; one is a PR; one involves a real composing IME on all three engines).

This does not invalidate rounds 1-3's line-level traces — those are source-verified and stand. It invalidates the *inference* drawn from them.

### 5.2 What lands in this doc's files

`-4.md` §7 decides: xterm owns keys, the app owns text. `disableStdin` returns to `false`, the app-owned overlay `<textarea>` and its CSS are deleted, `useTerminalInput.js` and `useWkImeGuard.js` are deleted, and one ~90-line composable claims only the text path (a capture-phase `input` drain on xterm's own textarea, a narrow keypress veto, one 229 Backspace/Enter claim). Four consequences reach this doc:

1. **The compose row is re-gated to `showKeyRow` (`!isHost`)** — `-4.md` §8. Once direct typing works on the Mac, the row's own stated Mac justification is spent, and Extreme Narrow (`CLAUDE.md`) removes it. **This changes who §2 and §3 are for**: the compose input becomes a companion-only surface, so §2.1's font-override removal and all of §3 are phone-visible only. Nothing in §2 or §3 becomes wrong; the audience narrows.
2. **§2.4's first candidate stops being reachable on the Mac.** If the owner's "lỗi font" was the compose input's mono override, re-gating makes it disappear from the Mac window regardless of §2.1. The surviving Mac candidate is therefore the second one — Vietnamese combining marks under OpenKey's decomposed-Unicode mode — which no font declaration change fixes. §6's row 2 should be read with that narrowing.
3. **§4.1's consequence 5 survives the deletion and still needs §4.2.** `useWkImeGuard.js` goes away, but the new drain sends through `term.input(...)` into the same `onData`, so an armed Ctrl still meets a multi-character chunk. §4.2's "preserve, do not clear" remains the fix, and is now the *only* thing standing between an armed latch and a silently eaten syllable.
4. **The room's cheap A/B falsifier does not currently work** — a new finding, not previously recorded here. `.xterm-helper-textarea { display: none !important }` (`TerminalView.vue:461`) is **not** gated on the mode flag, so `localStorage['aki-input-mode']='legacy'` reverts the JavaScript while leaving xterm with an unfocusable textarea. Any instruction anywhere in this batch to fall back to legacy mode is void until that rule is gated. `display:none` also defeats xterm's `_syncTextArea()`, which is what positions the OS candidate window at the cursor.

**§3.4's open prerequisite is not resolved by any of this.** Returning paste handling to xterm fixes the *browser* paste path (`useTerminalInput.js:337` sent it raw, losing bracketed paste); it says nothing about whether the app can *read* bracketed-paste mode in order to decide the compose row's multi-line wire form. §6 row 4 stands unchanged.

### 5.3 Sequencing — #13 before #2 and #8, after or beside #3

Revised from §4.6, which predates this. #13 now deletes two files, flips `disableStdin`, removes the overlay textarea and its CSS, and re-gates the compose row's `v-if` — all inside `TerminalView.vue`'s markup and `<style>`, which is exactly where §2 and §8 land. Doing #2/#8 first means editing font rules and an Enter handler on markup that #13 then moves behind a gate. **#3 is unaffected either way**: its two latches live in `usePtyTerminal.js` and `fireKey`, neither of which #13 touches, so §4.6's severity ranking (#3 first, because it is the only defect here that can put wrong control bytes into a live shell) survives intact.

Order: **#3 → #13 → #2 + #8.**

One item is a hard prerequisite of #13 rather than a sibling: on the Mac, the `aki-input-mode` A/B must be made to work (consequence 4 above) *before* the deletions, or the change ships with no way to revert it at runtime.

---

## 6. Unverified — every Mac observation, with its settling command

Nothing below can be checked from the dev box: it is headless Linux, the app is a macOS Tauri build, and `@xterm/xterm` is not even installed here (§2.3). Per `RULE-coding.md` §B3 these are reported as unverified, never as done.

| # | Claim / open question | What settles it |
|---|---|---|
| 1 | That the toggles read correctly at a glance on a real phone once the state is one latch (§4.4) | Arm Ctrl on the companion, look at the row without hunting for it; then tap another button and confirm the previously-tapped one does not retain a cyan border (the stuck-`:hover` false positive) |
| 2 | Whether the mono override is what the owner sees as "lỗi font", or whether it is Vietnamese combining marks (§2.4) | Apply §2.1, look at the compose input on the Mac. If it persists, check OpenKey's Unicode mode ("Unicode dựng sẵn" vs "Unicode tổ hợp") |
| 3 | xterm's `fontFamily` default (§2.3's Courier New consequence) | `grep -rn "courier" node_modules/@xterm/xterm/src/common/services/OptionsService.ts` after a full `npm install` |
| 4 | Whether xterm 5.x exposes a readable bracketed-paste state — **prerequisite** for §3.4 | `grep -rn "bracketedPaste\|BracketedPaste" node_modules/@xterm/xterm/typings/xterm.d.ts node_modules/@xterm/xterm/src/common/InputHandler.ts` after a full `npm install` |
| 5 | Which multi-line wire form the target CLIs actually receive (§3.4) | In the in-app terminal run `cat -v`, Send a two-line compose, and read whether `^[[200~…` / `^[^M` / `^J` arrives; compare against a physical Shift+Enter in a `/terminal-setup`-configured iTerm2 |
| 6 | Whether an auto-growing compose textarea causes host PTY re-wrap oscillation | Owned by the geometry item, not this doc: the textarea's `field-sizing: content` changes `.pty-terminal-mount`'s height → `ResizeObserver` → `scheduleFit` → `fitAddon.fit()` → `hostResize` → `pty_resize` (`TerminalView.vue:261-298`). Fallback with zero geometry cost: a fixed 2-row textarea |
| 7 | That a latched Ctrl now works with focus in the compose input (§4.3), without eating IME composition or ordinary capitals | On the companion: run `sleep 60`, tap Ctrl, type `c` in the compose box — the process must die. Then arm Shift and type Vietnamese in the same box — every character must arrive intact |
| 8 | #13 — owned by `ime-research`, not by this doc | Listed for completeness only: `scripts/capture-ime-evidence.sh`, then `__akiIme.tail(20)`; plus the falsifier `localStorage['aki-ime-guard'] = 'off'` and repeat the repro |

---

## 7. Doc-sync obligations — listed, not performed

To be done by whoever executes the code, in the same task as the change (CLAUDE.md, "Feature changed?"):

- `docs/feat/in-app-terminal.md` — the user-facing in-app-terminal feature doc. Its "Terminal font size" and Vietnamese-input passages both describe behaviour this doc changes.
- `docs/arch/terminal-stack.md` — the terminal-stack architecture doc. The §4.2 single-funnel change alters the documented input pipeline.
- `README.md` and `src/components/modals/IntroModal.vue` — required by CLAUDE.md for any user-visible feature change, i.e. for **#3 and #8** (English, terse). #2 is internal hygiene and needs neither.
- `CHANGELOG.md` `[Unreleased]` — one entry per shipped defect. **No version number anywhere in this doc or in the entries** (`RULE-release.md` §A5).
- `docs/plan/backlog-jul27.md` — the workstream tracker. **Not amended here**; `plan-docs` owns that edit.
