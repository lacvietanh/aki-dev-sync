// Bottom-dock GEOMETRY: how tall the dock is, and whether it is maximised.
//
// Separate file from useTerminalPanel.js on purpose (CLAUDE.md's name-by-actual-scope rule): that
// module owns the TERMINAL STACK's collapse state, while the height below governs the whole dock —
// terminal stack and log stack together. Folding it in there would give a "terminal panel" module a
// lever over the log panel, which is precisely the vague-blast-radius shape that rule exists to
// prevent.
//
// PER-SCREEN, NOT MIRRORED — same reasoning as terminalStackCollapsed's file header: this lives in
// a composable rather than src/store/, so services/mirror.js never discovers it. A phone dragging
// its dock taller must not resize the Mac's window layout.
import { computed, ref, watch } from 'vue'
import { isLogExpanded } from '../store/logStore'
import { terminalStackCollapsed } from './useTerminalPanel'

const STORAGE_KEY = 'aki-dock-height-pct'

/** The drag range, as a percentage of window height. The floor keeps the header + one row of the
 *  terminal visible (below it the dock is unreadable and the collapse chevron is the right gesture
 *  instead); the ceiling keeps the project table from disappearing entirely — MAXIMIZE is the
 *  explicit, reversible way to go further. */
const MIN_PCT = 15
const MAX_PCT = 85
const DEFAULT_PCT = 40

/** Maximised height. Leaves exactly the app header (`.top-header`, one `--titlebar-h` row) on
 *  screen — VS Code's ⌃` maximise, whose whole point is that you can still get back out. */
const MAXIMIZED_CSS = 'calc(var(--vvh, 100vh) - var(--titlebar-h))'

function clampPct(v) {
  if (!Number.isFinite(v)) return DEFAULT_PCT
  return Math.min(MAX_PCT, Math.max(MIN_PCT, v))
}

function load() {
  const raw = localStorage.getItem(STORAGE_KEY)
  return raw === null ? DEFAULT_PCT : clampPct(parseFloat(raw))
}

export const dockHeightPct = ref(load())

/** MAXIMIZE is deliberately NOT persisted, unlike the height. A dock restored maximised on the next
 *  launch hides the project table before the user has done anything, and the app would look broken
 *  rather than configured. The dragged height is a preference; maximise is a momentary mode. */
export const dockMaximized = ref(false)

/** True while a splitter drag is in flight — AppConsole.vue uses it to suppress the dock's own
 *  height transition, which would otherwise make the drag lag a few frames behind the pointer. */
export const dockDragging = ref(false)

watch(dockHeightPct, (v) => {
  try {
    localStorage.setItem(STORAGE_KEY, String(v))
  } catch {
    // Quota/private-mode failure: the height still applies for this session.
  }
})

/** Both stacks collapsed: the dock is two header rows sized by their own content, and CSS
 *  (`.is-all-collapsed { height: auto }`) owns its height. Every height gesture is inert in this
 *  state, so this is what the splitter and the MAXIMIZE button hide themselves on — an affordance
 *  that silently does nothing is a flow break (METHOD-flow-audit), not a harmless extra.
 *
 *  Defined here, not recomputed per component: the geometry rule and the controls that obey it must
 *  read the same condition or they will drift apart. */
export const dockAllCollapsed = computed(
  () => terminalStackCollapsed.value && !isLogExpanded.value
)

// Collapsing everything also LEAVES maximised mode, because the button that toggles it hides itself
// in that state. Left set, it would silently re-apply on the next expand — the user would get a
// full-height dock they never asked for and cannot see they enabled. A momentary mode must not
// outlive the reach of its own control.
watch(dockAllCollapsed, (allCollapsed) => {
  if (allCollapsed) dockMaximized.value = false
})

/** What `.dashboard-bottom`'s `height` should be right now. */
export const dockHeightCss = computed(() =>
  dockMaximized.value
    ? MAXIMIZED_CSS
    : `calc(var(--vvh, 100vh) * ${dockHeightPct.value} / 100)`
)

/** Drag handler input: the pointer's Y in client coordinates. Converted here (not in the component)
 *  so the clamp and the percentage conversion have exactly one home. Dragging always leaves
 *  maximised mode — the pointer is now the authority on the height, and silently ignoring the drag
 *  because a mode flag was set is the kind of dead gesture METHOD-flow-audit calls a flow break. */
export function setDockHeightFromPointer(clientY) {
  const h = window.innerHeight || 1
  dockMaximized.value = false
  dockHeightPct.value = clampPct(((h - clientY) / h) * 100)
}

/** Double-click-the-splitter reset. Lives here so `DEFAULT_PCT` has one definition — a component
 *  re-declaring the number would drift from the one `load()` falls back to. */
export function resetDockHeight() {
  dockMaximized.value = false
  dockHeightPct.value = DEFAULT_PCT
}

export function toggleDockMaximized() {
  dockMaximized.value = !dockMaximized.value
}
