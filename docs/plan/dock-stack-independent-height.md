# Dock stacks own their own height

**Status:** code landed 2026-08-12, runtime verification outstanding (§4). Architecture: `docs/arch/terminal-stack.md` — the dock/stack layering. User-facing: `docs/feat/in-app-terminal.md` § Panel height.

## 1. The defect

The bottom dock presents two independent panels — separate headers, separate collapse chevrons — whose heights were coupled. `.dashboard-bottom` had a stored height (`dockHeightPct`) that did not depend on which stacks were expanded, and the two `.dock-stack` children divided it with `flex: 1 1 0`. So the space a collapsing stack freed was not returned to the app; the sibling's `flex-grow` swallowed it.

| Gesture | Was | Should be |
|---|---|---|
| Both expanded → collapse terminal | log jumps up to fill | dock shrinks, log does not move |
| Both expanded → collapse log | terminal grows downward | dock shrinks, terminal does not move |
| Terminal collapsed + log expanded → expand terminal | log dips to half | dock grows upward, log does not move |
| Terminal collapsed + log expanded → collapse log | reads as both closing at once | only the log's own length disappears |

The last row is a consequence of the others rather than a separate bug: the log had already grown into the terminal's share, so closing it removed far more height than the log itself ever occupied.

## 2. The model

**The dock's height is the sum of the two stacks' lengths.** Each stack owns a percentage of window height (`stackPct`, per-stack); a collapsed stack contributes its header instead (plus one peek line for the log stack). Nothing has `flex-grow`, so a sibling's collapse is arithmetically invisible.

- **Resize** — one 3px handle at the top of each *expanded* stack, resizing only that stack (`DockStack.vue`, reusing the existing `.dock-splitter`). The dock's single top-edge splitter is gone. Double-click resets that one stack.
- **Clamp** — each stack ≥10% of window height; the ceiling is on the SUM (85%), so a drag never has to write the sibling to stay inside it. MAXIMIZE remains the explicit way past it.
- **Maximize** — unchanged in behaviour and the one mode that suspends the sum: dock goes to `calc(100vh - titlebar)` and the expanded stacks share it with `flex: 1 1 0`.
- **Persistence** — `localStorage['aki-dock-stack-pct']` = `{terminal, log}`, replacing `aki-dock-height-pct`. Still per-screen: `useDockLayout.js` is a composable, so `services/mirror.js` (which scans `src/store/` only) cannot discover it and a phone's drag cannot resize the Mac.

### Why it is not `height: auto`

The dock height computes to exactly the sum, so `auto` would give the same number — and would kill the collapse animation, because a CSS length cannot be transitioned to or from `auto`. `dockHeightCss` is therefore a `calc()` string built from the same `--dock-header-h` / `--dock-peek-h` tokens the stylesheet uses, and every per-stack endpoint stays a numeric length (`0px`, never the `flex` shorthand's implicit `auto`). This is the same discipline the collapse transition already followed for `flex-basis`.

## 3. Files

| File | Change |
|---|---|
| `src/composables/useDockLayout.js` | rewritten around the per-stack length map: `dockHeightCss` (sum), `dockStackFlex(key)`, `setStackHeightFromPointer(key, y, bottomY)`, `resetStackHeight(key)`. `dockHeightPct`/`resetDockHeight`/`setDockHeightFromPointer` are gone |
| `src/components/DockStack.vue` | owns each stack's splitter (two root nodes — the handle must sit outside `overflow: hidden`), takes `stackKey`, applies `dockStackFlex` inline. Dead `title`/`icon`/`titleClass` props removed (both stacks fill the `#title` slot) |
| `src/components/AppConsole.vue` | dock-level splitter, its handlers and `.is-all-collapsed` removed; keeps the height binding, the transition listeners and Esc |
| `src/assets/main.css` | `--dock-collapsed-h` → `--dock-header-h`; `.is-all-collapsed`, `.dock-stack.is-collapsed`, `.has-peek.is-collapsed` deleted; `.dock-stack + .dock-stack` → `~` (a splitter now sits between them); `.is-dragging` also kills `.dock-stack`'s transition |
| `src/components/dock/{TerminalStack,LogStack}.vue` | pass `stack-key` |

`.is-dragging .dock-stack { transition: none }` is load-bearing, not tidiness: the drag now writes `flex-basis`, and without it the drag would both lag the pointer and fire `transitionrun`, which sets `dockAnimating` and suspends `TerminalView`'s live per-frame fit. `DOCK_ANIM_PROPS` needed no change — `height`, `flex-grow` and `flex-basis` were already the animated set.

## 4. Verification

Static (settled by reading the flow): the all-collapsed sum reproduces the old `--dock-collapsed-h` arithmetic exactly; no reference to a removed export, class or token remains (grep, `src/**`); mirror isolation unchanged (no new `src/store/` ref).

Runtime — **unverified, needs a Mac run** (`npm run build:app` targets `aarch64-apple-darwin`; the dev box cannot compile it):

1. Both expanded → collapse terminal: log stays exactly where it is, dock shrinks.
2. Both expanded → collapse log: terminal stays exactly where it is, dock shrinks.
3. Terminal collapsed + log expanded → expand terminal: log does not move.
4. Terminal collapsed + log expanded → collapse log: only the log's own height disappears.
5. Drag each handle: tracks the pointer with no lag, terminal re-fits live during its own drag, sibling unmoved. Double-click resets only that stack.
6. MAXIMIZE / restore still works; collapsing everything leaves maximised mode.
7. Quit and relaunch: both heights restored (one-time reset expected — the old single-height key is not migrated).
