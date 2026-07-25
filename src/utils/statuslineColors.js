// SOURCE OF TRUTH for the statusline's whole color vocabulary: the eight-color field picker and
// the five-tier dynamic-color ladder. One record per color, `{ key, label, ansi, hex }`:
//   ansi - the bare SGR parameters the shell script prints (no ESC[ prefix, no trailing m)
//   hex  - what that exact code renders as in a terminal, NOT a hand-picked approximation
// The two used to be chosen independently (Tailwind hexes here, ANSI codes in the script), so the
// swatch drawn by the app never matched the line printed by the terminal - at every single tier.
// Deriving both from one record is what stops that drift coming back.
//
// PALETTE ASSUMPTION (the reason the two sides could drift at all): codes 30-37/90-97 have no
// fixed hex - they are whatever palette the terminal ships. These hexes are xterm's own defaults,
// and bold (`01;`) on a 30-37 foreground is taken to promote it to the bright 90-97 variant, which
// is what xterm, Terminal.app, iTerm2 and gnome-terminal all do by default: `01;32` is therefore
// bright green #00FF00, not normal green #00CD00. Codes of the form `38;5;N` address the 256-color
// cube instead and are palette-independent, so those hexes are exact everywhere.
//
// MIRRORS - neither Rust nor shell can import this file, so they carry copies. Change this file
// first, then the two below; nothing else may restate a statusline color:
//   src-tauri/src/statusline.rs        ansi_for()      <- STATUSLINE_COLORS[].ansi
//   src-tauri/src/statusline-unified.sh  BOLD_* block  <- STATUSLINE_TIERS[].ansi
// Hex consumers: ClaudeSettingModal.vue (pickers, ladder, live preview) and AppHeader.vue (paints
// its dropdown label with this palette so the row demonstrates the feature).
export const STATUSLINE_COLORS = [
  { key: 'white',   label: 'White',   ansi: '97',    hex: '#FFFFFF' },
  { key: 'cyan',    label: 'Cyan',    ansi: '36',    hex: '#00CDCD' },
  { key: 'green',   label: 'Green',   ansi: '01;32', hex: '#00FF00' },
  { key: 'blue',    label: 'Blue',    ansi: '01;34', hex: '#5C5CFF' },
  { key: 'grey',    label: 'Grey',    ansi: '90',    hex: '#7F7F7F' },
  { key: 'red',     label: 'Red',     ansi: '31',    hex: '#CD0000' },
  { key: 'yellow',  label: 'Yellow',  ansi: '01;33', hex: '#FFFF00' },
  { key: 'magenta', label: 'Magenta', ansi: '35',    hex: '#CD00CD' },
];

// The dynamic-color ladder, lowest tier first. The upper four `key`s are also the stored threshold
// keys (`cfg.thresholds.<key>`); the bottom tier has no threshold of its own - it is everything
// below `green` - so it is named for its role ("calm", plenty left) rather than for a hue that
// would go stale. It used to be bold blue (`01;34`), too dark to read on a dark terminal; xterm 86
// aquamarine is brighter while still reading colder than the green tier above it, so the ladder
// keeps working as a temperature ramp.
export const STATUSLINE_TIERS = [
  { key: 'calm',   label: 'aqua',   ansi: '01;38;5;86',  hex: '#5FFFD7' },
  { key: 'green',  label: 'green',  ansi: '01;32',       hex: '#00FF00' },
  { key: 'yellow', label: 'yellow', ansi: '01;33',       hex: '#FFFF00' },
  { key: 'orange', label: 'orange', ansi: '01;38;5;208', hex: '#FF8700' },
  { key: 'red',    label: 'red',    ansi: '01;31',       hex: '#FF0000' },
];

// Tooltip for any swatch drawn from a record above: names the code the terminal will actually
// print, so the app's color can be compared against the real line instead of trusted.
export function swatchTitle(c) {
  return `${c.label} - ANSI ${c.ansi} (${c.hex})`;
}
