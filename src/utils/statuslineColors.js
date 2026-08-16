// SOURCE OF TRUTH for statusline color vocabulary (field picker and dynamic color ladder).
// Mirrors in src-tauri/src/statusline.rs and src-tauri/src/statusline-unified.sh.
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

// Dynamic-color ladder: lowest tier first; 'calm' (xterm 86 aqua) is below green threshold.
export const STATUSLINE_TIERS = [
  { key: 'calm',   label: 'aqua',   ansi: '01;38;5;86',  hex: '#5FFFD7' },
  { key: 'green',  label: 'green',  ansi: '01;32',       hex: '#00FF00' },
  { key: 'yellow', label: 'yellow', ansi: '01;33',       hex: '#FFFF00' },
  { key: 'orange', label: 'orange', ansi: '01;38;5;208', hex: '#FF8700' },
  { key: 'red',    label: 'red',    ansi: '01;31',       hex: '#FF0000' },
];

// Tooltip for any swatch drawn from a record above: names the code the terminal will actually print, so the app's color can be compared against the real line instead of trusted.
export function swatchTitle(c) {
  return `${c.label} - ANSI ${c.ansi} (${c.hex})`;
}
