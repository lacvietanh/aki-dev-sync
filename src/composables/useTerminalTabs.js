// Per-screen glue around the shared tab LIST (src/store/terminalTabsStore.js). Everything in this
// file is local to the screen that calls it — liveness tracking, "has this tab ever been shown"
// bookkeeping, the companion activation queue, the per-scope last-active memory — none of it
// belongs on the wire (same reasoning as useTerminalPanel.js's terminalStackCollapsed).
//
// Terminal v2 (scoped terminal groups): a SCOPE is a tab group, keyed by `tab.projectId ||
// GLOBAL_SCOPE`. Scope is pure frontend grouping over a field the tab list already carries — see
// docs/arch/terminal-stack.md for the full model.
import { ref, computed, watch } from 'vue'
import {
  terminalTabs,
  activeTerminalTabId,
  activeTerminalScope,
  GLOBAL_SCOPE,
  addTerminalTab,
  closeTerminalTab,
  adoptTabs,
  MAX_TABS,
  MAX_TABS_PER_SCOPE,
  scopeTabLimitMessage,
  CEILING_TAB_LIMIT_MESSAGE,
} from '../store/terminalTabsStore'
import { projects, Toast } from '../store/projectStore'
import { invoke } from '../utils/tauri'
import { expandTerminalStack } from './useTerminalPanel'
import { tabLiveness, startTabLivenessTracking, seedTabLiveness } from './usePtyTerminal'

/** { [tabId]: 'unknown' | true | false } — LOCAL, not a store ref: each screen's PTY event stream
 *  is its own, exactly as documented in usePtyTerminal.js's `alive` doc comment, so this is never
 *  mirrored. Re-exports usePtyTerminal.js's module-level `tabLiveness` tracker (S3 fix) rather than
 *  owning a ref of its own — that tracker is written by listeners registered once at module scope
 *  (`startTabLivenessTracking`, called below) that survive every TerminalView mount/unmount and run
 *  on a companion too, unlike the old per-mounted-view aggregation this replaced. Existing consumers
 *  (TerminalTabStrip.vue, TerminalCell.vue) need no changes — they only ever read this by tabId. */
export const tabAlive = tabLiveness

// Started once at module scope (same reasoning as the `watch(terminalTabs, ...)` below it): every
// screen that imports this file — host or companion — gets exactly one set of liveness listeners.
startTabLivenessTracking()

/** Tabs this screen has ever activated — TerminalStack.vue mounts a TerminalView lazily on first
 *  activation (`v-if="activatedTabs.has(t.id)"`) and then keeps it mounted (`v-show`) so switching
 *  back never re-spawns or loses scrollback-render state. Also keeps a companion from spawning N
 *  xterm instances the instant it joins a session with N tabs already open. */
export const activatedTabs = ref(new Set())

/** `scopeOf(tab) = tab.projectId || GLOBAL_SCOPE` — the aggregate-root key. A scope is a tab group;
 *  a tab's membership in one is entirely determined by its `projectId`, nothing new on the wire. */
function scopeOf(tab) {
  return (tab && tab.projectId) || GLOBAL_SCOPE
}

/** { [scopeKey]: tabId } — the tab each scope was last looking at on THIS screen, so re-entering a
 *  group returns you where you were (VSCode group behaviour). Per-screen: never mirrored. */
const lastTabByScope = ref({})

/** The tab a scope should return to: its remembered tab if that tab still exists, else the most
 *  recently added tab in the scope. `null` when the scope currently has no tabs at all. */
function resolveScopeTab(scope) {
  const inScope = terminalTabs.value.filter((t) => scopeOf(t) === scope)
  const remembered = lastTabByScope.value[scope]
  if (remembered != null && inScope.some((t) => t.id === remembered)) return remembered
  return inScope[inScope.length - 1]?.id ?? null
}

/** Records the active tab for ONE scope. */
function rememberScopeTab(scope, tabId) {
  lastTabByScope.value = { ...lastTabByScope.value, [scope]: tabId }
}

/** Forgets ONE scope's remembered tab — multi-entity guard (CLAUDE.md): never clears the map.
 *  Called only when the scope's last tab closes. */
function forgetScopeTab(scope) {
  if (!(scope in lastTabByScope.value)) return
  const next = { ...lastTabByScope.value }
  delete next[scope]
  lastTabByScope.value = next
}

/** Queue-of-one: opening a tab on a COMPANION cannot read back the tab `addTerminalTab` created
 *  (the action stub returns `undefined`), so the caller names the SCOPE it is waiting for instead
 *  and the watcher below claims the newest tab that shows up mirrored from the host in that scope.
 *
 *  WITH AN EXPIRY, because there is no failure path: `capReached()` is checked here against the
 *  possibly-stale MIRRORED tab list, so the host can still refuse the add (terminalTabsStore.js's
 *  own cap check) after this claim is set — and when it does, the Toast fires on the MAC and no tab
 *  ever arrives. Without the TTL the claim would sit set indefinitely and the NEXT tab opened in
 *  that scope, possibly hours later from the Mac, would be claimed by it and yank the phone's focus.
 *  A claim is only meaningful for as long as a round-trip takes.
 *
 *  (An awaitable action() would remove the guesswork entirely, but that is a transport redesign —
 *  see the report accompanying this change.) */
const pendingActivateScope = ref(null)
const PENDING_CLAIM_TTL_MS = 15_000
let pendingClaimAt = 0
let pendingClaimTimer = null

/** THE EXPIRY IS A REAL TIMER, and it has to be. The claim used to expire lazily inside
 *  `pendingClaimLive()`, which is only ever reached from the `watch(terminalTabs, …)` below — i.e.
 *  only when a tab actually arrives. In the one case the expiry exists for (the host refused the add,
 *  so NO tab ever arrives and the list never changes) that check is never reached at all, and a
 *  timer is the only thing that can speak. The lazy check stays as a cheap guard for the case where
 *  some unrelated tab change beats the timer to it.
 *
 *  Hedged wording on purpose: the refusal reason genuinely never crossed the wire (the host's Toast
 *  fired on the Mac), so this states a guess as a guess. It is the backstop for the stale-mirror
 *  race only — `capReached()` is what refuses in the ordinary case. */
function setPendingClaim(scope) {
  clearPendingClaim()
  pendingActivateScope.value = scope
  pendingClaimAt = Date.now()
  pendingClaimTimer = setTimeout(() => {
    pendingClaimTimer = null
    if (pendingActivateScope.value === null) return
    clearPendingClaim()
    Toast.fire({ icon: 'error', title: 'No terminal tab opened on the Mac. It may have reached a terminal limit.' })
  }, PENDING_CLAIM_TTL_MS)
}

function clearPendingClaim() {
  if (pendingClaimTimer) clearTimeout(pendingClaimTimer)
  pendingClaimTimer = null
  pendingActivateScope.value = null
  pendingClaimAt = 0
}

/** Is there a claim that is still worth honouring? */
function pendingClaimLive() {
  if (pendingActivateScope.value === null) return false
  if (Date.now() - pendingClaimAt > PENDING_CLAIM_TTL_MS) {
    clearPendingClaim()
    return false
  }
  return true
}

/** BOTH tab caps, checked BEFORE the action is invoked — on a companion the store's own checks never
 *  run (action() replaces the body with an RPC stub), so this is the only place the phone can be told
 *  why nothing opened. BOTH conditions have to be replicated here, not just the per-scope one: leave
 *  the global ceiling out and every ceiling refusal becomes host-only, and the phone's tap does
 *  nothing at all for 15 seconds and then nothing.
 *
 *  Same order as the store applies them (scope, then global) so the two checkers can never name
 *  different reasons for the same refusal. Returning true must also stop the caller from queueing a
 *  pending activation — there will be no tab to claim. The messages themselves are the store's, picked
 *  the same way here as there, so one wording can never reach only one of the two checkers.
 *
 *  Residual race, not closable here: the mirrored tab list is one round-trip stale, so the host can
 *  still refuse an add this pre-check let through. `pendingClaimLive()`'s expiry is that backstop. */
function capReached(scope) {
  if (terminalTabs.value.filter((t) => scopeOf(t) === scope).length >= MAX_TABS_PER_SCOPE) {
    Toast.fire({ icon: 'error', title: scopeTabLimitMessage(scope) })
    return true
  }
  if (terminalTabs.value.length >= MAX_TABS) {
    Toast.fire({ icon: 'error', title: CEILING_TAB_LIMIT_MESSAGE })
    return true
  }
  return false
}

function markActivated(id) {
  if (id == null) return
  if (!activatedTabs.value.has(id)) {
    // New Set instance: activatedTabs is a Vue ref, and mutating a Set in place does not notify
    // deep watchers unless they are set up with { deep: true } — replacing the value is the
    // reliable form, matching the rest of this codebase's array-replace convention.
    const next = new Set(activatedTabs.value)
    next.add(id)
    activatedTabs.value = next
  }
}

function setActiveTab(id) {
  if (id == null) return
  activeTerminalTabId.value = id
  markActivated(id)
  const tab = terminalTabs.value.find((t) => t.id === id)
  if (tab) {
    activeTerminalScope.value = scopeOf(tab)
    rememberScopeTab(activeTerminalScope.value, id)
  }
}

/** THE one algorithm behind every "show me a terminal for X" gesture — the project TERMINAL button,
 *  the header's global terminal icon, and ⌘T / the strip's `+`. All three used to spell out the same
 *  six steps (switch scope → reuse the scope's tab → cap check → add → activate, or queue the
 *  companion claim); only the scope key and whether reuse is wanted ever differed.
 *
 *  @param {string} scope                     GLOBAL_SCOPE or a project id
 *  @param {object} [opts]
 *  @param {string} [opts.title]              title for a NEWLY created tab
 *  @param {string|null} [opts.cwd]           directory a NEWLY created tab opens in
 *  @param {boolean} [opts.reuse]             reuse the scope's existing tab instead of adding one.
 *         `true` for the entry-point buttons (never `cd` a shell that may be mid-command), `false`
 *         for ⌘T / `+`, whose entire purpose is a new tab.
 *  @param {boolean} [opts.expandStack]       expand the dock stack first — the whole point of a click
 *         on an entry-point button, but wrong for ⌘T, which is only reachable while it is open. */
function openScopeTerminal(scope, { title = 'Shell', cwd = null, reuse = true, expandStack = false } = {}) {
  // Captured before anything moves: the scope switch below happens BEFORE the cap check (it has to —
  // the per-scope check is about the target group), so a refusal would otherwise leave the user
  // standing in the refusing group, which is empty and whose `+` will refuse too, with every other
  // group's tabs still existing but unreachable from where they now are.
  const priorScope = activeTerminalScope.value
  if (expandStack) expandTerminalStack()
  activeTerminalScope.value = scope // switch group BEFORE any claim is queued (companion too)
  if (reuse) {
    const existing = resolveScopeTab(scope)
    if (existing != null) {
      setActiveTab(existing) // NO cd — the shell may be running something long-lived
      return
    }
  }
  // Repeat-tap guard (companion only) — see "Companion add is fire-and-forget" in docs/arch/terminal-stack.md for why this is needed and what it does not cover.
  if (reuse && pendingActivateScope.value === scope && pendingClaimLive()) return
  if (capReached(scope)) {
    activeTerminalScope.value = priorScope // put the screen back where it was; the Toast says why
    return
  }
  const projectId = scope === GLOBAL_SCOPE ? null : scope
  const tab = addTerminalTab({ title, projectId, cwd })
  if (tab) setActiveTab(tab.id)
  else setPendingClaim(scope) // companion: claim it once the mirror delivers it
}

// Watches the shared tab list for arrivals this screen is waiting on (companion paths above). Set
// up once at module scope, not per-call — useTerminalTabs() may be invoked from more than one
// component (dock/TerminalStack.vue, a future consumer), and a duplicate watcher would just double
// the (harmless but wasteful) work.
watch(terminalTabs, (tabs) => {
  if (pendingClaimLive()) {
    // Claim the LAST tab in the list whose scope matches — most-recently-added, not first.
    const matches = tabs.filter((t) => scopeOf(t) === pendingActivateScope.value)
    if (matches.length > 0) {
      clearPendingClaim()
      setActiveTab(matches[matches.length - 1].id) // setActiveTab derives + sets the scope itself
      return
    }
  }
  // Reconcile `activeTerminalTabId` against the mirrored list itself (S2). Two ways it can go
  // stale that neither branch above covers:
  //  1. A COMPANION NEVER RUNS `initTerminalTabs` (host-only, see its own doc comment below) — so
  //     nothing ever calls `setActiveTab` for it on boot/reconnect, and `activeTerminalTabId`
  //     defaults to 0 whether or not tab 0 is the one that actually exists.
  //  2. The HOST closes whatever tab this screen happened to have active (e.g. from a different
  //     screen), leaving this screen's id pointing at a tab that no longer exists in the list.
  // Falls back to the CURRENT scope's tabs first, then the full list if the current scope itself
  // is empty (and in that case resets the scope back to global too).
  if (tabs.length && !tabs.some((t) => t.id === activeTerminalTabId.value)) {
    const inScope = tabs.filter((t) => scopeOf(t) === activeTerminalScope.value)
    if (inScope.length > 0) {
      setActiveTab(inScope[0].id)
    } else {
      activeTerminalScope.value = GLOBAL_SCOPE
      setActiveTab(tabs[0].id)
    }
    return
  }
  // The active id IS present in the list but this screen may never have "activated" it —
  // TerminalStack.vue only mounts a TerminalView once its id is in `activatedTabs`
  // (`v-if="activatedTabs.has(t.id)"`), and a companion that boots straight into the host's
  // existing tab 0 (or reconnects to it) never goes through `setActiveTab`, which is the only other
  // place that marks a tab activated. Without this, such a companion shows an empty stack for a tab
  // that is, in fact, already active.
  if (tabs.some((t) => t.id === activeTerminalTabId.value)) {
    markActivated(activeTerminalTabId.value)
  }
})

export function useTerminalTabs() {
  const tabs = computed(() => terminalTabs.value) // FULL list — mount loop only, see TerminalStack.vue
  const scope = computed(() => activeTerminalScope.value) // read-only view
  const scopedTabs = computed(() => terminalTabs.value.filter((t) => scopeOf(t) === activeTerminalScope.value))
  const scopeProject = computed(() =>
    activeTerminalScope.value === GLOBAL_SCOPE
      ? null
      : projects.value.find((p) => p.id === activeTerminalScope.value) || null
  )
  const activeTabId = computed({
    get: () => activeTerminalTabId.value,
    set: (v) => setActiveTab(v),
  })
  const activeTab = computed(() => tabs.value.find((t) => t.id === activeTabId.value) || null)

  function newTab() {
    const scopeNow = activeTerminalScope.value
    openScopeTerminal(scopeNow, {
      title: scopeProject.value?.name ?? 'Shell',
      cwd: scopeNow === GLOBAL_SCOPE ? null : scopeProject.value?.local_path ?? null,
      reuse: false, // ⌘T / + always means a NEW tab in the current group
    })
  }

  function closeTab(id) {
    const list = scopedTabs.value
    const idx = list.findIndex((t) => t.id === id)
    if (idx === -1) return // not in this scope (shouldn't happen from the strip, but be safe)
    const isGlobal = activeTerminalScope.value === GLOBAL_SCOPE
    if (isGlobal && list.length <= 1) {
      // Never close the last GLOBAL tab (the store enforces this too — the dock must always have
      // one). Say so: ⌘W used to do nothing at all here, with no feedback of any kind.
      Toast.fire({ icon: 'info', title: 'The last global terminal tab stays open' })
      return
    }
    // Pick the fallback BEFORE closing: on a companion the list itself only updates once the
    // mirror echoes the removal back, so this cannot be derived from the post-close array.
    let fallbackId = null
    let fallbackToGlobal = false
    if (activeTabId.value === id) {
      const neighbor = list[idx + 1] || list[idx - 1]
      if (neighbor) {
        fallbackId = neighbor.id
      } else if (!isGlobal) {
        // Last tab of a PROJECT scope closing — the group empties, fall back to global.
        fallbackToGlobal = true
      }
    }
    closeTerminalTab(id)
    if (fallbackId != null) {
      setActiveTab(fallbackId)
    } else if (fallbackToGlobal) {
      forgetScopeTab(activeTerminalScope.value)
      activeTerminalScope.value = GLOBAL_SCOPE
      const target = resolveScopeTab(GLOBAL_SCOPE)
      if (target != null) setActiveTab(target)
    }
  }

  function cycleTab(dir) {
    const list = scopedTabs.value
    if (list.length < 2) return
    const idx = list.findIndex((t) => t.id === activeTabId.value)
    const nextIdx = ((idx === -1 ? 0 : idx) + dir + list.length) % list.length
    setActiveTab(list[nextIdx].id)
  }

  /** A project's TERMINAL button (TerminalCell.vue, and ProjectTable's OPEN popup, consume this):
   *  switch the stack to that project's SCOPE and reuse the group's last-active (or most recent)
   *  tab rather than cd-ing into a shell that may be mid-command, otherwise start a fresh one
   *  already in the project's directory. */
  function openProjectTerminal(project) {
    if (!project) return
    openScopeTerminal(project.id, { title: project.name, cwd: project.local_path, expandStack: true })
  }

  /** Header terminal-icon entry point — the GLOBAL_SCOPE mirror of openProjectTerminal. */
  function openGlobalTerminal() {
    openScopeTerminal(GLOBAL_SCOPE, { title: 'Shell', expandStack: true })
  }

  return {
    tabs,            // FULL list — mount loop only
    scopedTabs,      // the strip, cycling, and the close-fallback use this
    scope, scopeProject,   // stack header identity
    activeTab, activeTabId, setActiveTab,
    newTab, closeTab, cycleTab,
    openProjectTerminal,   // scope-aware
    openGlobalTerminal,
  }
}

let initStarted = false

/** HOST BOOT ONLY (src/App.vue's onHostBoot, WP-C's one call there). Re-adopts orphan shells a
 *  frontend reload left running on the backend, else seeds tab 0 so the dock always has one tab. */
export async function initTerminalTabs() {
  if (initStarted) return
  initStarted = true
  let list = []
  try {
    list = await invoke('pty_list_tabs')
  } catch (e) {
    console.error('[useTerminalTabs] pty_list_tabs failed', e)
  }
  if (Array.isArray(list) && list.length > 0) {
    adoptTabs(list)
    seedTabLiveness(list) // list still carries each tab's raw `alive` — adoptTabs' own mapped shape drops it
  } else {
    addTerminalTab({ title: 'Shell', projectId: null, cwd: null })
  }
  activeTerminalScope.value = GLOBAL_SCOPE // defensive: setActiveTab would derive the same, but boot should not depend on it
  const first = terminalTabs.value[0]
  if (first) setActiveTab(first.id)
}
