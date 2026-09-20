// Terminal tab list: shared session state auto-mirrored host↔companion via services/mirror.js (docs/arch/terminal-stack.md, WP-C).
// NO alive FIELD ON PURPOSE: PTY liveness travels on pty_output/pty_exit channel (useTerminalTabs.js tabAlive); mirroring here would create competing SSOT.
import { ref } from 'vue'
import { action } from '../services/action'
import { invoke } from '../utils/tauri'
import { Toast } from './projectStore'

/**
 * Tab list ref: [{ id, title, projectId, cwd, titleLocked?, resizeOwner?, pinned? }]
 * resizeOwner: PTY size driver — 'host' (Mac) or companion frame.from connection id (docs/plan/done/wish-terminal-manual-resize-authority.md).
 * pinned: display-only flag across groups.
 */
export const terminalTabs = ref([])

/** Per-screen active tab id (navigation state listed in services/mirror.js PER_SCREEN_KEYS, never mirrored). */
export const activeTerminalTabId = ref(0)

export const GLOBAL_SCOPE = 'global'

/** Per-screen active tab scope ('global' | projectId; in PER_SCREEN_KEYS so screen navigation is isolated). */
export const activeTerminalScope = ref(GLOBAL_SCOPE)

function nextTabId() {
  return terminalTabs.value.length === 0 ? 0 : 1 + Math.max(...terminalTabs.value.map((t) => t.id))
}

/**
 * Host-side tab allocator via action() (docs/arch/terminal-stack.md, callers use openScopeTerminal).
 * Optional runKind/pendingCmd tag DEV/BUILD tabs for command dispatch without extra round-trip (docs/plan/done/dev-build-in-app-launch.md).
 */
export const addTerminalTab = action('terminalTabsStore.addTerminalTab', ({ title, projectId = null, cwd = null, runKind = null, pendingCmd = null } = {}) => {
  const tab = { id: nextTabId(), title: title || 'Shell', projectId, cwd, runKind, pendingCmd, pinned: false }
  terminalTabs.value = [...terminalTabs.value, tab]
  return tab
})

/** Re-arms an existing tab's pending command on dead-tab respawn (id-scoped per CLAUDE.md multi-entity guard). */
export const setTabPendingCmd = action('terminalTabsStore.setTabPendingCmd', (id, cmd) => {
  const idx = terminalTabs.value.findIndex((t) => t.id === id)
  if (idx === -1) return
  const next = { ...terminalTabs.value[idx], pendingCmd: cmd }
  terminalTabs.value = [...terminalTabs.value.slice(0, idx), next, ...terminalTabs.value.slice(idx + 1)]
})

/** Atomically reads and clears ONE tab's pending command to prevent double execution across screens. */
export const consumeTabPendingCmd = action('terminalTabsStore.consumeTabPendingCmd', (id) => {
  const idx = terminalTabs.value.findIndex((t) => t.id === id)
  if (idx === -1) return null
  const cmd = terminalTabs.value[idx].pendingCmd
  if (!cmd) return null
  const next = { ...terminalTabs.value[idx], pendingCmd: null }
  terminalTabs.value = [...terminalTabs.value.slice(0, idx), next, ...terminalTabs.value.slice(idx + 1)]
  return cmd
})

/** Closes ONE tab by id (CLAUDE.md multi-entity guard) and notifies host PTY via pty_close_tab. */
export const closeTerminalTab = action('terminalTabsStore.closeTerminalTab', (id) => {
  const list = terminalTabs.value
  const idx = list.findIndex((t) => t.id === id)
  if (idx === -1) return
  terminalTabs.value = [...list.slice(0, idx), ...list.slice(idx + 1)]
  // Optimistic UI close: toast warning if backend pty_close_tab fails to avoid silent orphaned shells.
  invoke('pty_close_tab', { tabId: id }).catch((e) => {
    console.error('[terminalTabsStore] pty_close_tab failed', e)
    Toast.fire({ icon: 'error', title: 'Tab closed, but its shell may still be running on the Mac' })
  })
})

/**
 * Renames ONE tab by id (CLAUDE.md multi-entity guard).
 * auto=true: shell OSC title change (ignored if titleLocked); auto=false: user manual rename (sets titleLocked).
 */
export const renameTerminalTab = action('terminalTabsStore.renameTerminalTab', (id, title, { auto = false } = {}) => {
  const trimmed = (title || '').trim()
  if (!trimmed) return
  const idx = terminalTabs.value.findIndex((t) => t.id === id)
  if (idx === -1) return
  const tab = terminalTabs.value[idx]
  if (auto && tab.titleLocked) return
  const next = { ...tab, title: trimmed }
  if (!auto) next.titleLocked = true
  terminalTabs.value = [...terminalTabs.value.slice(0, idx), next, ...terminalTabs.value.slice(idx + 1)]
  // Mirror to the backend's durable TabMeta so a later reload's adoptTabs sees this title, not a placeholder.
  invoke('pty_rename_tab', { tabId: id, title: trimmed }).catch((e) =>
    console.error('[terminalTabsStore] pty_rename_tab failed', e)
  )
})

/** Flips ONE tab's pinned flag (display-only across group strips, id-scoped per CLAUDE.md multi-entity guard). */
export const toggleTabPinned = action('terminalTabsStore.toggleTabPinned', (id) => {
  const idx = terminalTabs.value.findIndex((t) => t.id === id)
  if (idx === -1) return
  const tab = terminalTabs.value[idx]
  const next = { ...tab, pinned: !tab.pinned }
  terminalTabs.value = [...terminalTabs.value.slice(0, idx), next, ...terminalTabs.value.slice(idx + 1)]
  // Mirror to the backend's durable TabMeta so a later reload's adoptTabs sees this pin, not the default unpinned state.
  invoke('pty_set_tab_pinned', { tabId: id, pinned: next.pinned }).catch((e) =>
    console.error('[terminalTabsStore] pty_set_tab_pinned failed', e)
  )
})

/** Sets PTY resize authority for ONE tab ('host' or companion connection id; docs/plan/done/wish-terminal-manual-resize-authority.md). */
export function setResizeOwner(id, owner) {
  const idx = terminalTabs.value.findIndex((t) => t.id === id)
  if (idx === -1) return
  const tab = terminalTabs.value[idx]
  if ((tab.resizeOwner || 'host') === owner) return
  const next = { ...tab, resizeOwner: owner }
  terminalTabs.value = [...terminalTabs.value.slice(0, idx), next, ...terminalTabs.value.slice(idx + 1)]
}

/** Host one-tap reclaim of PTY resize authority (resets resizeOwner to 'host'). */
export function reclaimResizeAuthority(id) {
  setResizeOwner(id, 'host')
}

/**
 * Host boot only: rebuilds the tab list from the backend's own record after a frontend reload wiped
 * local state (the PTY sessions survive; this JS module does not). `pty_list_tabs` now returns each
 * tab's durable `project_id`/`title`/`pinned` (src-tauri/src/pty.rs `TabMeta`), so a tab is re-adopted
 * into its actual project scope with its real title and pin — landing in the global scope with a
 * placeholder title is now the DEGENERATE case, applied only to a tab the backend genuinely has no
 * owner recorded for, not blindly to every tab.
 *
 * Called only while `terminalTabs.value` is still empty (see `initTerminalTabs`), so replacing the
 * whole array is not the multi-entity-wipe CLAUDE.md warns about — there is no sibling tab state here
 * to lose, this call IS the bootstrap of that state from the one source (the backend) that survived.
 */
export const adoptTabs = action('terminalTabsStore.adoptTabs', (list) => {
  if (!Array.isArray(list) || list.length === 0) return
  terminalTabs.value = list.map((t) => ({
    id: t.id,
    title: t.title || `Shell ${t.id}`,
    projectId: t.project_id ?? null,
    cwd: null,
    pinned: !!t.pinned,
  }))
})
