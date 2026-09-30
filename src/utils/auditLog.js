import { invoke } from './tauri'

// Always written to usage.log regardless of debug mode (logger::audit): what went where - sync, deploy, host switch.
export function auditLog(tag, msg) {
  invoke('log_frontend', { level: 'audit', tag, msg }).catch((e) => console.error('[audit] log failed', e))
}
