# Plan — Sync divergence safety (prevent accidental destructive PUSH)

Status: **DONE** (2026-06-30). All items closed. DRY RUN guard bug fixed last. EC-3-sym (remote delete inflates PUSH badge) fixed in v1.7.0.

Completed: EC-1 ✓, EC-2 ✓, EC-3/Tier-2 ✓ (baseline manifest, bidirectional), EC-3-sym ✓, EC-5 ✓ (fresh preview at click), DIVERGED state ✓, delete-confirm ✓, per-side counts ✓, DRY RUN guard ✓. EC-4 & EC-6 accepted as-is.

Cross-refs:
- Research / full analysis: `docs/research/sync-button-semantic-analysis.md`
- Feature contract: `docs/feat/sync-flow.md` (CRITICAL section)
- Code: `src-tauri/src/sync.rs` → `count_rsync_changes`, `build_rsync_args`; `src/composables/useSync.js:24–39`

## Context

Workflow model: **Local (Mac) = Source of Truth, Remote = AI engine.** PUSH = Local→Remote, PULL = Remote→Local. The app glows the PUSH/PULL buttons from a background `rsync --dry-run` every 60s.

## Incident (2026-06-29)

After a heavy context switch, an `app.akinet.me` project had been edited **on the Remote** (AI engine) while the Mac held an older snapshot. The background checker lit **both** PUSH and PULL. "Last action" read "PUSH 13h ago". The operator misremembered which side was authoritative and clicked **PUSH** with `delete_on_push` ON. Result on the Remote:
- New files created on Remote were **deleted**.
- Files previously deleted on Remote were **restored** from the Mac.
- Files merely **edited** on Remote (newer mtime) **survived**.

That asymmetric "Frankenstein" outcome is the diagnostic fingerprint of `rsync -u --delete`.

## Root cause (evidence)

1. **`-avzu` always includes `-u` (--update)** — `src-tauri/src/sync.rs:138` (`vec!["-avzu"...]`). `-u` skips files that are newer on the receiver.
2. **`--delete` is added on top** when `delete_on_push`/`delete_on_pull` is set — `sync.rs:165`.
3. **`-u` + `--delete` is an incoherent combination.** `-u` says "respect the receiver's newer files"; `--delete` says "make the receiver an exact mirror of the sender". Together they: keep receiver-newer **edits**, but delete receiver-only **new files** and resurrect receiver-side **deletions**. Exactly the incident.
4. **rsync is a stateless 2-way diff** — it cannot distinguish "Remote *created* X" from "Local *deleted* X"; both render as "a difference". This is why deleting a file on one side lights up *both* buttons (the operator's observation). Without a stored baseline, this ambiguity is unresolvable in principle.
5. **No first-class "divergence" state.** `check_sync_status` (`sync.rs:362`) computes `push_count` and `pull_count` independently and returns `has_local_changes`/`has_remote_changes` as two booleans. When both are true (a genuine conflict) the UI still presents two ordinary glowing buttons, not a "DIVERGED — resolve manually" state.
6. **"Last action" is a misleading freshness cue.** It records the last *operation*, not *which side currently holds newer content*. Under fatigue the operator anchored on it.
7. **The existing safety guard fired but was click-through-able.** `src/composables/useSync.js:24-39` already shows a red SweetAlert when `delete_on_push && hasPendingPull===true` ("PUSH sẽ XÓA SẠCH... Vẫn PUSH"). It triggered — and was dismissed in one click. Weaknesses: (a) single-click confirm, (b) shows **no list/count of what will be deleted**, (c) trusts the possibly-stale 60s-poll `hasPendingPull` instead of recomputing at click time, (d) only covers the `delete_on_push` push path, not `delete_on_pull`.
8. **`count_rsync_changes` counted `deleting` lines toward the wrong direction's button.** The status check ran with `--delete` (inherited from project settings) and then counted ALL non-noise lines, including `deleting …` entries. A `deleting` line in a PULL dry-run means "local has a file remote doesn't" — the opposite of "remote has new content." This caused PULL to light when the remote was empty but local was full. **Fixed** (2026-06-29): added `!l.starts_with("deleting ")` to the filter in `count_rsync_changes`, making the count additive-only — each direction counts only what its source can offer the destination.

## Edge cases catalogued (2026-06-29)

### EC-1: Dotfiles survive `rm -fR ./*` → `-u` protects them → button stays lit after sync

`rm -fR ./*` uses shell glob `*` which in bash (without `dotglob`) does **not** match dotfiles. After deleting all "visible" files on remote, `.gitignore`, `.env`, `.DS_Store` etc. survive with their original mtimes.

Sequence:
1. User `rm -fR ./*` on remote → remote keeps `.gitignore` (original mtime)
2. User PUSH (with `delete_on_push=true`): `rsync -avzu --delete LOCAL/ REMOTE/`
3. `-u` sees remote's `.gitignore` mtime ≥ local's → **skips it** (does not overwrite)
4. PULL dry-run after PUSH: remote's `.gitignore` is still "newer" (or same + slightly different) → transfer listed → **PULL stays lit**
5. Operator PULL dry-runs to check → only `.gitignore` appears → confusing loop

**Root:** `-u` + `--delete` incoherence. With mirror intent (`--delete`), the sender is authoritative; `-u` must be dropped so local fully overwrites remote regardless of mtime.

**Fix (Tier 1, item 3):** In mirror mode (`delete_on_push` / `delete_on_pull` ON), drop `-u`; use `-avz --delete`. In merge mode (delete OFF), keep `-u`. Never mix both.

### EC-2: PULL lights when remote is newly empty

After `rm -fR ./*` on remote (dotfiles aside), PULL dry-run with `--delete` listed every local file as `deleting …`, inflating `pull_count`. **Fixed** by EC-1 reference + the `deleting` filter fix (root cause #8 above).

### EC-3: Stateless ambiguity — "remote new file" vs "local deleted file" (PULL side)

If remote has a file local doesn't, rsync sees: "remote→local: file to transfer." PULL lights. But the cause could equally be "local deleted that file intentionally" (in which case PUSH with delete should propagate the deletion). Without a baseline, the two are indistinguishable.

**Fixed (Tier 2, 2026-06-29):** Baseline manifest written to `{appDataDir}/baselines/{project_id}.json` after every full successful sync. `compute_sync_counts` in `sync.rs`: PULL dry-run file (a) in baseline AND (b) missing locally → locally deleted → folds into `push_count`. File NOT in baseline → remote created it → stays `pull_count`. Git constraint: accidental local deletions are restored via `git restore`, not by pulling from remote, so all local deletions are intentional by definition.

### EC-3-sym: Stateless ambiguity — "Mac new file" vs "remote deleted file" (PUSH side)

The symmetric case: if Mac has a file remote doesn't, rsync sees: "local→remote: file to transfer." PUSH lights. But the cause could equally be "remote deleted that file intentionally" (AI agent removed a file during refactor, for example). Without a baseline, the two are indistinguishable.

This is the **dominant case** when most coding happens on the remote server. Files deleted on remote keep lighting PUSH on Mac until the next PULL --delete, creating persistent and misleading badge inflation.

**Fixed (v1.7.0):** Same baseline used for PUSH side: PUSH dry-run file in baseline → both sides had it at last sync; remote no longer has it → remote deleted it → suppress from `push_count`. File NOT in baseline → Mac created it (new file) → real `push_count`.

### EC-4: `--modify-window=2` in status check but NOT in actual sync

`count_rsync_changes` inserts `--modify-window=2` to tolerate APFS (ns) vs ext4 (1s) mtime precision. The actual `run_sync` does NOT. Consequence: status check shows "in sync" (difference < 2s window is ignored), but an actual sync can still transfer files (rsync sees the raw mtime delta). This can cause unnecessary transfers or create a perpetual "looks clean, syncs anyway" loop.

**Proposed fix:** Apply `--modify-window=2` to the actual sync command too, or document and accept as-is.

### EC-5: `hasPendingPull` in safety guard is 60s-stale

`useSync.js:25` gates the PUSH+delete warning on `projectRuntime.hasPendingPull`. This value is from the last background poll (up to 60s old). If background poll just ran and the status was clean (or optimistically cleared after a prior push), the guard silently passes — even if remote actually has pending changes that appeared in the last 60s.

**Fix (Tier 1, item 2):** Re-run a fresh dry-run at click-time, not the cached poll value.

### EC-6: Optimistic `hasPendingPush=false` after push, before recheck

`useSync.js:78–84` immediately marks `hasPendingPush=false` after a successful push, then schedules a recheck 3s later. If the push was partial (e.g., `-u` skipped remote-newer files), the 3s window shows a false "all clear." The recheck corrects it, but there's a brief visual lie.

## Fix design

### Tier 1 — safety guards (low risk; mostly JS/Vue + small Rust)

1. **First-class DIVERGED state.** When `has_local_changes && has_remote_changes`, surface a distinct red "⚠ DIVERGED — resolve manually" indicator instead of two ordinary glowing buttons. Block one-click PUSH/PULL in this state; route through an explicit resolution dialog.
   - Touch: `useSyncStatus.js` (derive a `diverged` flag), `AppHeader.vue`/the project row UI, button components.
2. **Make destructive confirms concrete and non-trivial.** Before any `--delete` sync, run a fresh `--delete --dry-run` at confirm time, parse the `deleting ` lines, and show the **count + a sample list of files to be deleted**. Require a **type-to-confirm** (e.g. type the project name or `DELETE`) rather than a single red button. Recompute at click time — do not rely on the 60s-poll `hasPendingPull`.
   - Touch: `useSync.js:24-39` (the guard), a small Rust helper to return parsed delete-list from the dry-run (reuse `count_rsync_changes` plumbing in `sync.rs`).
   - Extend the guard to the `delete_on_pull` path too (symmetric risk Local→ loses local-only files).
3. **Resolve the `-u` + `--delete` incoherence — separate semantics.** Do not mix. Two clear modes:
   - **Merge** (additive): keep `-u`, never `--delete`. Safe default.
   - **Mirror** (exact): drop `-u`, use `--delete`, full authoritative overwrite. Only via the explicit, type-to-confirm path above.
   - Touch: `build_rsync_args` (`sync.rs:129-170`) — choose flag set by mode, not by toggle-on-top.
4. **Replace "last action" with per-side freshness.** Show `push_count` vs `pull_count` and the newest mtime on each side (e.g. "Remote: 47 changes, newest 2m ago | Local: 12 changes, newest 13h ago") so the operator reads *which side is hotter* instead of guessing.
   - Touch: extend `SyncStatusResult` (`sync.rs`) with counts + newest-mtime per side; render in the project row.

### Tier 2 — baseline manifest ✓ IMPLEMENTED (bidirectional, v1.7.0)

Per-project baseline at `{appDataDir}/baselines/{project_id}.json` (Tauri appDataDir; `~/Library/Application Support/Aki Dev Sync/baselines/` on macOS). Old path `~/.aki/devsync-baselines/` read as fallback for smooth upgrade. Written by `write_baseline` after every successful full (non-dry, non-partial) sync. Read by `read_baseline` in `compute_sync_counts` at status-check time. `AppHandle` is passed to `check_sync_status` and `run_sync` to resolve appDataDir; cached in a `static OnceLock<PathBuf>` (`APP_DATA_DIR`) so downstream helpers don't need it threaded through.

4-way classification in `compute_sync_counts`:

| File appears in… | In baseline? | Missing locally? | Classification |
|---|---|---|---|
| PULL dry-run | Yes | Yes | Local deleted → `push_count` |
| PULL dry-run | No | — | Remote created → `pull_count` |
| PUSH dry-run | Yes | — | Remote deleted → suppress from `push_count` |
| PUSH dry-run | No | — | Mac created → `push_count` |

No baseline yet (first run) → all files stay in their raw direction (safe degraded mode).

Code: `sync.rs` → `APP_DATA_DIR`, `baseline_dir`, `legacy_baseline_path`, `baseline_path`, `collect_local_files`, `write_baseline`, `read_baseline`, `compute_sync_counts`, `rsync_change_files`.

## Acceptance criteria — verdict

| Criterion | Status |
|---|---|
| Both-sides-changed → DIVERGED indicator (orange outline + count badge) | ✓ |
| `--delete` sync requires fresh dry-run preview + type-to-confirm | ✓ |
| No sync command sends `-u` and `--delete` simultaneously | ✓ |
| Per-side counts on button (push_count / pull_count) | ✓ |
| Local file deletion lights PUSH, not PULL (Tier 2 baseline) | ✓ |
| Remote file deletion does NOT inflate PUSH badge (EC-3-sym, v1.7.0) | ✓ |
| Dry run mode: delete confirm dialog does NOT appear | ✓ |

## Deliverables (complete)

| File | Changes |
|---|---|
| `src-tauri/src/sync.rs` | `build_rsync_args` mirror/merge split; `rsync_change_files`; `compute_sync_counts` with baseline; `write_baseline`/`read_baseline`; `get_sync_delete_preview` command |
| `src-tauri/src/lib.rs` | Registered `get_sync_delete_preview` in invoke handler |
| `src/composables/useSync.js` | `isDryRun` hoisted before `isDeleteOp`; delete guard skipped in dry run; correct toast |
| `src/composables/useSyncStatus.js` | `pushCount`/`pullCount` stored in runtime; DIVERGED log |
| `src/components/ProjectTable.vue` | `.sync-btn-wrap` + `.sync-count-badge` (absolute overlay); `.is-diverged` orange outline |
| `CLAUDE.md` | Narrow UI principle — absolute rule |
| `docs/feat/sync-flow.md` | CRITICAL semantic contract added |
| `docs/research/sync-button-semantic-analysis.md` | Full incident analysis, EC-1..EC-7, Tier 2 design, DRY RUN bug |
| `docs/plan/done/fix-sync-divergence-safety.md` | This file |

## Notes

- Rust changes built/tested on Mac (cannot compile on remote dev server).
- EC-4 (`--modify-window=2` asymmetry) and EC-6 (3s optimistic window) accepted as-is — cosmetic, no data loss risk.
