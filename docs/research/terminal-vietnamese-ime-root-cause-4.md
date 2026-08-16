# Vietnamese typing in the in-app terminal — the mechanism was inverted; the decided architecture

Status: §7 point 1 (the "narrow" keypress veto) and the first row of its exclusivity table superseded by `docs/research/terminal-vietnamese-ime-root-cause-5.md` (the jul31 double-space blocker proved that row false — `_keyPress` does not force-cancel space or uppercase A-Z). Every other part of this doc's architecture (the drain, composition stand-down, the 229 irreducible keydown claim, no vendoring, no bundled 6.x upgrade) stands unchanged.

Chain: follows `terminal-vietnamese-ime-root-cause-3.md` (residual auto-restore bug, three ranked candidates, a gated `classify()` patch). This doc reverses that chain's central mechanism against engine source, closes its open candidate analytically, and decides the architecture. It edits nothing in `-jul27.md`, `-2.md` or `-3.md` beyond the single `Status: superseded by` line `docs.B2` allows.

## Start time

2026-07-30 04:10 +08 — fourth round, opened because the owner overturned the previous round's refusal on a factual point. Source-only research from the Linux dev box; the Mac was not available and, per the owner's explicit instruction for this round, "I cannot test it" was not an acceptable stopping point.

## Initial purpose

The owner's instruction has been unchanged across four documents: **"hãy làm sao để gõ được như VSCode không lỗi"** — make typing work the way it does in VS Code. Round 3 answered that there is nothing to copy, citing `microsoft/vscode#267568` as proof that VS Code's own terminal is broken and therefore not an oracle.

The owner overturned that on a fact: *"người ta lỗi là lỗi trên bộ gõ mac mặc định (telex của mac) chứ openkey ko bị"* — the bugs reported against VS Code are with **macOS's built-in Vietnamese input method**, not with **OpenKey**, the third-party utility he actually uses. OpenKey works correctly in VS Code. That converts a vague comparison into a controlled one:

| | Engine | Input utility | xterm.js | Result |
|---|---|---|---|---|
| VS Code integrated terminal | Electron / Chromium | OpenKey | `^6.1.0-beta.291` | **works** |
| This app's in-app terminal | Tauri v2 / WKWebView | OpenKey | `5.5.0` (pinned) | **broken** |

Two variables, not one — the second (xterm version) was itself an assumption the earlier rounds never checked.

Second standing instruction, about method: *"tìm đủ các nguồn khác nhau đi đừng vội kết luận khi chưa đủ nhiều luồng thông tin"* — gather enough independent streams before concluding. Round 3 closed the question on one issue number. This round was required to fan out.

Context at the time: `@xterm/xterm` pinned 5.5.0; Tauri v2 ⇒ WKWebView, macOS-only, so there is no engine to switch to; PTY input has zero local echo (`usePtyTerminal.js` `sendRaw`); the working tree carries an **untracked** `src/composables/useTerminalInput.js` (commit `7ce5804`) that already replaced guard v2 and runs xterm with `disableStdin` — a state none of `-jul27`/`-2`/`-3` describes.

## Strategy

Six independent evidence streams, each required to produce quotable source or a fetched issue rather than a paraphrase, plus a full read of the live working tree (not of the code the chain describes):

1. Engine source for the `keyCode 229` rule — WebKit and Chromium, both sides, not one side inferred from the other.
2. VS Code's terminal input layer at the concrete level: constructor options, `attachCustomKeyEventHandler` body, composition hooks, and the xterm version it actually pins.
3. OpenKey's injection mechanics per setting, and its own users' reports of which host apps work.
4. xterm.js's own record — issues, PRs, and the 5.5.0-vs-master diff of `CompositionHelper` and the key handlers.
5. Prior art in other Tauri / WKWebView / WebKit-embedded apps that made IME input work (CJK is the same problem class and far better documented).
6. The hidden-input-surface pattern across Monaco, CodeMirror 6, ProseMirror and xterm.js, including whether WebKit fires `beforeinput` on a `<textarea>` at all — the one fact `-2.md` recorded as undecidable from documentation.

## Checklist

- [x] WebKit `EventHandler.cpp` / `WebViewImpl.mm` / `WebEditorClientMac.mm` read for the 229 condition
- [x] Chromium `render_widget_host_view_cocoa.mm` read for its 229 condition
- [x] VS Code `xtermTerminal.ts` / `terminalInstance.ts` / `ime.ts` read; xterm version confirmed from `package.json`
- [x] xterm.js 5.5.0 `Terminal.ts`, `CompositionHelper.ts`, `Keyboard.ts` read; diffed against `master`
- [x] xterm.js #5887, #5894, #5704, vscode #267568 fetched directly and re-classified
- [x] Chromium 864911 / 41402349 fetched and re-classified
- [x] OpenKey per-setting injection matrix and host-app reports collected from its own tracker
- [x] Monaco / CodeMirror 6 / ProseMirror WebKit accommodations read from source
- [x] `beforeinput`-on-`<textarea>` support in WebKit settled against MDN BCD + the Input Events spec
- [x] Live working tree read: `useTerminalInput.js`, `useWkImeGuard.js`, `TerminalView.vue`, `usePtyTerminal.js`, `docs/plan/done/terminal-ime-input-layer-separation.md`
- [ ] Runtime confirmation on the Mac — still not run, and this doc is written so that it is a *confirmation*, not a *gate* (see Verification)

## Result

### 1. The chain's central mechanism was backwards

Every previous round rests on: *"WKWebView tags OpenKey's synthetic keys `keyCode 229` and shunts them into xterm's IME fallback paths; Chromium does no such tagging, and that is the whole VS Code difference."* Read from both engines' source, the opposite is true.

**WebKit sets 229 only when a real input method consumed the event.** `Source/WebCore/page/EventHandler.cpp:206-207` defines `CompositionEventKeyCode = 229`; `EventHandler::internalKeyEvent` (`:4333-4346`) applies it only when `handleInputMethodKeydown` marks the keydown default-handled, which is sourced from `[NSTextInputContext handleEventByInputMethod:]` returning YES. Modern WebKit then goes further and *un-sets* it — `Source/WebKit/UIProcess/mac/WebViewImpl.mm:5886-5940`:

```objc
// Route it through the keypress flow (handled=NO) so the keydown reports the real keyCode
if (handled && hasOnlyInsertText && !checkedThis->m_page->editorState().hasComposition)
    handled = NO;
```

A modeless engine that only calls `insertText:` and never establishes marked text gets its **real** keyCode back. OpenKey is exactly that: `CGEventTap` + `CGEventKeyboardSetUnicodeString`, no `compositionstart` ever.

**Chromium sets 229 without any IME at all when the inserted text is longer than one character.** `content/app_shim_remote_cocoa/render_widget_host_view_cocoa.mm:1599-1618`:

```objc
if (_hasMarkedText || oldHasMarkedText || _textToBeInserted.length() > 1) {
  NativeWebKeyboardEvent fakeEvent = event;
  fakeEvent.windows_key_code = 0xE5;  // VKEY_PROCESSKEY
  fakeEvent.skip_if_unhandled = true;
```

OpenKey's corrected syllable is one CGEvent carrying the whole string (`SendNewCharString()`, up to 16 UTF-16 units) — `_textToBeInserted.length() > 1` is satisfied, so **Chromium 229-tags it and WebKit does not.**

### 2. That inversion is what explains the controlled comparison

Follow each engine into xterm 5.5.0:

- **Chromium → 229 → `CompositionHelper.keydown` returns `false` → `_handleAnyTextareaChanges()`.** That handler snapshots the textarea, re-reads it on `setTimeout(0)`, and on growth sends the **whole diff** — the complete multi-character string. VS Code works *because* Chromium mis-tags the carrier as an IME key and xterm's 229 path is value-based.
- **WebKit → no 229 → ordinary key path.** `windowsKeyCodeForKeyEvent()` (`PlatformEventFactoryMac.mm:591-616`) derives the code from `[event characters]` first, so the kVK-0 carrier surfaces as a normal keydown whose `event.key` is the entire string (`"gì"`, or `"ex"` on the auto-restore path). `evaluateKeyboardEvent` has no case for a multi-character `key`, so nothing is cancelled and `keypress` fires — and xterm 5.5.0 `Terminal.ts:1120-1164`:

```ts
if (ev.charCode) { key = ev.charCode; } …
key = String.fromCharCode(key);
this.coreService.triggerDataEvent(key, true);
```

`String.fromCharCode` of a single UTF-16 code unit. **The whole bug, in one line: xterm assumes one key event carries one character.** `"ăn gì" → "ăn g"`. `"ex" → "e"`.

That second case closes `-3.md`'s open question analytically: its **Candidate 1** is correct, and it did not need a Mac dump — the restore carrier is not 229-tagged because nothing composed, so `classify()`'s all-ASCII exclusion was never even the deciding gate; the payload was going to be truncated by `_keyPress` either way.

Corroboration from an independent report of the same *shape*: xterm.js **#5894** (WKWebView-only, "Chromium hosts not affected") traces to `keypress.charCode` with `event.key === "~/"` — a two-character `event.key` truncated to its first unit. Its own text does not involve 229.

### 3. What actually differs between VS Code and this app — and what is portable

Read directly from `microsoft/vscode@main`:

- VS Code's terminal contributes **zero** composition code. `grep -rn "composition|isComposing" src/vs/workbench/contrib/terminal*` returns one hit, in an unrelated test name. All IME handling is xterm.js's `CompositionHelper`.
- `attachCustomKeyEventHandler` (`terminalInstance.ts:1140-1197`) is keybinding arbitration via `keybindingService.softDispatch` — chords, `commandsToSkipShell`, mnemonics, tab-focus, shift+tab, alt+F4, ctrl+v clipboard fallback. **No IME carve-out.** Round 3's search for one was shallow but its conclusion was right.
- `IME.enable()`/`disable()` exists (`src/vs/base/common/ime.ts`) but is chord-mode-only and consumed exclusively by the *editor*'s edit-context implementations. Nothing in `contrib/terminal` imports it.
- VS Code pins `@xterm/xterm ^6.1.0-beta.291`; this app pins `5.5.0`. "Same library" was an assumption and it was false.

So there is no VS Code *terminal* mechanism to port — but there **is** a VS Code mechanism, and it is in the editor: **derive text from the input surface's value, never from a key event.** `vs/editor/browser/controller/editContext/textArea/textAreaEditContextInput.ts` handles `input` with `TextAreaState.readFromTextArea(...)` + `TextAreaState.deduceInput(...)`. The same discipline appears independently in two more places:

- **CodeMirror 6 abandoned the hidden textarea for `contenteditable` specifically because "support for IME and screen readers is better in the contenteditable model"**, and derives changes by diffing DOM mutations rather than interpreting key events.
- **xterm.js itself** already uses value-diffing — but only on the 229 branch, with its own comment explaining why: *"The compositionend event's data property is unreliable… the last compositionupdate event's data property does not always accurately describe the character."*

Three independent codebases converge on the same rule. The reason it is 900 lines in Monaco and four here is that Monaco's textarea holds surrounding document context, while a **no-local-echo terminal's textarea has exactly one correct resting value: empty**. "Read all of it and empty it" is the entire algorithm.

An important counter-example that stops "do it like VS Code" from being a blank cheque: VS Code's newer `EditContext` path **broke EVKey entirely** (EVKey#76 → vscode#239559), and `EditContext` is unimplemented in WebKit (WebKit bug 269922). VS Code is an oracle for the value-reading discipline, not for its current input stack.

### 4. Prior art on WKWebView specifically — this configuration is genuinely un-battle-tested

Every widely-used xterm.js terminal (Tabby, Hyper, Wave, Termius) is Electron; Warp and Rio are native. **Tauri + xterm.js is the untested combination**, and the three open WKWebView bugs in xterm's tracker all name Tauri 2. Two things are directly reusable:

- `sotasan/piyo`'s `src/lib/xtermDeadKey.ts` — a Tauri-targeted xterm addon that suppresses the synthetic `keypress` whose `charCode` equals the committed character and **extracts the real key from a two-character `event.key`**. Independent confirmation of both the shape and the discriminator this doc's design uses.
- WebKit fires `compositionend` **before** `keydown`, the reverse of Chromium. CodeMirror 6 (`input.ts:175-183`, 100 ms) and ProseMirror (`input.ts:439-451`, 500 ms) both guard post-composition Enter with a timestamp window on `browser.safari`. This app's compose row guards only on `isComposing || keyCode === 229`, which is exactly the guard WebKit's ordering defeats.

At the Tauri layer there is nothing to fix: wry removed all `keyDown:` overrides in 0.23.2 (wry#798) and does not touch `NSTextInputClient` on macOS.

### 5. Three citations the chain rests on do not say what they were used for

- `crbug.com/864911` / `issues.chromium.org/41402349`, cited in `-3.md` as independent confirmation that 229-tagging is not WebKit-specific, is an **Android** report (OS field Android, Chrome 67; the reporter's own words: *"Every other browsers, including Chrome for desktop works just fine"*). On Android every soft keyboard is an IME, so 229 there is correct behaviour. It says nothing about macOS.
- xterm.js **#5704** is a **pull request**, not an issue. Its content is useful and was never used: WKWebView does not fire composition events for Hangul at all, delivering `insertText` then `insertReplacementText` instead.
- xterm.js **#5887**'s IME is **Doubao, a real composing IME** — which is why it reproduces identically on Safari, Chrome and WKWebView. It was never evidence about OpenKey. (Its environment line, quoted verbatim in `-3.md`, is accurate; the inference drawn from it was not.)

### 6. The live code is two generations past what the chain describes, and its encoding half is wrong

`src/composables/useTerminalInput.js` (untracked, commit `7ce5804`, design in `docs/plan/done/terminal-ime-input-layer-separation.md`) runs xterm with `disableStdin` and captures keys on an app-owned transparent overlay `<textarea>`. The **separation** is right and it does handle OpenKey's carrier correctly (it reaches `beforeinput` with the full string in `ev.data`). The **encoding** half re-implements xterm's `evaluateKeyboardEvent` — ~450 lines of terminal-protocol knowledge — in about 40, and is wrong in six ways, all read from the file:

| Defect | Line | Consequence |
|---|---|---|
| `classifyKey` returns `key229` before the `CTRL_KEYS` check; `onKeydown` then does nothing for `key229` | `:115`, `:218` | a true composing IME's 229-tagged Backspace/Enter is dropped outright (not OpenKey — see §1) |
| arrows always emit `\x1b[A`-style | `:255` | no DECCKM: arrow keys wrong inside vim/less/any application-cursor-mode program |
| `altKey && !ctrlKey → 'pass'`, no handling | `:112` | Option+Backspace / Option+arrow word motions send nothing |
| no modifier encoding at all | — | Shift+arrow, Ctrl+arrow lost |
| `F5` emits `\x1b[5~` | `:275` | that is PageUp; xterm's own table emits `\x1b[15~` |
| paste sent raw | `:337` | bracketed paste gone — a multi-line paste executes line by line |

Two more, outside that file: `.xterm-helper-textarea { display: none !important }` (`TerminalView.vue:461`) is **not** gated on the mode flag, so `localStorage['aki-input-mode']='legacy'` reverts the JavaScript and leaves xterm with an unfocusable textarea — the room's cheap A/B falsifier does not currently work. And `display:none` also defeats xterm's `_syncTextArea()`, which is what positions the OS candidate window at the cursor.

### 7. Decided architecture — a text drain on xterm's own textarea

One rule decides the split: **xterm owns keys; the app owns text.** Keys are a solved, mode-dependent protocol problem that xterm already solves correctly and must never be re-implemented. Text is where xterm's one-event-one-character assumption fails, and that is the only thing the app should claim.

Concretely, `disableStdin` goes back to `false`, the overlay textarea and its CSS are removed, xterm's own hidden textarea is the capture surface again, and one small composable adds three claims:

1. **Keypress veto, narrow.** Via the public `attachCustomKeyEventHandler`, return `false` when `ev.key.length > 1 && ev.charCode === ev.key.codePointAt(0)` — the multi-character carrier / dead-key signature, the same discriminator `piyo`'s addon uses. Returning `false` aborts xterm's processing **without** `preventDefault`, so the browser's own insertion proceeds instead of being destroyed (this is precisely what guard v1 got backwards).
2. **The drain.** A capture-phase `input` listener on `term.element`: read `textarea.value`, empty it, strip OpenKey's invisible sentinels (`U+202F`, `U+200C` — the same characters iTerm2 fails to absorb in OpenKey#95), normalise newlines to `\r`, send once, `stopPropagation()` so xterm's `_inputEvent` cannot re-send. Skipped when composing, and for `insertFromPaste`/`insertFromDrop`/any composition `inputType`.
3. **One irreducible keydown claim.** `keyCode === 229 && !isComposing && (key === 'Backspace' || key === 'Enter')` → send `\x7f`/`\r`, `preventDefault` only. Against an empty textarea these produce no `input` event at all, so nothing else can claim them. This is for real composing IMEs; OpenKey never reaches it.
4. **Composition stands down**, with `composing` released on a `setTimeout(0)` after `compositionend` so xterm's own `_finalizeComposition` (also deferred) wins, plus the WebKit ordering guard from §4 — ignore an Enter within ~100 ms of `compositionend` — applied both here and to the compose row's `onComposeKeydown`.

Everything else — DECCKM, modifier-encoded sequences, Alt-as-meta, F1–F24, Ctrl mappings, bracketed paste, selection, scroll, focus — returns to xterm untouched.

**Why this is exclusive by construction, not by classification.** The whole design rests on one invariant that is a DOM specification guarantee rather than an xterm internal or a WebKit quirk: *`preventDefault` on a key event suppresses the textarea mutation, and therefore the `input` event.* Traced against xterm 5.5.0 source:

| Path | xterm behaviour | Drain |
|---|---|---|
| physical printable | `_keyPress` sends it and calls `cancel(ev)` → textarea never mutates | no `input` fires — inert |
| multi-char carrier (WebKit, OpenKey) | keypress vetoed → xterm sends nothing, does not cancel | browser inserts the full string → drain sends it once |
| 229 key (real IME, non-composing) | `CompositionHelper.keydown` returns `false`, `_keyDown` bails without cancelling; a `setTimeout(0)` diff is scheduled | drain reads and empties synchronously; the diff then compares `'' vs ''` and is a structural no-op, so its one-DEL-per-shrink defect can never fire |
| any non-text key | `evaluateKeyboardEvent` encodes and cancels | textarea unmutated — inert |
| composition | xterm owns it | drain stands down |

Note what the third row buys for free: it also neutralises xterm.js #5887 (the Doubao "second character lost" defect) for this app, because the drain claims before the buggy diff runs.

**No vendoring or patching of xterm.js is required.** Everything above uses public API (`term.textarea`, `term.element`, `term.input`, `attachCustomKeyEventHandler` — all declared in `typings/xterm.d.ts`). The genuinely upstream-shaped fix is three lines in `_keyPress` (prefer `ev.key` when `ev.key.length > 1`); that belongs in an upstream issue, not in a vendored copy here.

**Do not bundle an xterm 6.x upgrade with this.** It is tempting because 6.x is what VS Code runs and its `_handleAnyTextareaChanges` gained a de-duplication timer, but (a) `_keyPress` still does `String.fromCharCode(ev.charCode)` on master, so the upgrade neither fixes nor is needed for OpenKey, and (b) master removed `this.cancel(ev)` from both `_keyPress` and `_inputEvent`, which invalidates the exclusivity proof above. The upgrade is a separately sized item whose entry cost is redoing that table.

### 8. What the compose row is for after this

The compose row currently renders on **every** surface, and `TerminalView.vue:73-74`'s own comment gives its Mac justification as Vietnamese IME plus the no-local-echo terminal. Once direct typing works, that justification is spent for OpenKey. Two justifications survive and neither is about the Mac window: the phone/companion surface (no physical keyboard) and true composing IMEs, whose WKWebView support is independently poor (PR #5704: no composition events for Hangul at all) and which the owner has scoped out of this problem. So the row should be re-gated to `showKeyRow` (`!isHost`) — the same construction the key row and the font-zoom buttons already use, and the same Extreme Narrow argument (`CLAUDE.md`) that keeps the Mac window from spending pixels on a control its keyboard already has.

### A1 sizing (`METHOD-deep-think`)

**Reversible (two-way doors) — everything the fix consists of.** `disableStdin` is one boolean. The overlay textarea and its CSS are additive and removable. The drain composable is one new file behind the existing `aki-input-mode` flag. The compose row re-gating is a `v-if`. Deleting `useTerminalInput.js` and `useWkImeGuard.js` is recoverable from commit `7ce5804` and from git history respectively.

**One-way doors — none in this change, one adjacent.** The only irreversible-shaped decision in the neighbourhood is the `@xterm/xterm` 5.5.0 → 6.x upgrade, which changes a pinned dependency across a major version and invalidates the static proof above; §7 keeps it out. The second, softer one is *deleting* the compose row rather than re-gating it — a UI removal the owner would have to ask for again, which is why this doc recommends gating.

**Verification**

- The 229 conditions in §1 are verified by direct source read of both engines (WebKit `EventHandler.cpp`, `WebViewImpl.mm`, `WebEditorClientMac.mm`; Chromium `render_widget_host_view_cocoa.mm`), not paraphrased from issues.
- The xterm 5.5.0 behaviours in §2 and §7 are verified by direct read of `Terminal.ts`, `CompositionHelper.ts` and `Keyboard.ts` at tag 5.5.0, and diffed against `master`.
- VS Code's terminal facts in §3 are verified against `microsoft/vscode@main` source, including the absence of any composition code in `contrib/terminal` and the xterm version in `package.json`.
- `beforeinput` firing on `<textarea>` in WebKit — the one fact `-2.md` recorded as undecidable from documentation — is now settled: supported since Safari 10.1 (MDN browser-compat-data `api.Element.beforeinput_event`; Input Events Level 2). The related real limitation is that `getTargetRanges()` is specified to return an empty array for `input`/`textarea` (w3c/input-events#26), which is why a value drain, not a range diff, is the right shape here.
- The six defects in §6 are verified by line-level read of the working-tree file.
- **Not verified, and stated as such:** nothing in this doc has been run on a Mac. The design is written to need the Mac as *confirmation* rather than as a gate — see the residual below.
- **Residual uncertainty, one line:** whether the macOS build the owner runs contains WebKit's `handled = NO` un-tagging (`WebViewImpl.mm:5886`), i.e. whether the carrier arrives non-229 (§1) or 229-tagged as the older chain assumed. **The design is deliberately robust to both branches** — a non-229 carrier is claimed by the keypress veto plus the drain, and a 229-tagged carrier is claimed by the drain before xterm's textarea diff can run. The observation is worth taking for the record (`__akiTermInput.tail(20)`, read `keyCode` on the entry whose `key` is the whole syllable), but no decision in this doc waits on it.

**Corroborating links**

- WebKit `Source/WebCore/page/EventHandler.cpp` (`CompositionEventKeyCode = 229` at :206, applied at :4333-4346) · `Source/WebKit/UIProcess/mac/WebViewImpl.mm:5886-5940` (`handled = NO` when `hasOnlyInsertText && !hasComposition`) · `Source/WebKit/WebProcess/WebCoreSupport/mac/WebEditorClientMac.mm:58` · `Source/WebCore/platform/mac/PlatformEventFactoryMac.mm:591-616` (`windowsKeyCodeForKeyEvent` reads `[event characters]` first)
- Chromium `content/app_shim_remote_cocoa/render_widget_host_view_cocoa.mm:1599-1618` (`_textToBeInserted.length() > 1` → `0xE5`) and `:2704-2726` (`insertText:` fills `_textToBeInserted`)
- xterm.js 5.5.0 `src/browser/Terminal.ts:1120-1164` (`_keyPress`, `String.fromCharCode`), `:1172-1193` (`_inputEvent` gate), `src/browser/input/CompositionHelper.ts:94-117` (229 gate), `:184-205` (the `setTimeout(0)` diff and its single-`C0.DEL` shrink branch), `src/common/input/Keyboard.ts` (DECCKM, modifier encoding, `\x1b[15~` for F5)
- https://github.com/xtermjs/xterm.js/issues/5887 — real composing IME (Doubao), reproduces on Safari + Chrome + WKWebView alike; not an OpenKey report
- https://github.com/xtermjs/xterm.js/issues/5894 — WKWebView-only dead-key defect; traces to `keypress.charCode` with a two-character `event.key`, the same shape as OpenKey's carrier
- https://github.com/xtermjs/xterm.js/pull/5704 — a PR, not an issue: WKWebView fires no composition events for Hangul, delivering `insertText`/`insertReplacementText`
- https://github.com/microsoft/vscode/issues/267568 — Electron/Chromium terminal, macOS built-in Hangul IME; does not support any WKWebView-vs-Chromium claim
- https://issues.chromium.org/issues/41402349 (was crbug 864911) — **Android**, Chrome 67; miscited in `-3.md`
- https://github.com/microsoft/vscode — `src/vs/workbench/contrib/terminal/browser/xterm/xtermTerminal.ts:236-279`, `terminalInstance.ts:1140-1197`, `src/vs/base/common/ime.ts`, `src/vs/editor/browser/controller/editContext/textArea/textAreaEditContextInput.ts`, `package.json` (`@xterm/xterm ^6.1.0-beta.291`)
- https://github.com/codemirror/view — `src/browser.ts` (`safari` via `navigator.vendor`), `src/input.ts:175-183` and `:901-904`, `src/domobserver.ts:497-525`; https://codemirror.net/docs/migration/ (why CM6 left the hidden textarea)
- https://github.com/ProseMirror/prosemirror-view/blob/master/src/input.ts (post-composition Enter window)
- https://github.com/sotasan/piyo/blob/main/src/lib/xtermDeadKey.ts — Tauri-targeted xterm addon; two-character `event.key` + charCode-matching keypress suppression
- https://github.com/tuyenvm/OpenKey — `OpenKey.mm` `SendBackspace()` / `SendNewCharString()`; issues #38, #89, #95 (iTerm2 over-deletes because it ignores the `U+202F` sentinel), #123, #182, #247, #277, #319
- https://github.com/lamquangminh/EVKey/issues/76 → https://github.com/microsoft/vscode/issues/239559 — VS Code's `EditContext` path breaks EVKey; `EditContext` unimplemented in WebKit (https://bugs.webkit.org/show_bug.cgi?id=269922)
- https://github.com/tauri-apps/wry/pull/798 (all `keyDown:` overrides removed, 0.23.2) · https://github.com/tauri-apps/tauri/issues/13421 (WeChat IME, 229 with `isComposing:false`, WKWebView only)
- MDN browser-compat-data `api.Element.beforeinput_event` (Safari 10.1) · https://www.w3.org/TR/input-events-2/ · https://github.com/w3c/input-events/issues/26 (`getTargetRanges()` empty for `input`/`textarea`)
- In-tree: `src/composables/useTerminalInput.js`, `src/composables/useWkImeGuard.js`, `src/components/TerminalView.vue:336-362` and `:461-488`, `src/composables/usePtyTerminal.js` (`sendRaw`, `wireInput`), `docs/plan/done/terminal-ime-input-layer-separation.md`

## Decision

**Action** — the architecture in §7 and the re-gating in §8, to be sequenced by `plan-consolidator` in the terminal-input plan doc. This research doc decides *what* is built and *why*; it writes no code and edits no plan.

**Rejected/closed** — `-3.md`'s gated `NAMED_KEYS` `classify()` patch is **dropped**, for two independent reasons: it patches `useWkImeGuard.js`, which is no longer on the default path, and the distinction it draws (ASCII carrier vs accented carrier, 229 vs non-229) stops existing once the payload is read from the textarea's value rather than from the event. `-3.md`'s three ranked candidates are closed with Candidate 1 confirmed from engine source rather than from a dump.

**No action** — vendoring or patching xterm.js; upgrading `@xterm/xterm` to 6.x as part of this work (§7 states the reason, and it should be filed as its own item rather than dropped silently); anything at the Tauri/wry layer, which has no macOS IME surface left to configure since wry 0.23.2.

**Follow-up research** — none required to act. If, after the fix, direct typing still garbles, the discriminating observation is `__akiTermInput.tail(20)` around one bad syllable: whether the carrier keydown carries `keyCode 229` decides which of the two branches in the Verification residual is real, and both branches are already covered by the design, so a contradiction there would be a genuinely new finding worth a `-5`.

**Cross-references** — `terminal-vietnamese-ime-root-cause-3.md` (given its `Status: superseded by` line by this round), `docs/plan/done/terminal-ime-input-layer-separation.md` (its §1 "what VS Code actually does" is corrected by §1-§3 here; its §3 built event-payload reading where its own §1 named value reading), `docs/plan/done/terminal-input-surface.md` (§5.1's VS Code framing and the compose-row work in §2/§3), `docs/feat/in-app-terminal.md` §Vietnamese input, `docs/index.md` (chain head moves to this doc), `src/composables/useTerminalInput.js` and `src/composables/useWkImeGuard.js` (both superseded by the §7 design), `docs/plan/done/backlog-jul27.md` WS-D (no longer blocked on Mac evidence).
