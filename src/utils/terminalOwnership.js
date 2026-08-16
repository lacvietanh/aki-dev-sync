// Pure attribution — no Vue, no `invoke`, no store import. Given the raw session inventory
// (`list_terminal_sessions`) and the current project list, decides which project's badge (if any)
// each session counts on, and the global complement — §3's three-rule priority and §5's
// dedup-by-session complement (docs/plan/done/terminal-ownership-model.md).
//
// This is the SINGLE decision point (`pattern.A8`): the badge, the global badge and (once wired) the
// modal must all read attribution from here, never re-derive it, or the two could disagree.

function normalizeDir(p) {
  const t = String(p || '').replace(/\/+$/, '')
  return t === '' ? '/' : t
}

/**
 * @param {Array<{pid:number, ppid:number, tty:string, cwd:string, owner:?string}>} sessions
 * @param {Array<{id:string, local_path:?string}>} projects
 * @returns {{ byProjectId: Record<string, number>, globalCount: number, ownerOf: (session: object) => (string|null) }}
 */
export function attributeTerminalSessions(sessions, projects) {
  const listedIds = new Set(projects.map((p) => p.id))
  const idsByDir = new Map()
  for (const p of projects) {
    if (!p.local_path) continue
    const dir = normalizeDir(p.local_path)
    if (!idsByDir.has(dir)) idsByDir.set(dir, [])
    idsByDir.get(dir).push(p.id)
  }

  const byProjectId = {}
  for (const p of projects) byProjectId[p.id] = 0

  let globalCount = 0
  for (const session of sessions || []) {
    // Rule 1 — tagged, authoritative, cwd not consulted at all. A tag whose project was since
    // removed from the list stays global rather than falling through to rule 2 (§5: "listed" is
    // resolved at read time; adoption is not a fallback for an unlisted tag).
    if (session.owner) {
      if (listedIds.has(session.owner)) {
        byProjectId[session.owner] = (byProjectId[session.owner] || 0) + 1
      } else {
        globalCount++
      }
      continue
    }
    // Rule 2 — adopted, untagged, exact cwd match. Counts on EVERY listed project sharing that directory (§5), but only once toward the global complement regardless of how many match.
    const adopted = idsByDir.get(normalizeDir(session.cwd)) || []
    if (adopted.length === 0) {
      globalCount++
    } else {
      for (const id of adopted) byProjectId[id] = (byProjectId[id] || 0) + 1
    }
  }

  /** Single-owner resolution for a label (rule 1 then rule 2, first match) — rule 3 is `null`. */
  function ownerOf(session) {
    if (session.owner) return listedIds.has(session.owner) ? session.owner : null
    const adopted = idsByDir.get(normalizeDir(session.cwd))
    return adopted && adopted.length ? adopted[0] : null
  }

  return { byProjectId, globalCount, ownerOf }
}
