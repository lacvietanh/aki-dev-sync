# Sync Flow (Push, Pull, Select)

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

**Consequence:** The status checker (`count_rsync_changes`) must count **additive transfers only** - files the source has to send to the destination. `deleting …` lines from rsync must be excluded from the count. See `sync.rs → count_rsync_changes`.

**Resolved (Tier 2 Baseline Manifest - v1.7.0):**  
rsync is stateless. Without extra context it cannot distinguish "remote created file X" from "local deleted file X" (PULL ambiguity), nor "Mac created file Y" from "remote deleted file Y" (PUSH ambiguity). Both issues are now resolved using a local baseline snapshot written to `{appDataDir}/baselines/{project_id}.json` after every full sync. See §2 Sync Status Checker below for the full reclassification logic.

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
  - **Safety Guard**: If `delete_on_push` is ON and the app detects pending Pull changes (i.e., the AI generated new files remotely), clicking Push will trigger a **SweetAlert2 confirmation dialog**. This prevents accidental destruction of AI-generated work.

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
- The app polls the sync status in the background every 60 seconds (or on-demand via the Refresh button).
- It runs a silent `rsync --dry-run` to detect changes.
- **Button Glow**: If there are changes to Push, the PUSH button lights up. If there are changes to Pull, the PULL button lights up.
- **Additive-only count (CRITICAL):** The status checker counts only **transfer lines** from rsync output - lines representing content the source has to offer the destination. `deleting …` lines are **excluded** from the count. A deletion listed in a PULL dry-run means "local has a file remote doesn't" - that is the opposite direction's signal, not incoming remote content. Including it caused PULL to light incorrectly when the remote was empty. See `docs/research/sync-button-semantic-analysis.md` for full analysis.
- **Tier 2 Baseline Reclassification (v1.7.0):** After every full successful sync, a snapshot of the local file list is written to `{appDataDir}/baselines/{project_id}.json`. On the next status check, both PUSH and PULL lists are filtered against this baseline:

  | Case | rsync sees | Baseline says | Classification |
  |------|-----------|---------------|----------------|
  | PULL file + in baseline + absent locally | remote has X, Mac doesn't | X existed at last sync | Mac deleted X → `push_count` |
  | PULL file + not in baseline | remote has X, Mac doesn't | X is new | Remote created X → `pull_count` |
  | PUSH file + in baseline | Mac has X, remote doesn't | X existed at last sync | Remote deleted X → suppress from `push_count` |
  | PUSH file + not in baseline | Mac has X, remote doesn't | X is new | Mac created X → `push_count` |

  The PUSH-side suppression is especially important for workflows where most coding happens on the remote server - without it, every file deleted on the remote would falsely light the PUSH badge on Mac.

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
- **Logs**: Every sync action outputs standard `rsync` logs into the Project Log panel, including the Local and Remote rsync versions.
- **Hooks**: You can configure Pre/Post Push and Pull shell scripts (e.g., restarting a service or running `npm install`). These hooks can execute locally or remotely via SSH based on the `run_hooks_on_remote` flag.
