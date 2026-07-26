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

// TWO CAPS, not one. The user-facing rule is the per-SCOPE one; the global one is a machine guard.
//
// EXPORTED because both are enforced by the CALLERS (useTerminalTabs.js), not (only) here: on a
// companion `addTerminalTab`'s body never runs — action() replaces it with an RPC stub — so the
// checks below could not produce the Toast they promise, and the phone's ⌘T/+ silently did nothing.
// The checks stay here as defence in depth for the host and for any future direct caller.

/** How many tabs one GROUP may hold. This is the number a user is meant to have in their head: five
 *  shells is a working set, and wanting a sixth genuinely means closing one. Frontend-only — the PTY
 *  backend is scope-blind and has no idea what a project is. */
export const MAX_TABS_PER_SCOPE = 5

/** The global ceiling, mirroring src-tauri/src/pty.rs's MAX_TABS — kept in sync by comment on this
 *  side and by a unit test on the Rust side (pty.rs's `constant_guards`), because the Rust and JS
 *  build graphs don't share a constant.
 *
 *  DERIVED, not picked: `1 + 3 × MAX_TABS_PER_SCOPE` — the one global tab `closeTerminalTab`
 *  guarantees can never be closed, plus three project groups each at their full per-scope cap. The
 *  binding requirement is that a project group's FIRST tab must never be refused; a ceiling that
 *  cannot seat a third group refuses on an empty group's first tap, for a reason living in a group
 *  the screen is not showing. It is a resource guard, never a budget the user manages — which is why
 *  no tooltip in this app ever states it ahead of time. */
export const MAX_TABS = 16

/** THREE refusals, three problems — deliberately not one parameterised string. Used by BOTH checkers
 *  (this store's own, and useTerminalTabs.js's pre-invoke one) so the phone and the Mac cannot drift
 *  apart. All interpolate their constant; none hardcodes a digit.
 *
 *  The first two are the SAME cause (a group is full) but cannot share wording: the per-scope cap
 *  applies to the global group too, and that group is not a project, so naming one there would
 *  describe something the user is not looking at. Choose with `scopeTabLimitMessage(scope)`. */
export const PROJECT_TAB_LIMIT_MESSAGE = `This project already has ${MAX_TABS_PER_SCOPE} terminal tabs. Close one to open another.`
export const GLOBAL_GROUP_TAB_LIMIT_MESSAGE = `The global group already has ${MAX_TABS_PER_SCOPE} terminal tabs. Close one to open another.`

/** `"in any group"` is load-bearing here and ONLY here: the ceiling is the one refusal whose cause
 *  genuinely lives somewhere the user cannot see, and the TERM column's count badges are where they
 *  can go find it. Saying it on a per-scope refusal would send them hunting in the wrong place.
 *  Named for the CEILING, not for the global group — the two limits are different numbers with
 *  different causes, and a name that blurs them is how a future edit picks the wrong one. */
export const CEILING_TAB_LIMIT_MESSAGE = `All ${MAX_TABS} terminal tabs are in use. Close one in any group first.`

/** [{ id: number, title: string, projectId: string|null, cwd: string|null }] */
export const terminalTabs = ref([])

/** PER-SCREEN — which tab THIS screen is looking at, exactly like logStore.activeLogProjectId.
 *  Listed in services/mirror.js's PER_SCREEN_KEYS so it is never mirrored: which tab a screen has
 *  focused is that screen's own navigation, not session state (same reasoning as isLogExpanded). */
export const activeTerminalTabId = ref(0)

export const GLOBAL_SCOPE = 'global'

/** Which "group is full" wording a scope gets. Not a builder — it picks between two fixed strings,
 *  because the difference is what the group IS, not a number to substitute. */
export function scopeTabLimitMessage(scope) {
  return scope === GLOBAL_SCOPE ? GLOBAL_GROUP_TAB_LIMIT_MESSAGE : PROJECT_TAB_LIMIT_MESSAGE
}

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
  // SCOPE FIRST, THEN GLOBAL — the same order useTerminalTabs.js's capReached() applies, so the two
  // checkers can never name different reasons for the same refusal. A user sitting in a 1-tab group
  // who hits the GLOBAL ceiling must be told about the other groups, not told their group is full.
  const scope = projectId || GLOBAL_SCOPE
  if (terminalTabs.value.filter((t) => (t.projectId || GLOBAL_SCOPE) === scope).length >= MAX_TABS_PER_SCOPE) {
    Toast.fire({ icon: 'error', title: scopeTabLimitMessage(scope) })
    return null
  }
  if (terminalTabs.value.length >= MAX_TABS) {
    Toast.fire({ icon: 'error', title: CEILING_TAB_LIMIT_MESSAGE })
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
