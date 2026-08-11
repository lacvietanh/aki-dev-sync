# SimpleView — mobile terminal UX regression

**Start time:** 2026-08-11.

**Initial purpose:** Owner reports "severe UX regression on mobile" from a recent terminal
change, and wants a direction to restore the original remote terminal experience. Context/
constraint at the time: repo tree is clean, `HEAD` is `d221593` (`chore(release): cut 1.23.0`,
2026-08-11) — the release immediately after the change under suspicion.

**Strategy:** Walk `CHANGELOG.md`/`git log` for the most recent terminal-touching change,
confirm it against the architecture doc and the wish-plan that authored it, then check whether
the current code gives a companion (phone) any way back to the pre-change behaviour.

**Checklist:**
1. `git log --oneline -30` — every terminal commit since 1.22.0.
2. Read `docs/plan/wish-terminal-split-simpleview.md` (the design), `docs/arch/terminal-stack.md`
   (the shipped architecture), `CHANGELOG.md` 1.23.0 entry.
3. Read `src/composables/useTerminalViewType.js` (the actual host/companion switch) to check
   for an existing opt-out.
4. `git show 0008a90 --stat` — confirm file-level shape of the shipping commit.

**Result:**

1.23.0 (`0008a90`, 2026-08-03, "SimpleView phone plain-text stream, paste/copy fixes") changed
what a phone/companion mounts for the in-app terminal:

- **Before:** companion mounted `TerminalView.vue` — the same xterm.js PTY grid the Mac uses,
  driven by the shared `FRAME_PTY_RESIZE` frame (single PTY, Mac's `cols`/`rows`).
- **After:** companion mounts `SimpleView.vue` — a plain ANSI-stripped scrolling text stream
  (`usePtyStream.js` + `ansiStrip.js`), never `@xterm/xterm`. The switch is unconditional and
  has no per-user/per-session opt-out:

  ```js
  // src/composables/useTerminalViewType.js
  export function useTerminalViewType() {
    const ViewComponent = computed(() => (isHost ? TerminalView : SimpleView))
    return { ViewComponent }
  }
  ```

SimpleView's own spec (`docs/plan/wish-terminal-split-simpleview.md`, "Hard boundary" +
"Accepted scope limitation") states it deliberately does **not** implement colour, cursor
addressing, columns, or any grid geometry, and explicitly calls out that full-screen TUI
programs — **`vim`, `htop`, `less`, and by the same mechanism `claude`/`agy`'s own
mouse-tracking CLI UI** — "will render garbled." This matches "severe UX regression on
mobile": any workflow that runs a TUI program (including this app's own primary remote
use case, driving Claude Code from a phone) lost its screen on the phone in 1.23.0, with no
way back.

This is not an accidental bug. SimpleView is a documented, adversarially-reviewed
architecture decision:
- Motivating defect (`docs/plan/wish-terminal-split-simpleview.md`, "Root cause"):
  `FRAME_PTY_RESIZE` carries the Mac's `cols`/`rows` to every companion; the phone's xterm
  calls `term.resize()` against those values, forcing a grid shaped by the Mac's viewport —
  causing word-wrap breakage, buffer misalignment, and IME preedit conflicts on the phone.
- The decision record (`docs/research/audit-terminal-split-wish.md`, decisions SV-1..SV-7) was
  reviewed, one decision (SV-1, the ANSI strip rule) was caught by measurement and corrected
  before ship, and the doc is marked "✅ built, shipping in 1.23.0."
- Hand-tested 2026-08-03 (`docs/plan/done/remaining-1.23.md`, `docs/plan/done/handtest-1.23.md`)
  for its own stated scope (paste/copy, reset, SV-SELECT) — not for TUI/full-screen behaviour,
  since that was explicitly out of scope by design, not an oversight.

**Verification:** Traced to source for every claim above (`useTerminalViewType.js`,
`wish-terminal-split-simpleview.md`, `audit-terminal-split-wish.md`, `CHANGELOG.md`,
`git show 0008a90 --stat`). Not verified: which specific mobile action the owner hit (TUI
garble vs. something else) — the owner has not yet confirmed the exact symptom; the plan
below asks before scoping a fix.

**Corroborating links:**
- `docs/plan/wish-terminal-split-simpleview.md` — the design, incl. "Hard boundary" and
  "Accepted scope limitation" sections that predict this exact regression class.
- `docs/research/audit-terminal-split-wish.md` — the pre-ship review that settled SV-1..SV-7.
- `docs/arch/terminal-stack.md` — shared-PTY-size architecture (`FRAME_PTY_RESIZE`, "How the
  bytes move") that SimpleView was built to route around.
- `CHANGELOG.md` `[1.23.0]` — the shipped, owner-visible description of the change.

**Decision:** Follow-up research needed before any fix is scoped — see Action.
- **Action:** `docs/plan/restore-terminal-mobile-ux.md` — lays out three restoration
  directions and asks the owner to confirm both the exact symptom and which direction to take,
  since a straight revert reopens closed defects (see that doc).
- **Cross-references:** `docs/plan/wish-terminal-split-simpleview.md` (status line should be
  revisited once a direction is chosen — it currently reads "✅ built, shipping," which will
  no longer be the full story), `docs/index.md` (entries added for this doc and the plan).
