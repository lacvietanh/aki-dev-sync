// Single source of truth for the statusline customizer's color palette.
// Consumed by ClaudeSettingModal.vue (the color picker for each statusline field) and by
// AppHeader.vue (the dropdown menu, which paints its own "Statusline Customizer" label with
// this exact palette so the row demonstrates the feature instead of describing it in words).
// Keep this the only place these hex values are written - anything else should import from here.
export const STATUSLINE_COLORS = [
  { key: 'white',   label: 'White',   hex: '#e2e8f0' },
  { key: 'cyan',    label: 'Cyan',    hex: '#22d3ee' },
  { key: 'green',   label: 'Green',   hex: '#34d399' },
  { key: 'blue',    label: 'Blue',    hex: '#60a5fa' },
  { key: 'grey',    label: 'Grey',    hex: '#94a3b8' },
  { key: 'red',     label: 'Red',     hex: '#f87171' },
  { key: 'yellow',  label: 'Yellow',  hex: '#fbbf24' },
  { key: 'magenta', label: 'Magenta', hex: '#e879f9' },
];
