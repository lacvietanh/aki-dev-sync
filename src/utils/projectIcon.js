// ICON-1 (docs/plan/done/remote-control.md §7.0): resolves project icon src for host (custom protocol) vs companion (data URI mirror).
// Call sites treat empty string as "render no <img>".
import { assetBase } from '../services/bridge'
import { projectIcons } from '../store/projectStore'

export function projectIconSrc(id, timestamp) {
  if (!id) return ''
  // assetBase is empty on companion (ENV-1 boundary resolved in services/bridge.js).
  if (assetBase) {
    // Suppress icon-less projects on explicit null to avoid WebKit 404 logs (docs/plan/done/hygiene-jul27.md §2).
    const known = projectIcons.value
    if (known && id in known && !known[id]) return ''
    return `${assetBase}${id}?t=${timestamp}`
  }
  return projectIcons.value?.[id] || ''
}
