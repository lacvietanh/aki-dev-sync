import { reactive } from 'vue'

// Tracks which "view" (local-ag / local-cc / remote) each usage slot (A/B) is currently
// showing, so the two slots can refuse to both display the same source at once — with only
// one instance of each source, showing it twice would just waste screen space that could be
// showing the other one instead. Native-pattern lock: the would-be-duplicate option is simply
// disabled in the other slot, no auto-swap cascading.
export const slotViews = reactive({ A: null, B: null })
