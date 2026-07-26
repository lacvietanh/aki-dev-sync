// Seam P — producer gate (docs/plan/done/remote-control.md §5).
//
// The browser must never *produce* — no polling, no boot sequence, no double-fetching, no
// tripping a rate limit, no starting a second `listen()` on an event stream that doesn't exist
// for it. This is what makes "the companion is dumb" structural rather than a habit: the 5
// producer sites enumerated in §5/§8 R-4 gate through these two helpers instead of calling
// `setInterval`/their boot function directly.
//
// Cosmetic UI-only timers (e.g. a spinner animation tick) are NOT producers and must not be
// routed through here — gating them would just make the companion's own animations freeze.
import { isHost } from '../services/bridge'

/** Host: real setInterval, returns the handle. Companion: no-op, returns null. */
export const hostInterval = isHost ? (fn, ms) => setInterval(fn, ms) : () => null

/** Host: runs `fn` immediately. Companion: no-op — boot-sequence data arrives via the mirror. */
export const onHostBoot = isHost ? (fn) => fn() : () => {}
