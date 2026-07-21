import { ref } from 'vue';

const STORAGE_KEY = 'aki-usage-tier-count';
const saved = parseInt(localStorage.getItem(STORAGE_KEY) || '1', 10);

export const tierCount = ref(isNaN(saved) || saved < 1 ? 1 : saved);

export function setTierCount(count) {
  tierCount.value = count;
  localStorage.setItem(STORAGE_KEY, String(count));
}
