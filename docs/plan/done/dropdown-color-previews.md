# Dropdown menu: self-demonstrating color previews

## What changed

Two rows in the app-icon dropdown menu (`src/components/AppHeader.vue`) now show their own
effect directly on the row, instead of only describing it in the tooltip/label text.

1. **Statusline Customizer row** — each letter of the label "Statusline Customizer" is painted
   with one color from the statusline customizer's actual supported palette, cycling through the
   8 colors in order (skipping spaces, which don't consume a color slot). The row is a live
   swatch of what the customizer can produce.
2. **Enable SSH Terminal Color row** — the row's background and text color now use the exact
   OSC 11 tint (`#1a0f0f`) that `install_ssh_terminal_color` patches into the terminal
   (`src-tauri/src/system.rs`, `SSH_COLOR_SNIPPET`). Hovering lightens the same tint slightly for
   feedback; the foreground stays a legible light red against it.

No new rows, labels, or separators were added — both previews reuse the existing
`.icon-dropdown-item` row and its icon/hover idiom (per the project's "Extreme Narrow" UI rule).

## Where the palette SSoT now lives

The 8-color statusline palette (`white/cyan/green/blue/grey/red/yellow/magenta` with their hex
values) previously lived only as a hardcoded array inside `ClaudeSettingModal.vue`. It is now
extracted to `src/utils/statuslineColors.js`, exporting `STATUSLINE_COLORS`:

- `ClaudeSettingModal.vue` imports it and assigns `const COLORS = STATUSLINE_COLORS` at the same
  definition site the literal used to occupy — everything else in that file (the `HEX` map, the
  color `<select>`, etc.) is unchanged.
- `AppHeader.vue` imports the same array to build `statuslineLabelChars`, the per-letter
  color/char pairs used to render the label.

This is a Law 1 (single source of truth) fix: the palette is now written once and referenced
twice, so a future palette change (adding/removing a color, adjusting a hex) only requires editing
`statuslineColors.js`.

The SSH terminal tint (`#1a0f0f`) is a one-off value with a single JS-side use site, so it was not
worth a shared module for it — it is declared once as a CSS custom property
(`--ssh-terminal-bg` on `.icon-dropdown` in `AppHeader.vue`) with a comment pointing at its real
source of truth, `src-tauri/src/system.rs`'s `SSH_COLOR_SNIPPET`. If that value is ever mirrored
in JS at a second location, it should move into a shared module at that point (rule of three /
risk-weighted evidence — one use site doesn't justify it yet).

## Why

- Keeps the app's existing "show, don't tell" instinct for state (color/outline/tooltip on
  existing elements) — extended here to *feature previews*, not just state.
- Removes a duplicated palette definition that would have silently drifted the next time either
  file's color list was edited without the other being updated.

## Files touched

- `src/utils/statuslineColors.js` (new — palette SSoT)
- `src/components/modals/ClaudeSettingModal.vue` (surgical: `COLORS` literal replaced with import)
- `src/components/AppHeader.vue` (template, script, and scoped style for both preview rows)
