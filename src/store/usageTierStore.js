import { ref } from 'vue';
import { action } from '../services/action';

const STORAGE_KEY = 'aki-usage-tier-count';

// One tier row is always a pair of slots - the layout invariant every consumer here derives from.
export const SLOTS_PER_ROW = 2;

// Ceiling on rows, and the only place the app's slot vocabulary ends. 4 rows = 8 slots (A..H) =
// 4 * 161 + 3 * 10 = 674px of panel, already taller than the narrow window's usual height, so the
// panel scrolls past that. Raise this number and every consumer below follows with no other edit.
export const MAX_TIER_ROWS = 4;
export const MAX_SLOTS = MAX_TIER_ROWS * SLOTS_PER_ROW;

// The header dropdown talks to the user in SLOTS (2/4/6/8) - that is what is on screen - but the
// value stored and mirrored here stays a ROW count, same unit and meaning it had before the
// dropdown existed. So an upgrading install's `aki-usage-tier-count` needs no migration, and these
// two helpers are the only places the two units meet.
export const rowsToSlots = (rows) => rows * SLOTS_PER_ROW;
export const slotsToRows = (slots) => Math.ceil(slots / SLOTS_PER_ROW);

// The row-count options offered in the UI, low to high: [1, 2, .., MAX_TIER_ROWS].
export const TIER_ROW_OPTIONS = Array.from({ length: MAX_TIER_ROWS }, (_, i) => i + 1);

function clampRows(value) {
  const n = parseInt(value, 10);
  if (isNaN(n) || n < 1) return 1;
  return Math.min(n, MAX_TIER_ROWS);
}

export const tierCount = ref(clampRows(localStorage.getItem(STORAGE_KEY) || '1'));

/**
 * Slot id for a flat slot index: 0 -> 'A', 1 -> 'B', ... Row R (0-based) owns indices
 * R*SLOTS_PER_ROW .. R*SLOTS_PER_ROW+SLOTS_PER_ROW-1, so 'A'/'B' are row 1 exactly as they were
 * when the four ids were written out by hand.
 *
 * Past index 25 letters would run off the end of A-Z into punctuation - and these ids are keys in
 * persisted JSON and in `aki-usage-slot-<id>-*` localStorage keys, where a stray '[' is a silent
 * mess rather than an error. The numeric form keeps them readable if MAX_TIER_ROWS ever goes there.
 */
export function slotIdAt(index) {
  return index < 26 ? String.fromCharCode(65 + index) : `S${index}`;
}

/** Inverse of `slotIdAt`. Returns -1 for an id this build does not recognise. */
export function slotIndexOf(slotId) {
  if (typeof slotId !== 'string' || slotId.length === 0) return -1;
  if (slotId.startsWith('S')) {
    const n = parseInt(slotId.slice(1), 10);
    return isNaN(n) ? -1 : n;
  }
  const n = slotId.charCodeAt(0) - 65;
  return n >= 0 && n < 26 ? n : -1;
}

/** Slot ids of row `rowIndex` (0-based), left to right. */
export function rowSlotIds(rowIndex) {
  return Array.from({ length: SLOTS_PER_ROW }, (_, col) => slotIdAt(rowIndex * SLOTS_PER_ROW + col));
}

/** Every slot id this build can address, in row order - the seed list for per-slot state. */
export function allSlotIds() {
  return Array.from({ length: MAX_SLOTS }, (_, i) => slotIdAt(i));
}

// `tierCount` already mirrors host→companion (it is a store ref). Wrapping the setter as an action
// closes the other direction: a companion picking a row count runs it on the host, which mirrors the
// new count back. On the host action(fn)===fn. localStorage is the host's (the setting is the Mac's).
//
// Clamping here (not only in the picker) is what stops a mirrored or hand-edited value above
// MAX_TIER_ROWS from asking the section for rows whose slots have no persisted target.
export const setTierCount = action('usageTierStore.setTierCount', (count) => {
  const rows = clampRows(count);
  tierCount.value = rows;
  localStorage.setItem(STORAGE_KEY, String(rows));
});
