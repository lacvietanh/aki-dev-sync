# Plan - Claude Code Cleanup (Local)

**Status**: IMPLEMENTED (W1-W6 all landed), awaiting runtime verification per §6
**Ships in**: `[Unreleased]` (release state at plan time: *Unreleased open*, `package.json` 1.21.0 == CHANGELOG top - no bump in this task)
**Input spec**: `docs/ref/claudecode-cleanup-paths.md` - path catalogue and impact-per-path for Claude Code CLI leftovers
**Feature doc on completion**: `docs/feat/claudecode-cleanup.md`

---

## 1. What it is

A modal, opened from the app-icon menu, that shows how much disk each category of Claude Code CLI
state occupies and lets the user delete whole categories. It answers two questions the CLI itself
never answers: *where did my disk go*, and *how do I sign out / start clean without losing my rules
and skills*.

Not a general disk cleaner. It only ever touches paths this app names explicitly.

## 2. The one invariant everything else follows from

**Deny by default: a path is deletable only if it appears literally in the Rust catalogue.**

The frontend never sends a path. It sends *entry keys*; Rust resolves each key against a static
table and refuses anything else. This is what makes `~/.claude/skills/` structurally unreachable
rather than merely "not currently checked" - the failure mode being designed out is a UI bug (a
mis-bound checkbox, a companion frame) turning into `rm -rf` on the user's rule corpus.

Two consequences worth stating because they are choices, not accidents:

- **Anything the catalogue does not know about is kept, not deleted.** The live `~/.claude/` tree
  already contains directories the reference doc never listed (`sessions/`, `daemon/`, `chrome/`,
  `ide/`, `jobs/`, `debug/`, `downloads/`) - the CLI adds state faster than any hand-written list
  tracks. So the modal shows those under a read-only **Unlisted** line with their size, and never
  offers to delete them. Under-deleting is recoverable; over-deleting is not.
- **`~/.claude/projects/` is never deleted as one unit**, contrary to §2 of the reference doc. It
  contains `projects/<slug>/memory/` - the agent's persistent memory, which is authored content, not
  transcript. The two are split into separate entries in separate groups: clearing transcripts
  empties each slug folder *around* its `memory/`, and memory has its own entry that deletes it (and
  drops the slug folder once nothing is left). Memory is fully deletable - it is simply never
  deletable *by accident*, which is why it does not sit inside the Data group where a select-all
  would sweep it up. The reference doc is corrected in the same task.

## 3. Groups shown in the modal

Five groups. Selection is **per entry** - every individual path has its own checkbox, and the group
checkbox is only a shortcut that ticks or unticks that group's own entries (tri-state: none / some /
all). Nothing is a unit larger than one path.

| Group | Deletable | Contents | What the user loses |
|---|---|---|---|
| **Account** | yes | `~/.claude.json`, `.credentials.json`, `auth-cache.json`, `stats-cache.json`, `rate-limits-cache.json`, `daemon-auth-status.json`, `daemon-auth-cooldown` | Signed out. `claude` prompts to log in again. |
| **Data** | yes | `projects/` (transcripts only, minus every `memory/`), `history.jsonl`, `file-history/`, `sessions/`, `paste-cache/`, `plans/`, `session-env/`, `tasks/`, `shell-snapshots/`, `downloads/` | Chat history, prompt history, and the ability to undo a file Claude edited. Usually the largest group. |
| **Agent memory** | yes | every `projects/*/memory/` | Everything the agent has remembered across sessions. |
| **Cache** | yes | `cache/`, `backups/`, `plugins/`, `telemetry/`, `debug/`, `daemon.log`, `~/Library/Caches/claude-code/` | Nothing. Regenerates on demand. |
| **Kept** | **no** | `settings.json`, `settings.local.json`, `config.json`, `CLAUDE.md`, `CLAUDE.local.md`, `*.aki*` backups, `skills/`, `hooks/`, `statusline-command.sh`, plus an **Unlisted** aggregate for everything else | - |

**Why memory gets a group of its own rather than a row inside Data.** It is authored content sitting
in a directory tree that is otherwise disposable, so any "select all" over that tree would sweep it
up - and the user's mental model of that click is "clear my chat history", not "erase what the agent
knows". A group boundary makes the exclusion structural instead of a special case someone has to
remember when adding the next Data entry. A unit test asserts the memory group holds exactly one
entry and that no Data entry can reach memory.

## 4. Sizes

Rust returns **bytes** per entry and per group; it never formats. The frontend converts.

Unit base is platform-conditional because the two platforms genuinely disagree about what "GB"
prints as: macOS Finder is decimal (1000), Windows Explorer is binary (1024) while still labelling
it `GB`. `src/utils/bytes.js` owns that decision in one place - `formatBytes()` plus a
`detectByteBase()` whose Windows branch is written now and unreachable until a Windows bundle ships
(the app is macOS-only today; see `CLAUDE.md` § THIS PROJECT).

## 5. Work items

- **W1 - `src-tauri/src/claude_cleanup.rs`**: the catalogue, a recursive size walk, two commands.
  - `scan_claude_cleanup() -> Vec<CleanupGroup>` - sizes every catalogue entry plus the unlisted aggregate.
  - `run_claude_cleanup(keys: Vec<String>) -> CleanupReport` - deletes only resolved keys, returns freed bytes and per-entry errors.
  - Both `async fn` + `spawn_blocking`. A recursive walk of a several-hundred-MB `projects/` tree is
    exactly the "must not run on the IPC dispatch thread" case (`RULE-stack-tauri` A1); the rule's
    "small local file read is fine" exemption does not cover a tree walk of unbounded size.
  - `projects/` splits into two kinds per §2: `Kind::ProjectsDir` (transcripts, skips `memory/`) and
    `Kind::MemoryDirs` (every `memory/`, then drops each slug folder left empty).
  - Registered in `lib.rs`. No `capabilities/default.json` change - custom commands need no grant,
    only core/plugin APIs do (`RULE-stack-tauri` B2 applies to the latter).
- **W2 - `src/utils/bytes.js`**: `formatBytes()` + `detectByteBase()` per §4.
- **W3 - `src/components/modals/ClaudeCleanupModal.vue`**: group rows with a tri-state checkbox +
  size, expanding into per-entry checkboxes (multi-select at path granularity), `Kept` group rendered
  dimmed with a lock and no checkboxes. Follows the narrow-UI principle: no banner rows, no status
  bar - the delete button itself carries the confirm state, and the memory group's warning reads as a
  border tint rather than an extra row.
- **W4 - `src/components/AppHeader.vue`**: one menu row under `Claude Code Profile (Local)`, inside
  `<template v-if="nativeWindow">`. Host-only for the same reason the profile and statusline rows
  are: it writes this Mac's filesystem, and `services/hostInvoke.js` keeps such commands off the
  companion allowlist, so on a phone it could only ever open a modal whose action fails. The two new
  commands are **not** added to `COMPANION_ALLOWED_COMMANDS`.
- **W5 - post-cleanup refresh**: deleting the Account group invalidates what the app's own usage
  monitors read (`auth-cache.json`, `rate-limits-cache.json` - see `docs/arch/usage-claudecode.md`).
  The modal triggers a usage refresh after a successful run so the panel does not keep rendering
  numbers whose source file no longer exists.
- **W6 - docs**: `docs/feat/claudecode-cleanup.md`, `docs/index.md` entry, `CHANGELOG.md`
  `[Unreleased] → Added`, and the two corrections to `docs/ref/claudecode-cleanup-paths.md` (the
  `memory/` carve-out, and a note that the live tree carries entries the doc does not list).
  `README.md` + `IntroModal.vue` per `CLAUDE.md` § THIS PROJECT.

## 6. Verification

- ✅ `cargo test --lib` - 181 passed, including 7 new: catalogue shape (no absolute path, no `..`,
  unique keys), no protected path reachable through any key, unknown keys resolve to nothing, memory
  reachable only through its own single-entry group, and both directions of the `projects/` split
  exercised end to end on a temp tree (clearing transcripts leaves memory and its folder; clearing
  memory leaves transcripts; doing both leaves no hollow slug folder behind).
- ✅ Live `scan` run against this machine's real `~/.claude` (read-only): every group resolves,
  `file-history/` is the 70 MB the feature exists to surface, and an existing-but-empty `memory/`
  correctly reads as present at 0 B rather than absent.
- ✅ `npm run build` - frontend compiles clean.
- **Runtime, user-triggered, not done by the agent**: actually running a delete against a real
  `~/.claude`. The destructive path is the one thing static reading cannot settle
  (`RULE-coding` B3), so this plan is not "done" until a Mac run confirms it - the feature doc
  records that state honestly rather than claiming verified.

## 7. Deliberately out of scope

- Deleting the `claude` binary or its npm/homebrew install. Uninstalling the CLI is a different
  operation from clearing its state, and mixing them into one button is how a "free up disk" click
  becomes "my CLI is gone".
- Per-project selective cleanup (delete transcripts for one project only). Real want, but the group
  granularity has to prove itself first - YAGNI.
- A scheduled/automatic cleanup. Destructive work stays user-initiated.
