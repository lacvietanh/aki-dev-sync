# Batch 1.24 — pinned tabs · notes travel with rsync · one button pattern · GlobalNote · inline host

Five items cut from one owner request (2026-08-12). They are grouped only because they ship together; each closes on its own criterion. Council room: `~/.aki/agent-council/aki-dev-sync/2026.08.12-0707-dock-batch-1240/`.

**File partition — the reason these can run in parallel.** Items A/B/D/E own disjoint files and are forbidden to touch `src/assets/main.css`; item C owns that file alone and runs last, hoisting whatever the others added into the unified pattern. `CHANGELOG.md` is written once at the end, by the lead, so five writers never collide on it.

## A — Pin a terminal tab so it stays in the strip across groups

**Now:** the strip renders only the active scope's tabs (`TerminalTabStrip.vue`, filtered by `activeTerminalScope`). Switching to another project's group hides every tab of the previous one, global included.

**Change:** each chip gets a small pin toggle at its left edge. A pinned tab renders in *every* group's strip — project or global — and sorts ahead of the unpinned ones. `pinned: boolean` is a field on the tab object in `terminalTabsStore.js`, so it mirrors to a paired phone like the rest of the tab list.

**Closes when:** pin a global tab, switch to a project group, and it is still in the strip; unpin it and it disappears from every group but its own. Caps (5 per scope, 16 global) and the collapse-on-zero-tabs watch behave as before.

## B — `.akidevsync/` no longer excluded from rsync

**Now:** `NOTES_DIR_EXCLUDE = '.akidevsync/'` sits in both default lists (`useProjectConfig.js`), plus a migration that injected it into existing projects. The stated reason was pull-side data loss: with the directory excluded from PUSH but not PULL, a pull deletes the local task list because the remote never received one.

**Change:** the owner wants notes/metadata to travel with the project. Removing the entry from **both** directions restores the symmetry that made the original exclusion necessary — pushed up, pulled back. Remove it from the defaults and add a migration that strips it from existing projects' lists.

**Residual risk, accepted by the owner:** a project pulled before it was ever pushed has no `.akidevsync/` on the remote, so a pull with `--delete` removes the local one. Whoever implements this states in the plan record whether the pull path actually passes `--delete`.

**Closes when:** neither default list contains `.akidevsync/`, existing projects are migrated, and a push/pull round-trip carries `notes.json`.

## C — One button pattern for the whole app

**Now:** `--control-h: 28px` is the only sizing token; every `border-radius`, `padding` and `gap` is a hardcoded literal. `.btn-secondary`/`.btn-save`/`.btn-delete` share one byte-identical skeleton differing only in colour; `.btn-icon-only` and `.btn-cell-trigger` are two spellings of the same icon square; four modals redefine `padding: 7px 12px; border-radius: 6px` locally; `.btn-icon` has zero template usages.

**Change:** one base button shape carrying space/size/radius, with modifier classes that change presentation only. Radius and control padding become tokens beside `--control-h`. The six project-table row controls — git, terminal, OPEN trigger, task/note, log, settings — end up on that one shape, and the `!important` overrides in `ProjectTable.vue`'s scoped block that currently re-patch padding per column go away. Dead and duplicate rules are deleted, not kept for safety.

**Closes when:** the six buttons agree on height, radius and gap without a per-column override; no `!important` padding patch remains in `ProjectTable.vue`; `.btn-icon` and every component-local duplicate of a global button skeleton are gone.

## D — GlobalNote textarea shrinks further

The narrow-mode block and the 42px floor already landed (`verify-pending.md` U2/U3). What is left is horizontal, not vertical: at narrow widths the note card's own 16px side padding nests inside the modal body's, so the writing area starts far from both edges while every other modal has been trimmed to use the full width. Trim both paddings together. **The textarea's own height is out of scope** — a first pass lowered the floor to one row, which the owner rejected outright on 2026-08-12; 42px stands.

## E — Change a project's host from the table

**Now:** the row prints `remote_host:remote_path` as static text; changing the host means opening Project Settings.

**Change:** the host half becomes a compact `<select>` over `sshHosts` (already imported into `ProjectTable.vue`), writing straight to `p.remote_host` and persisting through the existing project-save path. `remote_path` stays text. Extreme Narrow applies: the select replaces the text in place and must not widen the column.

**Closes when:** changing the host in the table persists across a relaunch and is the same value Project Settings shows.

## Closure — 2026-08-12

All five items built, code-complete, uncommitted. Everything below is settled by static reading; the runtime checks are `docs/plan/done/verify-pending.md` T6, U5, U6, I3.

**Did this answer what was asked?** Yes on all five, checked against the owner's own words rather than a restatement. The one place the build went past the literal request is B: the owner asked only that notes travel with rsync, and simply deleting the exclude would have satisfied the sentence while re-opening the delete-on-first-pull hole that the exclude was added to plug in 1.22.0. The `--filter=P .akidevsync/` protect rule is what makes "travels" and "cannot be erased" hold at once, so it was built rather than handed back as a question.

**What was cut.** Item C removed rather than added: `.btn-icon` (zero usages), `.btn-icon-only` (a second spelling of `.btn-cell-trigger`), four modal-local copies of one skeleton, one component-local copy of the host select, and every `!important` padding patch in `ProjectTable.vue` — including two blocks the review pass traced back to a matching pair of dead `.col-project-info`/`.col-git-status` rules in `main.css`, deleted on both sides so the computed values did not move. Net −81 lines of CSS before that last pass, more after.

**What the review pass changed after the makers finished.** Two `!important` padding blocks the C maker had left standing (dead against equally-dead `main.css` rules); a stale SimpleView section in `docs/feat/in-app-terminal.md` describing a branch `useTerminalViewType.js` no longer has; three unit tests in `src-tauri/src/sync.rs` asserting the protect filter travels with `--delete` in both directions and with neither in a non-mirror transfer — the highest-stakes line in the batch had no test while the exclude logic three lines away had several. The tests are written but never compiled: this box does not build Rust.

**Cost.** Lead thread (`claude-opus-5`, 231 turns): 315.2k output, 1.39M cache-write, 29.3M cache-read. Subagents, counted from their own completion reports rather than the script (which reads the lead's transcript only): ~186k the button/CSS maker, ~166k the challenger, ~154k the verifier, ~36k the cost run — roughly 542k on top. Five items, four of them one-file changes; the CSS unification and the two review passes are where the spend actually went.

**Left open, deliberately.** Clicking a pinned tab from a foreign group moves the whole panel to that tab's home group, since a pinned tab keeps its real owner. That is defensible and is what the code does; whether the pin is meant to mean "quick access without leaving my group" instead is the owner's call, recorded on `verify-pending.md` T6 rather than guessed at.
