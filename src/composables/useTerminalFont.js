// Terminal font zoom — the VS Code affordance (⌘+ / ⌘- / ⌘0), as a SCALE rather than a pixel size.
//
// WHY A SCALE AND NOT A FONT SIZE: the two surfaces still disagree about what the GRID means — on the Mac the size is authoritative and cols/rows follow it (T-4), while on a phone the host alone owns cols/rows and only the rendered SIZE is local.
// But both now compute that size the same way: `TerminalView.vue`'s `doFit` multiplies the same `BASE_FONT_SIZE` by this device's own scale (1.22.0 — the phone no longer measures its container and scales to fill it, see "Terminal font size" in docs/feat/in-app-terminal.md for why that was removed).
// A scale, not a stored pixel size, is what lets ⌘0/reset mean the same thing on both surfaces.
//
// PER-DEVICE, NOT MIRRORED STATE: this lives in localStorage and is deliberately NOT part of the host→companion state mirror.
// How big text should be is a fact about the screen you are looking at, not about the project — a phone held at arm's length and a 27" Mac have no business agreeing on it, and mirroring it would make each screen fight the other's preference.
import { ref, watch } from 'vue'

const STORAGE_KEY = 'aki-terminal-font-scale'

// 0.1 steps rather than VS Code's ±1px: the phone's effective size is a measured float, so a pixel
// delta has no fixed meaning there. Two taps ≈ one comfortable notch on both surfaces.
const STEP = 0.1
const MIN_SCALE = 0.5
const MAX_SCALE = 3

function clamp(v) {
  if (!Number.isFinite(v)) return 1
  // Rounded to 2dp so repeated ±0.1 cannot accumulate float dust (0.7999999999999999) into the
  // stored value and, from there, into every font-size calculation downstream.
  return Math.round(Math.min(MAX_SCALE, Math.max(MIN_SCALE, v)) * 100) / 100
}

function load() {
  const raw = localStorage.getItem(STORAGE_KEY)
  return raw === null ? 1 : clamp(parseFloat(raw))
}

export const terminalFontScale = ref(load())

watch(terminalFontScale, (v) => {
  try {
    localStorage.setItem(STORAGE_KEY, String(v))
  } catch {
    // Private-mode / quota failures must not break zooming — the scale still applies this session.
  }
})

export function zoomInTerminalFont() {
  terminalFontScale.value = clamp(terminalFontScale.value + STEP)
}

export function zoomOutTerminalFont() {
  terminalFontScale.value = clamp(terminalFontScale.value - STEP)
}

export function resetTerminalFont() {
  terminalFontScale.value = 1
}

/** True when the scale is off its default — the only thing the RESET button needs to know to say
 *  whether it has anything to undo. */
export function isTerminalFontZoomed() {
  return terminalFontScale.value !== 1
}
