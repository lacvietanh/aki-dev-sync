# Remaining after 1.22.0

Single entry point for everything still outstanding after the `1.22.0` release (shipped and tagged 2026-07-31). This file indexes and ranks — it does not absorb the detail of the plans/research it points at; go to the owning file for steps, evidence, and file:line citations. Written 2026-07-31.

## Ranked table

| # | ID | Description | Kind | Blocks a future release? | Owning file |
|---|---|---|---|---|---|
| 1 | GBOARD | Android/Gboard double-insert in the in-app terminal (`"ăn gì" → "aăn giì"`) — diagnosed at code level, not fixed, not confirmed on hardware | defect | No — already shipped 1.22.0 with the caveat stated in CHANGELOG; not a regression from today's release | `docs/research/terminal-gboard-double-insert.md`, `docs/plan/done/terminal-input-jul31.md` §2.2 |
| 2 | T1 | Ctrl/Shift armed-button display + tab alive-status (`shallowRef` fix) — fixed, needs on-device confirm | verify | No | `docs/plan/verify-pending.md#t1` |
| 3 | T2 | Option+arrow word-jump with the double-space fix in place — owner deliberately deferred this measurement | verify | No | `docs/plan/verify-pending.md#t2` |
| 4 | T3 | Compose-textarea auto-grow vs PTY resize oscillation — pre-existing mechanism, never confirmed not to fight the resize chain | verify | No | `docs/plan/verify-pending.md#t3` (also the one still-open row of `terminal-input-surface.md` §6) |
| 5 | T4 | Stuck `:hover` false-positive on iOS Safari key-row buttons — guarded in code, needs on-device confirm | verify | No | `docs/plan/verify-pending.md#t4` |
| 6 | U1 | Projects table column-flex reshape at minimum width (~420px) | verify | No | `docs/plan/verify-pending.md#u1` |
| 7 | U2 | GlobalNoteModal narrow-mode padding repeat (≤700px) | verify | No | `docs/plan/verify-pending.md#u2` |
| 8 | U3 | GlobalNoteModal textarea min-height re-tune | verify | No | `docs/plan/verify-pending.md#u3` |
| 9 | I1 | Companion WebSocket reconnect convergence on real sleep/wake | verify | No | `docs/plan/verify-pending.md#i1` |
| 10 | I2 | Project-icon 404 avoidance (no request at all when absence is already known) | verify | No | `docs/plan/verify-pending.md#i2` |
| 11 | HARDWRAP | Hard-wrap sweep, code buckets — JS/Vue (~67 files) and Rust (~13 files), ~2,239 lines, never attempted; docs bucket (48 files) already done | debt | No | `docs/plan/done/hygiene-jul27.md` item 3, `docs/plan/done/backlog-jul27.md` WS-E |
| 12 | WS-B | Terminal chrome settings: 3-dot visibility menu on the terminal stack header | unbuilt | No | `docs/plan/done/terminal-chrome-settings.md` |
| 13 | WS-A-S3-7 | External terminal ownership model, steps S3–S7 (tty capture/tagging, provenance in the modal, button rename) | unbuilt | No | `docs/plan/done/terminal-ownership-model.md` |
| 14 | T5 | Terminal resize propagation (host↔companion) — static analysis cleared, one runtime check outstanding, incl. the genuinely unsettled font-zoom re-measure question | verify | No | `docs/plan/verify-pending.md#t5` |

None of the 14 remaining items blocks a future release in the sense of "cannot ship without it" — 1.22.0 is already out with every one of these either caveated in the CHANGELOG (GBOARD) or simply not yet built/verified. GBOARD is ranked first because it is the only *defect users can hit right now*; everything else is either confirming already-shipped correctness, paying down debt, or work that has not started. PT-DOC and WS-C-RESIDUAL (formerly #14 and #16 of an original 16) closed 2026-08-01 — see "Resolved" below; both were answerable from the repo alone, no device or owner input needed.

## Items

### 1. GBOARD — Android/Gboard double-insert

**Status:** root cause traced to code (`src/composables/useTerminalTextDrain.js`'s "read all, then empty" textarea model silently drops a `deleteContentBackward` that lands on a textarea the drain already emptied), not confirmed on hardware, not fixed. Separate mechanism from the already-fixed double-space blocker (`-5.md`) — confirmed different because the double-space fix landed and this defect's shape was untouched.

**Next concrete action:** the one hardware measurement that resolves the open fork (does Gboard wrap its correction in a real DOM composition, or fire plain `deleteContentBackward`/`insertText` outside one) — `__akiTermInput.clear()` then type `báo` on the Android device, then `__akiTermInput.dump()`, read via `chrome://inspect` remote debug. Documented in full at `docs/research/terminal-gboard-double-insert.md` §6.

**Decision the owner owes:** both candidate fixes cost an invariant 1.22.0 just bought. (a) Discriminate `deleteContentBackward` outside composition and translate it to real backspace bytes — reintroduces `inputType`-based classification, which `-5.md`'s fix explicitly removed ("one branch removed, not one added") and the file's own header argues against by design. Only correct if the plain-delete branch is confirmed. (b) Track a shadow buffer of already-sent-but-still-editable characters — reintroduces the app-owned text-state tracking the 1.22.0 architecture deliberately deleted (`useTerminalInput.js`, the overlay textarea). Neither can both preserve the "exactly one input path, no branch-by-classification" invariant and correctly reconcile a delete against text the drain already forwarded and erased. If the hardware measurement instead shows the composition-wrapper branch, the fix is not in this file at all — it is in xterm's own composition-completion path, a different investigation. Wait for the measurement before picking a direction.

### 2–10, 14. verify-pending — 10 runtime checks

All ten (T1–T5, U1–U3, I1–I2) are code-complete (T5's underlying mechanism predates this batch) or committed; nothing is missing work, each is blocked only on someone at the Mac (or a phone against it) observing the result. `docs/plan/verify-pending.md` is the operational checklist — walk it top to bottom, delete a row when it passes, and follow that row's pointer back to its source plan to close it per `docs.B1` if it was the plan's last open item. Three are worth flagging individually:

- **T2** — the owner explicitly deferred this measurement in the same session that closed the double-space blocker ("chưa đo, không tính là PASS"). Not an oversight; a deliberate skip.
- **T3** — the only row `docs/plan/done/terminal-input-surface.md` still has open (that plan doc already moved to `done/` alongside this cleanup pass, per `docs/plan/done/terminal-input-jul31.md` §5's condition — T4's twin, §2.3's Ctrl/Shift armed display, still awaits owner confirmation).
- **T5** — three of its four sub-checks (dock-splitter drag, tab switch, late-join/reconnect) are re-confirming static analysis that already cleared; only the font-zoom re-measure sub-check is a genuinely open question with no static answer.

### 11. HARDWRAP — hard-wrap sweep, code buckets

**Status:** PARTIAL. Docs bucket (48 files) landed in `1c00e3b`/`3c5188e` (2026-07-28) and is closed. JS/Vue (~67 files, ~1,286 lines) and Rust (~13 files, ~953 lines) were never attempted — confirmed by direct read (`src/services/bridge.js:13-17`, `src-tauri/src/web_server.rs` both still carry the ~100-column wrap signature).

**Next concrete action:** a follow-up rewrap pass, scoped file-by-file: rejoin each flagged comment/prose run into one logical line per sentence/bullet, skip everything under `hygiene-jul27.md`'s "unsafe-to-collapse" list (markdown tables, `---` thematic breaks, multi-line `import` blocks, CHANGELOG bullets), and land in two commits (touched-by-recent-diffs first, legacy sweep second). Must run **last** among all in-flight workstreams — it collides line-for-line with anything else touching the same files.

### 12. WS-B — terminal chrome settings menu

**Status:** fully designed, zero code written. Spec covers a 3-dot drop-up on the terminal stack header with one checkbox per toggleable chrome control (compose input, key row, text-size buttons, tab strip, group name, external-terminals button, maximize), host/companion-aware defaults, and a full acceptance-criteria list (AC-1..AC-18).

**Next concrete action:** implement per `docs/plan/done/terminal-chrome-settings.md` §8 (drop-up spec) and §9 (acceptance criteria). The defaults question the doc raised (§6, compose input shown by default on the Mac) is **already decided** — `backlog-jul27.md` "Decisions taken by the user, 2026-07-27" confirms compose-input-shown-by-default against the user's original "hide everything" rule. No decision is owed here; this is pure unbuilt work. One dependency: WS-A's control inventory (item 13) — the menu can only toggle controls that exist, and WS-A may add/remove/rename the external-terminals header control.

### 13. WS-A S3–S7 — external terminal ownership model, unbuilt half

**Status:** S1/S2/S5 shipped 2026-07-28 as a narrower MVP (the global badge's missing counts, via adoption-only attribution). S3–S7 — tty capture and spawn-origin tagging, provenance labels in the sessions modal, the `TerminalCell.vue` → `TerminalScopeButton.vue` rename — are still exactly as designed in `docs/plan/done/terminal-ownership-model.md` and not implemented.

**Feasibility risk:** S3's whole tagging mechanism rests on one unverified assumption — that AppleScript's `do script` returns a tab object whose `tty` property can be read back in the same script, and that the tty is populated immediately rather than after the shell starts. Neither has been executed on a Mac (this environment has no `osascript`). If it fails, every session simply falls back to today's cwd-adoption behavior — the design is built to degrade non-destructively, so the risk is scoped to "S3–S7 doesn't ship as designed," not to a regression.

**Next concrete action:** if resumed, follow the doc's own S3 → S4 → S5 → S6 → S7 order (§10); S3 first, since S4–S6 all depend on the owner token S3 introduces.

**Decided 2026-08-01** (both were reasoning-only, no device needed — the source doc `terminal-ownership-model.md` §11 keeps its original text as the immutable design record; the answers live here): **Q2** — No, the global terminal button stays single-verb (open/reactivate the global group); a sessions-modal filter would be a second, different verb on the same button, which the design already argued against. **Q3** — Yes, non-persistence across restarts is acceptable: the whole feature is built to degrade non-destructively to today's cwd-adoption behavior (already the fallback for S3's AppleScript risk above), and a restart legitimately has no live process to own a claim over yet — the registry rebuilding from a fresh scan each session is the correct behavior, not a gap.

### 14. T5 — terminal resize propagation

**Status:** see verify-pending row T5 above and `docs/plan/done/backlog-jul27.md` T-9. Static analysis across two rounds cleared all four candidate origins of a propagation break; the honest close is "no defect found statically, one runtime check outstanding," not "no bug." A named-but-not-required optional optimisation surfaced alongside it (`hostResize()` sends a resize frame on every `doFit()` cycle with no cols/rows equality check, so a continuous drag gesture is chatty) — correct, not broken, and left unscheduled pending someone actually measuring the cost.

**Next concrete action:** run `scripts/verify-pty-resize.sh`'s four-part walkthrough on a real Mac with a real phone paired (detail in verify-pending.md#t5). If all four pass, close as verified-with-no-change; per the batch's own CHANGELOG rule that gets no entry.

## Resolved (2026-08-01)

Both closed by re-checking the claim against the current repo — no device and no owner decision was actually needed for either.

### PT-DOC — `ProjectTable.vue` doc

**Decision:** create the doc. Done: `docs/feat/project-table.md`, indexed in `docs/index.md`. Documents the subgrid alignment fix (U1) and the wide/narrow `--grid-cols` tracks as current-state, so future table changes (WS-A S5's button rename, any column-layout fix) have a canonical sync target.

### WS-C-RESIDUAL — re-verified against current code, both halves already fine

- **CSRF token in the `agy` branch:** already fixed. `scripts/get-antigravity-usage.sh`'s `agy` branch (~line 224 on) calls `extract_arg "$cmdline" "--csrf_token"` and builds `hdr_csrf` exactly like the language-server branch, with a comment recording the fix. The plan's claim was accurate when written but the code has since moved on; no action needed.
- **`claudecode.rs`'s `agent_name`/`"antigravity"` handling:** re-read in context (`src-tauri/src/agent_usage/claudecode.rs:45-50`) — this is not leftover dead code. `provision_agent_usage` (`src-tauri/src/agent_usage/mod.rs`) is one generic Tauri command called with either agent id from the frontend; `claudecode.rs` is where it lands and the `agent_name == "antigravity"` branch is a deliberate no-op success (Antigravity has nothing to provision) rather than a stray string from before the module split. Nothing to remove.

## Not in scope / deliberately not doing

- **macOS Telex (system built-in IME)** — decided out of scope by the owner (`terminal-input-jul31.md` §3): VS Code breaks the same way, so this is not a defect specific to this app. Only OpenKey is supported. A future `ăn ăn gì, gì`-shaped report must be checked against this decision before being treated as a regression.
- **Multi-line paste / bracketed paste discovery** — decided out of scope by the owner: "quá phức tạp so với giá trị" (too complex for the value). Current behavior (each line sent as a separate command) stays. The open question in `terminal-input-surface.md` §3.4 about reading xterm 5.x's bracketed-paste state no longer needs an answer.
- **`aki-input-mode='legacy'` escape hatch** — removed outright 2026-07-31 (`0a6d314`), not deferred. The owner's instruction was explicit: the legacy path was buggy and kept only as rot.
- **`docs/plan/done/backlog-jul27.md`'s own rename** (from a dated batch-tracker name to a permanent `backlog.md`) — flagged in that doc's 2026-07-30 status log as warranted but deliberately not executed, on account of its large inbound-reference fan-out. Not re-listed here as an open item since the source doc already owns and tracks that deferral.
- **Hardcoded-hex sprawl** (416 literal colour values across `.vue` files vs 14 CSS custom properties) and **CHANGELOG entries drifting to essay length** (`release.B4/B5`) — both carried forward from `docs/research/audit-release-window-jul28.md` as `docs.B2` "No action, deliberately unscheduled" (`docs/plan/done/backlog-jul27.md` D-11). Neither is dangerous; the hex sprawl was already declined once on blast-radius grounds (a whole-app visual diff can't be reviewed alongside a functional one).

## Not covered by this file

Two open threads exist in the source docs that this file's mandate does not include and that are therefore deliberately excluded, not overlooked: `docs/plan/done/cc-account-identity-ssot.md` (T-4, fix shipped, awaiting the owner's on-device rebuild+verify) and `docs/plan/ui-sweep-misses.md` (T-7's status was not re-checked in the 2026-07-31 pass per `backlog-jul27.md`). Both are tracked in `backlog-jul27.md`'s own T-table; if either is still open, it belongs there or in a future revision of this file, not silently folded in here.
