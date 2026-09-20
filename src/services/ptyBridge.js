// In-app terminal: HOST-side relay (docs/plan/done/1.20.0-terminal-and-remote-sync.md §4.4 & remote-control.md §9, ENV-1).
// Bridges PTY Tauri-native events onto WS relay for companions, and relays companion keystrokes back to PTY.
import { listen } from '@tauri-apps/api/event'
import { connectionState, isHost, isSocketCongested, onFrame, send } from './bridge'
import { invoke } from '../utils/tauri'
import { FRAME_PTY_INPUT, FRAME_PTY_OUTPUT, FRAME_PTY_EXIT, FRAME_COMPANION_CONNECTED } from '../constants/protocol'
import { activeTerminalTabId, terminalTabs } from '../store/terminalTabsStore'
import { selectHydratedTabs, tabIdOf } from './replayHydration'

let started = false

/**
 * Full scrollback is deliberately bounded per connection: empty reset frames still
 * establish every tab's authoritative size/liveness without making a reconnect burst
 * grow with the number of tabs.
 */
const REPLAY_SCROLLBACK_TAB_LIMIT = 4
const RESYNC_RETRY_MS = 250
let resyncTimer = null

async function pushScrollbacks(to) {
  let tabs
  try {
    tabs = await invoke('pty_list_tabs')
  } catch (e) {
    console.debug('[ptyBridge] tab list unavailable, scrollback push skipped', e && e.message ? e.message : e)
    return false
  }
  if (!Array.isArray(tabs) || tabs.length === 0) return true

  // Bound which tabs carry full scrollback so the reconnect burst never grows with tab count.
  // Selection logic + rationale live in replayHydration.js (guarded by scripts/tests/replay-hydration.test.mjs).
  const activeId = activeTerminalTabId.value
  const pinnedIds = new Set((terminalTabs.value || []).filter((t) => t && t.pinned).map(tabIdOf))
  const hydrated = selectHydratedTabs(tabs, { activeId, pinnedIds, limit: REPLAY_SCROLLBACK_TAB_LIMIT })

  let allSent = true
  for (const tab of tabs) {
    const tabId = tabIdOf(tab)
    try {
      const snapshot = await invoke('pty_get_scrollback', { tabId })
      const { cols, rows, alive } = snapshot
      // Only hydrated tabs carry history; every other tab still gets an authoritative empty reset so
      // cold tabs have correct existence, dimensions, and liveness on the companion.
      const data = hydrated.has(tabId) ? snapshot.data || '' : ''
      if (!send({ t: FRAME_PTY_OUTPUT, tab_id: tabId, data, reset: true, cols, rows, alive, to: to ?? undefined })) {
        allSent = false
      }
    } catch (e) {
      console.debug('[ptyBridge] scrollback push skipped for tab', tabId, e && e.message ? e.message : e)
      allSent = false
    }
  }
  return allSent
}

// A resync request is emitted by the relay as companion-connected and includes
// the affected connection key, so replay stays scoped to that companion.
function scheduleResync(to) {
  if (resyncTimer) return
  resyncTimer = setTimeout(async () => {
    let owed = true
    try {
      owed = connectionState.value !== 'open' || isSocketCongested() || !(await pushScrollbacks(to))
    } finally {
      resyncTimer = null
      if (owed) scheduleResync(to)
    }
  }, RESYNC_RETRY_MS)
}

/** Boot host-side PTY bridge (idempotent, host-only). */
export function initPtyBridge() {
  if (!isHost || started) return
  started = true

  // Relays Tauri pty-output events to WS companions (local TerminalView listens directly).
  listen('pty-output', (event) => {
    const payload = (event && event.payload) || {}
    // Forwards data, reset flags (clear/restart), and shell liveness state to companions.
    const hasAlive = typeof payload.alive === 'boolean'
    if (payload.data || payload.reset || hasAlive) {
      // tab_id routes bytes to correct xterm tab across content-blind relay coalescing.
      const frame = {
        t: FRAME_PTY_OUTPUT,
        tab_id: payload.tab_id ?? 0,
        data: payload.data || '',
        reset: !!payload.reset,
      }
      if (hasAlive) frame.alive = payload.alive
      // Refused send triggers full resync to heal dropped terminal escape sequences.
      if (!send(frame)) scheduleResync()
    }
  })

  // Relays tab-specific shell exit events to companions.
  listen('pty-exit', (event) => {
    const payload = (event && event.payload) || {}
    send({ t: FRAME_PTY_EXIT, tab_id: payload.tab_id ?? 0 })
  })

  // Handles raw pty_input keystrokes and companion connect replays.
  onFrame((frame) => {
    if (frame && frame.t === FRAME_PTY_INPUT && frame.data) {
      // Defaults missing tab_id to 0 for legacy companion compatibility.
      invoke('pty_write', { tabId: frame.tab_id ?? 0, data: frame.data }).catch((e) => {
        console.error('[ptyBridge] pty_write failed', e)
      })
    } else if (frame && frame.t === FRAME_COMPANION_CONNECTED) {
      // Replay all tabs targeted specifically to the newly connected companion connection ID.
      pushScrollbacks(frame.id ?? null)
    }
  })
}
