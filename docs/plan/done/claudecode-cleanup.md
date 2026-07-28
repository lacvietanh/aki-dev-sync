# Plan - Claude Code Cleanup (Local)

**Status**: ✅ DONE — W1-W6 all landed, runtime verified
**Ships in**: `[Unreleased]` (no version bump in this task; `package.json` 1.21.0)
**Input spec**: `docs/ref/claudecode-cleanup-paths.md`
**Feature doc**: `docs/feat/claudecode-cleanup.md`

---

## 1. What it is

A modal (opened from the app-icon menu) showing disk usage per Claude Code CLI category, letting the user delete whole categories. Answers: *where did my disk go* and *how do I sign out / start clean without losing rules and skills*.

Not a general disk cleaner — only touches paths named explicitly in the catalogue.

## 2. Core invariant

**Deny by default: a path is deletable only if it appears literally in the Rust catalogue.**

The frontend sends *entry keys*; Rust resolves each key against a static table and refuses anything else. This makes `~/.claude/skills/` structurally unreachable — a UI bug cannot become `rm -rf` on the rule corpus.

Two design choices:
- **Unknown paths are kept, shown as read-only Unlisted.** The live `~/.claude/` tree has directories the reference doc never listed (`sessions/`, `daemon/`, `chrome/`, etc.) — under-deleting is recoverable, over-deleting is not.
- **`~/.claude/projects/` is never deleted as one unit.** It contains `projects/<slug>/memory/` (authored content, not transcript). The two split into separate entries in separate groups: transcripts empty each slug folder *around* its `memory/`; memory has its own group entry. Memory is fully deletable — just never by accident.

## 3. Groups

Five groups. Selection is per entry; group checkbox is tri-state shortcut only.

| Group | Deletable | Contents | What the user loses |
|---|---|---|---|
| **Account** | yes | `~/.claude.json`, `.credentials.json`, `auth-cache.json`, `stats-cache.json`, `rate-limits-cache.json`, `daemon-auth-status.json`, `daemon-auth-cooldown` | Signed out |
| **Data** | yes | `projects/` (transcripts only, minus `memory/`), `history.jsonl`, `file-history/`, `sessions/`, `paste-cache/`, `plans/`, `session-env/`, `tasks/`, `shell-snapshots/`, `downloads/` | Chat history, prompt history, file-undo ability |
| **Agent memory** | yes | every `projects/*/memory/` | Everything the agent remembered across sessions |
| **Cache** | yes | `cache/`, `backups/`, `plugins/`, `telemetry/`, `debug/`, `daemon.log`, `~/Library/Caches/claude-code/` | Nothing — regenerates on demand |
| **Kept** | **no** | `settings.json`, `settings.local.json`, `config.json`, `CLAUDE.md`, `CLAUDE.local.md`, `*.aki*` backups, `skills/`, `hooks/`, `statusline-command.sh`, + Unlisted aggregate | — |

Memory gets its own group (not a row in Data) so any select-all over Data cannot sweep it up. A unit test asserts the memory group holds exactly one entry and no Data entry can reach memory.

## 4. Sizes

Rust returns **bytes** per entry and per group; never formats. The frontend converts via `src/utils/bytes.js` → `formatBytes()` + `detectByteBase()`. Unit base is platform-conditional: macOS Finder = decimal (1000), Windows Explorer = binary (1024). Windows branch is written but unreachable until a Windows bundle ships.

## 5. Work items

- **W1 - `src-tauri/src/claude_cleanup.rs`**: catalogue, recursive size walk, two `spawn_blocking` commands:
  - `scan_claude_cleanup() -> Vec<CleanupGroup>`
  - `run_claude_cleanup(keys: Vec<String>) -> CleanupReport`
  - `Kind::ProjectsDir` (transcripts, skips `memory/`) + `Kind::MemoryDirs` (every `memory/`, drops empty slug folders)
  - Registered in `lib.rs`. No `capabilities/default.json` change.
- **W2 - `src/utils/bytes.js`**: `formatBytes()` + `detectByteBase()`
- **W3 - `src/components/modals/ClaudeCleanupModal.vue`**: group rows with tri-state checkbox + size, expanding per-entry. Kept group dimmed with lock, no checkboxes. Delete button carries the confirm state; memory group warning is a border tint.
- **W4 - `src/components/AppHeader.vue`**: one menu row under `Claude Code Profile (Local)`, inside `<template v-if="nativeWindow">`. Host-only (writes this Mac's filesystem). Not added to `COMPANION_ALLOWED_COMMANDS`.
- **W5 - post-cleanup refresh**: deleting Account invalidates `auth-cache.json` / `rate-limits-cache.json`. Modal triggers a usage refresh after a successful run.
- **W6 - docs**: `docs/feat/claudecode-cleanup.md`, `docs/index.md`, `CHANGELOG.md [Unreleased] → Added`, corrections to `docs/ref/claudecode-cleanup-paths.md`, `README.md` + `IntroModal.vue`.

## 6. Verification

- ✅ `cargo test --lib` — 181 passed, including 7 new: catalogue shape, no protected path reachable through any key, unknown keys resolve to nothing, memory reachable only via its own group, both directions of the `projects/` split on a temp tree.
- ✅ Live `scan` against this machine's real `~/.claude` (read-only): every group resolves, `file-history/` is the 70 MB the feature exists to surface, existing-but-empty `memory/` reads as 0 B (not absent).
- ✅ `npm run build` — frontend compiles clean.
- ✅ Runtime (user-triggered): delete run against a real `~/.claude` — confirmed.

## 7. Deliberately out of scope

- Uninstalling the `claude` binary — separate from clearing its state.
- Per-project selective cleanup — group granularity must prove itself first.
- Scheduled/automatic cleanup — destructive work stays user-initiated.
