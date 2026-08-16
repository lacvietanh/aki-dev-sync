// Local-screen terminal glue: liveness tracking, activation history, companion queue, and per-scope memory.
// Scoped terminal groups (Terminal v2): scope is keyed by `tab.projectId || GLOBAL_SCOPE`.
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
  setTabPendingCmd,
} from '../store/terminalTabsStore'
import { projects, Toast } from '../store/projectStore'
import { invoke } from '../utils/tauri'
import { expandTerminalStack } from './useTerminalPanel'
import { tabLiveness, startTabLivenessTracking, seedTabLiveness } from './usePtyTerminal'

/** Local per-screen tab liveness tracker ({ [tabId]: 'unknown' | true | false }) re-exported from usePtyTerminal.js. */
export const tabAlive = tabLiveness

// Initialized once at module scope so all screens get a single set of liveness listeners.
startTabLivenessTracking()

/** Tabs activated on this screen; enables lazy mounting on first access and v-show retention thereafter. */
export const activatedTabs = ref(new Set())

/** Returns the scope key (`tab.projectId || GLOBAL_SCOPE`) for a terminal tab. */
function scopeOf(tab) {
  return (tab && tab.projectId) || GLOBAL_SCOPE
}

/** Local memory map ({ [scopeKey]: tabId }) tracking the last active tab per scope for re-entry. */
const lastTabByScope = ref({})

/** Resolves the target tab for a scope: remembered active tab if valid, otherwise latest tab or null. */
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

/** Forgets the remembered tab for a single scope when its last tab closes (multi-entity guard). */
function forgetScopeTab(scope) {
  if (!(scope in lastTabByScope.value)) return
  const next = { ...lastTabByScope.value }
  delete next[scope]
  lastTabByScope.value = next
}

/** Companion single-item activation queue: claims the newest mirrored tab matching the pending scope within TTL. */
const pendingActivateScope = ref(null)
const PENDING_CLAIM_TTL_MS = 15_000
let pendingClaimAt = 0
let pendingClaimTimer = null

/** Sets a companion activation claim with a timeout backstop in case host cap enforcement drops the request. */
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

/** Client-side pre-check for scope and global tab limits to provide immediate companion feedback before action dispatch. */
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
    // Replace Set reference on mutation to reliably trigger Vue reactivity watchers.
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
/**
 * Opens or focuses a terminal tab for a given scope (reusing active tabs or allocating new ones).
 * @param {string} scope GLOBAL_SCOPE or project id
 * @param {object} [opts]
 * @param {string} [opts.title] Title for a new tab
 * @param {string|null} [opts.cwd] Working directory for a new tab
 * @param {boolean} [opts.reuse] Reuse existing scope tab instead of creating a new one
 * @param {boolean} [opts.expandStack] Expand dock terminal stack on entry
 */
function openScopeTerminal(scope, { title = 'Shell', cwd = null, reuse = true, expandStack = false } = {}) {
  // Preserve prior scope to restore selection if cap validation fails after scope switch.
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
  // Repeat-tap guard (companion only) — see "Companion add is fire-and-forget" in docs/arch/terminal-stack.md.
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

// Module-level watcher reconciles active tab state and companion claims across shared tab updates.
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
  // Reconcile active tab when current selection is removed or uninitialized, falling back to current scope then global.
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
  // Ensure active tab is marked activated so TerminalView mounts upon initial companion boot/reconnect.
  if (tabs.some((t) => t.id === activeTerminalTabId.value)) {
    markActivated(activeTerminalTabId.value)
  }
})

export function useTerminalTabs() {
  const tabs = computed(() => terminalTabs.value) // FULL list — mount loop only, see TerminalStack.vue
  const scope = computed(() => activeTerminalScope.value) // read-only view
  // Tab strip view: includes current scope tabs plus pinned foreign tabs sorted stably at the front.
  const ownedScopeTabs = computed(() => terminalTabs.value.filter((t) => scopeOf(t) === activeTerminalScope.value))
  const scopedTabs = computed(() => {
    const curScope = activeTerminalScope.value
    const visible = terminalTabs.value.filter((t) => scopeOf(t) === curScope || t.pinned)
    return [...visible].sort((a, b) => Number(!!b.pinned) - Number(!!a.pinned))
  })
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
    // Pick fallback neighbor before dispatching close since companion tab lists update asynchronously.
    let fallbackId = null
    let scopeEmptied = false
    if (activeTabId.value === id) {
      const neighbor = list[idx + 1] || list[idx - 1]
      if (neighbor) {
        fallbackId = neighbor.id
      } else {
        // When last tab in scope closes, mark scope emptied to fall back to global scope.
        scopeEmptied = true
      }
    }
    closeTerminalTab(id)
    if (fallbackId != null) {
      setActiveTab(fallbackId)
    } else if (scopeEmptied) {
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

  /** Switches to project scope and reuses its last active tab, or spawns a new one in the project cwd. */
  function openProjectTerminal(project) {
    if (!project) return
    openScopeTerminal(project.id, { title: project.name, cwd: project.local_path, expandStack: true })
  }

  /** Header terminal-icon entry point — the GLOBAL_SCOPE mirror of openProjectTerminal. */
  function openGlobalTerminal() {
    openScopeTerminal(GLOBAL_SCOPE, { title: 'Shell', expandStack: true })
  }

  /**
   * Dedicated DEV/BUILD launch handler deduplicating tabs by (scope, runKind).
   * Focuses live tabs without re-typing, respawns dead tabs with re-armed commands, or allocates a new tab.
   */
  function openRunCommand(project, cmd, kind) {
    if (!project || !cmd) return
    const scope = project.id
    const priorScope = activeTerminalScope.value
    expandTerminalStack()
    activeTerminalScope.value = scope
    const existing = terminalTabs.value.find((t) => scopeOf(t) === scope && t.runKind === kind)
    if (existing) {
      setActiveTab(existing.id)
      if (tabLiveness.value[existing.id] === false) {
        setTabPendingCmd(existing.id, cmd)
        invoke('pty_spawn', { tabId: existing.id, cwd: project.local_path }).catch((e) =>
          console.error('[useTerminalTabs] openRunCommand respawn failed', e)
        )
      }
      return
    }
    if (capReached(scope)) {
      activeTerminalScope.value = priorScope // put the screen back where it was; the Toast says why
      return
    }
    const tab = addTerminalTab({
      title: kind === 'dev' ? 'DEV' : 'BUILD',
      projectId: scope,
      cwd: project.local_path,
      runKind: kind,
      pendingCmd: cmd,
    })
    if (tab) setActiveTab(tab.id)
    else setPendingClaim(scope) // companion fallback — same mechanism openScopeTerminal uses
  }

  /** Spawns a dedicated SSH remote terminal tab using the backend-generated SSH command string. */
  function openProjectRemoteTerminal(project, sshCmd) {
    if (!project || !sshCmd) return
    const scope = project.id
    const priorScope = activeTerminalScope.value
    expandTerminalStack()
    activeTerminalScope.value = scope
    if (capReached(scope)) {
      activeTerminalScope.value = priorScope // put the screen back where it was; the Toast says why
      return
    }
    const tab = addTerminalTab({
      title: `${project.name} (SSH)`,
      projectId: scope,
      cwd: project.local_path,
      runKind: 'ssh',
      pendingCmd: sshCmd,
    })
    if (tab) setActiveTab(tab.id)
    else setPendingClaim(scope)
  }

  return {
    tabs,            // FULL list — mount loop only
    scopedTabs,      // the strip, cycling, and the close-fallback use this — owned + pinned-foreign
    ownedScopeTabs,  // cap display only — never includes a pinned foreign tab (see comment above)
    scope, scopeProject,   // stack header identity
    activeTab, activeTabId, setActiveTab,
    newTab, closeTab, cycleTab,
    openProjectTerminal,   // scope-aware
    openProjectRemoteTerminal,
    openGlobalTerminal,
    openRunCommand,
  }
}

let initStarted = false

/** Host boot handler: re-adopts existing orphan backend PTY sessions without seeding phantom default tabs. */
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
  }
  activeTerminalScope.value = GLOBAL_SCOPE // defensive: setActiveTab would derive the same, but boot should not depend on it
  const first = terminalTabs.value[0]
  if (first) setActiveTab(first.id)
}
