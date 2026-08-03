# Audit — `wish-terminal-split-simpleview.md`

**Date:** 2026-08-01
**Mode:** akiflow tier=2 audit. Report only; no code, no config, and no edit to the wish doc itself.
**Baseline:** `docs/plan/wish-terminal-split-simpleview.md` as of the prior council `2026.08.01-1732-right-dock-mac-rerun`, whose four closed items became that doc's Right-Dock Decisions 1–4.
**Council record:** `~/.aki/agent-council/aki-dev-sync/2026.08.01-1834-review-terminal-split-wish/` (`chat.md` for the argument, `checklist.md` for the closures).
**Purpose:** establish whether the wish plan is factually true about the current code, architecturally sound, coherent as a user flow, and executable now — before any of it is built.

## Verdict

The plan is structurally sound and one of its eight resolved decisions is broken. Seven decisions hold. **SimpleView Decision 1 (V1 = strip all ANSI to plain text) does not survive measurement** and must be reopened before SimpleView is built. Separately the doc contains four factual errors about the codebase and three undecided items recorded as if settled.

Nothing here is a reason to abandon the design. The module boundaries, the frame-protocol reuse, and the eight-step ordering all check out against the real tree.

## Finding 1 — SimpleView D1 is broken (BLOCKING, measured)

The plan's V1 stripper (`wish-terminal-split-simpleview.md:99-105`) removes ANSI escape codes and deletes `\r`, then pushes the result into an append-only `lines` array. That removes the *codes* but not their *effect*: terminal output that redraws in place depends on a grid model an append-only array does not have.

Implemented verbatim and run against `script -q`-captured raw PTY bytes:

| Sample | What a human sees in a terminal | What the phone would render |
|---|---|---|
| Spinner — 20 `\r`-only progress frames, no `\n` (curl/npm shape) | one continuously updating line | **1 entry containing all 20 frames concatenated**: `...16%\ Installing dependencies... 21%/ Installing dependencies... 26%...100%` |
| Status box — 4 lines, `\x1b[5A` cursor-up + `\x1b[2K` erase + reprint, 7 ticks (Claude Code shape) | one static box, three values updating in place | **37 entries — 7 permanent duplicate copies of the whole box** |

Both failure branches confirmed. The regex behaviour is deterministic, so the mechanism result holds regardless of which CLI emits the pattern.

**Why two councils missed it:** Decision 1 was argued as a V1/V2 staging question about **SGR colour**. Colour was deliberately deferred; legibility was never considered. The architect on this council stated plainly that its own HOLD covered staging and did not assess redraw semantics.

**Smallest fix (diagnosis, not a design):** buffer the in-progress line and replace on a bare `\r`, pushing to the array only on a real `\n`; treat `\x1b[<n>A` as pop-last-N-lines. Both require the stream processor to hold a little grid state — a step up from "strip codes, push text", and a change to `usePtyStream.js`'s specified contract.

**Caveat (ASSUMPTION):** the box sample is a faithful synthetic reconstruction of the documented redraw mechanism, not literally-observed `claude` REPL bytes — `claude -p` suppressed its live UI under capture (27 bytes). Capturing a real interactive session would convert this to fully observed.

## Finding 2 — four factual errors in the doc (SERIOUS)

| # | Claim in the doc | Reality |
|---|---|---|
| F2.1 | `useTerminalChrome.js` is the existing precedent for the boundary/capability composable | **The file does not exist anywhere.** It is specified in `docs/plan/done/terminal-chrome-settings.md` §5/AC-3 but was never built, despite that doc being filed under `done/`. The real precedent is `usePtyTerminal.js` importing `isHost` (line 20) and publishing derived flags `ownsPtySize` / `showKeyRow`, described at `docs/arch/terminal-stack.md:39-41` |
| F2.2 | `SimpleView.vue` props must match `TerminalView.vue` exactly — `tabId`, `active` | `TerminalView.vue:123-127` declares **three** props; `cwd: {type: String, default: null}` is omitted from the parity requirement, and `TerminalStack.vue` already passes `:cwd` unconditionally |
| F2.3 | ENV-1 from `docs/feat/remote-control.md`; T-4 from `docs/feat/in-app-terminal.md` | Both invariants are real and live, but cited from the archived copies: ENV-1 at `docs/plan/done/remote-control.md:484`, T-4 labelled at `docs/plan/done/1.20.0-terminal-and-remote-sync.md:144`. `docs/feat/remote-control.md` is the live doctrine and says so |
| F2.4 | "`TerminalView.vue`: untouched; remains the Mac-only xterm.js mount" | **False in the present tense.** `TerminalStack.vue` mounts it with no host/companion branch; `usePtyTerminal.js:590` `showKeyRow = !isHost` feeds a companion key row at `TerminalView.vue:29` and a companion compose row at `:78`. It *becomes* Mac-only as a result of this work |

**F2.1 is the one worth a second look beyond this plan.** A doc under `docs/plan/done/` describes a file that was never built, and a later plan cited it as established prior art. That is a docs-drift failure with a blast radius past this wish.

## Finding 3 — the zero-changes claim is a wording overclaim, not a hole (SERIOUS → resolved)

The plan lists six files needing zero changes. Challenged and then verified against the tree:

- **Nothing breaks.** `tabLiveness` is a **module-scope** tracker (`usePtyTerminal.js:25-92`) registered once, independent of any `TerminalView` mount — its own comment records that it exists *because* the old design "never ran at all on a companion, which does not mount every tab up front". `activatedTabs` reconciliation (`useTerminalTabs.js:260-267`) runs off the shared store, and both components sit behind the same `activatedTabs.has(t.id)` gate. `ptyBridge.js:140` relay is `isHost`-gated and per-tab.
- **But branches die.** Once `<component :is>` stops mounting `TerminalView` on companion, these become unreachable: `TerminalView.vue:29-66` (key row + font-zoom), `TerminalView.vue:78-~100` (compose textarea), `usePtyTerminal.js:590`. Also flagged but not individually confirmed: `usePtyTerminal.js:228`, `:404`, `:472`.

**Restate as:** "zero diff; N branches become dead code owned by a later cleanup." Zero diff and zero footprint are not the same claim.

## Finding 4 — three items recorded as decided that are not (MATERIAL)

1. **The LogInspector has no exit.** Decision 1 specifies entry ("via the app-icon dropdown") and persistence ("no backdrop/focus trap, preserves terminal mounts") and never states dismissal. A nonmodal panel with no documented close path is an incomplete flow by construction.
2. **"Global" names two different things.** The D3 rail's "Global" is a *terminal scope* (tabs with no project). The D1 dropdown's "Global Activity" is an *event-log feed*. Different domains, one word. Per `RULE-design-core.md` A7 (name by role), the two surfaces survive but the shared label does not — suggested split: rail keeps "Global", the dropdown item becomes "Activity" or "All Activity".
3. **Single-tab-per-project on the phone is a silent scope cut.** `tabId: Number` default 0 with no phone-side switcher caps SimpleView at one tab per project while the Mac supports many. Unlike the ANSI question, which names its V1/V2 split explicitly, nothing in the doc says "V1 = one tab only". The silence is the defect, not necessarily the choice.

## Finding 5 — Right-Dock pre-mortem risks (SERIOUS, unresolved)

1. **`RIGHT_DOCK_MIN_WIDTH` was never computed with real pixel values** in any of the four Right-Dock decisions, while D4 deliberately provides no vertical fallback. `MAIN_STACK_COMPACT_MIN + SCOPE_RAIL_WIDTH + TERMINAL_CONTENT_MIN` could exceed common laptop widths. None of those three constants exists in `src/` yet, so the sum has never been evaluated.
2. **The LogInspector trigger lands in a hover-only reveal.** `AppHeader.vue`'s `.icon-dropdown` opens on `:hover` (`:688`). The prior council itself noted this, and Decision 1 contains no accessibility or keyboard-reachability step.
3. **`core:window:allow-set-min-size` is absent** from `src-tauri/capabilities/default.json` (13 other `core:window:*` permissions are granted). Per `CLAUDE.md`'s IPC-capability rule this fails **silently** — no error, no log. The plan predicts this one correctly; it stays a hard precondition for D4.

## Finding 6 — decisions are numbered 1–4 twice (COSMETIC)

The doc carries two separately-numbered "Decisions (resolved 2026-08-01)" sections — SimpleView 1–4 and Right-Dock 1–4 — with no disambiguating prefix. The first cross-reference by bare number will land on the wrong one.

## What holds

Worth stating plainly, because most of the plan does:

- All four `FRAME_PTY_*` constants exist (`src/constants/protocol.js:93,100,107,116`).
- `decodeBase64ToBytes` (`usePtyTerminal.js:94-99`) is self-contained and genuinely extractable to `ptyCodec.js`.
- `pty_get_scrollback` exists (`src-tauri/src/pty.rs:821`), called from `usePtyTerminal.js:375` and `services/ptyBridge.js:70` — scrollback replay reuses it unchanged.
- `TerminalStack.vue` does not import `isHost` today; 15 other modules do. The ENV-1 boundary is intact.
- Resize authority sites are exactly where the plan says: `usePtyTerminal.js:448` (`term.resize` on `FRAME_PTY_RESIZE`), `:604` (`invoke('pty_resize')`), `:378`.
- `max-width: 700px` is a real, widely-used breakpoint (17 files; `min-width: 701px` at `src/assets/main.css:1448`), so D4's "700px as sole breakpoint" rests on something real.
- The eight-step implementation order has no forward dependency: steps 1–4 create independent new files, 5–6 patch after those exist, 7–8 test last.
- `useTerminalViewType.js` as a **new file** is correct, not a Rule-of-Three violation — there is no duplicated logic to extract, and folding view-selection into `usePtyTerminal.js` would force a per-tab xterm-coupled dependency onto a routing decision made before any tab exists. Keep it to ~5–10 lines: read `isHost`, pick the component, nothing else.
- Extreme Narrow doctrine passes. The LogInspector reuses an existing dropdown with zero idle footprint; rail overlays are absolute-positioned badges, not inline text. The rail's new fixed-width column is the one arguable point and is defensible as load-bearing navigation.

## Recommended sequencing

**SimpleView first, alone; Right-Dock later.** SimpleView's driver is a *bug* (`FRAME_PTY_RESIZE` coupling) and is cheaply falsifiable — disabling that frame on companion alone tests the premise before any new file is written. Right-Dock's driver is *ergonomic* and far less reversible: window min-size enforcement, `AppHeader` accessibility, and a breakpoint rewrite. The doc already calls them separate milestones; this is that call, sharpened by which one is reversible.

Preconditions before SimpleView is built:
1. Reopen SimpleView D1 and re-specify the stream processor to hold minimal grid state (`\r` replace-line, `\x1b[<n>A` pop-lines). This changes `usePtyStream.js`'s contract.
2. Correct the four factual errors in Finding 2.
3. Decide and write down the three items in Finding 4.

Preconditions before Right-Dock is built, in addition:
4. Compute `RIGHT_DOCK_MIN_WIDTH` with real pixel values and check it against the narrowest laptop the app must support.
5. Grant `core:window:allow-set-min-size` and build-validate it — it fails silently if missing.
6. Give the LogInspector trigger a keyboard-reachable path.

## Phase B — what was actually changed (2026-08-01, same session)

The owner ruled that the council decides and executes rather than escalating. All three escalations above were closed as lead decisions and applied. This section records what changed and, more usefully, what the review rounds caught **after** each decision was made.

### Decisions taken

| # | Decision |
|---|---|
| SV-1 replaced | The phone's stream processor became an **incremental stateful parser**, not a pure stripper. One-line write buffer + committed `lines` (capped 2000). `\n` commits; a bare `\r` clears the buffer; `\x1b[<n>A` drops `min(n, lines.length)` entries; `\x1b[2K`/`\x1b[K` clear the buffer; every other CSI and all SGR still stripped, so V1 remains colourless. A pending tail carries incomplete escape sequences and a trailing `\r` across chunk boundaries, bounded at 64 bytes, truncated only at code-point boundaries |
| Hard boundary | The phone honours cursor-up, line-erase and carriage-return **only**. No columns, no cursor addressing, no scroll regions, never learns `cols`/`rows`. Enforced by a grep guard shipped as a manually-run npm script modelled on `scripts/lint-remote-scripts.js` (including its comment-stripping precaution) — **not** CI-grade, because this repo has no CI |
| Accepted limitation | SimpleView targets line-oriented CLI output. Full-screen TUIs (vim, htop, less) that redraw by absolute addressing will render garbled. Accepted, not deferred |
| LogInspector exit | Esc while focused, click outside, or re-selecting the dropdown item toggles it shut. No backdrop, no focus trap |
| Naming | Rail keeps "Global" (a terminal scope); the dropdown item becomes "Activity" (an event feed) |
| Phone tab scope | V1 is one tab per project, showing the active tab. Multi-tab on companion deferred to V2 as a named limitation |
| Sequencing | SimpleView ships first and alone |
| Right-Dock activation | Activating Right-Dock expands the window to ≥1200px. If the display cannot accommodate it, the toggle is **disabled with a tooltip** rather than allowing a squeezed layout — D4 chose no vertical fallback, which is only safe if the minimum is enforced at the toggle |

### RIGHT_DOCK_MIN_WIDTH, computed

Never calculated before this audit. From real CSS:

| Constant | Value | Basis |
|---|---|---|
| `MAIN_STACK_COMPACT_MIN` | ~499px | ProjectTable min-content sum; `ProjectTable.vue:1336-1338` documents the SYNC cluster at ~176px |
| `SCOPE_RAIL_WIDTH` | ~96px (ESTIMATE) | derived from `TerminalTabStrip.vue:121` chip `min-width: 84px`, badge `min-width: 16px` overhanging 6px, 14px icon, 6px section padding |
| `TERMINAL_CONTENT_MIN` | 592px | 80 cols × ~7.2px cell (`TerminalView.vue:349` `BASE_FONT_SIZE = 12`, Menlo) + 16px mount padding |
| **Sum** | **1187px** | |

Against `tauri.conf.json`: default width **720**, minWidth **420**; the app's own presets are Narrow 420 and Wide 768. Right-Dock is therefore unreachable at every current window size. Fit: 1280 (Air 13) leaves 93px — near-maximised only; 1440 workable; 1512 and 1728 comfortable.

### What the review rounds caught after the decisions were made

Two adversarial passes ran on deliberately different lenses. Both found real defects in the lead's own rulings — recorded because the pattern matters more than the individual fixes.

**Round 1 (should this have been done?)** found that the first SV-1 amendment never addressed escape sequences split across WebSocket frames (`\x1b[2` in one frame, `K` in the next — neither half matches, the catch-all strip rule mangles them, the buffer is never cleared: the original garble, reappearing at chunk boundaries); that `\x1b[<n>A` had no floor, so a naive `splice(-n)` would wipe the whole array; that `\r\n` split across a chunk boundary would commit phantom blank lines; and that the "hard boundary" was a sentence with no mechanism. It also caught the lead over-correcting an earlier finding: `docs/feat/remote-control.md` does not contain the string "ENV-1" anywhere — the invariant text lives only at `docs/plan/done/remote-control.md:484`.

**Round 2 (can it be built?)** found the lead's own fix to be self-contradictory: it specified both `min(n, committedLineCount)` and "clamp to the oldest surviving line", which cannot both hold. It also found the pending tail had no terminating condition (a lone `\x1b` that never terminates freezes the view permanently with no visible error); that the clear-list omitted `FRAME_PTY_EXIT`, which `ptyBridge.js:170-180` shows is a bare notice with no reset flag while the common respawn path via idempotent `pty_spawn` emits no reset frame at all; that live frames arriving during the `pty_get_scrollback` await were unqueued and would commit out of order; and that the grep guard was wired to nothing, since the repo has no `.github/workflows` and no `.husky`.

Both rounds' findings were applied and independently verified. A third round was declined deliberately: round 1 attacked the decision and round 2 attacked the build, which are the two lenses that exist before code — the next real instrument is the implementation.

### `docs/plan/done/` batch defect

Swept 49/49 files at breadth (does each named artifact exist), 12/49 at depth (full manual read) — **capped, and stated**. Three of the four new defects share one mechanical root cause: docs were finalised citing `file:line` locations that a concurrent or immediately-prior refactor had already moved. The clearest case is `terminal-input-surface.md:123`, which cites `useWkImeGuard.js` — deleted in commit `4851f40`, *the same commit that edited that very doc line*. This is a missing "re-verify citations before landing in `done/`" step, not careless drafting.

Repaired: two stale `agent_usage.rs` citations (that file was split into `src-tauri/src/agent_usage/` by `610fd93`); a false filesystem claim in `cc-account-identity-ssot.md:172` (`google_accounts.json` and `state.json` both exist — only `oauth_creds.json` is absent — so the "blank AG tag" diagnosis built on it is also wrong, since `statusline-unified.sh:543-544` guards on that file and reads `.active`); an inverted behaviour claim in `terminal-input-surface.md:123` (current `usePtyTerminal.js:569-570` does the opposite of what the doc warns about); and a dangling reference in `improve-jun24.md` to `deferred-auto-update-toast-pos.md`, a file that never existed.

One item had genuinely fallen out of the cleanup: **§3 Toast positioning** had no owning doc, was absent from `remaining-1.22.md`, and the current code (`projectStore.js:7`, `position: 'bottom'`) matches neither the original problem state nor either proposed fix. It is now item 15 in `remaining-1.22.md`.

Three symbols flagged by a failed sweep were all cleared, not defects: `run_project_dev` (removed 2026-07-30, correctly cross-referenced in five docs), `useUpdater` (never built, but correctly recorded as dropped), `useToggleableSource` (removed by `59aeccc`, cited as a then-accurate refactor proposal).

### Open, and deliberately not acted on

`src-tauri/src/agent_usage/antigravity_logout.rs:134` deletes `["oauth_creds.json", "google_accounts.json", "state.json"]` on AG logout. `google_accounts.json` holds `{"active": …, "old": […]}` — the multi-account list. Deleting it on logout destroys the record of every other account, which is the class of defect `CLAUDE.md`'s "Regression Guard — Multi-entity State" exists to prevent after the 1.9.3 incident. Left untouched: it is code, outside this wish, and the project's own guard requires the owner's decision before a multi-entity store's clear-path changes.

## Method note

Findings were produced by a five-specialist council with a read-only mandate. Two candidate blockers from the first cheap sweep were **withdrawn on re-verification** (the app-icon dropdown does exist at `AppHeader.vue:5`; the capability pattern is real and only its filename was invented) — a flash-tier sweep's known skimming failure, caught because those findings were held provisional rather than acted on. Finding 3 reached its conclusion only because one agent refuted another's verdict and the second then checked the files and conceded half; neither opening position was right. Finding 1 was settled by running code, not by argument.
