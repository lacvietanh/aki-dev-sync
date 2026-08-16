# Remaining after 1.22.0

Single entry point for everything still outstanding after the `1.22.0` release (shipped and tagged 2026-07-31). This file indexes and ranks — it does not absorb the detail of the plans/research it points at; go to the owning file for steps, evidence, and file:line citations. Written 2026-07-31.

## Ranked table

| # | ID | Description | Kind | Blocks a future release? | Owning file |
|---|---|---|---|---|---|
| 1 | GBOARD | Android/Gboard double-insert in the in-app terminal (`"ăn gì" → "aăn giì"`) — diagnosed at code level, not fixed, not confirmed on hardware | defect | No — already shipped 1.22.0 with the caveat stated in CHANGELOG; not a regression from today's release | `docs/research/terminal-gboard-double-insert.md`, `docs/plan/done/terminal-input-jul31.md` §2.2 |
| 2 | U3 | GlobalNoteModal textarea min-height re-tune | verify | No | this file item 2 (no operational checklist file currently tracks it — pre-existing gap, see item 2 below) |

None of the remaining items blocks a future release in the sense of "cannot ship without it" — 1.22.0 is already out with every one of these either caveated in the CHANGELOG (GBOARD) or simply not yet built/verified. GBOARD is ranked first because it is the only *defect users can hit right now*; everything else is either confirming already-shipped correctness, paying down debt, or work that has not started. PT-DOC and WS-C-RESIDUAL (formerly #14 and #16 of an original 16) closed 2026-08-01 — see "Resolved (2026-08-01)" below; both were answerable from the repo alone, no device or owner input needed. T1–T6, U1, U2, U4, I1, I2 closed 2026-08-15 — see "Resolved (2026-08-15)" below. HARDWRAP, TOAST, WS-B and WS-A-S1-2 closed 2026-08-16 — see "Resolved (2026-08-16)" below.

## Items

### 1. GBOARD — Android/Gboard double-insert

**Status:** root cause traced to code (`src/composables/useTerminalTextDrain.js`'s "read all, then empty" textarea model silently drops a `deleteContentBackward` that lands on a textarea the drain already emptied), not confirmed on hardware, not fixed. Separate mechanism from the already-fixed double-space blocker (`-5.md`) — confirmed different because the double-space fix landed and this defect's shape was untouched.

**Next concrete action:** the one hardware measurement that resolves the open fork (does Gboard wrap its correction in a real DOM composition, or fire plain `deleteContentBackward`/`insertText` outside one) — `__akiTermInput.clear()` then type `báo` on the Android device, then `__akiTermInput.dump()`, read via `chrome://inspect` remote debug. Documented in full at `docs/research/terminal-gboard-double-insert.md` §6.

**Decision the owner owes:** both candidate fixes cost an invariant 1.22.0 just bought. (a) Discriminate `deleteContentBackward` outside composition and translate it to real backspace bytes — reintroduces `inputType`-based classification, which `-5.md`'s fix explicitly removed ("one branch removed, not one added") and the file's own header argues against by design. Only correct if the plain-delete branch is confirmed. (b) Track a shadow buffer of already-sent-but-still-editable characters — reintroduces the app-owned text-state tracking the 1.22.0 architecture deliberately deleted (`useTerminalInput.js`, the overlay textarea). Neither can both preserve the "exactly one input path, no branch-by-classification" invariant and correctly reconcile a delete against text the drain already forwarded and erased. If the hardware measurement instead shows the composition-wrapper branch, the fix is not in this file at all — it is in xterm's own composition-completion path, a different investigation. Wait for the measurement before picking a direction.

### 2. U3 — GlobalNoteModal textarea min-height re-tune

**Status:** code-complete, not yet confirmed. Blocked only on someone at the Mac observing the result. **Pre-existing doc drift, found 2026-08-15:** this row has always pointed at `docs/plan/done/verify-pending.md#u3`, but that file never actually contained a U3 entry (confirmed against every version of the file seen this session — its UI section only ever held U1, U2, U4). No operational checklist currently tracks the exact steps for this item; one needs to be written (or the item re-scoped/dropped) before it can be verified.

The other checks from that same batch (T1–T6, U1, U2, U4, I1, I2) were confirmed passed by the owner on a real Mac 2026-08-15 and are closed — see "Resolved (2026-08-15)" below and `docs/plan/done/verify-pending.md`.

### 3. WS-A §11 open reviewer questions — decided 2026-08-01

WS-A itself (all of S1–S7) is now fully built — see "Resolved (2026-08-16)" below. This item is unrelated to that build status: it records the two open questions `terminal-ownership-model.md` §11 left for the reviewer.

**Decided 2026-08-01** (both were reasoning-only, no device needed — the source doc `terminal-ownership-model.md` §11 keeps its original text as the immutable design record; the answers live here): **Q2** — No, the global terminal button stays single-verb (open/reactivate the global group); a sessions-modal filter would be a second, different verb on the same button, which the design already argued against. **Q3** — Yes, non-persistence across restarts is acceptable: the whole feature is built to degrade non-destructively to today's cwd-adoption behavior (already the fallback for S3's AppleScript risk above), and a restart legitimately has no live process to own a claim over yet — the registry rebuilding from a fresh scan each session is the correct behavior, not a gap.

## Resolved (2026-08-16)

### HARDWRAP — hard-wrap sweep, code buckets

**Done.** Rust: 71 `[WRAP]` instances to 0 across 13 files under `src-tauri/src/`. JS/Vue: 88 to 0 in composables/services/store/assets and 55 to 0 across 15 `.vue` files. **Correction, 2026-08-16:** this was first reported as "88 to 2" (`usageMonitor.js:311-312`); re-reading those exact lines shows line 311 is a self-contained `──` section-divider comment and line 312 is one already-single-physical-line comment — two structurally atomic, independent comments, not one logical line hard-wrapped across two, so neither is an actual `[WRAP]` hit and the residual count is 0, not 2. Verified: `git diff --stat` shows the 12 changed `src-tauri/src/*.rs` files (`Cargo.lock` unaffected) plus the composables/services/store/components sweep. The docs bucket closed earlier (`1c00e3b`/`3c5188e`, 2026-07-28).

**Residual, not chased further here:** the `[YAP]` bucket (comments narrating what/how rather than a genuine why) was reviewed by sampling during this sweep and largely left in place as genuine rationale — reviewed-not-exhausted, not a completed audit. A future pass would need to re-run the sweep with `[YAP]` as its own target rather than a byproduct of the wrap pass.

### WS-B — terminal chrome settings menu

**Built.** 3-dot drop-up in the terminal stack header (`src/components/terminal/TerminalChromeMenu.vue`), one checkbox per toggleable chrome control (compose input, key row, text-size buttons, tab strip, group name, external-terminals button, maximize), host/companion-aware defaults, per-device `localStorage` preference (`src/composables/useTerminalChrome.js`), per `docs/plan/done/terminal-chrome-settings.md` §8/§9. Doc-synced: `docs/feat/in-app-terminal.md` § Terminal chrome visibility, `docs/arch/terminal-stack.md` § The capability pattern, `README.md`, `IntroModal.vue`, `CHANGELOG.md`.

### WS-A S3–S6 — external terminal ownership model, spawn-origin half

**Built.** `TerminalOwnership` (`src-tauri/src/system.rs`, `by_tty` registry), `tag_terminal_launch` at both launch sites (OPEN popup's Terminal item, an in-app tab's "open externally"), `reconcile_terminal_owners` run at the top of every scan, the `TerminalCell.vue` → `TerminalScopeButton.vue` rename, and the sessions modal's "launched from X" / "in X's folder" provenance labels are all live.

### WS-A-S1-2 — session-inventory command + frontend attribution module

**Built.** `list_terminal_sessions()` (`src-tauri/src/system.rs`) replaces `count_external_terminals`, returning `{pid, ppid, tty, cwd, owner}` per session, polled every 5s. `list_external_terminals` renamed `describe_terminal_sessions`, keeping its `paths` argument (deviation from the plan's stated S1 acceptance — the modal's "in X's folder" label needs `project_path`). `count_external_terminals_global` deleted too, subsumed by the new pure module. New `src/utils/terminalOwnership.js` (`attributeTerminalSessions(sessions, projects) -> { byProjectId, globalCount, ownerOf }`) is now the single decision point feeding both `externalTermCounts` and `externalTermGlobalCount` from one poll. Net effect: the `TERM` cell badges now honour spawn-origin, not just cwd — an SSH session opened from a project's OPEN popup counts on that project's badge even though its cwd is the local `$HOME`. See `docs/plan/done/terminal-ownership-model.md` § "S1/S2 — built 2026-08-16" and `docs/arch/terminal-stack.md` § Spawn-origin ownership.

### TOAST — toast positioning, recovered from a lost cleanup row

**Done.** `src/store/projectStore.js`'s `Toast` mixin position changed from `bottom` (bottom-center) to `top-end`, per the fix proposed in `docs/plan/done/improve-jun24.md` §3 and never landed at the time. A `.swal2-top-end` override was added to `src/assets/main.css`, respecting the 42px titlebar boundary (`top: var(--titlebar-h)`, never `top: 0`). The 5-modal re-walkthrough this item's own next-action called for was not separately re-run as part of this change.

## Resolved (2026-08-15)

T1-T6, U1, U2, U4, I1, I2 — all confirmed passed by the owner on a real Mac 2026-08-15; full historical detail now lives in `docs/plan/done/verify-pending.md`.

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

Two open threads exist in the source docs that this file's mandate does not include and that are therefore deliberately excluded, not overlooked: `docs/plan/done/cc-account-identity-ssot.md` (T-4, fix shipped, awaiting the owner's on-device rebuild+verify) and `docs/plan/done/ui-sweep-misses.md` (T-7's status was not re-checked in the 2026-07-31 pass per `backlog-jul27.md`). Both are tracked in `backlog-jul27.md`'s own T-table; if either is still open, it belongs there or in a future revision of this file, not silently folded in here.
