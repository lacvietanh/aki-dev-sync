// ICON-1 (docs/plan/done/remote-control.md §7.0) — one place that answers "what goes in this <img>'s
// src for a project icon", so no component has to know which side it is running on.
//
// HOST:      the `aki-devsync-icon://` custom protocol, exactly as before — zero behaviour change.
// COMPANION: the data URI mirrored into `projectStore.projectIcons`, because that protocol is
//            registered by Tauri inside its own webview and resolves to nothing in a phone browser.
//
// Returns '' when there is no icon for that project on either side; call sites must treat '' as
// "render no <img>", never as a URL.
import { assetBase } from '../services/bridge'
import { projectIcons } from '../store/projectStore'

export function projectIconSrc(id, timestamp) {
  if (!id) return ''
  // `assetBase` is '' on a companion — the ONE role-derived value this module needs, already
  // resolved in the Seam-T boundary (ENV-1), so no component ever reads the role marker itself.
  if (assetBase) {
    // The host also consults `projectIcons` so an icon-less project is never requested at all —
    // see docs/plan/done/hygiene-jul27.md §2 for why (WebKit logs an unsuppressible 404 on every miss).
    //
    // Suppress only on an explicit null: `known` is a complete map once filled, but the fill is
    // async, so an id simply ABSENT from it must still be requested — that means "not checked yet",
    // never "confirmed no icon".
    const known = projectIcons.value
    if (known && id in known && !known[id]) return ''
    return `${assetBase}${id}?t=${timestamp}`
  }
  return projectIcons.value?.[id] || ''
}
