// Seam P producer gate (docs/plan/done/remote-control.md §5, §8 R-4): prevents browser companion from polling or starting producer timers.
import { isHost } from '../services/bridge'

/** Host: real setInterval (returns handle). Companion: no-op (returns null). */
export const hostInterval = isHost ? (fn, ms) => setInterval(fn, ms) : () => null

/** Host: runs fn immediately. Companion: no-op (boot data arrives via mirror). */
export const onHostBoot = isHost ? (fn) => fn() : () => {}
