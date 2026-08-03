# Hand-test checklist — 1.23.0 (Mac only)

Written 2026-08-03. Version state: `package.json` reads `1.23.0` (working tree, uncommitted-to-release), but the newest real tag is still `1.22.0` (`git tag -l | sort -V | tail -5` → 1.18.0, 1.19.0, 1.20.0, 1.21.0, 1.22.0) — **1.23.0 is not tagged and not built** anywhere yet.

Dev box for this file is Linux headless and cannot run the Tauri app or compile Rust. Every item below is therefore a static-analysis conclusion, **not yet verified at runtime** — this file exists to collect every item that needs a human on a real Mac before 1.23.0 ships. Nothing in this file has been executed; do not read any item as already passing.

## Items

### 1+2. Window bounds remember + four presets — mechanism CLOSED by static proof, one timing check remains

**Closed by static analysis, do not re-derive:** read the actual `tao` 0.35.3 / `tauri-runtime-wry` 2.11.3 source matching this project's `Cargo.lock` checksums, plus a hand-computed Retina-2x-plus-external-1x case.
The old physical-px code was off by 914 points and landed on the wrong screen; the new logical-point code is correct because a `Logical` value passes through unscaled, so the current per-display scale factor cannot corrupt it. A second hand-worked case — secondary display placed to the LEFT (negative-origin coordinates) — also proved NOT a bug under the new code.
**Still needs a human:** whether there is any OS/IPC-level timing gap between the two `await`ed calls at startup (capture-then-restore ordering) — this is a runtime race question static reading cannot settle.
**Do:** on a Mac with 2 displays of different scale factor, drag the app onto the secondary display, enable remember-view, quit fully, relaunch; separately, fire ⌘1/⌘2/Narrow/Wide on single- and multi-monitor setups.
**Correct:** window reopens on the same display/position/size; each preset lands at the expected width/position with no drift.
**Fail means:** anything wrong here now points at a startup-ordering race, not the logical/physical conversion math (that part is closed).

### 3. stickTopLeft spans full work-area height — CLOSED by static proof, one optional observation left

**Closed:** `work_area()` reads `NSScreen.visibleFrame`, which already excludes the menu bar (and excludes the Dock whenever the Dock is not set to auto-hide); `decorations:false` on this Borderless window means outer size equals inner size, so there is no hidden titlebar to eat into that height. No human check needed for the core mechanism.
**Optional, low priority, not a gate:** whether any real monitor in use has a `workArea.height` under 500 points, which would squeeze the stick preset uncomfortably short. Note it if seen; do not block the release on it.

### 4. Repeated in-app SSH open → new tab, cap toast — Mac-only flow CLOSED, phone-companion race remains

**Closed on Mac alone:** the old dedup guard is gone (verified removed), and `capReached` reads the live ref directly, so it blocks before the 15s timer is ever armed — no double-toast path exists solo.
**Still needs a human, phone-paired only:** the code itself documents a race that cannot be closed statically — the companion (phone) side's tab list lags the Mac by one round-trip. This item is now phone-only.
**Do:** with a phone paired, open the same project's SSH terminal repeatedly from the Mac until the tab cap is hit, watching the phone's tab list throughout.
**Correct:** phone's tab list catches up to the Mac's with no stale/duplicate entry; exactly one cap toast appears on each side.
**Fail means:** the phone shows a stale or duplicate tab list, or a second stray cap toast appears roughly ~15 seconds later.

### 5. Paste into terminal then press SPACE — mechanism CLOSED, only the IME case remains

**Closed:** the fix runs unconditionally at the paste event itself, so whatever key is pressed afterward no longer matters — the plain paste-then-SPACE case is settled by reading the code, not runtime-dependent.
**Still needs a human:** paste while a Vietnamese IME composition is active (e.g. OpenKey) — the safety of draining unconditionally during an active composition is only asserted by a code comment, with no runtime evidence yet.
**Diagnostic-method warning:** the old "paste then press SPACE" trick to detect residue is now INVALID as a diagnostic — the fix always clears residue, so SPACE will never show doubling even if some other bug exists. Do not use it to rule anything out.
**Do:** start an IME composition (e.g. typing Vietnamese), paste mid-composition, and observe.
**Correct:** no duplication, no corruption of the in-progress composition.
**Fail means:** the composition is corrupted or the pasted text duplicates — report exactly what appeared.

### 6. Multi-line paste — no auto-submit per line, DevTools diagnostic no longer needed

Candidate 1 (drain claims the paste) is already ruled out; no DevTools root-cause hunt is required. The only unknown is the actual `inputType` WKWebView reports for a paste on Linux vs Mac, and there are two ways to close it — either observe once, or skip straight to a fix that removes the dependency entirely.
**Option A — one-time observation:** in the webview DevTools console, paste a multi-line block, then run `window.__akiTermInput.tail(10)`; read the `inputType` field.
**Option B — remove the dependency:** implement the app-owned paste handler described in `remaining-1.23.md` §TERM-PASTE ("Robust fix"), which does not exist yet in the code (`grep "addEventListener('paste'" src/` returns 0 matches) — this makes the WKWebView `inputType` value irrelevant.
**Do:** paste a multi-line block into a bracketed-paste-aware program (e.g. `claude` or `agy`) and separately into a bare shell prompt.
**Correct:** one paste, no line is auto-submitted as a separate command, in both targets.
**Fail means:** each line executes as its own command — take Option A's reading (if not already done) to confirm it is bracketed-paste-off on the receiver, not a drain regression.

### 7. Copy out of terminal under a mouse-mode TUI — now a fix-verify, not a diagnosis

**What changed:** static proof shows ⌥-drag did NOT work with this project's prior config — xterm's `SelectionService.shouldForceSelection` on macOS requires `altKey && rawOptions.macOptionClickForcesSelection`, which defaults `false` and was never set here. A parallel seat has added `macOptionClickForcesSelection: true` to the `Terminal` constructor in `TerminalView.vue`. This item is no longer "does the escape hatch work" — it is "confirm the fix that was just added works."
**Do:** with a mouse-mode TUI running (e.g. `claude` or `agy` in the in-app terminal), hold **⌥ Option** and drag across some text, then release and press **⌘C**.
**Correct:** the Option-drag visibly highlights a text selection despite mouse-mode being active, and ⌘C copies it (paste elsewhere to confirm).
**Fail means:** Option-drag still does not produce a selection, or ⌘C does not copy it — the new flag did not take effect; re-check the constructor wiring.

### 8. SimpleView/phone reset — CLEAR/RESTART CLOSED by code reading, only congestion-resync remains

**Closed:** reading `usePtyStream.js`'s `handleLiveFrame` confirms the `if (frame.reset)` branch clears `lines.value = []` directly (not inside `resetDisplay()`, which only resets parser+buffer) — so the CLEAR and RESTART triggers, which emit `reset:true` from Rust's `pty_clear`/`pty_restart`, are settled without needing a Mac.
**Still needs a human:** whether the JS-side `pushAllScrollbacks()` congestion-resync path actually fires and clears correctly under real network congestion — that is runtime behavior no static read can confirm.
**Do (minimal):** pair a phone, force network congestion by flooding continuous terminal output on the Mac, and confirm the phone's screen replaces its content rather than appending below stale scrollback.
**Fail means:** the phone still shows doubled/stale-then-fresh output when the congestion-resync fires.

### 9. Build/typecheck gates unrun on this dev box

**What:** `npm run lint:scripts` and `npm run build` both pass on the dev box as of 2026-08-03 (the earlier `@xterm/xterm` failure was a local install gap, since resolved); `cargo check`/`cargo build` have never run for this working tree since Rust is Mac-only here.
**Build must be re-run, not reused:** two code fixes landed after this file was first written — the window-bounds/preset size clamp on restore, and the `macOptionClickForcesSelection` flag (item 7) — so any earlier build predates both and cannot stand in for this gate.
**Do on Mac:** `npm install`, then a Rust build (`cargo check` at minimum), then `npm run build:rmud` for the release bundle.
**Correct:** all four commands complete without error; the resulting `.dmg` launches.
**Fail means:** any step errors — report the exact command and error text, since none of these have been run against the current working tree yet.

## Results — fill in by hand, do not pre-fill

| # | Item | Pass/Fail | Notes |
|---|---|---|---|
| 1+2 | Window bounds remember + four presets — startup-timing check only | Pass | |
| 3 | stickTopLeft full-height — optional workArea.height observation only | Pass | |
| 4 | Repeated SSH open → new tab + cap toast — phone-companion race only | Pass | |
| 5 | Paste + SPACE — IME-composition case only | Pass | |
| 6 | Multi-line paste — inputType observation or ship the app-owned paste handler | Pass | |
| 7 | Copy under mouse-mode TUI — verify macOptionClickForcesSelection fix | Pass | |
| 8 | SV-RESET — congestion-resync case only | Pass | |
| 9 | Build/typecheck gates (npm install, vite build, cargo, build:rmud) — rebuild required | Pass | `npm install`, `cargo check`, `npm run build:rmud` all clean on Mac 2026-08-03; dmg built and launched |

**Result: all 9 items pass. 1.23.0 cleared to release 2026-08-03.**
