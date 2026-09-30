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

- [x] Fixture test: confirm `--out-format` `%l`/`%M` report the **sender's** attributes in a dry-run pull. **A real fixture now exists and passed, on GNU rsync 3.2.7 on Linux**: `sync::tests::real_rsync_out_format_reports_the_senders_size_and_mtime_in_a_dry_run_pull` runs the local `rsync` binary (GNU 3.2.7 at `/usr/bin/rsync` on this box) local-to-local in temp dirs, with the sender's copy of `known.txt` at a known size and known mtime and a **receiver copy of the same path at a deliberately different size and a different mtime** (beyond the modify-window), using the app's own pull flags (`-avz --dry-run --out-format=%n\t%l\t%M --modify-window=2`, no `-u`). It asserts the parsed `%l`/`%M` equal the sender's values, not the receiver's differing ones — the case pass-2's empty-receiver fixture could not distinguish (skips, does not fail, if `rsync` is absent from PATH). Since the fixture confirms `%l`/`%M` are the sender's even when a differing receiver copy exists, the `get_file_conflict_info` fallback stays unwired — the plan's own condition for skipping it. This result is GNU rsync 3.x on Linux only; it does not prove openrsync's behavior on the Mac — that check stays unticked below: see § Mac checks. (`conflict.rs::parse_out_format_line` + table-driven tests.)
- [x] Unknown rsync (openrsync, or `%M` missing from the output): classification is skipped and today's counts are shown, never a guessed class (the stock-macOS-rsync false-positive history). Also degrades when the metadata call itself fails (e.g. an rsync build that rejects `--out-format` outright), reusing the push dry-run already fetched rather than re-running it. (`conflict::parse_out_format_output`'s `degraded` flag; `sync::compute_sync_status_full`'s two degrade branches; tested.)
- [x] Pull dry-run: drop `-u`, add `--out-format`, reapply the `-u` filter in Rust with real rsync `-u` semantics (skip only when the **local/receiver** copy is strictly newer — an equal-mtime, different-size file still counts); `%M` is parsed as this machine's own local time, not UTC, via an explicit offset parameter (`conflict::local_utc_offset_secs`, TZ-injection tests instead of mutating process env). Unit test that `pull_count` matches the old behaviour on non-conflict fixtures. (`sync::rsync_pull_diff_with_metadata` + `conflict::select_pull_after_u_filter`.)
- [x] Classifier (table in §2) as a pure function over `(L, R, B, sizes)`; one unit test per row, including the restored remote-deletion suppression (`FileClass::Suppressed`) and the remote-behind signal (`FileClass::PushStaleRemote`). (`conflict::classify_file`.)
- [x] Checksum residue call with the cap, converged/differing results cached in memory keyed by `(project, host, path, L, R, size)` so an unchanged residue file is not re-checksummed every poll. (`sync::rsync_checksum_residue` + `conflict::split_checksum_residue`/`classify_checksum_result`, `CHECKSUM_CAP = 200`, `sync::CHECKSUM_CACHE`.)
- [x] `SyncStatusResult.conflicts` + per-class, per-top-directory breakdown counts + remote-behind signal; `⚠ n` badge; tooltip lines split by direction. Direction fallback is set-membership only (push counting comes only from the push dry-run set, never defaulted). Covered through the pure classification core `sync::classify_candidates` (push, pull, conflict, `.git`, and suppression cases) since the full impure pipeline needs a real rsync/SSH round-trip. (`sync.rs::SyncStatusResult.conflicts`/`.git_count`/`.remote_behind`/`.by_top_dir`; `CountBadgeWrap.vue`; `ProjectTable.vue` tooltip functions.)
- [x] Read-only breakdown popover with Explain, reachable from **any** lit badge overlay (push count, pull count, or the conflict badge), not only when conflicts exist. (`ProjectTable.vue` `.conflict-popup`, native Popover API.)
- [x] `explain_sync_status` command (async + `spawn_blocking`, capability entry): payload builder (now carrying `remote_behind`/`by_top_dir`/a unified diff per conflict, `agy` pinned to the flash model tier), agy call, schema-parsed result including `per_conflict` (rendered in the popover). Conflict `rel` paths are validated (relative, no `..`, no absolute) before any temp-dir or local read. A new cheap `check_agy_available` command lets the frontend disable Explain with its reason before any click. No rsync/git mutation. (Implemented as async + `spawn_blocking`; the capability entry was deliberately not added — see report: no existing custom command in this project has one.)
- [x] Update `docs/feat/sync-flow.md` §2 table, `README.md`, `IntroModal.vue`.

## Mac checks after the code is done

The steps above can be written and unit-tested on Linux. Leave these unticked until run on the Mac:

- [ ] Run the pull dry-run with `--out-format='%n\t%l\t%M'` using the rsync binary the app resolves (`rsync --version` from the app's PATH) and confirm the size/mtime are the remote's.
- [ ] Confirm `--out-format` parsing against stock macOS rsync (openrsync, not GNU rsync): `rsync --version` on the Mac to identify which is on PATH, run the same pull dry-run, and check whether `conflict::parse_out_format_output` parses it or degrades — passing tests against Linux GNU rsync in this run does not prove openrsync's line shape or field order.
- [ ] Real case from § Evidence (`tuvi.akinet.me` vs `bien`): badges, tooltip breakdown and `⚠ n` match the hand analysis.
- [x] Badge overlay and popover look right in the real window (WKWebView), narrow and wide. **Found broken on the Mac (2026-09-28): the popover never visibly opened on any click.** Root cause: `--conflict-anchor-${p.id}` was declared as the CSS `anchor-name` on three different elements at once (the PUSH count badge, the PULL count badge, and the conflict badge), all coexisting in the DOM - `position-anchor`/`anchor()` on `.conflict-popup` can't resolve to one of three same-named anchors, so `top`/`left` fell back to an invalid/default position off where the user could see it. `.open-popup` (the pre-existing, working popover) was the control case: exactly one element declares its `--open-anchor-${p.id}`. Fixed by keeping the anchor-name on the PUSH count badge only; the PULL and conflict badges keep their `popovertarget` (still open the same shared popover) but no longer also claim the anchor. See § Amendments (the 2026-09-29 entry gives the actual root causes).
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

## Amendments

**2026-09-28 (popover never opened).** The owner reported the badge popover as never once openable. Investigated by comparing against `.open-popup` (`ProjectTable.vue`), the pre-existing popover that does work: it declares exactly one CSS `anchor-name` per project row. `.conflict-popup` declared the same `anchor-name` on three coexisting elements (PUSH count badge, PULL count badge, conflict badge) - `position-anchor` cannot pick among three same-named anchors, so `.conflict-popup`'s `anchor(bottom)`/`anchor(left)` (no fallback given) computed to an invalid position instead of anchoring under the clicked badge. Fixed by removing the duplicate `anchor-name`/`conflict-anchor-name` bindings from the PULL and conflict badges - all three keep their `popovertarget` wiring (any lit badge still opens the one shared popover), only the PUSH count badge remains the position anchor. Not caught by the unit tests (`CountBadgeWrap.vue`, `ProjectTable.vue` tooltip functions) because CSS anchor-positioning resolution is a real-browser behavior, exactly the class of check § Mac checks' last bullet was left unticked for. `CHANGELOG.md` [Unreleased] Fixed.

**2026-09-28.** The Decisions section's own reopen trigger fired: "no resolution actions and no pre-sync guard in this feature... reopen if: the owner asks for it as its own feature." A mirror PUSH/PULL (`delete_on_push`/`delete_on_pull` on) drops `-u` (`build_rsync_args`), so besides the deletions `get_sync_delete_preview` already caught, it also silently transfers over any file the destination currently holds a newer-or-equal copy of - a second destructive effect with no preview, no confirm, and no log line. This is a distinct mechanism from what this doc designs (conflict detection is read-only status reporting on the *poll*; this is a pre-sync guard on the *mirror transfer itself*), so it did not reopen § Scope's "no keep-local/keep-remote/merge buttons" pin - it extended the sibling `get_sync_delete_preview` safety-guard mechanism instead, adding `get_sync_overwrite_preview` (`sync.rs`) alongside it in the same typed-confirmation flow (`useSync.js`'s `isDeleteOp` branch). Documented in `docs/feat/sync-flow.md`'s Safety Guard bullet, not in this doc's own § Design, since it is not part of the badge/breakdown/agy pipeline this plan builds. This doc's Scope pin otherwise stands: still no resolution actions, still no keep-local/keep-remote/merge UI.

**2026-09-29 (re-verified, no residual gap).** Re-checked whether the classify_file Pull-vs-Conflict split (above the 2026-09-28 amendment) could let a silent mirror overwrite through undetected. It cannot: `get_sync_overwrite_preview` computes straight from the real mirror/`-avzu` dry-run diff (`sync.rs`), never from `classify_file`'s badge classification, so a misclassified badge affects only the Explain/agy popover, never the safety-preview/confirm gate. `startSync` (`useSync.js`) is the one funnel for the normal PUSH/PULL button and the phone-mirrored dispatch, so every non-dry mirror sync through it hits both previews. The one caller that skips the `isDeleteOp` branch is SELECT push (`openSelectDialog`, `specificPaths` non-empty) - `build_rsync_args` still computes `-avz` (no `-u`) for it when mirror mode is on, a latent inconsistency, but `openSelectDialog` already carries its own stricter, independent guard (a per-file local/remote-mtime table, shown for any remote-existing file regardless of which side is newer, confirmed per file before push) - zero live risk today. Reopen if a future `specific_paths` caller is added without its own equivalent guard.

**2026-09-29 (corrects the 2026-09-28 popover amendment).** The badges never opened the popover for two reasons, neither of which was the shared `anchor-name`. (1) The badges were `<span>` elements, and `popovertarget` is defined only on `<button>` and `<input>` - on a `<span>` the click had no default action at all. `.open-popup`'s invoker is a `<button>`; that, not the anchor count, was the real difference from the control case. The badges are now `<button type="button">` (`CountBadgeWrap.vue`). (2) Once a click worked, the popup was cropped: `left: anchor(left)` on a badge at the window's right end put most of the 260px popup outside the 440px window, so only a sliver was visible. `.conflict-popup` now takes `left: clamp(8px, anchor(right) - w, 100vw - w - 8px)` with `w = min(260px, 100vw - 16px)`, keeping both window edges clear (`main.css`). (3) The 2026-09-28 fix left the one anchor on the PUSH count badge, which renders only while the push count is above zero: a row whose lit badges are `⚠` or PULL alone (oscarfamily.vn: 0 to push, 1 conflict, 61 to pull) had no anchor, so its popup had no position. `anchor-name` now sits on the `.sync-btn-wrap` of the PUSH `CountBadgeWrap`, present in every row; the badges carry none (`CountBadgeWrap.vue`). (4) The badges sit inside `<fieldset :disabled="!syncCheckEnabled">`, and a button inside a disabled fieldset does not toggle its popover, so with Sync check off (counts are kept) a lit badge did nothing. Found by an independent read, not in the app. The fieldset no longer carries `disabled`; the PUSH, PULL and DRY controls and the popover's Explain button each take `!syncCheckEnabled` themselves (`ProjectTable.vue`), and the count badge is a `<button>` only when it has a popover (the git-changes badge has none).
Verified in the real WKWebView on 2026-09-29: a temporary log probe (since removed) recorded the click reaching the PUSH count badge, `toggle newState=open` and `:popover-open` true, and screenshots show the full popup inside the window, before and after the anchor moved to the wrapper (PUSH count badge clicked in both). The `⚠` and PULL badges share the component and the same `popovertarget` wiring but were not clicked: `bien` refused SSH that day, so no row could light them.
Reopen if the `⚠` or PULL badge fails to open the popup once `bien` is reachable, or if a window width other than the narrow default misplaces it.

## Cross-references

- `docs/plan/settings-and-state-layout.md` — plan 1, per-host state and baseline.
- `docs/research/akidevsync-project-config-scope-2.md` — why state is per (machine, project, host).
- `docs/feat/sync-flow.md` §2 — baseline reclassification this plan extends.
- `docs/plan/backlog.md` #6 — the backlog item this plan resolves (note `task-1787393248179`).

## Amendment 2026-09-29 (agy runs in an in-app terminal tab)

`explain_sync_status` originally ran `agy -p --output-format json --json-schema …` headless with the prompt on stdin. That was wrong on two counts recorded in the owner's agy facts ([harness-facts.md — Cross-CLI worker](https://github.com/lacvietanh/akidevrule/blob/79fb6695a64254df91fd61e1318b3b8ec5d5eac3/skills/akiflow/references/harness-facts.md#cross-cli-worker-claude-code-lead--agy-headless) and its Model tiers table, read 2026-09-29): `-p` takes the prompt as its own value and must come last, and agy carries its thinking tier inside the model slug, so there is no separate effort dial. A silent headless call also hung with nothing to watch. Decision: the command builds the prompt (instruction + payload) into a temp file and returns `agy --model <slug> --mode plan -p "$(cat <file>)"; rm -f <file>`, which the frontend types into a fresh project-scoped terminal tab (`openExplainTerminal`), so the answer streams in where the owner can see and cancel it. `--mode plan` is the fact file's read-only-by-mechanism flag, which enforces the descriptive-only design line. An interactive `agy -i` variant was tried first and failed on a live run (`plan model not specified`), and the facts already name agy one-shot as the only reliable shape (stateful agy is a trap), so follow-up questions are not offered. The reply is not shown inside the popup and the `headline`/`situation`/`per_conflict` schema and its Rust types are removed. The model comes from the AI Settings dialog (`aiSettingsStore`, list read from `agy models` by `list_agy_models`, default `gemini-3.8-flash-high`) and is validated in Rust (`validate_agy_model`, never empty, never flag-shaped) before it reaches the command line. Reopen if the fact file records a working interactive shape or the owner wants follow-ups.

## Amendment 2026-09-29 (Explain prompt is a briefing, owner-editable)

Decision: `Explain` is read at the moment the owner is about to press PUSH or PULL, so the prompt asks for a briefing shaped by that question (situation sentence; per-collision Local/Remote lines from the diff with the changed lines quoted and a same-region vs different-region verdict; one "PUSH would…" and one "PULL would…" consequence line; a closing line only when it changes the decision), at most 12 plain-text lines. The descriptive-only floor is kept: consequences are stated, never a recommended side, confidence, or command. The payload gains `now` (unix seconds) so ages are computed, not guessed, plus the project name, both absolute roots (`local_root`, `host:remote_root`) and each conflict's full local and remote path; the prompt forbids running commands, searching files or editing, because a first live run had agy scanning the remote with `find /` to discover a path the payload had not given it. The prompt lives in the frontend (`DEFAULT_EXPLAIN_PROMPT`, `aiSettingsStore`) as its single source, is editable in AI Settings, and is passed to `explain_sync_status`, which only bounds its size; an unedited prompt is stored as empty so later improvements to the default still reach it. Reopen if the owner wants a verdict line (that changes the descriptive-only design line 10).

## Amendment 2026-09-29 (interactive agy, two-column popup)

Decision: Explain opens interactive `agy --model <slug> --mode plan -i "<prompt>"` in the in-app terminal tab instead of the headless `-p` form, reversing amendment (a)'s one-shot choice: the owner wants agy's own UI so a multi-command investigation streams and can be steered, and the popup already lists the per-directory and conflict files, so the prompt is now a summary of what matters (risk, what changed, what PUSH/PULL would do) that may run read-only commands inside the two roots. Evidence: `agy --help` lists `-i` = `--prompt-interactive` ("run an initial prompt interactively and continue the session"); the CLI log of the earlier `plan model not specified` failure shows a launch with no `--mode` override, while launches with `--mode plan` + `--model` start the interactive UI. Not verified live end-to-end. Reopen if: `-i` with `--mode plan` fails again with `plan model not specified` - then drop `--mode plan` for `-i` and rely on the prompt's read-only limits, or return to `-p`.

The popup is titled "Sync Changes" and splits into two equal columns, PUSH | PULL, coloured by the global `--color-local` / `--color-remote` tokens (`src/assets/main.css` `:root`); the PUSH/PULL buttons, the config-modal PUSH/PULL blocks, last-action colours and the intro diagram use the same tokens.
