# Sync Flow (Push, Pull, Select)

This document covers the core synchronization capabilities of Aki Dev Sync, designed to support the **Lạc Việt Anh Workflow** where the Local machine acts as the Source of Truth and the Remote acts as the AI Engine.

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
- Opens a modal listing all modified, untracked, or deleted files (retrieved via `git status`).
- You can multi-select specific files to push to the Remote.
- **Why?**: When Claude has already worked on a feature, you may want to push only a single modified file (e.g., a config fix) without waiting for a full directory scan, dramatically saving time.

## 2. Dry Run & Status Indicator

### DRY RUN Toggle
- A global toggle per project that enables `--dry-run` for `rsync`.
- When ON, clicking Push or Pull will only simulate the operation and print exactly what would happen in the logs, without modifying any files on disk.

### Sync Status Checker
- The app polls the sync status in the background every 60 seconds (or on-demand via the Refresh button).
- It runs a silent `rsync --dry-run` to detect changes.
- **Button Glow**: If there are changes to Push, the PUSH button lights up. If there are changes to Pull, the PULL button lights up.
- **Deletions Support**: The status checker correctly accounts for `deleting ` output from rsync. Therefore, if you delete a file locally and `delete_on_push` is ON, the PUSH button will light up indicating there is a pending deletion to sync.

## 3. Logs & Hooks
- **Logs**: Every sync action outputs standard `rsync` logs into the Project Log panel, including the Local and Remote rsync versions.
- **Hooks**: You can configure Pre/Post Push and Pull shell scripts (e.g., restarting a service or running `npm install`). These hooks can execute locally or remotely via SSH based on the `run_hooks_on_remote` flag.
