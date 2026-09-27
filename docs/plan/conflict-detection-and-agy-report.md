# Sync conflict detection + agy report

**Order: plan 2 of 3.** Depends on `docs/plan/settings-and-state-layout.md` (plan 1): the per-host baseline this plan reads is `~/.aki/devsync/state/<project_id>/<host>/baseline.json`, created there, and it already carries plan 1's F2 fix (a merge push no longer forgets local deletions). Independent of plan 3 (`docs/plan/deploy-action.md`). Target release: 1.32.0.

## Scope — pinned

**This feature answers one question: "both badges are lit — what is actually going on?"** It makes the situation understandable at a glance. It does **not** resolve anything.

- **agy's role is interpreter, not advisor.** It turns the classified data into a plain-language explanation the owner can grasp in seconds, instead of reading file lists by hand. It does not recommend a side, does not score confidence, does not write files.
- **Acting is out of scope, by design.** The owner resolves in a terminal with Claude/agy — a separate responsibility with its own context. This feature adds no keep-local / keep-remote / merge buttons and no pre-sync overwrite guard.
- Any future step that proposes an action from this UI is a scope change, not an extension — it reopens this section first.

## Problem

Push/pull badges show a count of files each side would send (`sync.rs::compute_sync_counts`). That one number hides three different situations:

1. **Conflict** — the same file was edited on both sides since the last common state.
2. **One-sided change** — edited on one side only. What the badge was built for.
3. **Stale comparison** — the remote being compared is behind, or the state was recorded against another host, so the whole tree reads as changed.

Both badges lit at once does **not** mean conflict: with `-u` a file edited on both sides appears in exactly one list (the newer side's). Two lit badges only mean each side has something the other lacks — which is exactly why the owner cannot tell what it is.

## Current mechanism (read from code)

- `rsync_change_files` runs `-avzu --dry-run --modify-window=2` per direction and keeps file lines only: names, no size or mtime.
- `write_baseline` stores `path → local mtime` after every full non-dry sync, one file per project, with no record of the host (plan 1 moves it per host and records `remote_path`).
- `compute_sync_counts` uses the baseline for **existence** only (local deletion → push; push file with local mtime = baseline → suppressed). It never looks at the remote side's change since the baseline, so it cannot see a conflict. Table: `docs/feat/sync-flow.md` §2.

## Design

**Root principle.** A 2-way diff only proves a difference exists; it cannot say who caused it. Attribution needs a third point — the last common state (baseline) — for the same reason git needs a merge base. A common state belongs to a **pair** (this project, this host), hence plan 1.

**Cost principle.** Each step exists only to shrink what the next, more expensive step reads: mtime (free, already walked) → size (free, same listing) → checksum (reads content, residue only).

### 1. Remote metadata from the call already made

The pull dry-run drops `-u` and adds `--out-format='%n\t%l\t%M'`, so the one existing SSH call returns every file that differs **plus the remote size and mtime**. The `-u` filter (remote newer than local, 2 s window) is reapplied in Rust so `pull_count` keeps its meaning. The push dry-run is unchanged. Zero extra round-trips on the 60 s poll.

### 2. Classify each differing file (L = local mtime, R = remote mtime, B = baseline for the current host, 2 s window)

| Case | Class |
|---|---|
| L absent | existing rule: in B → local deleted → push; not in B → remote created → pull |
| path under `.git/` | never a conflict: counted in its own `.git` group (push includes `.git` by design, and `.git/index` changes on both sides constantly) |
| no baseline for this host, baseline `remote_path` ≠ current, or not in B with both present | no ancestor → today's `-u` count, never called a conflict |
| L = B, R > B | pull |
| L > B, R = B | push |
| R < B | push — remote behind (stale) |
| L > B, R > B, sizes differ | **conflict** |
| L > B, R > B, same size | checksum (step 3) |
| anything else (e.g. L < B) | today's `-u` count, not a conflict |

### 3. Checksum only the same-size residue

One `rsync -c --dry-run --files-from=<residue>`, only when the residue is non-empty. Identical → converged (typical: the same commit pulled via git on both sides), dropped from all counts. Different → conflict. Capped at 200 files (starting value); above the cap the rest are marked "conflict (unverified)".

### 4. UI — show, never act

Conflicts are removed from `push_count`/`pull_count` and returned as `SyncStatusResult.conflicts` (path, local/remote size + mtime, verified flag), plus per-class, per-top-directory counts for both lists.

- **Badge.** A `⚠ n` overlay via `CountBadgeWrap`, only when n > 0 (Extreme Narrow). Button tooltips carry the breakdown in one line: "12 local edits · 450 .git · 3 conflicts excluded".
- **Breakdown popover.** Clicking a lit badge overlay (the button itself still syncs) opens a read-only popover: the per-class counts, the conflict rows (path, which side is newer, sizes, mtimes), and one **Explain** button. No action buttons.

### 5. agy explanation (on demand, never on the poll)

Explain is disabled with a tooltip giving the reason when no `agy` binary resolves (e.g. a Linux build without it): never a silent no-op. One call from **Explain**: `agy -p --output-format json --json-schema <schema> --model <flash tier> --print-timeout 120`; binary resolved from static candidates first (`~/.local/bin/agy`) per the cold-start PATH rule; inside `spawn_blocking`.

Payload — the whole picture behind both lit badges, not only conflicts:
- Per-class, per-top-directory counts for both directions, host, last sync time with this host, whether the remote is behind.
- Per conflict: metadata row + unified diff of local vs remote (remote copy fetched to a temp dir). Text only, 400 lines per file; binaries and secret-named files (`.env*`, `*.pem`, `*key*`) send metadata only. The name filter does not catch a secret inside an ordinary file; accepted because Explain is an explicit click and the popover says the diffs are sent to agy.

Output schema: `{headline, situation, per_conflict: [{path, what_changed_local, what_changed_remote}]}` — descriptive fields only. The prompt forbids recommendations; the schema has no field to put one in.

## Execution steps

- [ ] Fixture test: confirm `--out-format` `%l`/`%M` report the **sender's** (remote) attributes in a dry-run pull; if not, fall back to `get_file_conflict_info` on the candidate set (one extra SSH, only when candidates exist). The output line is printed by the **local** rsync, so the result on Linux (GNU rsync 3.x) does not prove the Mac: see § Mac checks.
- [ ] Unknown rsync (openrsync, or `%M` missing from the output): classification is skipped and today's counts are shown, never a guessed class (the stock-macOS-rsync false-positive history).
- [ ] Pull dry-run: drop `-u`, add `--out-format`, reapply the `-u` filter in Rust; unit test that `pull_count` is unchanged on non-conflict fixtures.
- [ ] Classifier (table in §2) as a pure function over `(L, R, B, sizes)`; one unit test per row.
- [ ] Checksum residue call with the cap.
- [ ] `SyncStatusResult.conflicts` + breakdown counts; `⚠ n` badge; tooltip line.
- [ ] Read-only breakdown popover with Explain.
- [ ] `explain_sync_status` command (async + `spawn_blocking`, capability entry): payload builder, agy call, schema-parsed result. No rsync/git mutation.
- [ ] Update `docs/feat/sync-flow.md` §2 table, `README.md`, `IntroModal.vue`.

## Mac checks after the code is done

The steps above can be written and unit-tested on Linux. Leave these unticked until run on the Mac:

- [ ] Run the pull dry-run with `--out-format='%n\t%l\t%M'` using the rsync binary the app resolves (`rsync --version` from the app's PATH) and confirm the size/mtime are the remote's.
- [ ] Real case from § Evidence (`tuvi.akinet.me` vs `bien`): badges, tooltip breakdown and `⚠ n` match the hand analysis.
- [ ] Badge overlay and popover look right in the real window (WKWebView), narrow and wide.
- [ ] Explain runs end to end with `~/.local/bin/agy`.

## Decisions

- Decided: remote metadata from the existing pull dry-run · because zero extra SSH per poll · rejected: `get_file_conflict_info` every poll (doubles round-trips for every actively edited project) · reopen if: the fixture test shows `%M` is not the sender's mtime.
- Decided: no size in the baseline · because size only matters local-vs-remote once both sides changed · reopen if: touch-only mtime churn proves noisy.
- Decided: agy receives aggregates + conflict diffs, not every file row · because the classifier already sorts the non-conflict files deterministically; agy's job is to narrate the picture and read conflict content · reopen if: explanations miss something only per-file rows would show.
- Decided: no resolution actions and no pre-sync guard in this feature · because the painpoint is "cannot tell what the two lit badges mean"; acting is the owner's terminal work · reopen if: the owner asks for it as its own feature.

## Evidence (2026-09-27, `tuvi.akinet.me` vs `bien`)

Hand-run dry-runs, read-only.

- **Pull = 4:** all four files exist on `bien`, absent locally, absent from the baseline → "remote created", zero conflicts.
- **Push = 646:** 450 under `.git/` (push includes `.git` by design, `docs/feat/sync-flow.md` §1) + 196 across the whole tree. `bien` HEAD was `4e02bb2` (2026-08-22), local `ef57422` (2026-09-20), and the baseline had been written against `akicloud`. The `R < B` row classifies the 196 as pushes to a stale remote even with one baseline, but would also swallow any edit made on `bien` before the `akicloud` sync. Only a per-host baseline (plan 1) gives `bien` its own ancestor. The explanation the owner needed was one line: "`bien` is a month behind; 450 are git history; no conflicts".

## Cross-references

- `docs/plan/settings-and-state-layout.md` — plan 1, per-host state and baseline.
- `docs/research/akidevsync-project-config-scope-2.md` — why state is per (machine, project, host).
- `docs/feat/sync-flow.md` §2 — baseline reclassification this plan extends.
- `docs/plan/backlog.md` #6 — the backlog item this plan resolves (note `task-1787393248179`).
