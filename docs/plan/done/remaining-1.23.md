# Remaining after 1.23.0 — CLOSED, 1.23.0 released 2026-08-03

Everything that was outstanding after the `1.23.0` SimpleView finalize pass. Written 2026-08-01; the two release gates (SV-RESET, SV-BUILD) and the terminal copy/paste items (3-5) were all hand-tested on the Mac 2026-08-03 per `docs/plan/done/handtest-1.23.md` and passed. Continues `docs/plan/remaining-1.22.md` (that file's own remaining items still stand, unrelated to this release).

## Live — 2 gates, both "run it on the Mac", not open work

| # | ID | Description | Kind | Blocks 1.23.0? |
|---|---|---|---|---|
| 1 | SV-RESET | SimpleView `reset:true` now clears `lines` (blocker fixed in `src/composables/usePtyStream.js`) — needs on-Mac runtime confirm | verify | **Resolved — passed on Mac 2026-08-03** |
| 2 | SV-BUILD | `npx vite build` + `cargo check` never ran on the dev box (partial `node_modules` missing `@xterm/xterm`; Rust compiles Mac-only) | verify | **Resolved — clean build on Mac 2026-08-03** |

### 1. SV-RESET — reset-frame scrollback clear
Fixed in working tree, unverified at runtime. `resetDisplay()` (`usePtyStream.js`) cleared only parser+buffer, not `lines`, so a `reset:true` frame (`pty.rs:239` = "replace everything") appended fresh output below stale scrollback on the phone. Four emit points send that frame: Rust `pty_clear`/`pty_restart` via `emit_reset()` (`pty.rs`), and JS `pushAllScrollbacks()` via congestion-resync and companion-join/reconnect (`ptyBridge.js`). Now `handleLiveFrame`'s `if (frame.reset)` branch clears `lines` directly, alongside calling `resetDisplay()` for parser+buffer; EXIT (keeps scrollback) and start-hydration unchanged. Not covered by `test-ansistrip.mjs`.
**On Mac:** with a phone paired — CLEAR, RESTART, reconnect, force a congestion-resync; confirm the phone screen replaces rather than appends.

### 2. SV-BUILD — build/typecheck gates unrun on the dev box
`test-ansistrip.mjs` (5/5) and `lint:simpleview` passed on the dev box. `npx vite build` **failed** there — `@xterm/xterm`/`@xterm/addon-fit` declared in `package.json` but absent from the box's incomplete `node_modules`; an install gap on the Mac xterm view (`TerminalView.vue`), **not** a SimpleView defect. `cargo check` skipped (Rust Mac-only); the Antigravity-logout deletion cleared statically by grep (no dangling ref, `invoke_handler`/`mod.rs` cleaned, no JS caller) but not compiled.
**On Mac:** `npm install`, then `npx vite build`, then the Rust build; then `npm run build:rmud`, tag `1.23.0`, push, `gh release`.

## Terminal copy/paste — flow-traced 2026-08-02, unbuilt (owner-reported against build 1.22.1706)

Two defects reported in the in-app terminal: (1) pasting multi-line text auto-submits each line as a separate command; (2) copying text out is "gần như không được" when a mouse-mode TUI (claude/agy, or a TUI over ssh) is running. Investigated by flow-tracing only (dev box can't run/build the Tauri app). **Key fact: neither is a 1.23 regression** — `TerminalView.vue` and `useTerminalTextDrain.js` (the two files that own the Mac paste/copy path) are byte-identical to HEAD; the 1.23 refactor only touched the phone `SimpleView` path. So the 1.22-reported behavior is unchanged in the working tree.

| # | ID | Description | Kind | Status |
|---|---|---|---|---|
| 3 | TERM-PASTE | Multi-line paste auto-submits each line — mechanism narrowed to 2 candidates, decisive diagnostic + robust fix below | fix | **Verified on Mac 2026-08-03: no per-line auto-submit in claude/agy or a bare shell.** No code change was needed for this release; the "robust fix" (app-owned paste handler, §3) remains open, optional future work for discoverability/robustness, not a defect |
| 4 | TERM-COPY | Copy out impossible under mouse-mode; `user-select` exemption is a **no-op** for xterm | fix | **Fixed and verified on Mac 2026-08-03** — `macOptionClickForcesSelection: true` added to `TerminalView.vue`, Option-drag now selects under mouse-mode TUIs, `⌘C` copies. CHANGELOG 1.23.0 |
| 5 | SV-SELECT | `SimpleView` plain-text stream `.sv-stream` added `user-select: text` in `SimpleView.vue` | landed | verified in code |

### 3. TERM-PASTE — multi-line paste executes line-by-line

Verified paste flow (source: `node_modules/@xterm/xterm` clipboard handler): `⌘V → xterm 'paste' listener → handlePasteEvent` (calls only `stopPropagation()`, **not** `preventDefault()`) `→ bracketTextForPaste(text, bracketedPasteMode) → triggerDataEvent(wrapped, true) → term.onData → emitKey({char}) → sendRaw → pty_write`. When the receiving program has bracketed-paste on (Claude Code does) and the paste travels this path, the block is wrapped in `\x1b[200~…\x1b[201~` and does **not** split — correct. The reported symptom means the wrapped path did not deliver. Two candidate mechanisms, not separable without one runtime observation:
- **Candidate 1 (primary suspect): the drain claims the paste.** `src/composables/useTerminalTextDrain.js` `onInputCapture` passes an `input` event through only when `inputType` is one of `insertComposition*`/`insertFromComposition`/`insertFromPaste`/`insertFromDrop`. If WKWebView delivers the paste as an `input` event whose `inputType` is anything else (empty, `insertText`), the drain claims it, runs `raw.replace(/\r\n|\n|\r/g, '\r')` (every newline → CR), and `term.input(text, true)` sends the whole block as one PTY write full of `\r` — a readline shell treats each `\r` as accept-line, so every line executes.
- **Candidate 2:** the target program had bracketed-paste off at that moment.
- **Latent secondary bug — FIXED 2026-08-03, and it refutes Candidate 1.** Because xterm does **not** `preventDefault` on paste, the browser's default paste also fired and re-inserted the text into the textarea; the next uncancelled keystroke then dragged that residue along. This shipped as the reported "paste then press SPACE doubles the text" defect and is fixed in `useTerminalTextDrain.js` by draining the textarea on `insertFromPaste`. It is specifically SPACE and not any key because `Keyboard.ts:381` only sets `result.key` for `keyCode >= 48`, so letters/digits/punctuation are force-cancelled by xterm at `Terminal.ts:1078-1079` and never reach the browser's insertion path. **The existence of that residue proves WKWebView does report `insertFromPaste`** — had the drain claimed the paste, the textarea would have been empty and SPACE could not have doubled anything. Candidate 1 is therefore out; the remaining multi-line auto-submit is Candidate 2 (bracketed paste off in the receiver), plus `Clipboard.ts:14`'s `\n`→`\r` conversion.

**Decisive diagnostic (no guessing, ~10s on the Mac).** The app ships a pull-based ring at `window.__akiTermInput`. In the webview DevTools: paste once, then `__akiTermInput.tail(10)`. A `drained` entry containing the full multi-line text ⇒ Candidate 1 (drain claimed it, `inputType` will be shown). Only an `input-paste-cleared {inputType:'insertFromPaste'}` entry ⇒ xterm handled it and the issue is bracketed-mode/receiver side. Note the label changed from `input-passed` when the paste residue fix landed.

**Robust fix (removes the WKWebView-inputType dependency entirely).** Attach an app-owned `paste` handler on `term.textarea`: `preventDefault()` + `stopPropagation()`, read `e.clipboardData.getData('text/plain')`, then `sendRaw` the text wrapped in `\x1b[200~…\x1b[201~` **iff** `term.modes.bracketedPasteMode`, else send raw with `\n`→`\r`. This makes the app the single owner of paste, kills the double-processing/leftover, and works no matter what `inputType` WKWebView reports. **`term.modes.bracketedPasteMode` is confirmed a real public accessor in this xterm build** — this is exactly the accessor `TerminalView.vue`'s `onComposeSend` comment flagged as "unverified (xterm 5.x)"; now verified, so it ALSO unblocks the compose-row multi-line decision (§ that comment) the same way.
**On Mac:** run the diagnostic first to confirm Candidate 1; then apply the paste handler; test pasting a multi-line block into claude/agy and into a bare shell prompt — expect one paste, no per-line execution.

### 4. TERM-COPY — cannot select/copy out under a mouse-mode TUI

Correction to the intuitive "exempt `user-select`" fix: **it will not help xterm.** `css/xterm.css:41` sets `.xterm { user-select: none }` itself and xterm uses its own selection model (`term.getSelection()` reads the buffer, `⌘C` via xterm's own copy handler) — independent of the page's `user-select`. The app's global `user-select:none` (`src/assets/main.css:51`) is therefore not the cause here.

Real cause: claude/agy (and TUIs over ssh) enable **mouse-tracking mode** (DECSET 1000/1002/1006); xterm then forwards mouse-drag as mouse events to the program instead of building a selection, so there is nothing for `⌘C` to copy. `dock/TerminalStack.vue`'s `onKeydown` was checked and does **not** intercept `⌘C`/`⌘V` — not a factor.
- **⌥-drag was NOT a zero-code escape hatch — correction.** xterm's `SelectionService.shouldForceSelection` on macOS requires `altKey && rawOptions.macOptionClickForcesSelection`, and that option defaults to `false`; this project never set it, so ⌥-drag+⌘C did not actually work as this doc previously claimed. Fixed by adding `macOptionClickForcesSelection: true` to the `Terminal` constructor options in `TerminalView.vue:411`.
- **Remaining robust fix options (still open, discoverability):** (a) a right-click context menu "Copy"/"Paste" using the public `term.getSelection()` + the TERM-PASTE handler; (b) a "select mode" toggle that suspends mouse forwarding; (c) teach ⌥-drag in `IntroModal.vue`/README. `getSelection`/`hasSelection`/`onSelectionChange` are all public.
**On Mac:** verify the `macOptionClickForcesSelection` fix — Option-drag under a mouse-mode TUI must now highlight a selection, and ⌘C must copy it — before deciding whether (a)/(b)/(c) are still worth building for discoverability.

### 5. SV-SELECT — SimpleView stream needs the `user-select` exemption (the owner's instinct, correct here)

Unlike xterm, `SimpleView.vue`'s `.sv-line` divs are ordinary DOM text and **are** blocked by the global `user-select:none` (`main.css:51`), which opts back in only `input, textarea, [contenteditable], .u-select-text` (`main.css:62-69`). The phone stream has none of those, so its output can't be selected/copied. Fix: add `user-select: text` (or the `.u-select-text` class) to `.sv-stream`. CSS-only, phone-only, no type surface — safe to apply on the dev box.
**Note:** this is where "miễn trừ đúng các nơi cần copy" actually applies; TERM-COPY (xterm) does not.

## Resolved 2026-08-01 (this pass, from the repo alone)

- **LOGOUT-DOCS** — 3 live docs that still described the removed Antigravity Log Out feature are fixed: `docs/arch/usage-antigravity.md` (Log Out sections collapsed to a "removed in 1.23.0" note; usage-probing content kept), `docs/index.md:26` (dropped the "smart multi-environment logout" advertisement → "per-account cache retention"), `CLAUDE.md:23` (canonical async example repointed from the deleted `logout_antigravity` to the surviving `agent_usage/mod.rs`'s `get_agent_usage`, verified to exist). Historical mentions in `docs/plan/done/*` and `docs/research/*` deliberately left — immutable event records.
- **SV-NITS** — SimpleView code nits fixed in `usePtyStream.js` (per-newline array rebuild → `commitLine` push+splice; unbounded `buffer` now capped at `MAX_LINE=65536`) and `ansiStrip.js` (`@param` doc corrected to decoded-UTF-8). `useTerminalViewType.js` `computed` nit left as-is (harmless; removing it risks the `<component :is>` binding). test 5/5 + lint re-verified green.
- **PLAN-SPLIT** — decided: keep `docs/plan/done/wish-terminal-split-simpleview.md` as one file with a two-milestone status header (SimpleView ✅ built/1.23.0, Right-Dock still unbuilt), **not** moved to `done/` (until 1.25.0 right-dock shipped). Splitting a doc whose Right-Dock half was live wishlist added extraction risk for little gain.
