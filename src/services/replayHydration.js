// Pure selection of which terminal tabs carry FULL scrollback history on a companion
// reconnect/resync (docs/plan/done/companion-replay-bound.md). Extracted from
// ptyBridge.pushScrollbacks so the burst-bounding invariant can be reasserted by a real
// N-tab test without dragging the Tauri/WS transport into the test runner.
//
// The burst must never grow with tab count: at most `limit` tabs are hydrated with full
// history; every other tab still gets an authoritative empty `reset` (size/liveness) frame
// from the caller. pty_list_tabs is sorted by tab id (not recency), so selection must be
// explicit — a list prefix would let the oldest tabs win and leave the active tab
// history-less.

/** Tab id with the legacy fallback: a tab without a numeric id is treated as id 0. */
export function tabIdOf(tab) {
  return tab && typeof tab.id === 'number' ? tab.id : 0
}

/**
 * Choose the tabs that carry full scrollback, capped at `limit`.
 * Priority: the active tab first (what the user is most likely viewing), then pinned tabs,
 * then remaining tabs in list order to fill the budget. Duplicates never consume budget.
 *
 * @param {Array<{id?: number}>} tabs   tabs to replay, in pty_list_tabs order
 * @param {object} [opts]
 * @param {number} [opts.activeId]      currently active tab id
 * @param {Iterable<number>} [opts.pinnedIds]  ids of pinned tabs
 * @param {number} [opts.limit]         max tabs that may carry full history
 * @returns {Set<number>} ids that should be hydrated with full scrollback
 */
export function selectHydratedTabs(tabs, { activeId, pinnedIds, limit } = {}) {
  const hydrated = new Set()
  if (!Array.isArray(tabs) || tabs.length === 0) return hydrated

  const cap = typeof limit === 'number' && limit > 0 ? limit : 0
  const pinned = pinnedIds instanceof Set ? pinnedIds : new Set(pinnedIds || [])
  const consider = (id) => { if (hydrated.size < cap) hydrated.add(id) }

  if (tabs.some((t) => tabIdOf(t) === activeId)) consider(activeId)
  for (const t of tabs) { if (pinned.has(tabIdOf(t))) consider(tabIdOf(t)) }
  for (const t of tabs) consider(tabIdOf(t))
  return hydrated
}
