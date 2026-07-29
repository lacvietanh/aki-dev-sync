Status: superseded by `docs/plan/terminal-ime-input-layer-separation.md` (the delivery-shape guard v2 is replaced by the input-layer separation — `disableStdin` + app-owned textarea overlay). Root-cause analysis in this doc and `-jul27.md` stands.

# Vietnamese typing in the in-app terminal — guard v1 post-mortem, delivery-shape matrix, guard v2

Chain: follows `terminal-vietnamese-ime-root-cause-jul27.md` (root cause of the original breakage; its Decision's "Follow-up research" clause is this doc).

## Crux — read this first

**Typing Vietnamese directly in the terminal works on the Mac as of guard v2 (2026-07-27).** The one thing v1 got wrong and v2 got right, in one sentence: *v1 severed xterm's delivery channels while covering only three `beforeinput` shapes, so any other shape had NO claimant; v2 classifies each keydown and guarantees exactly one claimant for every shape OpenKey can produce — crucially including the multi-char unicode-string carrier, which is how OpenKey actually sends a corrected syllable.*

The load-bearing discovery behind that, from OpenKey's own source, is that **a corrected syllable arrives as ONE synthetic key event carrying the whole string** (`CGEventKeyboardSetUnicodeString`, carrier keycode 0), not as one event per character. Every earlier symptom follows from code that assumes one event = one character: xterm's `_keyPress` emits only the first UTF-16 unit (the historical "ăn gì" → "ăn g"), and v1's keypress veto removed even that without covering the carrier itself ("ăn gì" → "ăn ").

Two claims here are still **runtime-unconfirmed**, and both are one console command away — do this before treating the crux as closed (it is why `window.__akiIme` exists):
1. *That the guard is what fixes it* — `localStorage['aki-ime-guard']='off'`, reopen the terminal tab, type Vietnamese. It must break again. If it does not, the guard is not the cause and this doc's Result is wrong.
2. *Which claimant does the work* — `__akiIme.clear()`, type one Vietnamese word, `__akiIme.status()`. The `counts` field names it: `sentAtBeforeInput` vs `sentAtInput` vs `sentAtKeydown`, and `keydownCarrier` > 0 confirms the multi-char carrier shape is real on this machine.

**Second finding, about method rather than about the bug** — the first debug layer (`console.log` gated on a `localStorage` flag read at mount) produced **zero evidence on the Mac** even with the flag set, and the flag-set console reported `"1"`. Three independent failure modes, any one of which is enough:
- Safari's Web Inspector does not replay console output emitted before it attached;
- **the flag must live in the app page's origin, and the Develop menu makes that easy to get wrong**: once an inspector window is open, Safari lists *its own UI* as an inspectable target named **`Main.html`** (`inspector-resource:///Main.html`) right next to the real ones. Under the machine's submenu, this app in `tauri dev` appears as **`localhost`** (devUrl `http://localhost:1420`), and its inspector window is titled *Web Inspector — localhost*. Typing the flag into the `Main.html` console sets it in `inspector-resource://`'s storage, where it reads back `"1"` and the app never sees it — the exact trap that cost this investigation a full round. Confirm the target with `document.title === 'Aki Dev Sync'`, or just `__akiIme.status().page`;
- a mount-time flag read ignores a flag set afterwards. The replacement records every event into an always-on bounded ring and returns it from `__akiIme.dump()`/`tail()`/`status()` — a returned value is printed by the console evaluator regardless of log filters or attach timing. **Rule for next time: diagnostics must be pull-based (recorded always, read on demand), never push-based through a channel that has to be armed correctly in advance.**

**Start time**: 2026-07-27 19:55 +08 (second multi-agent round: OpenKey source reader, WebKit event researcher, adversarial guard reviewer; chat transcript `/tmp/aichat.md` session 2 — ephemeral, key content reproduced here)

## Initial purpose

Guard v1 (`useWkImeGuard.js`, from the -jul27 doc's Action) failed on-Mac verification and CHANGED the symptom: pre-guard "ăn gì" → "ăn g" (partial retype survived); with v1 "ăn gì" → "ăn " (retyped chunk lost entirely). Question: which exact mechanism did v1 get wrong, what are ALL possible WKWebView delivery shapes for OpenKey's injected events, and what guard design gives every shape exactly one claimant (no drop, no double-send)?

Context: `npm run tauri dev` on the user's Mac (this dev box cannot build Tauri); xterm pinned 5.5.0; OpenKey is the engine in use.

## Strategy

Three agents: (1) read OpenKey's macOS source for the exact injection calls; (2) source WebKit's keyCode-229 tagging path and beforeinput coverage for textareas; (3) adversarial flow-audit of guard v1 against the new symptom (METHOD-flow-audit), required to attack the orchestrator's own hypothesis. Orchestrator synthesized into a delivery matrix and implemented v2.

## Checklist

- [x] OpenKey injection mechanics from source (function-level citations)
- [x] WebKit 229-tagging origin; beforeinput-on-textarea coverage assessed
- [x] v1 loss paths enumerated and ranked with code citations
- [x] v2 delivery matrix designed (each shape → exactly one claimant) and implemented
- [ ] **Runtime verification on the Mac — PENDING**, incl. the one fact research cannot settle (below)

## Result

**OpenKey injection (from `tuyenvm/OpenKey` source, `OpenKey.mm` / `Engine.cpp`):**
- Backspaces: N reused keydown/keyup pairs, kVK 51, posted back-to-back synchronously inside one tap callback (`SendBackspace()`).
- Replacement: **ONE** keydown/keyup pair, carrier kVK **0**, whose `CGEventKeyboardSetUnicodeString` payload is the WHOLE retyped chunk (≤16 UniChars, e.g. "gì" as one event) — `SendNewCharString()`. Per-char events only if the user enabled "Gửi từng phím" (`vSendKeyStepByStep`).
- The physical tone key ('f') is swallowed (`return NULL`) — the webview never sees it.
- Delete/retype window is computed per phonology (`VWSM` word-start-mark), not "whole syllable always".

**Why v1 made it worse (adversarial audit, confirmed against v1 code + xterm 5.5.0 source):**
- v1's `beforeinput` handler was the SOLE delivery channel for taken-over printables but matched only `insertText`/`deleteContentBackward`/`insertLineBreak`. `insertReplacementText` (payload in `ev.dataTransfer`, `ev.data` empty) fell through unclaimed; xterm's `_inputEvent` fallback was simultaneously dead (`_keyDownSeen` still true mid-burst). Two stacked silent gates ⇒ total loss.
- v1's keypress veto ran for ANY multi-char `key` regardless of takeover state — killing the accidental channel that used to deliver the partial retype pre-guard (`_keyPress` emits charCode = FIRST UTF-16 unit of the payload: that is precisely the old "ăn g").
- v1's `stopPropagation` on Backspace keydowns also severed xterm's textarea-diff safety net where preventDefault alone would have neutralized it harmlessly.
- Newly flagged, unrelated to v1: sticky-Ctrl (`usePtyTerminal.js` `wireInput`, armed 1-byte remap) can swallow a guard-sent single byte if armed mid-burst — phone-only surface, recorded as a known benign limit, no fix.

**v2 design (implemented in `useWkImeGuard.js`): per-keydown classification, one claimant per shape.**

| Delivery shape | Claimant | Why no double |
|---|---|---|
| 229 Backspace/Enter (incl. empty textarea → no beforeinput at all) | guard at keydown, `preventDefault` only | xterm's scheduled diff sees unmutated textarea → no-op |
| 229 printable, beforeinput fires (insertText / insertReplacementText via dataTransfer / delete / linebreak) | guard at beforeinput, prevent+stop | prevented mutation starves xterm's diff |
| 229 printable, NO beforeinput | guard stands down → xterm's own textarea diff | guard sent nothing |
| non-229 multi-char carrier (`key`="gì"), beforeinput fires | guard at beforeinput | keypress vetoed (else xterm emits first-unit-only); no diff exists for non-229 keys |
| non-229 multi-char carrier, no beforeinput, `input` fires | guard at `input` fallback, stopPropagation | only channel: no diff (non-229), keypress vetoed, `_inputEvent` stopped |
| any composition-flavored event (`insertCompositionText`, `insertFromComposition`, `isComposing`) | never claimed — xterm/compose-row path | guard inert |

Keypress veto now fires ONLY while a classified key is in flight (v1: unconditional for multi-char keys).

**Verification**: v1 failure mechanism confirmed statically (line-level, against 5.5.0 source and v1 code). OpenKey injection facts verified from its repository. **v2 is confirmed by user acceptance on the Mac (typing Vietnamese directly in the terminal behaves), but the causal attribution and the claiming channel are not yet instrument-confirmed** — see the two commands in §Crux. One load-bearing fact is empirically open and undecidable from documentation: *whether WKWebView fires `beforeinput` (and with which inputType/dataTransfer) for CGEvent-injected text into a `<textarea>`* — WebKit documents Input Events for contenteditable only. v2 is designed to deliver exactly-once under every answer to that question, but the answer itself must come from the `aki-ime-debug=1` log on the Mac (recipe in the -jul27 doc; v2 logs additionally carry `dataTransfer` types, `isComposing`, per-key class, and `input` events).

**Corroborating links**:
- https://github.com/tuyenvm/OpenKey — `Sources/OpenKey/macOS/ModernKey/OpenKey.mm` (`SendBackspace` L369-382, `SendNewCharString` L423-513, callback L601-790), `engine/Engine.cpp` (`handleModernMark` L698-709)
- xterm 5.5.0 `src/browser/Terminal.ts` L1003 (`_keyDownSeen` set), L1100 (reset at keyup), L1129-1160 (`_keyPress` charCode emit), L1172-1191 (`_inputEvent` gate); `CompositionHelper.ts` L96-118, L184-201 (229 diff path)
- https://github.com/xtermjs/xterm.js/issues/5887 · /5894 (upstream, still open)
- webkit.org/blog/7358 (Input Events scope = contenteditable; textarea coverage undocumented)
- WebKit `WindowsKeyboardCodes.h` (VK_PROCESSKEY 229 legacy) — NSTextInputContext path tags both real IME and CGEvent-injected keys

## Decision

**Action** — `src/composables/useWkImeGuard.js` rewritten to v2 (this doc's matrix); doc corrections in `CHANGELOG.md`, `README.md`, `IntroModal.vue`, `docs/feat/in-app-terminal.md` to stop overclaiming v1 as shipped-fixed.

**Follow-up research** — the two §Crux commands close the remaining attribution gap; record their output here only if it CONTRADICTS the crux (a contradiction opens doc `-3`; a confirmation needs no new doc). If Vietnamese input regresses later, `__akiIme.dump()` output is the raw material for `-3` — do not attempt a v3 guard without it.

**Cross-references**: `terminal-vietnamese-ime-root-cause-jul27.md` (Status line added pointing here), `docs/feat/in-app-terminal.md` §Vietnamese input, `docs/arch/terminal-stack.md` (input pipeline, unchanged), `usePtyTerminal.js` sticky-Ctrl note above.
