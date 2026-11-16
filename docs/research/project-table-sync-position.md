# Project table: where the PUSH/DRY/PULL buttons sit

## Start time

2026-11-16 (Asia/Saigon), against `2b60537` (1.36.0).

## Initial purpose

Owner request: the terminal, OPEN and refresh buttons sit left of PUSH/DRY/PULL; move them to the right so PUSH/DRY/PULL come forward. The reason given with it: the owner keeps only about the left half of the window in view (other windows cover the rest), so that half must hold what is used most. LOG and config already sat past that edge and are rarely used. Question: which column order serves that, and what the grid allows.

Context at the time:
- `src/components/ProjectTable.vue` is one CSS grid: `.projects-grid` owns `grid-template-columns: var(--grid-cols)`; the header and every row are `subgrid`s (`docs/feat/project-table.md`). Track N is cell N in DOM order, in header and row alike.
- Order: project info, TASKS, GIT, TERMINAL, ACTION (OPEN, refresh), SYNC (PUSH/DRY/PULL, last action, LOG, config).
- `--grid-cols` wide `minmax(12rem, 2fr) 2.5rem 2.5rem 2.5rem 7rem 1fr`, narrow (`@container main-view (max-width: 700px)`) `minmax(7.5rem, 2fr) 2.1rem 1.9rem 2.0rem 4.2rem 1fr`.
- SYNC is a bare `1fr` on purpose: an explicit `minmax(Nrem, 1fr)` replaced its min-content floor and crushed the buttons.

## Strategy

Static read of the grid and of everything positioned relative to a column, then measure candidate orders on the hidden dev instance (`npm run dev:agent` + `npm run ctl -- eval`, `docs/ref/agent-control.md`) at 436, 720 and 1000px.

## Checklist

- [x] Read the grid contract and both `--grid-cols`
- [x] List what depends on column order (popups, keyboard, drag, badges)
- [x] Measure each candidate order
- [x] Owner review of the installed build

## Result

Nothing but the grid depends on column order: the OPEN popup is `.pin-left` (viewport-relative), the conflict popup clamps to the viewport, and no keyboard navigation, drag/drop or badge position reads a column index. `SimpleView.vue` does not mirror the grid; the phone companion renders the same bundle.

| Order | Verdict |
|---|---|
| project, TASKS, GIT, SYNC, TERMINAL, ACTION, LOG + config | shipped: only terminal, OPEN and refresh move, to the right of PUSH/DRY/PULL |
| project, SYNC (with LOG, config), TASKS, GIT, TERMINAL, ACTION | built first, rejected by the owner: it moved TASKS and GIT, which were not part of the request, and pulled the rarely used LOG and config into the visible half |
| SYNC first, project after | rejected: a row can no longer be identified at a glance |
| either order behind a setting | rejected: a second grid definition and a new control for one user |

Shipped tracks: the original `--grid-cols` with `auto` inserted for SYNC and LOG + config split into a seventh track that keeps the bare `1fr` (`minmax(_, 2fr) _ _ auto _ _ 1fr`).

### Verification

- Measured at a 436px column: name 0-120, TASKS 126-160, GIT 166-197, PUSH/DRY/PULL 203-293 (about 310-420 before), terminal 299-331, OPEN 336-368, refresh 374-406, LOG 411-443, config 449-481. The grid is ~50px wider than the column, as before the change; the part past the edge is still LOG and config.
- Header and row cells share every track edge at 436 and 720px.
- `npm run audit:ui` exit 0.
- Owner reviewed the installed build and accepted it (2026-11-16).

### Corroborating links

- `docs/feat/project-table.md` § Column widths: why SYNC never gets an explicit length.

## Decision

**Action:** `docs/feat/project-table.md` (order, `--grid-cols`, measurements) and `src/components/ProjectTable.vue`.

Rejected: the three other orders in the table above.

Reopen if the owner wants LOG or config inside the visible half, or the window's usual visible width changes.
