// Seam N: native call (docs/plan/done/remote-control.md §4, R-3) routing real Tauri IPC on host vs RPC request on companion.
import { invoke as tauriInvoke } from '@tauri-apps/api/core'
import { isHost, request } from '../services/bridge'
import { FRAME_INVOKE } from '../constants/protocol'

export const invoke = isHost
  ? tauriInvoke
  : (cmd, args) =>
      request({ t: FRAME_INVOKE, cmd, args }).catch((e) => {
        // Named error logger identifying failing companion RPC command.
        console.error(`[invoke] companion RPC "${cmd}" failed:`, e && e.message ? e.message : e)
        throw e
      })
