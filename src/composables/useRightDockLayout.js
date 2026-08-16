// Right-dock GEOMETRY: in wide mode (≥ RIGHT_DOCK_BREAKPOINT), AppConsole becomes a right column and dashboard-left is capped at MAIN_VIEW_MAX_WIDTH; LogStack moves into dashboard-left (not AppConsole) so the terminal column is terminal-only.
// useDockLayout.js's height-summed model still governs each stack's internal vertical split unchanged in both modes.
//
// AXIS DECISION LIVES HERE, ONLY HERE (pattern.A8): `rightDockActive` is the single ref every consumer (App.vue, AppConsole.vue) reads — no consumer recomputes the trigger itself.
//
// PER-SCREEN, NOT MIRRORED: a composable, not src/store/, so services/mirror.js never discovers it.
import { ref } from 'vue'

/** Main view cap in right-dock mode. The terminal column takes everything to the right via flex:1 — no fixed width or drag splitter. SSoT: main.css reads --main-view-max-width back from here. */
export const MAIN_VIEW_MAX_WIDTH = 420

/** Right-dock engages at 900px: the main view gets its 420px cap and the terminal fills the rest. Deliberately separate from the 700px narrow-mode breakpoint (different meaning, not the same constant). */
export const RIGHT_DOCK_BREAKPOINT = 900

export const rightDockActive = ref(false)

if (typeof document !== 'undefined') {
  document.documentElement.style.setProperty('--main-view-max-width', `${MAIN_VIEW_MAX_WIDTH}px`)
}

const _ro = new ResizeObserver(([entry]) => {
  const w = entry.contentRect.width
  rightDockActive.value = w >= RIGHT_DOCK_BREAKPOINT
})
_ro.observe(document.documentElement)

