# Restore terminal mobile UX — direction options after 1.23.0's SimpleView

**Status:** RESOLVED 2026-08-11 — Direction C chosen and built. See
`docs/plan/done/wish-terminal-manual-resize-authority.md` for the actual design (a refined shape, not
literally the Direction C sketch below — the sketch's "per-screen PTY sizing" framing was replaced
during design with a simpler, adversarially-reviewed "explicit manual authority handoff" mechanism
that achieves the same goal without a backend session-model rewrite). Directions A and B below are
closed, not chosen. This doc is kept for its problem-statement/background section and as the
research trail; do not read its Direction C section as the shipped design.

Action item opened by `docs/research/simpleview-mobile-regression.md` (2026-08-11). Read that doc
first — it traces the reported "severe UX regression on mobile" to a specific, deliberate,
already-reviewed decision, not a bug: `0008a90` (1.23.0) made the phone/companion mount
`SimpleView.vue` (a plain ANSI-stripped text stream) instead of `TerminalView.vue` (the xterm.js
PTY grid the Mac still uses), with no per-user opt-out. `SimpleView`'s own spec
(`docs/plan/wish-terminal-split-simpleview.md`, "Hard boundary" + "Accepted scope limitation")
states up front that full-screen TUI programs — `vim`, `htop`, `less`, and by the same mechanism
`claude`/`agy`'s own mouse-tracking CLI UI — will render garbled on the phone. If that is the
symptom the owner hit, it is working as specified, not broken.

## Why this isn't a one-line revert

`SimpleView` exists to fix a real bug: `FRAME_PTY_RESIZE` carries the Mac's `cols`/`rows` to every
companion, and pre-1.23.0 the phone's xterm called `term.resize()` against those values — forcing
the phone to render a grid shaped by the Mac's viewport, which caused word-wrap breakage, buffer
misalignment, and IME preedit conflicts (`wish-terminal-split-simpleview.md` "Root cause"). That
defect was measured, not assumed, and the fix (`audit-terminal-split-wish.md`, decisions SV-1..SV-7)
went through two adversarial review rounds before shipping. Reverting the phone back to mounting
`TerminalView.vue` un-fixes exactly that.

**Verified, not undone by a revert:** `useTerminalTextDrain.js`'s double-space fix (`b99502a`),
the NBSP/typographic-space normalization, and `TerminalView.vue`'s `macOptionClickForcesSelection`
Option-drag fix all shipped in host-agnostic files the phone/companion split never touched — they
apply identically whether `TerminalView` mounts on Mac only or on both. A revert does **not**
reopen those specific fixes.

**Verified, genuinely reopened by a revert:** the Android/Gboard double-insert defect
(`docs/research/terminal-gboard-double-insert.md`) is diagnosed but was never fixed — it was live
on phone before SimpleView (phone used to mount `TerminalView.vue` + the drain too) and goes live
again the moment phone mounts xterm.js. Same for the `FRAME_PTY_RESIZE` grid-coupling bug itself
(word-wrap breakage, buffer misalignment, IME preedit conflicts) — that is the actual bug being
reopened, not a metaphor for "old bugs."

## Direction A — Blind revert (fast)

Make the companion mount `TerminalView.vue` again: collapse `useTerminalViewType.js`'s
`isHost ? TerminalView : SimpleView` back to always `TerminalView`, or delete the
`<component :is="ViewComponent">` switch in `TerminalStack.vue` entirely.

- **Cost:** smallest possible diff — a few lines in `useTerminalViewType.js` /
  `TerminalStack.vue`. `SimpleView.vue`, `usePtyStream.js`, `ansiStrip.js`, `ptyCodec.js` can stay
  in the tree unused or be deleted in a follow-up.
- **Gain:** phone immediately gets full-screen TUI support back (`vim`, `htop`, `claude`, `agy`),
  restoring the pre-1.23.0 experience exactly.
- **Reopens:** the `FRAME_PTY_RESIZE` grid-coupling bug SimpleView exists to fix, and the
  never-fixed Android/Gboard double-insert defect on phone. Both are real, previously-measured
  defects, not hypothetical risk.

## Direction B — Opt-in xterm.js toggle (near-term)

Keep `SimpleView` as the companion default (so plain CLI output stays clean and un-garbled by
default), but add a manual per-tab or per-session toggle that lets the phone opt into mounting
`TerminalView` when the user specifically needs a full-screen TUI.

- **Cost:** medium. `useTerminalViewType.js` gains a manual override ref (still reads `isHost`
  first — Mac is untouched); needs an Extreme-Narrow-compliant trigger (no new row/banner — an
  existing menu item or a tap on the SimpleView header, not a new button). Does not fix
  `FRAME_PTY_RESIZE`'s root cause; it makes the trade-off opt-in and visible instead of a silent
  default in either direction.
- **Gain:** default phone experience stays regression-free for the common case (plain CLI output,
  logs, `git`, `npm`), while `vim`/`claude`/`agy` on phone becomes reachable again for users who
  choose it, accepting the known IME/grid risk knowingly per session.
- **Risk carried forward, by design:** the grid-coupling bug and the Gboard defect are still live
  whenever the toggle is on — this direction scopes *when* the user hits them, not whether the
  underlying causes exist.

## Direction C — Per-screen PTY sizing (correct, bigger)

Fix `FRAME_PTY_RESIZE`'s actual root cause instead of routing around it: give each screen (Mac
host, each companion) its own independently-sized PTY/grid, so a phone can mount `TerminalView`
with a grid genuinely sized to its own viewport instead of inheriting the Mac's `cols`/`rows`. This
would let `TerminalView` be the default again everywhere, phone included, with no accepted scope
limitation.

- **Cost:** the largest of the three, by a wide margin. `src-tauri/src/pty.rs` currently exposes
  one PTY per tab, sized once, with the Mac as sole resize authority (`T-4`, labelled at
  `docs/plan/done/1.20.0-terminal-and-remote-sync.md:144`, restated at `docs/arch/terminal-stack.md`).
  Serving multiple grid sizes from one shell session needs either N independent PTYs per tab (one
  per active screen — a different backend session model) or a terminal-multiplexing layer inside
  the Rust process (tmux-shaped: one shell, many attached grids, each reflowed independently) —
  neither exists today and both break the T-4 invariant that other docs already cite as settled.
- **Gain:** the actual correct fix — no accepted scope limitation, no opt-in toggle to remember,
  full parity between Mac and phone.
- **Before any code:** this needs its own wish-doc-style design (module boundaries, wire-protocol
  changes to `FRAME_PTY_RESIZE`, `pty.rs` session model) and an adversarial review pass, the same
  weight `wish-terminal-split-simpleview.md` got before SimpleView shipped — not a quick patch.

## Resolution

Direction C was chosen (2026-08-11) without further owner back-and-forth on the two open questions
below — the owner asked for the correct fix built out fully rather than a symptom-confirmation
round-trip. See `docs/plan/done/wish-terminal-manual-resize-authority.md` for what was actually built,
including why it diverges from this doc's original Direction C sketch.

Original open questions, recorded for the trail:
1. ~~Confirm the exact symptom~~ — superseded: the shipped design restores full TUI support
   unconditionally, so the answer no longer gates anything.
2. ~~Pick a direction~~ — C, refined during design (see above).

## Cross-references

- `docs/research/simpleview-mobile-regression.md` — the research doc this plan executes the
  Action item for.
- `docs/plan/wish-terminal-split-simpleview.md` — SimpleView's design, "Hard boundary" and
  "Accepted scope limitation" sections, SV-1..SV-7.
- `docs/research/audit-terminal-split-wish.md` — the pre-ship adversarial review, incl. the
  `FRAME_PTY_RESIZE` measurement (Finding 1) SimpleView was built to fix.
- `docs/research/terminal-gboard-double-insert.md` — the still-unfixed defect Direction A and B
  both re-expose on phone.
- `docs/arch/terminal-stack.md` — current shared-PTY-size architecture; the T-4 invariant Direction
  C would need to revisit.
- `docs/index.md` — entry for this doc already added under Plans (Active).
