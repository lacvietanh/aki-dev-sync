// Bottom-dock GEOMETRY: each stack's length, the dock height that sums them, and maximise.
//
// THE SUM. The dock's height is the sum of the two stacks' lengths, never a stored total they divide. The inverse — one dock height and two `flex: 1 1 0` stacks — is what coupled two panels the UI presents as independent: the total did not depend on which stacks were expanded, so collapsing one handed its freed space to the sibling instead of shrinking the dock.
//
// Separate file from useTerminalPanel.js (CLAUDE.md's name-by-actual-scope rule): that module owns the terminal stack's collapse state, this one governs both stacks' geometry.
//
// PER-SCREEN, NOT MIRRORED: a composable, not src/store/, so services/mirror.js never discovers it — a phone dragging its dock must not resize the Mac's layout.
import { computed, ref, watch } from 'vue'
import { isLogExpanded } from '../store/logStore'
import { terminalStackCollapsed } from './useTerminalPanel'

const STORAGE_KEY = 'aki-dock-stack-pct'

/** Per-stack drag range, as a percentage of window height. The ceiling is on the SUM, not on either stack: what it protects is the project table above the dock, which does not care which of the two is tall. */
const MIN_PCT = 10
const MAX_TOTAL_PCT = 85
const DEFAULT_PCT = 20

const MAXIMIZED_CSS = 'calc(var(--vvh, 100vh) - var(--titlebar-h))'

/** Where each stack's collapse state comes from, and what it measures while collapsed — the log stack keeps one peek line above its header. CSS `calc()` over the stylesheet's own tokens, so the sum below never measures a pixel. */
const STACKS = {
  terminal: {
    collapsed: terminalStackCollapsed,
    collapsedCss: 'var(--dock-header-h)',
  },
  log: {
    collapsed: computed(() => !isLogExpanded.value),
    collapsedCss: 'calc(var(--dock-header-h) + var(--dock-peek-h))',
  },
}

/** Ceiling is whatever MAX_TOTAL_PCT leaves after the sibling, which bounds the sum without ever writing the sibling. A collapsed sibling counts as zero rather than re-deriving its header token in JS. */
function clampPct(key, v) {
  if (!Number.isFinite(v)) return DEFAULT_PCT
  const sibling = key === 'terminal' ? 'log' : 'terminal'
  const ceiling = STACKS[sibling].collapsed.value ? MAX_TOTAL_PCT : MAX_TOTAL_PCT - stackPct.value[sibling]
  return Math.min(Math.max(MIN_PCT, ceiling), Math.max(MIN_PCT, v))
}

const clampStored = (v) =>
  Number.isFinite(v) ? Math.min(MAX_TOTAL_PCT - MIN_PCT, Math.max(MIN_PCT, v)) : DEFAULT_PCT

function load() {
  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE_KEY))
    return { terminal: clampStored(saved.terminal), log: clampStored(saved.log) }
  } catch {
    return { terminal: DEFAULT_PCT, log: DEFAULT_PCT }
  }
}

/** Written only through the functions below, each copying the sibling through untouched (CLAUDE.md's multi-entity guard). */
const stackPct = ref(load())

/** MAXIMIZE is deliberately NOT persisted, unlike the lengths: a dock restored maximised on launch hides the project table and reads as broken rather than configured. */
export const dockMaximized = ref(false)

export const dockDragging = ref(false)

/** True during a collapse/expand/maximize transition, set from AppConsole's transition events; the PTY-owning terminal reads it (prop) to fit once at the end instead of on every ResizeObserver frame. Drag (transition:none) fires no such events, so it keeps its live per-frame fit. */
export const dockAnimating = ref(false)

watch(stackPct, (v) => {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(v))
  } catch {
    // Quota/private-mode failure: the lengths still apply for this session.
  }
})

/** What the MAXIMIZE button hides itself on — an affordance that silently does nothing is a flow break (METHOD-flow-audit). Each splitter needs no such condition: its stack renders it only while expanded. */
export const dockAllCollapsed = computed(
  () => STACKS.terminal.collapsed.value && STACKS.log.collapsed.value
)

// Collapsing everything leaves maximised mode too: the button that toggles it hides itself there, so a flag left set would silently re-apply on the next expand.
watch(dockAllCollapsed, (allCollapsed) => {
  if (allCollapsed) dockMaximized.value = false
})

function stackLengthCss(key) {
  return STACKS[key].collapsed.value
    ? STACKS[key].collapsedCss
    : `calc(var(--vvh, 100vh) * ${stackPct.value[key]} / 100)`
}

/** Both stacks plus the 1px rule between them (`.dock-stack ~ .dock-stack`, main.css). A `calc()` string and never `height: auto`, which computes the same number but cannot be transitioned to or from — an auto dock would snap on every collapse. */
export const dockHeightCss = computed(() =>
  dockMaximized.value
    ? MAXIMIZED_CSS
    : `calc(${stackLengthCss('terminal')} + ${stackLengthCss('log')} + 1px)`
)

/** Nothing grows: each stack is exactly its own length, which is what makes a sibling's collapse arithmetically invisible to it. Maximise suspends that — there the expanded stacks share a height the user asked for explicitly. Numeric endpoints only (`0px`, never the `flex` shorthand's implicit `auto`), or the transition cannot interpolate. */
export function dockStackFlex(key) {
  if (dockMaximized.value && !STACKS[key].collapsed.value) {
    return { flexGrow: 1, flexShrink: 1, flexBasis: '0px' }
  }
  return { flexGrow: 0, flexShrink: 0, flexBasis: stackLengthCss(key) }
}

/** `bottomY` is the stack's own floor: fixed for the drag's duration, since the dock is anchored to the bottom of the window and every stack below keeps its length. Dragging always leaves maximised mode — the pointer is now the authority on the height. */
export function setStackHeightFromPointer(key, clientY, bottomY) {
  const h = window.innerHeight || 1
  dockMaximized.value = false
  stackPct.value = { ...stackPct.value, [key]: clampPct(key, ((bottomY - clientY) / h) * 100) }
}

export function resetStackHeight(key) {
  dockMaximized.value = false
  stackPct.value = { ...stackPct.value, [key]: DEFAULT_PCT }
}

export function toggleDockMaximized() {
  dockMaximized.value = !dockMaximized.value
}
