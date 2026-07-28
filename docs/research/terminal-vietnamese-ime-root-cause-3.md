# Vietnamese typing in the in-app terminal — the auto-restore residual bug, VS Code verdict, ranked candidates

Chain: follows `terminal-vietnamese-ime-root-cause-2.md` (guard v1 post-mortem, delivery-shape matrix, guard v2, shipped and Mac-accepted for the plain-syllable path). That doc needs a `Status: superseded by terminal-vietnamese-ime-root-cause-3.md` line added at its top — **not done here**; this doc does not edit it (research docs are immutable event records, `RULE-docs.md` §B2). The orchestrator makes that edit.

## Start time

2026-07-27 (third round; source-only research, no Mac access from this session — this dev box cannot build/run the Tauri app, `CLAUDE.local.md`).

## Initial purpose

Guard v2 fixed direct Telex typing of plain syllables ("ăn gì" case). The user now reports a **different, narrower** residual bug, in their own words: with OpenKey's "spelling check + auto-restore wrong word" feature (`Kiểm tra chính tả` + `Phục hồi phím với từ sai`) on, typing `e x x i t` (Telex `ex` reads as a tone mark → `ẽ`, second `x` makes OpenKey decide the word is invalid Vietnamese and restore the raw keystrokes) produces `exit` correctly in every native app **and reportedly in VS Code's integrated terminal**, but produces `e` (the restored `x` is dropped) in this app's in-app terminal. macOS's own built-in Vietnamese input is explicitly out of scope (broken everywhere, including Terminal.app — not this app's bug per the user).

Question: (1) is the prior round's "Chromium does no such [229] tagging, that's why VS Code was fine all along" claim actually true, verified from source rather than inherited: if true, there is no VS Code mechanism to copy and that itself is the finding; (2) what are the plausible delivery shapes of OpenKey's *restore* path specifically (as opposed to the *correction* path guard v2 was built against), which claimant in `useWkImeGuard.js` (if any) picks up each shape, and where exactly one character is lost; (3) an exact, discriminating runtime-evidence recipe for the Mac.

Context at the time: `@xterm/xterm` pinned 5.5.0; Tauri v2 ⇒ WKWebView; guard v2 already shipped and accepted for the non-restore path; `useWkImeGuard.js` exposes `window.__akiIme` (pull-based ring, `dump()`/`tail()`/`status()`/`clear()`).

## Strategy

Source-first, no hypothesis inherited without a citation of its own:
1. Read OpenKey's own engine source for the *restore* feature specifically (`Engine.cpp`'s `checkSpelling()`/`checkRestoreIfWrongSpelling()`, `Engine.h`'s `vRestoreIfWrongSpelling`, `OpenKey.mm`'s `OpenKeyCallback` for how `HookState`/`vRestore` becomes actual CGEvents) — this is a **different code path** from the tone-mark correction path round 2 already covered, and round 2 never traced it.
2. Fetch xterm.js issues #5887 and #5894 directly (not paraphrased) for their current environment-affected lists, specifically whether each defect is WebKit-exclusive or also reproduces in Chromium.
3. Fetch a VS Code Electron-terminal IME issue and Chromium's own bug tracker for keyCode-229 behavior, to test round 2's claim against real reports rather than trusting it.
4. Re-read `useWkImeGuard.js`'s `classify()` line-by-line against the restore payload's actual shape (plain ASCII, e.g. `"ex"`) rather than the shape guard v2 was validated against (always-accented, e.g. `"gì"`).
5. Rank candidates by how directly each is supported by (1)-(4), and write the exact `__akiIme` command sequence that would tell them apart on the Mac.

## Checklist

- [x] OpenKey's spell-check/restore engine code fetched and read (`Engine.cpp`, `Engine.h`, `OpenKey.mm`)
- [x] xterm.js #5887 fetched directly — environment list re-checked
- [x] xterm.js #5894 fetched directly — environment list re-checked
- [x] VS Code terminal/editor IME issues fetched (#267568, #238452, #170688, #1168, #1412, #1237) and classified editor-vs-terminal
- [x] Chromium's own keyCode-229 bug reports checked (864911 / 41402349)
- [x] `useWkImeGuard.js`'s `classify()` re-audited against the restore payload's actual (ASCII) shape
- [ ] Runtime verification on the Mac — **PENDING**, this is the whole point of §Evidence recipe below

## Result

### VS Code verdict (answers the framing question first, since it decides whether there is anything to copy)

**Round 2's claim does not hold as a blanket statement, and there is still no VS Code mechanism to copy — but for a more nuanced reason than "Chromium never tags 229."** Verified from source, split by defect:

- **xterm.js #5894** (dead-key duplication + drop) — genuinely WebKit-only. The issue itself states this explicitly: *"Chromium doesn't generate the dead-key-char synthetic keypress and doesn't collapse the dead-key char into `event.key` for the next physical keydown."* Confirmed as WKWebView-exclusive.
- **xterm.js #5887** (second char lost when `keyCode=229` fires for every key) — **NOT WebKit-exclusive.** The issue's own environment list is *"WKWebView (Tauri 2), Safari, and Chrome, with all behaving identically."* Chromium's own bug tracker independently confirms keyCode 229 firing on ordinary keydowns in text inputs is a long-standing, non-WebKit-specific Chromium behavior (chromium issue 864911 / issues.chromium.org/issues/41402349, "Code 229 on any key in keydown event in input of text type"). Round 2's flat claim ("Chromium dispatches CGEvent-injected keys as ordinary keydown/keypress/input") is **refuted for this defect class** — the general premise "Chromium never does 229-style tagging" is false.
- **What this means for OpenKey specifically**: whether Chromium tags **OpenKey's exact CGEventTap-injected, non-composing** keystrokes as 229 the same way it tags a real composing IME's (Doubao's) keystrokes is **unverified — no source or issue was found testing that specific combination**. OpenKey's mechanism (open-loop CGEvent injection, no `compositionstart/end` ever) is not the same trigger as Doubao's (a real NSTextInputClient-backed IME). It is plausible Chromium's renderer only 229-tags input arriving through its own IME-adjacent code paths and passes raw CGEvent-injected keys through untagged (consistent with VS Code apparently working), while WKWebView routes *all* synthetic keyboard input — composing or not — through the same NSTextInputContext machinery uniformly. That is a real, sourced hypothesis, not yet a confirmed fact.
- **VS Code itself is not IME-bug-free** — it has a long, dated history of Vietnamese typing bugs with UniKey/EVKey/OpenKey (#170688, #238452, #79270, #1412, #1168, #1237), but **every one found is against the Monaco *editor*, none against the integrated *terminal***. That asymmetry is suggestive (the terminal's plain xterm.js keydown pass-through has a far simpler input model than Monaco's rich text buffer, so whatever breaks the editor may simply not apply to the terminal) but is an **inference, not a verified fact** — no issue explicitly confirms "the integrated terminal is immune" either; its absence from the issue list is not proof.
- **Bottom line**: there is no evidence of an active VS Code *mechanism* worth copying (no `attachCustomKeyEventHandler` IME carve-out was found in this round's search — the one apparent lead, "`xterm-bypass-policy.ts`", surfaced only inside an unmerged xterm.js issue commenter's own proposal, not confirmed as existing VS Code source; this is **unverified** and should not be cited as if it were). If VS Code's terminal is in fact immune to OpenKey's specific injection pattern, the likely explanation is the free consequence of Chromium's renderer not routing raw CGEvent-injected keys through the same code path a Cocoa-level composing IME uses — an engine difference, not a technique. **This confirms the task brief's decisive-finding framing: assuming that holds, there is nothing to port into this app; WKWebView is the fixed, sole variant on Tauri v2/macOS, so "switch engines" is not an available option (background, not a lead, per the task brief).**

### OpenKey's restore path, from source — the delivery shape guard v2 was never tested against

`Sources/OpenKey/engine/Engine.cpp`'s `checkSpelling()` sets `_spellingOK`/`_spellingVowelOK`; `checkRestoreIfWrongSpelling(handleCode)` is the feature the user names ("Phục hồi phím với từ sai" — confirmed as the exact README-listed toggle: *"Restore key if invalid word (on/off)"*). It walks `TypingWord[]`/`KeyStates[]` and populates a `vKeyHookState` with `backspaceCount` (chars to delete) and `newCharCount` + `charData[]` (the raw characters to put back).

`Sources/OpenKey/macOS/ModernKey/OpenKey.mm`'s `OpenKeyCallback`, on `pData->code == vRestore`, converts that struct into actual CGEvents **synchronously within one tap invocation, no runloop interleaving possible**:
1. `backspaceCount` reused Backspace keydown/keyup pairs via `SendBackspace()` (kVK 51) — same shape as the correction path guard v2 already handles.
2. Then, unless step-by-step mode is on, **one** `SendNewCharString()` call posts a single carrier keydown/keyup (kVK 0, `CGEventKeyboardSetUnicodeString`) carrying all `newCharCount` restored characters at once.

**This is structurally the identical shape as the tone-mark correction path** (N backspaces + one multi-char carrier) — round 2's delivery matrix should cover it. The one thing that differs, and the only thing that can differ, between the two paths' carriers:
- The **correction** carrier's payload always contains a Vietnamese diacritic (e.g. `"gì"`) — the whole reason OpenKey exists.
- The **restore** carrier's payload is, by construction, the **original raw ASCII the user actually typed** — for an English-shaped word like "exit", that is `"ex"`: two *plain Latin letters, zero diacritics*.

### The bug, traced against `useWkImeGuard.js`'s actual code

`classify()` in `src/composables/useWkImeGuard.js` (lines ~189-200):
```js
function classify(ev) {
  if (composing || ev.isComposing) return null
  if (ev.keyCode === 229) return 'synthetic229'
  if (ev.key && ev.key.length > 1 && !/^[A-Za-z]+$/.test(ev.key) && !ev.metaKey && !ev.ctrlKey) {
    return 'multiCarrier'
  }
  return null
}
```
The `multiCarrier` branch's own comment states the (now-shown-false-in-general) assumption plainly: *"a carrier payload for this engine class always contains a non-ASCII Vietnamese char"*. The restore carrier `"ex"` breaks that assumption directly — it is real, and it is plain ASCII. **If** this carrier's `keyCode` is not 229 (see Candidate 1 below), `classify()` falls through both branches and returns `null` — the guard stands down completely, `counts.stoodDown++`, and the keydown is left to xterm 5.5.0's own handling: `_keyPress` (per round 1's own citation, `Terminal.ts` L1129-1160) emits `charCode` = only the **first UTF-16 unit** of a multi-character `key` and prevents the real insertion. First unit of `"ex"` is `"e"` — **this reproduces the user's exact symptom, "restore to `e`, lost `x`", character-for-character.**

## Ranked candidates (none confirmed — each states its own discriminating command)

**1. ASCII-carrier evades the 229 gate; xterm's own first-UTF-16-unit bug drops the rest.** As traced immediately above. Requires the restore carrier to NOT be tagged `keyCode 229` by WKWebView — unverified whether WKWebView's 229-tagging depends on payload content or is uniform for all `CGEventKeyboardSetUnicodeString`-sourced synthetic keys regardless of ASCII-ness.
   - **Discriminator**: reproduce the bug, then `__akiIme.tail(20)`. Find the keydown entry with `key: "ex"` (or the equivalent 2-letter ASCII restore payload for whatever word was typed). If its recorded `keyCode !== 229` and `class: null` — this candidate is confirmed. If `class: 'synthetic229'` for that same entry, this candidate is refuted (falls to Candidate 2 instead).

**2. The restore carrier IS tagged 229 like the correction carrier, but WebKit never fires `beforeinput` for it — leaving it to xterm's own known-buggy textarea diff.** Guard v2's own design (documented in its `classify` comment block and the -2 doc's delivery matrix) explicitly stands down for "229 printable, NO beforeinput" and defers to "xterm's own textarea diff" as a "harmless" fallback — but that diff is precisely the mechanism round 1 found broken for multi-char bursts (`CompositionHelper.ts` L96-118/184-201, and independently xterm.js #5887's own root-cause description: *"the diff... captures the current textarea value... makes the diff unreliable"*). A 2-char plain-ASCII replacement landing in this "guard stands down" branch could under-deliver exactly as stock xterm did before any guard existed, even though the event was correctly classified.
   - **Discriminator**: same `tail(20)`. If the `"ex"`-keydown entry shows `class: 'synthetic229'`, check whether a subsequent `beforeinput` log entry exists before the next `keydown` — if none exists (or one exists with an `inputType` the guard doesn't claim, visible in its logged `inputType`/`dataTransfer` fields), this candidate is confirmed instead of Candidate 1.

**3. Backspace-count mismatch from OpenKey's Unicode encoding mode (precomposed vs. decomposed/combining marks).** If the user's OpenKey is configured for "Unicode tổ hợp" (combining/decomposed) rather than "Unicode dựng sẵn" (precomposed), `"ẽ"` may itself be 2 UTF-16 units (base `e` + combining tilde `U+0303`) rather than 1 precomposed codepoint — in which case the *delete* side of the restore (not the insert side) could be the one dropping a unit, independent of the ASCII question entirely. Lower confidence than 1/2 (no config value was confirmed for the user's setup) but structurally distinct enough to rule out separately.
   - **Discriminator**: in the same `tail(20)`, count the Backspace-classified keydown entries immediately before the `"ex"` carrier. Compare that count against how many *displayed* characters were actually removed on screen at that moment (visible from the terminal, not from the log). A mismatch there (not at the carrier) implicates this candidate instead of 1/2.

Per `RULE-agent-behavior.md` §B2: none of the three is asserted as *the* cause. The Mac dump is what promotes one of them from candidate to confirmed finding.

### Evidence recipe — exact, copy-pasteable, on the Mac

1. `npm run tauri dev` (already the user's normal dev loop — this is not a build-boundary violation, it runs the existing dev server the user starts themselves).
2. Open the in-app terminal tab. Open Safari's **Develop menu** → the machine's submenu → the target titled **`localhost`** (devUrl `http://localhost:1420`) — **not** any entry titled `Main.html` (`inspector-resource:///Main.html`; that is the Web Inspector's own UI, not the app — round 2's documented trap that cost a full round previously).
3. In that inspector's console, confirm the target before doing anything else:
   ```js
   __akiIme.status().page   // must read 'Aki Dev Sync' — if it reads anything else, you are in the wrong inspector window
   ```
4. Reset the ring for a clean repro:
   ```js
   __akiIme.clear()
   ```
5. With OpenKey active, spelling-check + auto-restore both ON (the user's existing settings — do not change them), click into the terminal and type exactly:
   ```
   exit
   ```
   as the sequence `e x x i t` (the `x x` is what triggers OpenKey's tone-then-invalid-word-restore). Do not press Enter yet if you want the ring undisturbed by the shell echo, though it will not corrupt the log either way.
6. Pull the evidence (returned, not logged — printed regardless of any console filter or attach timing):
   ```js
   __akiIme.tail(20)
   ```
   or, if more context is wanted:
   ```js
   __akiIme.dump()
   ```
7. In the returned array, find the keydown entry whose `key` is the two-letter restore payload (e.g. `"ex"`) and read off `keyCode` and `class` per the discriminators above. Also scan for any `beforeinput`/`input` entries between that keydown and the next one.
8. Optional A/B, to confirm the guard is even in play for this specific case (not assumed from the plain-syllable case): `localStorage['aki-ime-guard'] = 'off'`, reopen the terminal tab, repeat step 5. If the symptom is unchanged with the guard off, the guard is not the site of the bug at all and the loss happens upstream of it (xterm or WKWebView directly) — a different, larger finding that would redirect this whole investigation.
9. Report back the exact `tail(20)`/`dump()` JSON (or the relevant slice around the restore) — that is the raw material for confirming which of the three ranked candidates is real, or for opening a `-4` doc if none of them match.

**Verification**: OpenKey's restore mechanism verified by direct source read (`Engine.cpp`, `Engine.h`, `OpenKey.mm` — see links). xterm.js #5887/#5894 environment claims verified by fetching the issues directly, not paraphrased from round 2. Chromium's own general 229-tagging behavior verified against its own bug tracker. The three ranked candidates and the VS Code-terminal-is-untested-for-this-exact-mechanism gap are **explicitly not verified** — they require the Mac dump in the recipe above. Nothing in this doc was fabricated to fill that gap (`RULE-agent-behavior.md` §B2, `RULE-coding.md` §B3).

**Corroborating links**:
- https://github.com/tuyenvm/OpenKey — `Sources/OpenKey/engine/Engine.cpp` (`checkSpelling()`, `checkRestoreIfWrongSpelling()`), `Sources/OpenKey/engine/Engine.h` (`vRestoreIfWrongSpelling`, `vCheckSpelling`, `vTempOffSpelling`), `Sources/OpenKey/macOS/ModernKey/OpenKey.mm` (`OpenKeyCallback`'s `vRestore`/`vRestoreAndStartNewSession` branch, `SendBackspace()`, `SendNewCharString()` — same functions round 2 already cited for the correction path, confirming the restore path reuses them)
- https://github.com/tuyenvm/OpenKey/blob/master/README.md — confirms "Restore key if invalid word (on/off) - Phục hồi phím với từ sai" as the exact user-facing toggle name
- https://github.com/xtermjs/xterm.js/issues/5887 — fetched directly this round; environment list *"WKWebView (Tauri 2), Safari, and Chrome, with all behaving identically"* — refutes round 2's blanket Chromium-immunity claim for this specific defect
- https://github.com/xtermjs/xterm.js/issues/5894 — fetched directly this round; *"Chromium doesn't generate the dead-key-char synthetic keypress and doesn't collapse the dead-key char into `event.key`"* — confirms WebKit-exclusivity for this specific (different) defect
- https://issues.chromium.org/issues/41402349 and https://bugs.chromium.org/p/chromium/issues/detail?id=864911 — "Code 229 on any key in keydown event in input of text type" — independent confirmation that 229-tagging is not WebKit-exclusive in general
- https://github.com/microsoft/vscode/issues/267568 — VS Code's own Electron/Chromium terminal DOES have an open IME bug (Hangul), so "VS Code is immune to IME bugs" is false in general; this round found no terminal-specific report for OpenKey/EVKey/UniKey specifically
- https://github.com/microsoft/vscode/issues/238452, /170688, /1412, /1168, /1237 — VS Code's Vietnamese-IME bug history, all against the Monaco editor, none against the integrated terminal (an inference, not proof, that the terminal's simpler input model sidesteps them)
- `src/composables/useWkImeGuard.js` lines 189-200 (`classify()`), 219-230 (`onKeydown` synthetic229 handling), 233-264 (`onBeforeInput`), 271-283 (`onInput` fallback, multiCarrier-only) — the exact code this doc traces the bug against
- `docs/research/terminal-vietnamese-ime-root-cause-2.md` — the delivery-shape matrix and guard v2 this doc extends; its own §Result flags "the answer itself must come from the `aki-ime-debug=1` log on the Mac" as the one fact research alone cannot settle — this doc inherits that same constraint for the restore path specifically

## Decision

**No action** on a v3 guard patch — explicitly out of scope for this doc per the task brief; a patch on an unconfirmed hypothesis is exactly what produced v1's regression last round.

**Follow-up research** — whichever candidate the Mac `__akiIme.tail(20)`/`dump()` output confirms becomes the opening fact of a `-4` doc (or this doc's Result stands as closed if the recipe instead shows something not yet enumerated here). Do not attempt a v3 guard change without that dump, per the same rule that governed v2.

**Cross-references**: `terminal-vietnamese-ime-root-cause-2.md` (needs its `Status: superseded by` line added by the orchestrator, per this doc's own opening note — not edited here), `docs/feat/in-app-terminal.md` §Vietnamese input (unchanged pending the confirmed cause), `src/composables/useWkImeGuard.js` (unchanged — traced, not patched), `docs/plan/backlog-jul27.md` WS-D (still blocked on Mac evidence per its own dependency-order note).

---

## Addendum (2026-07-28) — evidence capture kit and gated patch proposal

This section is appended, not a rewrite: everything above this rule is the original round-3 event record, unchanged. This addendum is prep work for the still-`[ ]` "Runtime verification on the Mac" checklist item above — a capture kit and a *conditional* patch proposal, neither applied nor run. Nothing here promotes any of the three ranked candidates; that still requires the Mac dump.

### Evidence capture kit

`scripts/capture-ime-evidence.sh` — run on the Mac only (this dev box cannot build/run Tauri). Prints the exact numbered recipe (target-confirmation trap included), stages the console confirm+clear snippet on the clipboard via `pbcopy` when available, then starts the existing `npm run tauri dev` loop. It does not and cannot script Safari's Web Inspector itself — the interactive part (opening the correct inspector window, typing `e x x i t`, reading `tail(20)`) is still done by hand once. Verbatim recipe reproduced in this task's report to the user.

### Discriminator table — read the answer off `__akiIme.tail(20)`, no re-reasoning needed

For the keydown entry whose `key` is the two-letter ASCII restore payload (e.g. `"ex"`):

| Field on that entry | Value | Candidate confirmed |
|---|---|---|
| `keyCode` | `!== 229` and `class: null` | **Candidate 1** (ASCII carrier evades the 229 gate; `classify()`'s all-ASCII exclusion stands the guard down; xterm's own first-UTF-16-unit bug drops the rest) |
| `keyCode` | `=== 229`, `class: 'synthetic229'`, AND no `beforeinput` entry follows before the next `keydown` (or one exists with an `inputType` the guard does not claim) | **Candidate 2** (correctly classified as 229, but WebKit never fires a claimable `beforeinput` for this specific carrier — falls through to xterm's already-known-buggy textarea diff) |
| Backspace-classified keydown count immediately before the carrier | mismatches how many characters were *visibly* removed on screen at that moment (not from the log) | **Candidate 3** (decomposed-Unicode backspace-count mismatch — independent of the ASCII question, on the delete side not the insert side) |

Only one row can be true for a given repro. If none of the three rows match the dump, that is itself a finding — open a `-4` doc rather than forcing the dump into one of these three.

### Proposed patch — gated on Candidate 1, NOT applied

Applies **only if** the Mac dump confirms Candidate 1 (the first table row above). Untouched if Candidate 2 or 3 is confirmed instead — this patch does not address either of those paths at all (see "What this does NOT fix" below). `src/composables/useWkImeGuard.js` is unmodified pending that confirmation, per this doc's own "no v3 guard without the dump" decision.

**Design constraint (from the orchestrator, restated so the reviewer can check it was honoured):** the current all-ASCII-letters exclusion in `classify()` exists ONLY to keep named DOM `key` values (`'Enter'`, `'ArrowUp'`, `'Backspace'`, `'Dead'`, `'Shift'`, …) out of the `multiCarrier` branch — not to make any claim about the carrier's character content. Testing the payload's characters is what makes the branch wrong for a raw-ASCII restore carrier (`"ex"`); testing whether `ev.key` is one of the finite, spec-defined **named key values** is the same exclusion without that false assumption. A named key's `key` value is drawn from a fixed vocabulary (UI Events `KeyboardEvent.key` Key Values spec) — modifiers, editing/navigation keys, function keys, composition/IME meta-keys, lock keys — while any `key` value that is NOT in that vocabulary and has `length > 1` is, by construction, a synthetic multi-character carrier (Vietnamese-accented or plain ASCII, restore or correction), because physical single-key presses never produce an un-named multi-character `key`.

```js
// Named DOM `key` values reachable with focus in a terminal element — the finite vocabulary this
// engine class's synthetic multi-char carrier can never collide with, because a real key press
// only ever emits one of these OR a single printable character (key.length === 1). Enumerated from
// the UI Events KeyboardEvent `key` Key Values spec rather than inferred from character content,
// per the design constraint above: this is what actually distinguishes "named key" from "carrier
// payload", not whether the payload happens to contain a non-ASCII character.
const NAMED_KEYS = new Set([
  // Modifiers
  'Alt', 'AltGraph', 'CapsLock', 'Control', 'Fn', 'FnLock', 'Hyper', 'Meta',
  'NumLock', 'ScrollLock', 'Shift', 'Super', 'Symbol', 'SymbolLock',
  // Whitespace / editing
  'Enter', 'Tab', 'Backspace', 'Clear', 'Copy', 'CrSel', 'Cut', 'Delete',
  'EraseEof', 'ExSel', 'Insert', 'Paste', 'Redo', 'Undo',
  // Navigation
  'ArrowDown', 'ArrowLeft', 'ArrowRight', 'ArrowUp', 'End', 'Home', 'PageDown', 'PageUp',
  // UI / device
  'Accept', 'Again', 'Attn', 'Cancel', 'ContextMenu', 'Escape', 'Execute', 'Find',
  'Help', 'Pause', 'Play', 'Props', 'Select', 'ZoomIn', 'ZoomOut',
  'BrightnessDown', 'BrightnessUp', 'Eject', 'LogOff', 'Power', 'PowerOff',
  'PrintScreen', 'Hibernate', 'Standby', 'WakeUp',
  // Composition / IME meta-keys (distinct from an actual in-progress composition, which
  // `composing`/`ev.isComposing` already excludes above this check)
  'AllCandidates', 'Alphanumeric', 'CodeInput', 'Compose', 'Convert', 'Dead',
  'FinalMode', 'GroupFirst', 'GroupLast', 'GroupNext', 'GroupPrevious', 'ModeChange',
  'NextCandidate', 'NonConvert', 'PreviousCandidate', 'Process', 'SingleCandidate',
  'HangulMode', 'HanjaMode', 'JunjaMode', 'Eisu', 'Hankaku', 'Hiragana',
  'HiraganaKatakana', 'KanaMode', 'KanjiMode', 'Katakana', 'Romaji', 'Zenkaku',
  'ZenkakuHankaku',
  // Function keys
  'F1', 'F2', 'F3', 'F4', 'F5', 'F6', 'F7', 'F8', 'F9', 'F10', 'F11', 'F12',
  'F13', 'F14', 'F15', 'F16', 'F17', 'F18', 'F19', 'F20', 'F21', 'F22', 'F23', 'F24',
  'Soft1', 'Soft2', 'Soft3', 'Soft4',
  // Not a key value per se, but the spec's own escape hatch when the UA cannot map a
  // physical key at all — never a synthetic carrier payload either.
  'Unidentified',
])

function classify(ev) {
  if (composing || ev.isComposing) return null
  if (ev.keyCode === 229) return 'synthetic229'
  // Multi-char `key` = the unicode-string carrier (kVK 0 + CGEventKeyboardSetUnicodeString).
  // Excluded by NAMED_KEYS membership, not by character content: a carrier's payload can be
  // Vietnamese-accented ("gì", the correction path) OR plain ASCII ("ex", the auto-restore
  // path) — both are real synthetic carriers, neither is a named key, so both must classify
  // the same way. The previous /^[A-Za-z]+$/ regex conflated "is plain ASCII" with "is a named
  // key", which happened to hold for 'Enter'/'Backspace'/'Shift' but is false for an ASCII-shaped
  // restore payload like "exit"'s "ex" — that false premise is exactly what dropped the `x`.
  if (ev.key && ev.key.length > 1 && !NAMED_KEYS.has(ev.key) && !ev.metaKey && !ev.ctrlKey) {
    return 'multiCarrier'
  }
  return null
}
```

**Per-shape claimant check** (every shape from the -2 doc's delivery matrix plus this doc's restore-path finding, confirming exactly one claimant each, no shape gaining a second):

| Shape | Old `classify()` | New `classify()` | Changed? |
|---|---|---|---|
| 229 Backspace/Enter | `synthetic229` (matched before the multiCarrier check is ever reached) | `synthetic229` | No — `keyCode === 229` short-circuits first in both versions |
| 229 printable carrier (correction path, diacritic) | `synthetic229` | `synthetic229` | No — same short-circuit |
| non-229 multi-char Vietnamese carrier (`"gì"`, correction path) | `multiCarrier` (regex fails on the diacritic → excluded from the all-ASCII test → falls through to the branch) | `multiCarrier` (`"gì"` not in `NAMED_KEYS`) | No |
| non-229 multi-char **ASCII** carrier (`"ex"`, restore path — the bug) | `null` (regex matches all-ASCII-letters → wrongly excluded → guard stands down → xterm drops the rest) | `multiCarrier` (`"ex"` not in `NAMED_KEYS`) | **Yes — this is the fix** |
| Named key, all letters (`'Enter'`, `'Backspace'`, `'Escape'`, `'Shift'`, `'Dead'`, …) | `null` (regex matches, correctly excluded) | `null` (in `NAMED_KEYS`, correctly excluded) | No |
| Named key with non-letter chars (`'F1'`–`'F24'`, digits present) | `multiCarrier` — **regex-only exclusion missed these** (`/^[A-Za-z]+$/` fails on the digit, so the old code let function keys fall into the carrier branch) | `null` (in `NAMED_KEYS`) | **Yes — incidental latent-bug fix**, not this doc's reported symptom, flagged for reviewer awareness since it changes behavior even though nobody reported it |
| Single printable char (`'a'`, `'1'`, `'!'`) | `null` (`key.length > 1` false) | `null` (same guard) | No |
| Any key with `metaKey`/`ctrlKey` held | `null` (guarded off) | `null` (same guard) | No |
| Real composition in progress (`composing` or `ev.isComposing`) | `null` (short-circuits first) | `null` (same short-circuit) | No |

Exactly one shape changes from no-claimant to `multiCarrier` (the restore payload — the reported bug) and exactly one changes from a wrong claimant to no-claimant (function keys — an unreported latent defect this rewrite happens to also close). Every other shape's claimant is provably unchanged, since the `keyCode === 229` and `composing` checks that gate them run before `NAMED_KEYS` is ever consulted.

### What this change does NOT fix

- **Candidate 2** (229-tagged restore carrier with no claimable `beforeinput`) — untouched. This patch only changes which keys `classify()` routes into `multiCarrier`; a key that is tagged `keyCode 229` never reaches that branch at all (the first `if` returns `synthetic229` before `NAMED_KEYS` is checked), so if the restore carrier turns out to arrive 229-tagged, this patch is a no-op for it and the loss (if any) is in the `beforeinput`/textarea-diff path this doc's own Result section already flags as unresolved.
- **Candidate 3** (decomposed-Unicode backspace-count mismatch) — untouched. `classify()` only decides which branch a keydown's *insertion* payload is routed to; it has no bearing on how many Backspace keydowns precede it or how many UTF-16 units the deleted text actually spans.
- **Any true composing IME** (macOS built-in Vietnamese) — untouched by construction, guarded off by the `composing`/`ev.isComposing` check that runs first in both the old and new code.
- **The sticky-Ctrl 1-byte remap collision** noted in the -2 doc as a known benign phone-only limit — unrelated code path, not touched here.

### Verification gate

This patch stays a proposal — `src/composables/useWkImeGuard.js` is unmodified — until:
1. `scripts/capture-ime-evidence.sh`'s recipe is run on the Mac and confirms Candidate 1 per the discriminator table above, AND
2. the per-shape claimant check above is re-verified against the actual dump (not just the static trace here) to confirm no shape gained a second claimant in practice, not only on paper.

If the dump instead confirms Candidate 2 or 3, this patch must NOT be applied — open a `-4` doc with that finding and design against the confirmed mechanism instead, per this doc's own standing rule: no guard change without the dump that confirms which mechanism is real.
