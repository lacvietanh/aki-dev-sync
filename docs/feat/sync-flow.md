# Sync Flow (Push, Pull, Select)

> updated 2026-09-28 · v1.31.0

This document covers the core synchronization capabilities of Aki Dev Sync, designed to support the **Lạc Việt Anh Workflow** where the Local machine acts as the Source of Truth and the Remote acts as the AI Engine.

---

## ⚠ CRITICAL - Semantic Intent of PUSH/PULL Buttons

> **This is the authoritative design contract. All status-check logic and UI must conform to it.**

| Button | Lights when… | Means… |
|--------|-------------|--------|
| **PUSH** | Local has files or changes that Remote does not yet have | "Local is ahead - ready or needed to push up" |
| **PULL** | Remote has files or changes that Local does not yet have | "Remote is ahead - ready or needed to pull down" |

**What PUSH/PULL buttons must NOT mean:**
- PULL must not light because `--delete` would erase local files that remote no longer has. That is a deletion on the remote side, not new incoming content.
- PUSH must not light because `--delete` would erase remote files that local no longer has. That is the correct direction, but only when the user explicitly chose to mirror.

**Consequence:** The status checker (`rsync_change_files`) must count **additive transfers only** - files the source has to send to the destination. `deleting …` lines from rsync must be excluded from the count. See `sync.rs → rsync_change_files`.

**Resolved (Tier 2 Baseline Manifest - v1.7.0, relocated per-host in v1.32.0):**
rsync is stateless. Without extra context it cannot distinguish "remote created file X" from "local deleted file X" (PULL ambiguity), nor "Mac created file Y" from "remote deleted file Y" (PUSH ambiguity). Both issues are now resolved using a local baseline snapshot written to `~/.aki/devsync/state/<project_id>/<host>/baseline.json` after every full sync (`src-tauri/src/sync_state.rs`, sole owner of that directory). The baseline belongs to a **(project, host) pair**, not to the project alone — see §2 Sync Status Checker below for the full reclassification logic and the F2 carry-forward rule.

---

## 1. Core Sync Actions

### PUSH (Local → Remote)
- Transmits changes from the Local machine to the Remote.
- **`.git` Checkbox**: Controls whether the `.git/` folder is included in the sync.
  - **ON (Default)**: Gives Claude full context of git history and staged changes on the Remote.
  - **OFF**: Pushes only files, avoiding overwriting the git history on the remote if needed.
- **Delete on Push Toggle** (Config Modal): 
  - **OFF (Default)**: Safe mode. Pushing only adds or overwrites files. It will not delete files on the Remote, even if they were removed locally.
  - **ON**: Strict mirror mode. Pushing passes `--delete` to rsync, permanently deleting any file on the Remote that does not exist Locally.
  - **Safety Guard**: Before any delete-enabled sync runs, the app previews exactly what `--delete` would remove (`get_sync_delete_preview`) **and** what mirror mode would silently overwrite (`get_sync_overwrite_preview`) - mirror drops `-u` (`build_rsync_args`), so it also transfers over any file the destination currently holds a newer/equal copy of, an effect the delete preview alone never catches since it only matches rsync's `deleting ` lines. If anything is at risk from either preview, a typed-confirmation dialog blocks the sync until the project name is typed exactly, listing deletions and overwrites separately - mirrored to a paired phone (`docs/feat/remote-control.md`), not a Mac-only SweetAlert2. Flow-app artifacts (e.g. `REPORT.html`) unchanged since the last sync, and deletions or overwrites confined to a push-only excluded dir (e.g. `.git/`), auto-approve without asking. Either preview failing (e.g. SSH error) is treated as at-risk-unknown and still requires confirmation.
  - **First sync with a host**: both previews are blind to an empty or missing destination (nothing to delete or overwrite there), and the remote host is a one-click table dropdown. So the first real (non-dry) PUSH or PULL between a project and a host it has no `state/<id>/<host>/last_sync.json` for - mirror or not, SELECT push included - shows one confirm dialog naming the direction and exact `host:path` before anything transfers. Unreadable history counts as none (asks). Why: `docs/research/sync-host-safety.md`.

### PULL (Remote → Local)
- Retrieves files modified or created by the AI on the Remote back to the Local machine.
- **Delete on Pull Toggle** (Config Modal):
  - **ON (Default)**: Mirrors the Remote perfectly. Passes `--delete` to remove any files locally that aren't on the Remote.
  - **OFF**: Merges Remote changes into Local without deleting local-only files.

### SELECT / PUSH SPECIAL
- Allows you to push only **specific files** instead of running a full sync.
- Opens a **native macOS file picker** (multi-select, starts in project root) - no dependency on Git status.
- If any selected file already exists on the remote, a conflict table shows local vs. remote mtime side-by-side before asking to confirm the overwrite.
- **Why?**: Push a single modified file (e.g., a config fix) without waiting for a full directory scan, and get an explicit warning when the remote version is newer.

## 2. Dry Run & Status Indicator

### DRY RUN Toggle
- A global toggle per project that enables `--dry-run` for `rsync`.
- When ON, clicking Push or Pull will only simulate the operation and print exactly what would happen in the logs, without modifying any files on disk.

### Sync Status Checker
- **Gated on project settings being readable (v1.32.0):** PUSH, PULL and the status check itself are skipped entirely while a project's `.akidevsync/project.json` is not readable (missing with nothing to seed from, unavailable, or corrupt) — see `docs/arch/settings-and-state.md`. This JS-side gate is UX only; the actual floor is in Rust, which re-reads `.akidevsync/project.json` itself at sync time and refuses if it is not `ok`, so a stale or empty exclude list can never reach rsync regardless of what the JS-side object holds. The buttons show a tooltip explaining why; saving the config dialog (which creates the file) clears the gate.
- The app polls the sync status in the background every 60 seconds (or on-demand via the Refresh button).
- It runs a silent `rsync --dry-run` to detect changes.
- **A failed check is shown, never swallowed.** The buttons keep their last good counts (no flicker) but get a dashed red outline (`btn-sync-stale`) and their tooltip opens with the error (`syncCheckError`, `useSyncStatus.js`); the next successful check or a host switch clears it. The Rust side logs each distinct failure once per (project, host) to `usage.log` (`log_status_anomaly`, `sync.rs`).
- **Button Glow**: If there are changes to Push, the PUSH button lights up. If there are changes to Pull, the PULL button lights up.
- **Additive-only count (CRITICAL):** The status checker counts only **transfer lines** from rsync output - lines representing content the source has to offer the destination. `deleting …` lines are **excluded** from the count. A deletion listed in a PULL dry-run means "local has a file remote doesn't" - that is the opposite direction's signal, not incoming remote content. Including it caused PULL to light incorrectly when the remote was empty. See `docs/research/sync-button-semantic-analysis.md` for full analysis.
- **Tier 2 Baseline Reclassification (v1.7.0):** After every full successful sync, a snapshot of the local file list is written to `~/.aki/devsync/state/<project_id>/<host>/baseline.json`, alongside the `remote_path` it was recorded against. On the next status check, both PUSH and PULL lists are filtered against this baseline:

  | Case | rsync sees | Baseline says | Classification |
  |------|-----------|---------------|----------------|
  | PULL file + in baseline + absent locally | remote has X, Mac doesn't | X existed at last sync | Mac deleted X → `push_count` |
  | PULL file + not in baseline | remote has X, Mac doesn't | X is new | Remote created X → `pull_count` |
  | PUSH file + in baseline + local mtime unchanged since baseline | Mac has X, remote doesn't | X existed at last sync, not edited locally | Remote deleted X → suppress from `push_count` |
  | PUSH file + in baseline + local mtime changed since baseline | Mac has X, remote doesn't | X existed at last sync, edited locally | User modified X → keep in `push_count` |
  | PUSH file + not in baseline | Mac has X, remote doesn't | X is new | Mac created X → `push_count` |

  The PUSH-side suppression is especially important for workflows where most coding happens on the remote server - without it, every file deleted on the remote would falsely light the PUSH badge on Mac.

- **Per-host baseline, and what counts as "no baseline" (v1.32.0):** The baseline is read for the *active* `(project, host)` pair only — a baseline written while synced to host A is never applied to host B, and switching the Remote Host dropdown back to A restores A's own baseline untouched. A baseline whose recorded `remote_path` no longer matches the target's current `remote_path` counts as **no baseline** (a status check falls back to raw dry-run counts instead), because the file it was recorded against is someone else's tree once the path has changed.

- **F2 — baseline carry-forward on a merge push.** A baseline only records what the sync actually made common between both sides. After a PUSH **without** `--delete` (merge mode), any path present in the previous baseline but now missing locally is carried forward into the new baseline instead of being dropped: the local deletion has not reached the remote yet, so that file is still common (it still exists on the remote). Without this, the next status check would see the file present on the remote and absent from the new baseline and misclassify it as *remote created* → light PULL, when it is really still *local deleted, not yet pushed*. A mirror PUSH (`--delete` ON) needs no carry-forward, since the deletion did propagate; neither does either PULL direction, since a merge PULL restores the file locally and a mirror PULL makes both sides match the remote either way.

- **S2 — a merge push cannot clear the push badge F2 keeps lighting.** F2 correctly classifies the carried-forward path as *local deleted* every status check, which reclassifies it into `push_count` (the PUSH-side suppression table above only suppresses a file whose local mtime matches the baseline — a deleted file has no local mtime, so it is never suppressed). Pushing again in merge mode does not remove it: a merge PUSH never sends `--delete`, so the remote file is never actually removed, and the next status check carries the same entry forward again. The badge only clears via a mirror PUSH (`--delete` ON, propagates the deletion) or by manually removing the remote file. Read the rule as "PUSH is lit because you deleted this locally," not "pushing will clear it."

### Conflict detection (v1.32.0) — both badges lit is not "conflict"

Both PUSH and PULL lit at once only means each side has something the other lacks — with `-u`, a file edited on both sides appears in exactly one list (the newer side's), so a real conflict (same file, both sides genuinely edited since the last common state) needs a third point of comparison: the per-host baseline. `src-tauri/src/conflict.rs` is the sole owner of this classification and is pure logic only (no rsync/SSH I/O); `sync.rs::compute_sync_status_full` gathers the inputs and calls it.

- **Remote metadata from the existing pull dry-run.** The pull-side dry-run drops `-u` and adds `--out-format='%n\t%l\t%M'`, so the one existing SSH call also returns every differing file's remote size and mtime. `%M` is printed by rsync in the **local time zone of the process actually running rsync** (this app's Mac client, per rsync's own `timestring()`/`localtime()`), never UTC — `conflict::parse_rsync_mtime` takes an explicit UTC-offset parameter, with `conflict::local_utc_offset_secs()` the one real caller reading this machine's offset via `libc::localtime_r`. The `-u` filter is reapplied in Rust (`conflict::select_pull_after_u_filter`) with real rsync `-u` semantics: a file is skipped only when the **local (receiver) copy is strictly newer** than the remote beyond the 2s window — an equal-mtime, different-size file still counts. The push-side dry-run is unchanged.
- **Classifier — one pure function over (L, R, B, sizes)** (`conflict::classify_file`, table below; L/R/B = local/remote/baseline mtime within the existing 2s `--modify-window`):

  | Case | Class |
  |---|---|
  | path under `.git/` | never a conflict — counted separately for the tooltip, kept in whichever direction's dry-run reported it |
  | L absent, in baseline | local deleted → push |
  | L absent, not in baseline | remote created → pull |
  | R absent, L unchanged since baseline | **suppressed** — remote deleted the file and local never touched it, so nothing is sent either way (never counted as push) |
  | R absent, L changed since baseline | push — a real local edit, even though the remote lacks the file |
  | no baseline for this host/target, or file not in baseline while both sides present | direction comes only from set membership in the push/pull dry-run results, never a guessed conflict |
  | L = B, R > B | pull |
  | L > B, R = B | push |
  | R < B | push, and the remote is flagged behind (remote regressed relative to the common ancestor — stale) |
  | L > B, R > B, sizes differ | **conflict** |
  | L > B, R > B, same size | checksum (below) |
  | anything else (e.g. L < B) | direction by set membership only |

- **Direction fallback is set-membership only.** A file the classifier could not resolve to push/pull itself (`Unclassified`/`GitGroup` with no baseline) is counted push only if the push dry-run actually reported it, pull only if the `-u`-reapplied pull dry-run reported it — never defaulted to push. A file neither dry-run would actually transfer (e.g. push-excluded and also excluded by the reapplied `-u`) is dropped from every count.
- **Checksum residue, cached.** Same-size files that both changed since the baseline are checksummed with one `rsync -c --dry-run --files-from=<residue>` call (remote path passed home-relative, `~/x` → `x`: `--files-from` sends the path without remote-shell parsing, so a literal `~` would fail - `home_relative`, `sync.rs`), capped at 200 files (`conflict::CHECKSUM_CAP`) — above the cap the rest are marked conflict, `verified: false`. A converged/differing result is cached in memory keyed by `(project, host, path, local mtime, remote mtime, size)`, so an unchanged same-size residue file is not re-checksummed on every 60s poll — any change to L/R/size misses the cache and re-checksums. If the call fails, its files are reported as `verified: false` conflicts (never cached) and the rest of the status check still succeeds.
- **Degrade path.** Falls back entirely to the pre-1.32.0 counts (reusing the push dry-run already fetched, so this never doubles the SSH round-trips) in two cases, never guessing a class: the metadata call itself fails (e.g. this rsync build rejects `--out-format` outright), or its `--out-format` output does not parse (missing `%M`, unknown shape — the stock-macOS-openrsync false-positive history).
- **UI — reachable from any lit badge, not only conflicts.** `SyncStatusResult` carries `conflicts` (path, local/remote size+mtime, `verified`), `git_count`, `remote_behind` (bool — at least one file classified as remote-stale), and `by_top_dir` (per-top-directory push/pull/git/stale-remote counts) — all `#[serde(default)]`, so an old cached shape still deserializes. Clicking **any** lit badge overlay (push count, pull count, or the `⚠ n` conflict badge — the conflict badge itself still shows only when `n > 0`) opens the same read-only "Sync Changes" popover - two equal columns, PUSH (local side, `--color-local`) and PULL (remote side, `--color-remote`), each listing its top-level directories with counts, plus a conflict list below - with one **Explain** button that opens interactive `agy --model <slug> --mode plan -i "<prompt>"` (agy's own UI, streaming, steerable) in its own in-app terminal tab (on demand only, never on the 60s poll; `--mode plan` makes agy read-only by mechanism; the model and the editable briefing prompt come from AI Settings, default `gemini-3.8-flash-high`) to brief the owner on the whole picture — project and both absolute roots, per-class/per-directory breakdown, whether the remote is behind, and a unified diff per conflict with its full local and remote path — in plain language: descriptive only, no recommended side, no confidence score, no action buttons. The agy invocation facts (flag order, `--mode plan`, model slugs) are recorded in [harness-facts.md — Cross-CLI worker](https://github.com/lacvietanh/akidevrule/blob/79fb6695a64254df91fd61e1318b3b8ec5d5eac3/skills/akiflow/references/harness-facts.md#cross-cli-worker-claude-code-lead--agy-headless). Explain is disabled with its reason in its tooltip when no `agy` binary resolves, checked via a cheap pre-flight command before any click. Full design: `docs/plan/conflict-detection-and-agy-report.md`.

### Post-sync UI State

After a full successful sync (`dry_run=false`, `specificPaths` empty), the app updates button state immediately - no recheck, no timeout. The state is derived from rsync's return code and the sync semantics, not from a follow-up poll.

**Merge mode** (`--delete` OFF): sync is additive/unidirectional. Only the synced direction clears.  
**Mirror mode** (`--delete` ON): both sides become identical. Both directions clear.

| Case | pushCount / hasPendingPush | pullCount / hasPendingPull |
|---|---|---|
| Merge PUSH (`delete_on_push=false`) | **0 / false** | unchanged |
| Mirror PUSH (`delete_on_push=true`) | **0 / false** | **0 / false** |
| Merge PULL (`delete_on_pull=false`) | unchanged | **0 / false** |
| Mirror PULL (`delete_on_pull=true`) | **0 / false** | **0 / false** |

**Why mirror PUSH clears pullCount:** rsync `-avz --delete` makes remote = exact copy of local (overwrites remote-newer files, deletes remote-only files). Remote has nothing new to offer local → pullCount = 0.

**Why mirror PULL clears pushCount:** rsync `-avz --delete` makes local = exact copy of remote (overwrites local-newer files, deletes local-only files). Local has nothing new to offer remote → pushCount = 0.

**Dry run / partial sync:** state is not modified - a dry run changes nothing on disk; a partial sync only addresses specific files, not the full direction. The 60s background poll handles state updates for these cases.

Code: `src/composables/useSync.js` → `startSync` post-success block.

## 3. Logs & Hooks
- **Logs**: Every sync action outputs standard `rsync` logs into the Project Log panel, including the Local and Remote rsync versions. Every real or dry sync, confirmed deploy, deploy-config change and remote-host change is also written to `usage.log` regardless of debug mode (`logger::audit`, `docs/arch/logger.md`) - the record of which host a push actually went to.
- **Hooks**: You can configure Pre/Post Push and Pull shell scripts (e.g., restarting a service or running `npm install`). These hooks can execute locally or remotely via SSH based on the `run_hooks_on_remote` flag. **Hooks are sync-only** - preparing or finishing the sync itself on one host (install deps, rebuild after files land). Shipping the project to production is a separate action, DEPLOY (`docs/feat/deploy.md`), never a hook side effect (`docs/plan/deploy-action.md`).
