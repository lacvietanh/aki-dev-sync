import { ref } from 'vue';
import { action } from '../services/action';

const STORAGE_KEY = 'aki-usage-tier-count';
const saved = parseInt(localStorage.getItem(STORAGE_KEY) || '1', 10);

export const tierCount = ref(isNaN(saved) || saved < 1 ? 1 : saved);

// `tierCount` already mirrors host→companion (it is a store ref). Wrapping the setter as an action
// closes the other direction: a companion clicking Tier 1/2 runs it on the host, which mirrors the
// new count back. On the host action(fn)===fn. localStorage is the host's (the setting is the Mac's).
export const setTierCount = action('usageTierStore.setTierCount', (count) => {
  tierCount.value = count;
  localStorage.setItem(STORAGE_KEY, String(count));
});
