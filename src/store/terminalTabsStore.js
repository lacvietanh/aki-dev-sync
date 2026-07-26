// Terminal tab LIST — shared session state (docs/arch/terminal-stack.md, WP-C). Unlike `tabAlive` (composables/useTerminalTabs.js), which each screen derives locally off
// its own PTY event stream, WHICH TABS EXIST is genuinely shared: opening a tab on the phone must
// show up on the Mac's strip and vice versa. So this is a `src/store/*.js` ref — `services/mirror.js`
// auto-discovers and mirrors it like every other shared ref (SSOT-1).
//
// NO `alive` FIELD HERE ON PURPOSE. Liveness is host PTY state that travels on the pty_output /
// pty_exit channel (usePtyTerminal.js's tri-state `alive`, aggregated per-screen into
// `useTerminalTabs.js`'s `tabAlive`), not session data — mirroring it here would be a second,
// competing source of truth for the same fact the PTY events already carry with lower latency.
import { ref } from 'vue'
import { action } from '../services/action'
import { invoke } from '../utils/tauri'
import { Toast } from './projectStore'

// Mirrors src-tauri/src/pty.rs's MAX_TABS — kept in sync by comment, not by a shared constant
// (the Rust and JS build graphs don't share one).
//
// EXPORTED because this is enforced by the CALLERS (useTerminalTabs.js), not (only) here: on a
// companion `addTerminalTab`'s body never runs — action() replaces it with an RPC stub — so the
// check below could not produce the Toast it promises, and the phone's ⌘T/+ silently did nothing.
// The check stays here as defence in depth for the host and for any future direct caller.
export const MAX_TABS = 8

/** The one wording for "you cannot open another tab", used by BOTH checkers (this store's own, and
 *  useTerminalTabs.js's pre-invoke one). The cap is GLOBAL across all scopes — it mirrors the Rust
 *  cap, which knows nothing about groups — so "in any group" is what tells a user sitting in a
 *  1-tab project group why the + refuses. */
export const TAB_LIMIT_MESSAGE = `Terminal tab limit reached (${MAX_TABS}) — close a tab in any group first`

/** [{ id: number, title: string, projectId: string|null, cwd: string|null }] */
export const terminalTabs = ref([])

/** PER-SCREEN — which tab THIS screen is looking at, exactly like logStore.activeLogProjectId.
 *  Listed in services/mirror.js's PER_SCREEN_KEYS so it is never mirrored: which tab a screen has
 *  focused is that screen's own navigation, not session state (same reasoning as isLogExpanded). */
export const activeTerminalTabId = ref(0)

export const GLOBAL_SCOPE = 'global'

/** PER-SCREEN — which tab GROUP this screen is looking at ('global' | projectId). Same class of
 *  state as activeTerminalTabId: navigation, not session data. Listed in services/mirror.js's
 *  PER_SCREEN_KEYS so a phone switching to a project's terminal group never yanks the Mac's. */
export const activeTerminalScope = ref(GLOBAL_SCOPE)

function nextTabId() {
  return terminalTabs.value.length === 0 ? 0 : 1 + Math.max(...terminalTabs.value.map((t) => t.id))
}

/** Allocates a new tab id HOST-SIDE (action() always runs the real fn on the host, so two screens
 *  adding a tab "at the same time" can never collide — the host's array is the only place an id is
 *  ever picked). Returns the new tab object on the host; a companion's action stub returns
 *  `undefined`, which is why the caller (useTerminalTabs.js's openScopeTerminal) routes the
 *  companion case through its scope-keyed pending claim instead of this return value. */
export const addTerminalTab = action('terminalTabsStore.addTerminalTab', ({ title, projectId = null, cwd = null } = {}) => {
  if (terminalTabs.value.length >= MAX_TABS) {
    Toast.fire({ icon: 'error', title: TAB_LIMIT_MESSAGE })
    return null
  }
  const tab = { id: nextTabId(), title: title || 'Shell', projectId, cwd }
  terminalTabs.value = [...terminalTabs.value, tab]
  return tab
})

/** Closes ONE tab — named by its scope (Regression Guard - Multi-entity State, CLAUDE.md). Splices
 *  EXACTLY one entry. The floor is scope-aware, not "never below one tab" globally: the GLOBAL
 *  group (projectId == null) must always keep at least one tab (the dock's floor — initTerminalTabs
 *  seeds it and adoptTabs feeds it); a PROJECT group may go to zero — the group simply stops
 *  existing until its terminal button is clicked again. Also tells the host PTY to forget that
 *  tab's session+scrollback (`pty_close_tab` REQUIRES its tab_id argument on purpose — see
 *  usePtyTerminal.js's `close()` doc comment). */
export const closeTerminalTab = action('terminalTabsStore.closeTerminalTab', (id) => {
  const list = terminalTabs.value
  const tab = list.find((t) => t.id === id)
  if (!tab) return
  const isGlobal = !tab.projectId
  if (isGlobal && list.filter((t) => !t.projectId).length <= 1) return
  const idx = list.findIndex((t) => t.id === id)
  terminalTabs.value = [...list.slice(0, idx), ...list.slice(idx + 1)]
  // The list removal stays OPTIMISTIC (the chip disappears on the click, which is what a close
  // should feel like), but a failed invoke leaves a live shell with no chip to reach it — an orphan
  // the user can now neither see nor kill. That has to be said out loud rather than logged.
  invoke('pty_close_tab', { tabId: id }).catch((e) => {
    console.error('[terminalTabsStore] pty_close_tab failed', e)
    Toast.fire({ icon: 'error', title: 'Tab closed, but its shell may still be running on the Mac' })
  })
})

/** HOST BOOT ONLY: seed the tab list from `pty_list_tabs()` so a frontend reload re-adopts shells
 *  the backend kept alive, titled `Shell {id}`. Never called from a companion gesture — there is
 *  nothing for a companion to "adopt", it always gets the tab list through the mirror instead.
 *  Adopted tabs get `projectId: null`, so they land in the GLOBAL scope — exactly right, since the
 *  backend cannot tell us which project a re-adopted shell belonged to. */
export const adoptTabs = action('terminalTabsStore.adoptTabs', (list) => {
  if (!Array.isArray(list) || list.length === 0) return
  terminalTabs.value = list.map((t) => ({ id: t.id, title: `Shell ${t.id}`, projectId: null, cwd: null }))
})
