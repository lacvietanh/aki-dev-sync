# Research: SYNC column next to the project name

**Asked 2026-11-16.** The owner keeps only about the left half of the project list in view (other windows overlap the rest) and wants the PUSH/DRY/PULL cluster there, with TASKS, GIT, TERMINAL and ACTION (OPEN, per-project refresh) moved to the right edge. Shipped the same day; option A below was a misreading and was replaced, see § Correction (`CHANGELOG.md` [Unreleased] § Changed; layout contract in `docs/feat/project-table.md`).

## Today

- `src/components/ProjectTable.vue` is one CSS grid: `.projects-grid` owns `grid-template-columns: var(--grid-cols)`; the header and every row are `subgrid`s (`docs/feat/project-table.md`). Track N is cell N in DOM order, in header and row alike. No named areas.
- Order: project info, TASKS, GIT, TERMINAL, ACTION, SYNC.
- `--grid-cols` wide: `minmax(12rem, 2fr) 2.5rem 2.5rem 2.5rem 7rem 1fr`; narrow (`@container main-view (max-width: 700px)`): `minmax(7.5rem, 2fr) 2.1rem 1.9rem 2.0rem 4.2rem 1fr`. Both live only in `ProjectTable.vue`.
- SYNC is a bare `1fr` on purpose: its min-content floor is the cluster's own width, and every explicit `minmax(Nrem, 1fr)` tried crushed the buttons (`docs/feat/project-table.md` § Column widths).
- The OPEN popup is `.pin-left`: viewport-relative left edge, not anchored to its trigger, so moving the trigger to the right edge does not move the popup.
- No keyboard navigation, drag/drop or badge positioning depends on column order. `SimpleView.vue` does not mirror this grid; the phone companion renders the same Vue bundle, so it follows automatically.

## Options

| | Order | Left half shows | Cost |
|---|---|---|---|
| A (recommended) | project, SYNC, TASKS, GIT, TERMINAL, ACTION | icon, name, path, host, PUSH/DRY/PULL, last action | header + row DOM order, both `--grid-cols`, `.col-terminal` margin rule, docs |
| B | SYNC, project, ... | whole sync cluster, project icon only; name pushed right | same as A, but a row can no longer be identified at a glance |
| C | A behind a setting | either | a new setting and two grid definitions for one user (YAGNI, and a new control is against the narrow-UI rule) |

## Recommendation: A, as the only layout

- `--grid-cols`, project absorbs the spare width and SYNC hugs its content: wide `minmax(12rem, 1fr) auto 2.5rem 2.5rem 2.5rem 7rem`, narrow `minmax(7.5rem, 1fr) auto 2.1rem 1.9rem 2.0rem 4.2rem`. `auto` keeps the free min-content floor the doc requires.
- Move the `col-sync` header cell (label, `RefreshRing`, power toggle) and row cell right after `col-project-info`. TASKS ring and the global `>_` button move right with their columns.
- Recheck the narrow `.col-terminal { margin-left: 0 }` rule: TERMINAL then follows GIT instead of preceding ACTION.
- `.last-action` ("PUSH 17d ago", host) stays inside the SYNC cell; its `title` carries the full text.
- Measured on the hidden dev instance (`npm run ctl -- eval`), 436px column: project 0-120, PUSH/DRY/PULL 126-216, SYNC cell ends at 292. SYNC is its content width (166px narrow, 305px wide) whether the track is `auto`, `1fr`, `min-content` or `fit-content`. The grid ends at 481, ~50px past the column, the same total as before the reorder: the clipped part is now ACTION's refresh button instead of LOG/gear.

## Verification

- Header and row cells line up track for track (measured at 720px; header and rows are one subgrid, so every width shares the same tracks).
- The OPEN popup is `.pin-left` and the conflict popup clamps to the viewport, so neither depends on its trigger's column.
- `npm run audit:ui` exit 0.

## Correction (2026-11-16)

Option A read the request wrong: it moved the whole SYNC cell, LOG and config included, ahead of TASKS and GIT. The owner asked for one thing only: terminal, OPEN and refresh go to the right of PUSH/DRY/PULL. TASKS and GIT keep their place, and LOG and config stay at the far right, where they were already past the visible edge and rarely used. The goal is to fill the left part of the table with what is used most.

Shipped order: project, TASKS, GIT, SYNC (PUSH/DRY/PULL + last action), TERMINAL, ACTION (OPEN, refresh), LOG + config. LOG and config left the sync cell for a seventh track; `--grid-cols` is the original with `auto` inserted for SYNC (`minmax(_, 2fr) _ _ auto _ _ 1fr`). Measured at 436px: PUSH/DRY/PULL 203-293 (they sat at about 310-420 before), terminal 299-331, OPEN 336-368, refresh 374-406, LOG 411-443, config 449-481; header and row cells share every track edge at 436 and 720px.
