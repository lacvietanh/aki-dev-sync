// In-app terminal: HOST-side relay (docs/plan/done/1.20.0-terminal-and-remote-sync.md §4.4 & remote-control.md §9, ENV-1).
// Bridges PTY Tauri-native events onto WS relay for companions, and relays companion keystrokes back to PTY.
import { listen } from '@tauri-apps/api/event'
import { connectionState, isHost, isSocketCongested, onFrame, send } from './bridge'
import { invoke } from '../utils/tauri'
import { FRAME_PTY_INPUT, FRAME_PTY_OUTPUT, FRAME_PTY_EXIT, FRAME_COMPANION_CONNECTED } from '../constants/protocol'

let started = false

/** Push every tab's scrollback (reset frame) to target connection (or null to broadcast). */
async function pushAllScrollbacks(to) {
  let tabs
  try {
    tabs = await invoke('pty_list_tabs')
  } catch (e) {
    console.debug('[ptyBridge] tab list unavailable, scrollback push skipped', e && e.message ? e.message : e)
    return false
  }
  if (!Array.isArray(tabs) || tabs.length === 0) return true

  let allSent = true
  for (const tab of tabs) {
    const tabId = tab && typeof tab.id === 'number' ? tab.id : 0
    try {
      const { data, cols, rows, alive } = await invoke('pty_get_scrollback', { tabId })
      // Empty reset still delivers authoritative size/liveness; undefined 'to' serializes away.
      if (!send({ t: FRAME_PTY_OUTPUT, tab_id: tabId, data: data || '', reset: true, cols, rows, alive, to: to || undefined })) {
        allSent = false
      }
    } catch (e) {
      // Tab read failed: continue with remaining tabs; failure returned via allSent.
      console.debug('[ptyBridge] scrollback push skipped for tab', tabId, e && e.message ? e.message : e)
      allSent = false
    }
  }
  return allSent
}

// Congestion recovery: when socket drops frames, schedules full multi-tab scrollback replay once drained.
const RESYNC_RETRY_MS = 250
// Non-null timer handle represents active/pending resync state, preventing duplicate concurrent runs.
let resyncTimer = null

// Broadcasts full replay across all companions when host-side socket congestion drops terminal output.
function scheduleResync() {
  if (resyncTimer) return
  resyncTimer = setTimeout(async () => {
    let owed = true
    try {
      // Skip replay while disconnected or congested; retry until socket drains and push succeeds.
      owed = connectionState.value !== 'open' || isSocketCongested() || !(await pushAllScrollbacks(null))
    } finally {
      resyncTimer = null
      if (owed) scheduleResync()
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
      pushAllScrollbacks(frame.id ?? null)
    }
  })
}
