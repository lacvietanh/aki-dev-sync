# Vietnamese typing breaks in the in-app terminal — true root cause

Status: fix superseded by `terminal-vietnamese-ime-root-cause-2.md` (guard v1 failed on-Mac verification — the root-cause analysis below stands; the Action's implementation was replaced by v2)

**Start time**: 2026-07-27 19:20 +08 (multi-agent investigation, 3 web + 2 code/reasoning agents, transcript at `/tmp/aichat.md` session 1785151507 — ephemeral, key content reproduced here)

## Initial purpose

Direct typing with a Vietnamese Telex engine (OpenKey; user report) garbles in the in-app terminal — dropped characters, under-deleted syllables — while VS Code's integrated terminal (same xterm.js family) has handled Telex fine for years. 1.22.0's working tree already carries a workaround (the compose row rendered on the Mac too), whose inline rationale blamed the terminal's no-local-echo design (T-5) for **all** of the breakage. Question: what is the *true* root cause, and can direct typing be fixed outright rather than routed around?

Context at the time: `@xterm/xterm` pinned 5.5.0; Tauri v2 ⇒ the webview is **WKWebView** (WebKit), not Chromium; app ships macOS-only; PTY input is per-keystroke over IPC with zero local echo (`usePtyTerminal.js` `wireInput`/`sendRaw`).

## Strategy

Five concurrent agents with adversarial cross-review over a shared chat file: (1) how xterm.js/VS Code fixed IME composition, and what is still open on WebKit; (2) Tauri/wry/WKWebView IME bug surface; (3) how OpenKey-family engines actually generate input; (4) read-only trace of this repo's entire key path; (5) a skeptic maintaining competing hypotheses. Orchestrator injected one decisive challenge (see Result §"the echo-latency theory falls").

## Checklist

- [x] xterm.js 5.5.0 input pipeline read from source (`src/browser/Terminal.ts`, `input/CompositionHelper.ts`, tag 5.5.0)
- [x] Open upstream issues matched to the symptom (#5887, #5894, #5704, #6041)
- [x] OpenKey mechanism verified from its own source/docs (CGEventTap, backspace technique)
- [x] Repo key path traced end-to-end; app-side interference ruled out
- [x] Tauri/wry issue trackers swept — nothing IME-relevant at that layer
- [x] Fix implemented (`src/composables/useWkImeGuard.js`) — **runtime behaviour on a real Mac + OpenKey: unverified, see Verification**

## Result

**Two distinct causes; the one behind the user's OpenKey symptom is upstream xterm.js 5.5.0 mishandling WKWebView's `keyCode 229` tagging of synthetic keystrokes — not this app's code, not Tauri/wry, and not (for OpenKey) the no-local-echo design.**

1. **OpenKey/EVKey are not IMEs.** They run a CGEventTap and retype each syllable as a burst of raw synthetic keydowns — N backspaces + corrected characters. No `compositionstart/update/end` ever fires; the flow is open-loop (terminals cannot serve the AX readback such engines would need — a terminal's `AXValue` is the whole scrollback blob).
2. **The echo-latency theory falls for OpenKey.** Because the injection is open-loop and the PTY consumes bytes strictly in arrival order, display latency alone cannot corrupt the shell's line buffer — iTerm2/VS Code are PTY-echo too and work. The corruption requires keystrokes to be **dropped, collapsed, or duplicated before reaching the PTY**.
3. **That is exactly what xterm 5.5.0 does under WKWebView.** WebKit tags the synthetic keys `keyCode 229` ("composition character") even with no composition active, shunting them into IME fallback paths with three defects (all reproduced from 5.5.0 source):
   - `_keyDownSeen` set on every keydown, reset only on keyup ⇒ in a fast burst the `input`-event fallback drops chars (`Terminal.ts` L1003/L1100/L1176);
   - `CompositionHelper._handleAnyTextareaChanges()` diffs the hidden textarea on `setTimeout(0)` ⇒ same-tick keys diff wrong, and a shrinking textarea emits exactly **one** `DEL` however many chars were deleted ⇒ OpenKey's backspace burst collapses to one — the observed "deletes wrong number of characters";
   - WebKit's synthetic `keypress` on dead-key/composition commit re-emits the char (duplication).
   Chromium dispatches CGEvent-injected keys as ordinary keydown/keypress/input, so VS Code never enters these paths — that is the whole VS Code difference.
4. **True composing IMEs (macOS built-in Vietnamese) are a separate case**: they do use marked-text composition, WKWebView's composition-event delivery is genuinely broken/divergent there (#5704 Korean analogue), and the no-local-echo screen additionally cannot host a preedit sanely. For them the compose row remains the supported path.
5. App code was cleared: no `attachCustomKeyEventHandler`, no key/composition listeners in the terminal's path; `TerminalStack.vue`'s window capture only acts on ⌘-combos.

**Verification**: mechanism verified by direct source read of xterm 5.5.0 (not paraphrased from issues) and by symptom-shape match with three independent upstream reports; OpenKey mechanism verified from its repository. **The shipped fix is NOT yet runtime-verified** — it requires a Mac build with OpenKey active. Test recipe: build, open in-app terminal, type `tieengs vieejt as` fast and slow directly into the terminal; repeat with `localStorage['aki-ime-debug']='1'` (Safari Web Inspector → the app's WKWebView) to see what WKWebView delivers; `localStorage['aki-ime-guard']='off'` reverts to stock behaviour for A/B. Also regression-check plain English fast typing and macOS built-in Vietnamese (should behave no worse; compose row still its path).

**Corroborating links**:
- https://github.com/xtermjs/xterm.js/issues/5887 — 2nd char lost when keyCode 229 reported for every key; unfixed 5.5.0→6.1.0-beta
- https://github.com/xtermjs/xterm.js/issues/5894 — WKWebView dead-key synthetic keypress duplicates + drops; unfixed
- https://github.com/xtermjs/xterm.js/issues/5704 — WKWebView omits composition events (Korean analogue)
- https://github.com/xtermjs/xterm.js/issues/6041 — composition finalize double-send (closed, unmerged)
- https://github.com/tuyenvm/OpenKey — CGEventTap + backspace technique
- https://github.com/tuyenvm/OpenKey/issues/319 — same symptom class in Claude Code's terminal (different host, same engine)
- https://github.com/ghostty-org/ghostty/issues/9932 — terminals cannot serve AX readback (why the engines run open-loop)
- https://bugzilla.mozilla.org/show_bug.cgi?id=1312649 — precedent: Telex-family engines probing NSTextInputClient (true-IME path only)

## Decision

**Action** — `src/composables/useWkImeGuard.js` (capture-phase guard: takes over 229-tagged non-composing keystrokes before xterm's fallback paths can drop them; kill switch `aki-ime-guard=off`, event logger `aki-ime-debug=1`), wired in `src/components/TerminalView.vue`. Compose row kept as-is for true-composition IMEs.

**No action** on: vendoring/patching xterm itself (chasing three unmerged upstream fixes with WebKit-specific branches — high maintenance for the same outcome the guard achieves at the app layer); switching OpenKey modes or documenting per-app IME settings (cannot ship a fix inside someone else's utility).

**Follow-up research** — if the Mac test still garbles with the guard on, the `aki-ime-debug` event log is the next doc's raw input (it distinguishes "WKWebView never delivered the key" from "delivered but mishandled").

**Cross-references**: `docs/feat/in-app-terminal.md` (compose-row rationale revised), `README.md` + `IntroModal.vue` (Vietnamese-typing wording), `CHANGELOG.md` [Unreleased], comment block in `TerminalView.vue`.
