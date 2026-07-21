# README + IntroModal refresh (v1.15.0)

Status: done.

## Why

`README.md` and `IntroModal.vue` had been patched piecemeal across many releases and no longer
matched the code at 1.15.0. Several features referenced in both were removed or restructured in
1.13.0-1.15.0 without the user-facing docs catching up.

## What was inaccurate

- **Force Sync Quota** (README feature table + `README.md` "Under the Hood" + IntroModal feature
  card) described a headless `claude --model haiku -p /usage` probe. That entire active flow  - 
  force-sync, the headless probe, orphan-session cleanup - was deleted in 1.14.0 (CHANGELOG
  "Removed" section, `docs/arch/usage-claudecode.md` §5). The statusLine hook cache is now the
  only Claude Code usage data source; the app never runs `claude` itself to read usage.
- **Remote Mode** (single master switch) - split in 1.15.0 into two independent switches:
  `syncCheckEnabled` (sync/diff/PUSH/PULL) and the Claude Code remote monitor's own toggle
  (`aki-src-ccremote-enabled`). README and IntroModal both still described the old single-switch
  behavior.
- **Background Refresh** - described as three independent pollers with no shared concept. 1.15.0
  introduced one refresh controller / one unit of work (`refreshProject()`,
  `docs/arch/refresh-controller.md`): the global Refresh button no longer reloads the whole app,
  and busy state now lives on the check itself (a per-project counter), not on whichever button
  was clicked.
- **DEV button** - 1.14.0 added an auto-open-in-browser behavior for web projects; 1.15.0 removed
  it entirely after two failed fix attempts (`run_project_dev` now only opens Terminal, same as
  BUILD). Neither doc claimed the browser-open behavior outright, but README's Open Popup row was
  edited to make the current (Terminal-only) behavior explicit rather than silent.
- **Missing features entirely**: the app-icon dropdown menu (GitHub/release links, manual update
  check, SSH config, Enable SSH Terminal Color, Statusline Customizer, Claude Code Profile,
  AkiClaudeDoc install) and its window-size presets (Narrow/Wide/Stick-Top-Left/Center Primary)
  were never described in README or IntroModal at all, despite existing in the app since 1.11.0.
- **Build command**: README's "Build from source" section only mentioned `npm run build:app`
  (local `.app`, Apple Silicon only). Added `npm run build:rmud` as the release build (universal
  `.dmg`), per `CLAUDE.md`'s "Release build command" rule.
- **`docs/index.md`**: two existing doc files were not indexed anywhere  - 
  `docs/research/antigravity-usage-new-4line.md` and `docs/plan/done/narrow-mode-and-ux-1.14.0.md`.
  Both added.
- **Dead doc links**: README's Documentation section linked `docs/feat/remote-mode.md`, which does
  not exist (the switch was documented as `sync-check-and-usage-switches.md`). Fixed.

## What changed

- `README.md`: rewrote the Sync/Tools feature table entries for Agent Usage, the two switches,
  Background Refresh, plus new rows for the App-icon menu and Statusline Customizer. Rewrote
  "Under the Hood" to drop the Force-Sync-with-Auto-Probe bullet and add bullets for the
  single-source-of-truth Claude Code architecture and the one-refresh-controller design. Fixed
  dead doc links. Added `build:rmud` to the build section.
- `src/components/modals/IntroModal.vue`: replaced the "Remote Mode" feature card with a
  "Sync Check & CC Remote - 2 công tắc độc lập" card describing both switches. Replaced the
  "Force Sync Quota" feature card with a "Refresh - 1 unit of work" card. Added two Engineering
  Highlights bullets (Claude Code single-source-of-truth, app-icon menu + window presets) and
  removed the Force-Sync-với-Auto-Probe bullet. No new DOM rows were added beyond swapping
  existing cards/bullets 1:1 - kept to the project's Extreme Narrow UI principle.
- `docs/index.md`: added the two previously-unindexed docs above.
- Verified `npm run build` (vite build) passes after the `IntroModal.vue` edit.

## Keeping this in sync going forward

- Per `RULE-docs.md` (docs.B2): when a change lands that a `feat`/`arch` doc already describes,
  update that doc in the same change - don't let CHANGELOG be the only record.
- Any change that **removes** a feature described in README/IntroModal must, in the same PR,
  either delete or rewrite the corresponding row/card - not just add a CHANGELOG "Removed" entry.
  A "Removed" entry is a historical record; README/IntroModal are the current-state view and must
  never describe dead code.
- When splitting or merging a switch/store (as with `remoteModeEnabled` → `syncCheckEnabled` +
  `ccRemote`), grep README.md and IntroModal.vue for the old name before closing out the task  - 
  both files reference user-facing switches by their old names more often than code does.
- `docs/index.md` should be updated whenever a new file lands under `docs/{arch,feat,ref,plan,
  research}` - this refresh found two files that had silently gone unindexed for a full release
  or more.
