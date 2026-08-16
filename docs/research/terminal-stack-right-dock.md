# Terminal stack right-dock — research

**Start time:** 2026-08-16, this session.

**Initial purpose:** `docs/plan/remaining-1.25.md` item 3 (TERM-STACK-R) asks for the terminal stack to stop flying over the project table when the window is narrow and instead become a right-side column when the window is wide — main view capped at ~900px, terminal taking the extension space, with a breakpoint between the two layouts. The plan explicitly marks this "research first" and requires reading `docs/plan/done/dock-stack-independent-height.md` before forming an opinion, to avoid re-fighting the per-stack independent-height model that shipped and was verified on Mac in 1.24.0 (2026-08-15). Context at start: the dock currently has two vertically-stacked, independently-collapsible panels (TerminalStack, LogStack) whose combined height is a straight sum of two per-stack lengths — that model is one day old on the owner's Mac and its verification record is the thing this research must not silently invalidate.

**Strategy:** read the 1.24.0 independent-height design doc first (as instructed), then the architecture and feature docs for the terminal stack, then the actual layout code end to end — `App.vue`, `main.css`'s layout rules, `AppConsole.vue`, `DockStack.vue`, `TerminalStack.vue`, `useDockLayout.js` — to establish which parts of the current vertical, height-summed model a right-dock would have to change versus can reuse untouched. Every factual claim below is tagged FACT (read directly in the cited file:line), CONSTRAINT (a rule stated in `CLAUDE.md` or a rule file that bounds the design), or ASSUMPTION (not read anywhere, inferred or owner-only knowledge).

**Checklist:**
1. Read `docs/plan/done/dock-stack-independent-height.md` in full.
2. Read `docs/plan/remaining-1.25.md` item 3 and its cross-referenced entry points.
3. Read `docs/arch/terminal-stack.md` and `docs/feat/in-app-terminal.md` for the panel-height and mount-semantics sections.
4. Read `src/components/AppConsole.vue`, `src/components/DockStack.vue`, `src/components/dock/TerminalStack.vue`, `src/composables/useDockLayout.js` in full.
5. Read `src/App.vue` and the relevant `src/assets/main.css` layout rules (`.dashboard-layout`, `.dashboard-top`, `.dashboard-bottom`, `.dock-splitter`, `.dock-stack`) to see the outer flex chain the dock sits inside.
6. Grep the codebase for any existing width-driven breakpoint or JS width-tracking, to check for a reusable precedent before proposing a new one.

## 1. Current state — what a right-dock must change vs. reuse

**FACT — the whole layout is one vertical flex chain, top to bottom.** `App.vue:2` wraps everything in `.dashboard-layout`; `main.css:116-123` sets it `display: flex; flex-direction: column`. Inside it, `.dashboard-top` (project table, `App.vue:8-12`, `main.css:125-131`, `flex: 1`) sits above `<AppConsole />` (`App.vue:14`, `.dashboard-bottom`, `main.css:134-146`, `flex: none`, inline `height` from `useDockLayout.js`). A right-dock needs `.dashboard-layout` itself (or a wide-mode wrapper around it) to become row-based above the breakpoint — this is a change to the outermost layout, not something contained inside `AppConsole.vue`.

**FACT — the dock's own two stacks are also a vertical flex chain, one level in.** `AppConsole.vue:13-14` renders `<TerminalStack />` then `<LogStack />` as plain siblings inside `.dashboard-bottom` (`display: flex; flex-direction: column`, `main.css:138-139`). `main.css:200-203`'s `.dock-stack ~ .dock-stack { border-top: ... }` is a vertical-stacking-specific rule (general sibling selector, top border) that would need to become a `border-left` if the two stacks were ever placed side by side.

**FACT — `useDockLayout.js`'s whole sizing model is height-native, not axis-generic.** `stackPct` (`useDockLayout.js:54`) is a percentage of `window.innerHeight` (`stackLengthCss`, line 82-86: `calc(var(--vvh, 100vh) * ${pct} / 100)`); `MIN_PCT`/`MAX_TOTAL_PCT` (lines 15-16) bound that same height percentage; `setStackHeightFromPointer` (lines 104-108) reads `clientY` and a `bottomY` anchor computed from `getBoundingClientRect().bottom` (`DockStack.vue:72`) — a vertical-only drag math. The persisted schema (`STORAGE_KEY = 'aki-dock-stack-pct'`, `{terminal, log}`, lines 12, 44-51) stores height percentages. None of this generalizes to a width-based right column without either a parallel width-axis model or a rewritten axis-parametric one (see Option A below).

**FACT — `DockStack.vue`'s splitter is hardcoded to a vertical drag.** `.dock-splitter` is `height: 3px`, `cursor: ns-resize` (`main.css:154-165`); `onSplitterMove` (`DockStack.vue:70-73`) only ever reads `e.clientY`. A right-docked panel's resize handle (if one is wanted — see open question 3) would need an `ew-resize` variant reading `clientX`, which does not exist anywhere in the current code.

**FACT — MAXIMIZE is also height-native.** `dockMaximized` drives `MAXIMIZED_CSS = 'calc(var(--vvh, 100vh) - var(--titlebar-h))'` (`useDockLayout.js:19`) and `dockStackFlex`'s maximized branch (`flexGrow: 1, flexBasis: '0px'`, lines 96-101) — a width-based equivalent (`calc(100vw - <main-cap>)`) does not exist and the button that triggers it (`TerminalStack.vue:41-48`) has no width-mode concept.

**FACT — collapse/peek geometry is also height-shaped.** `STACKS.terminal.collapsedCss = 'var(--dock-header-h)'` and `STACKS.log.collapsedCss = 'calc(var(--dock-header-h) + var(--dock-peek-h))'` (`useDockLayout.js:22-31`) are both vertical lengths (a header row height, a peek-line height). A right-docked, width-sized panel's "collapsed" state has no obvious width equivalent to `--dock-peek-h` — a peek line is inherently a horizontal strip, which does not translate to a vertical strip in a column layout.

**FACT — `TerminalStack.vue`'s own content (`.terminal-mount-wrap`) is axis-agnostic.** It is `flex: 1; min-height: 0; display: flex` (`TerminalStack.vue:164-168`) and simply fills whatever box `DockStack.vue`'s `.dock-stack` gives it. `TerminalView.vue`'s fit logic (per `docs/arch/terminal-stack.md`'s mount-semantics section) reacts to container size via `ResizeObserver`, not to a specific parent flex direction — so the PTY view itself is not a blocker; only its two ancestor layers (`DockStack.vue`, `useDockLayout.js`) are vertical-native.

**FACT — no existing width-driven structural breakpoint to reuse.** `main.css:1357-1359` defines exactly one app-wide breakpoint, `@media (max-width: 700px)`, gating only `.u-narrow-hide`/companion narrow-mode utility classes (label hiding, icon-only buttons) — a CSS-only cosmetic toggle, not a structural layout switch, and not JS-tracked (`grep` for `window.innerWidth`/`innerWidth` in `src/**` returned zero component/composable hits). Any width-based mode switch for the right-dock (JS-driven, since flex-direction and which composable is active must be JS-conditional, not purely CSS) is new plumbing, not a reuse of an existing mechanism.

**CONSTRAINT — Extreme Narrow UI (`CLAUDE.md`).** No new row/banner/label for state; a right-dock's own mode indicator (if any) must ride existing chrome, not add a new element.

**CONSTRAINT — pattern-core Law 6 (module boundaries) and Law 8 (reshape, not guard).** A right-dock is explicitly named in the plan row as a Law 6/Law 8 case: the layout's *shape* changes with viewport width, so the fix belongs in the geometry model, not as an `if (isWide)` patch bolted onto the existing vertical one.

## 2. Design options

### Option A — generalize `useDockLayout.js` to an axis-parametric model

Rewrite the sum-of-lengths model to take an `axis: 'height' | 'width'` per mode, so `stackLengthCss`, `clampPct`, `setStackHeightFromPointer`, `dockStackFlex`, and `MAXIMIZED_CSS` all branch on axis instead of assuming height. `DockStack.vue`'s splitter becomes axis-aware (`ns-resize`/`clientY` vs `ew-resize`/`clientX`). `.dashboard-layout` becomes `flex-direction: row` above the breakpoint (CSS class bound to a new JS width-tracking ref). Both `TerminalStack` and `LogStack` could in principle dock right together, or only `TerminalStack` moves while `LogStack` stays vertical — either way the composable itself is now shape-agnostic.

- **Cost:** touches every function in `useDockLayout.js` (the file the 1.24.0 rework just finished and got a Mac-verified pass on), `DockStack.vue`'s splitter, `main.css`'s dock rules, plus new JS width tracking and a persisted-schema migration (percent-of-height values are meaningless once redocked to percent-of-width).
- **Risk:** the highest-risk file to touch is exactly the one the plan told this research to read first *in order to avoid re-fighting* — a rewrite here re-opens every one of the seven runtime checks `dock-stack-independent-height.md` §4 lists as owner-confirmed-passing 2026-08-15, one day before this research started. Any regression there is a second pass at a bug class already fixed once.
- **What it breaks (if done carelessly):** collapse/peek CSS tokens (`--dock-header-h`, `--dock-peek-h`) have no width equivalent and would need new tokens invented from scratch, not reused; MAXIMIZE has no width equivalent designed yet either (see open question 5).

### Option B — a new, separate width-axis layout for the terminal stack only, existing model untouched

Leave `useDockLayout.js`, `DockStack.vue`'s splitter, and the current vertical dock entirely as they are (`LogStack` always stays at the bottom, in both narrow and wide mode). Add a new JS width-tracking ref and a wide-mode branch in `App.vue`/`main.css` that, above the breakpoint, moves `<TerminalStack />` out of `AppConsole.vue` into a new right-column slot with its own new, independent width-based sizing composable (e.g. `useRightDockLayout.js`) — a parallel model, not a generalization of the existing one. `TerminalStack.vue`'s own template still needs *some* chrome (header/title/actions/collapse button); rather than duplicate `DockStack.vue`'s markup wholesale, extract only the header/slot chrome that is genuinely shared (title slot, actions slot, collapse button) into a small presentational piece both the existing `DockStack.vue` (vertical) and a new right-column wrapper (horizontal) compose — satisfying Law 5 (composition) without copy-pasting the whole component.

- **Cost:** new composable, new small shared-chrome extraction, `App.vue`/`main.css` wide-mode branch, JS width tracking (none exists yet, per §1). `LogStack.vue` and `useDockLayout.js` need zero changes.
- **Risk:** two parallel dock-chrome shapes to maintain going forward instead of one; if the owner later wants `LogStack` to also dock right (open question 2), this option's separation becomes a liability and Option A's generalization becomes the better-justified choice retroactively.
- **What it breaks:** nothing in the already-verified vertical model — by construction, the existing files are untouched.

## 3. Recommended direction

**Option B (narrow, additive shape), pending the open questions below.** The plan row names only "the terminal stack" moving right, not `LogStack` — as read, that is one stack changing axis, not two, which does not yet clear pattern-core's Rule of Three bar (`pattern.A2`) for generalizing the shared geometry model, and Option A's blast radius lands squarely on the file the plan explicitly asked to be read first *to avoid re-fighting* a one-day-old, Mac-verified change. Option B's cost is a new, additive composable plus one shared-chrome extraction — reversible, and it does not touch anything the 2026-08-15 verification record covers.

**What would falsify this recommendation:** if the owner's actual intent (open question 2) is that `LogStack` also becomes right-dockable, or that both panels share one resizable right column, then two things need the width axis rather than one, Option A's generalization clears the same evidence bar it currently misses, and building Option B first becomes wasted, throwaway work that a later Option A rewrite discards.

## 4. Open questions the owner must settle before code starts

1. **Breakpoint value and axis.** The app already has one width breakpoint, 700px (`main.css:1357`), for an unrelated narrow-mode label/icon toggle. Is the right-dock breakpoint the same 700px, a new second breakpoint, or is "main view capped at ~900px" actually a minimum-*total*-window-width trigger (switch to right-dock once the window is wide enough to hold 900px of table plus a usable terminal column)? Conflating the existing narrow-mode breakpoint with this one would be a real design mistake, not a cosmetic one.
2. **Does `LogStack` move too, or stay at the bottom in both modes?** Directly decides between Option A and Option B above.
3. **Is the ~900px main-view cap fixed, or user-resizable?** I.e. does the right dock get its own drag handle and persisted width, matching what the bottom stacks already have, or is it simply "whatever space is left"?
4. **Live breakpoint crossing.** If the window is resized across the breakpoint while running, does each stack's collapsed/expanded state carry over, and what happens to a persisted vertical percentage-of-height length once a stack is redocked to a percentage-of-width column — is it discarded, reset to a default, or does the schema need to store both?
5. **MAXIMIZE semantics in wide mode.** Does the right-docked terminal get an equivalent "fill available width" affordance, and if so, does it hide/interact with the project table the way MAXIMIZE today hides the app header?
6. **Untested code path for `TerminalView`'s fit logic.** Its `ResizeObserver`-driven fit has, per the architecture doc, only ever run inside a height-driven flex column; a width-driven container is a new geometry shape for it. This is not expected to be a structural blocker (§1 above) but has not been exercised and should be an explicit item in whatever plan doc follows this research.

## 5. Verification

Static reading only — every claim in §1 is fully determined by the cited file:line and needed no runtime check to settle (`coding.B3`). §2/§3 are architectural judgment on top of that reading, not verified against actual rendering; before committing to either option, a short throwaway spike (rough right-column CSS + one placeholder `TerminalStack` mount) is the cheapest way to catch a wrong assumption about how much of `DockStack.vue`'s chrome can genuinely be shared, since that is the one claim in §2 not settled by reading alone.

**Corroborating links:** `docs/plan/done/dock-stack-independent-height.md` (the model this research must not silently re-fight — read first, per instruction), `docs/arch/terminal-stack.md` § The dock is the sum of its stacks, `docs/feat/in-app-terminal.md` § Panel size and font zoom, `docs/plan/remaining-1.25.md` item 3 (the owning plan row).

## 6. Decision

**No action (code) taken — by design, this doc is the research step the plan row required before any implementation.** Recommended direction: Option B (§3), conditional on open question 2's answer. Next artifact, once the owner has answered §4: a `docs/plan/wish-terminal-right-dock.md` design/execution doc — not created in this pass.

**Cross-references:** `docs/plan/remaining-1.25.md` item 3 (TERM-STACK-R — should link here once this doc lands), `docs/arch/terminal-stack.md` (unchanged by this research; a right-dock's eventual landing will need a new section there), `docs/plan/done/dock-stack-independent-height.md` (the model this research is scoped not to disturb).
