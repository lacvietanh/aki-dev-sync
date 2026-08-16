import { ref } from 'vue';
import { action } from '../services/action';

const STORAGE_KEY = 'aki-usage-tier-count';

// One tier row is always a pair of slots - the layout invariant every consumer here derives from.
export const SLOTS_PER_ROW = 2;

// Row ceiling (4 rows = 8 slots A..H); panel scrolls beyond this height and derived constants scale automatically.
export const MAX_TIER_ROWS = 4;
export const MAX_SLOTS = MAX_TIER_ROWS * SLOTS_PER_ROW;

// Header UI displays slots (2/4/6/8) while storage preserves row counts for backwards compatibility.
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

/** 0-based index to slot id ('A'..'Z', then 'S26'+ to keep storage keys alphanumeric-safe). */
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

// Action forwards companion updates to host; clamp prevents out-of-bounds row requests.
export const setTierCount = action('usageTierStore.setTierCount', (count) => {
  const rows = clampRows(count);
  tierCount.value = rows;
  localStorage.setItem(STORAGE_KEY, String(rows));
});
