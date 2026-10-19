# PUSH/PULL badges counting files that only changed mtime

## Start time

2026-10-19 (last released: 1.33.0).

## Initial purpose

`aki-gegrok-bot` showed PULL 142 and `akiprx` PULL 376 against `grokvm` with nothing new on the server. Question: what do those counts contain, can PULL be pressed safely, and how should the badge count so that a number means "content would change".

Context: grokvm had a mass mtime rewrite on 2026-10-18 around 14:13–14:15 with content unchanged. Constraints: Extreme Narrow UI (no new rows or banners), a 60 s background poll, the existing per-poll checksum cap of 200 (`conflict::CHECKSUM_CAP`), and gegrok's `deploy.on_push: true` (a PUSH restarts the bot and announces a release, so PUSH is not a neutral way to realign mtimes).

## Strategy

Run the real status pipeline (`compute_sync_status_full`) against the real hosts from a temporary test, compare every counted file with `rsync -c --dry-run`, then change the classifier so the count follows content, and re-run the same probe.

## Checklist

- [x] List every file in both counts with local/remote mtime, size and baseline class.
- [x] Check what `rsync -c --dry-run` lists for files identical in content but different in mtime.
- [x] Route same-size moves (Push, PushStaleRemote, Pull, side-attributed Unclassified) through the checksum step; GitGroup stays uncounted and unchecksummed.
- [x] Unit tests for the routing and the tally, each shown to fail against the old code.
- [x] Re-run the real probe on both projects.

## Result

- **Cause:** `classify_file` decides direction from mtimes and the baseline. A same-size file with a newer remote mtime was counted as Pull without any content check; only the both-changed conflict residue was checksummed. The remote touch made almost every file "newer on the remote".
- **rsync fact:** `rsync -c --dry-run` does not list a file whose content matches and whose mtime differs, so one call separates identical from differing files. Checked on the Mac against grokvm.
- **Fix:** same-size moves join the conflict residue in one `rsync -c` call per poll (residue first, cache and cap unchanged). Identical files leave the counts and are reported as `time_only_push` / `time_only_pull`; past the cap or on a failed call a move keeps its count, a residue file stays an unverified conflict. Current behavior: `docs/feat/sync-flow.md` § Checksum residue, § Time-only differences.
- **Real probe after the fix:**

  | Project | Push | Pull | Time-only pull | Conflicts | Poll time |
  |---|---|---|---|---|---|
  | aki-gegrok-bot | 1 (`docs/plan/akirule-hard-inject.md`, local only) | 3 (`Icon\r` and `package-lock.json`, absent locally; `.akidevsync/project.json`) | 139 | 0 | 17.3 s first, 10.4 s cached |
  | akiprx (after the owner's PULL) | 2 (`package-lock.json` back to the git version, `.git/index`) | 0 | 0 | 0 | ~11 s |

- **Found on the way:** gegrok's server `.akidevsync/project.json` has the newer mtime but older content (no `data/` push exclude, no `commands.deploy`), so a PULL would revert both settings. Recorded against the owner's F1 decision in `docs/research/akidevsync-project-config-scope-2.md` § Amendments.

### Critique

- **Accepted risk:** a file the remote re-timed and the owner later edits locally has L > B and R > B, so it shows as ⚠ until a sync realigns the mtimes. It is a false alarm, never a lost edit, and the popover shows both sides. Reopen if false ⚠ badges appear after a server-side touch.
- **Over-cap undercount is impossible by construction:** a file the cap or a failure leaves unchecked keeps its count, so the badge can only be high for a few polls, never silently low.
- **Rejected:** PUSH to realign mtimes (gegrok deploys and announces on push); `rsync -c` on the full tree every poll (reads every file on both sides each 60 s); dropping identical files without a trace (the owner could not tell why the badge fell, so the number stays visible in the tooltips and the popover).

### Verification

`cargo test --lib -- sync:: conflict::`: 118 passed; both new tests (`classify_candidates_same_size_remote_touch_waits_for_the_content_check`, `tally_size_matched_counts_differing_and_unchecked_and_sets_identical_aside`) failed against the old code. Real probe above on grokvm. The owner pressed PULL on akiprx with the 1.33.0 app: no freeze. **Unverified:** the tooltips and the popover line in the installed app (the build with this change is not installed yet).

### Corroborating links

`docs/research/sync-button-semantic-analysis.md` (additive-only count), `docs/research/sync-host-safety.md` (checksum call with a home-relative path), `docs/plan/done/conflict-detection-and-agy-report.md` (the conflict residue this extends).

## Decision

**Action:** `src-tauri/src/sync.rs` (`check_content`, `route_move`, `tally_size_matched`, `SyncStatusResult.time_only_*`), `src-tauri/src/conflict.rs` (doc comments), `src/composables/useSyncStatus.js`, `src/components/ProjectTable.vue`, `src/store/aiSettingsStore.js` (Explain legend), `docs/feat/sync-flow.md`, `CHANGELOG.md` [Unreleased].

**Cross-references:** `docs/research/akidevsync-project-config-scope-2.md` § Amendments (F1 case), `docs/plan/backlog.md` #9.
