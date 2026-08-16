// Terminal chrome visibility — per-device preference layered over a role default (docs/plan/done/terminal-chrome-settings.md §§4-6). Composable, not src/store/, for the same reason useTerminalFont.js and useDockLayout.js live here: which chrome a screen shows is a fact about the screen you are looking at, not the project, so services/mirror.js must never discover this key.
//
// CONTROLS is the single list every export below derives from (SSoT — the same control must never be named a second time in the menu markup or the storage layer): capability × preference composition (docs/arch/terminal-stack.md § "The capability pattern") reads role-availability off CONTROLS, never `isHost` at the call site — TerminalStack.vue and TerminalView.vue import `chromeVisible` only.
import { computed, reactive, ref } from 'vue'
import { isHost } from '../services/bridge'
import { externalTerminalsSupported } from './useExternalTerminals'
import { rightDockActive } from './useRightDockLayout'

const STORAGE_KEY = 'aki-terminal-chrome'
const SEEN_KEY = 'aki-terminal-chrome-seen'

const CONTROLS = [
  {
    key: 'compose',
    label: 'Command bar',
    title: 'The text box under the terminal. Type a whole line, including Vietnamese, then press Enter.',
    hostAvailable: true,
    companionAvailable: true,
    hostDefault: true, // §6.2: the only working Vietnamese-composing-IME path, so it ships on by default rather than behind a menu the user has never opened.
    companionDefault: true,
  },
  {
    key: 'keyRow',
    label: 'Key row',
    title: 'Esc, Tab, Shift, Ctrl, arrows and Enter for a screen with no physical keyboard.',
    hostAvailable: false,
    companionAvailable: true,
    hostDefault: false,
    companionDefault: true,
  },
  {
    key: 'textSize',
    label: 'Text size buttons',
    title: 'Smaller, reset and larger text, next to the key row.',
    hostAvailable: false,
    companionAvailable: true,
    hostDefault: false,
    companionDefault: true,
  },
  {
    key: 'tabStrip',
    label: 'Tab strip',
    title: 'The row of terminal tabs and the + button.',
    hostAvailable: true,
    companionAvailable: true,
    hostDefault: true,
    companionDefault: true,
    companionLocked: true, // S1: a companion with no tab strip has no way to open, close or switch tabs.
    lockedTitle: 'The tab strip is the only way to open, close and switch tabs on this screen.',
  },
  {
    key: 'groupName',
    label: 'Group name',
    title: 'The project icon and name at the left of this header.',
    hostAvailable: true,
    companionAvailable: true,
    hostDefault: true,
    companionDefault: true,
  },
  {
    key: 'externalTerminals',
    label: 'External terminals',
    title: 'Which external Terminal.app sessions are running.',
    // Capability sync (§7): availability tracks externalTerminalsSupported without call-site checks.
    hostAvailable: externalTerminalsSupported,
    companionAvailable: false,
    hostDefault: false,
    companionDefault: false,
  },
  {
    key: 'maximize',
    label: 'Maximize button',
    title: 'Fill the window with the dock, and restore it.',
    hostAvailable: true,
    companionAvailable: true,
    hostDefault: true,
    companionDefault: true,
  },
]

function load() {
  try {
    return JSON.parse(localStorage.getItem(STORAGE_KEY) || '{}')
  } catch {
    return {}
  }
}

// Sparse storage (§5): untouched controls fall back to role defaults, avoiding migrations when new controls are added.
const stored = reactive(load())

function persist() {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(stored))
  } catch {
    // Private-mode / quota failure: preference still applies for this session (useTerminalFont.js pattern).
  }
}

function isAvailable(control) {
  if (control.key === 'maximize' && rightDockActive.value) return false
  return isHost ? control.hostAvailable : control.companionAvailable
}

function isLocked(control) {
  return !isHost && !!control.companionLocked
}

function roleDefault(control) {
  return isHost ? control.hostDefault : control.companionDefault
}

function preferenceFor(control) {
  if (isLocked(control)) return true
  const v = stored[control.key]
  return typeof v === 'boolean' ? v : roleDefault(control)
}

/** visible = capability(control, thisScreen) && preference[control] (§4): preference never widens capability. */
export const chromeVisible = computed(() => {
  const out = {}
  for (const control of CONTROLS) out[control.key] = isAvailable(control) && preferenceFor(control)
  return out
})

/** Menu rows filtered to device capability in inventory order (§8.3; §4 absent, not disabled-with-reason rule). */
export const chromeMenuRows = computed(() =>
  CONTROLS.filter(isAvailable).map((control) => ({
    key: control.key,
    label: control.label,
    title: isLocked(control) ? control.lockedTitle : control.title,
    locked: isLocked(control),
    checked: preferenceFor(control),
  }))
)

export function setChromePreference(key, value) {
  const control = CONTROLS.find((c) => c.key === key)
  if (!control || isLocked(control)) return
  stored[key] = value
  persist()
}

/** "Show all" (§5): touches only available controls, preserving multi-entity regression safety (CLAUDE.md). */
export const showAllVisible = computed(() =>
  CONTROLS.some((control) => isAvailable(control) && !isLocked(control) && !preferenceFor(control))
)

export function showAllChrome() {
  for (const control of CONTROLS) {
    if (isAvailable(control) && !isLocked(control)) stored[control.key] = true
  }
  persist()
}

function loadSeen() {
  try {
    return localStorage.getItem(SEEN_KEY) === '1'
  } catch {
    return true
  }
}

const chromeMenuSeen = ref(loadSeen())

/** §6.3: tints trigger until opened, providing a recognition cue when all controls are hidden. */
export const chromeMenuArmed = computed(
  () => !chromeMenuSeen.value && CONTROLS.every((control) => !isAvailable(control) || !preferenceFor(control))
)

export function markChromeMenuSeen() {
  if (chromeMenuSeen.value) return
  chromeMenuSeen.value = true
  try {
    localStorage.setItem(SEEN_KEY, '1')
  } catch {
    // Session-only tint state is an acceptable fallback (delete-safe per §6.3).
  }
}
