# Project Table — column grid

The main window's project list (`src/components/ProjectTable.vue`). This doc owns only the
column-grid layout contract, not the individual buttons/badges inside each column — those are
covered where they're introduced (e.g. `docs/feat/in-app-terminal.md` for the TERMINAL column).

## One real grid, two subgrids

`.projects-grid` is the only element with `display: grid; grid-template-columns: var(--grid-cols)`.
`.grid-header` and every `.grid-row` are `subgrid`s of it (`grid-template-columns: subgrid`), not
independent grids that merely share the same `--grid-cols` value. `.grid-body` (the transition-group
wrapper) sits between them as `display: contents` so it doesn't break the subgrid chain.

**Why it has to be one grid:** each column's `1fr`/bare track is really `minmax(auto, 1fr)` — its
floor is the min-content width of whatever sits inside *that specific grid*. Two independent grids
computed that floor separately: the header's SYNC cell holds only the short label "SYNC", a row's
SYNC cell holds the full PUSH/DRY/PULL/LOG/gear button cluster — different min-content, different
real pixel width, so the header drifted out of alignment with the rows below it. A single grid with
subgrid children shares one auto-floor computed once across all of them, so this can't happen no
matter what any individual row's content needs. Fixed 2026-07 (`CHANGELOG.md` "no longer drifts out
of alignment"), tracked as verify item U1 in `docs/plan/verify-pending.md` until confirmed at the
420px floor on a real window.

## Column widths (`--grid-cols`)

Six tracks: project info, GIT, TASKS, ACTIONS, LAST, SYNC.

- **Wide** (`.projects-table-container`, default): `minmax(12rem, 2fr) 2.5rem 2.5rem 2.5rem 7rem 1fr`
- **Narrow** (`≤700px`, `RULE-stack-tauri`-style breakpoint shared app-wide): `minmax(7.5rem, 2fr) 2.1rem 1.9rem 2.5rem 4.2rem 1fr`

Only two tracks flex: **project info** (`minmax(_, 2fr)`) and **SYNC** (bare `1fr`, i.e.
`minmax(auto, 1fr)`), weighted 2:1 toward the project column. A wider window spends its extra width
on the name/path, not on the sync button cluster. SYNC deliberately never gets an explicit
`minmax(Nrem, 1fr)` — an explicit length *replaces* the free min-content floor instead of adding to
it, and every rem guess tried so far ended up smaller than the PUSH+DRY+PULL+LOG+gear cluster's own
icon-only width (~176px at the narrow breakpoint), crushing the buttons into each other. Let the
bare `1fr` auto-floor stand instead of re-guessing a fixed minimum.

## Changing this layout

- Both `--grid-cols` values (wide + narrow) live only in `ProjectTable.vue` — there is no other
  copy to keep in sync.
- Adding, removing, or resizing a column: edit `--grid-cols` in both breakpoints, and check the
  matching `.grid-header-cell`/`.grid-row-cell` order lines up positionally (subgrid has no named
  columns — track N in `--grid-cols` is cell N in DOM order, in header and row alike).
- Never make SYNC (or any button-cluster column) a fixed `minmax(Nrem, 1fr)` — see above.

## Related

- `docs/plan/verify-pending.md#u1` — the one still-open runtime check for this layout (420px floor).
- `docs/plan/remaining-1.22.md` PT-DOC — the decision record for why this doc exists (it didn't
  until 2026-08-01, despite the table being the app's primary surface).
