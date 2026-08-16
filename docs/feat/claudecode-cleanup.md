# Claude Code Cleanup (Local)

> updated 2026-08-16 · v1.24.0

**Where**: app-icon menu (☰) → `Claude Code Cleanup (Local)`. Host-only - hidden on a paired phone.
**Code**: `src-tauri/src/claude_cleanup.rs` · `src/components/modals/ClaudeCleanupModal.vue` · `src/utils/bytes.js`
**Path catalogue & per-path impact**: `docs/ref/claudecode-cleanup-paths.md`
**Plan**: `docs/plan/done/claudecode-cleanup.md`

Answers two questions the CLI does not: *where did my disk go*, and *how do I sign out or start clean without losing my rules and skills*. It is not a general disk cleaner - it only ever touches paths this app names explicitly.

## What the modal shows

Five groups, each with its total size.

| Group | Deletable | What it holds | What you lose |
|---|---|---|---|
| **Account** | yes | `~/.claude.json`, `.credentials.json`, `auth-cache.json`, `stats-cache.json`, `rate-limits-cache.json`, `daemon-auth-status.json`, `daemon-auth-cooldown` | Signed out; `claude` asks you to log in again |
| **Data** | yes | `projects/` transcripts (minus every `memory/`), `history.jsonl`, `file-history/`, `sessions/`, `paste-cache/`, `plans/`, `session-env/`, `tasks/`, `shell-snapshots/`, `downloads/` | Chat transcripts, prompt history, and the ability to undo a file Claude edited. Usually the biggest group |
| **Agent memory** | yes | every `projects/*/memory/` | Everything the agent has remembered across sessions |
| **Cache** | yes | `cache/`, `backups/`, `plugins/`, `telemetry/`, `debug/`, `daemon.log`, `~/Library/Caches/claude-code/` | Nothing - regenerates on demand |
| **Kept** | **no** | `settings.json`, `settings.local.json`, `config.json`, `CLAUDE.md`, `CLAUDE.local.md`, `*.aki*` backups, `skills/`, `hooks/`, `statusline-command.sh`, plus an **Unlisted** aggregate | - |

## Selecting

Selection is **per path**. Clicking a group row expands it into its individual entries, each with its own checkbox and size; a path that does not exist shows `-` and cannot be ticked. The group's own checkbox is a shortcut over that group's entries and shows three states - empty, a dash when some are ticked, filled when all are. Nothing is ever selected as a unit bigger than one path, so "delete everything" is available and so is "delete just `file-history/`".

The delete button is two-state: the first click arms it and restates the amount, the second performs the delete. Changing the selection disarms it, so the second click can never delete a different set than the first click showed.

**Agent memory is deletable like anything else** - it simply sits in a group of its own rather than inside Data. It is authored content living in an otherwise disposable tree, and the mental model behind clicking "Data" is *clear my chat history*, not *erase what the agent knows*. Putting a group boundary between them makes that exclusion structural instead of a special case someone has to remember when adding the next Data entry, while leaving a clean slate exactly one click away. A unit test asserts the memory group holds exactly one entry and that no Data entry can reach memory.

## Safety

**Deny by default: a path is deletable only if it appears literally in the Rust catalogue.**

The frontend never sends a path - it sends catalogue *keys*, and the backend's `resolve` is the only thing that turns a key into a filesystem path. An unrecognized key is refused and logged, not interpreted. This is what puts `~/.claude/skills/` and the Aki rule corpus structurally out of reach, rather than merely unchecked: a mis-bound checkbox or a hostile frame has no path to express. Note the boundary this draws: it protects the app's *rules and configuration*, not the user's own choices - anything in a deletable group goes if it is ticked, memory included.

Three consequences, all deliberate:

- **Unknown state is kept, not deleted.** The live `~/.claude/` tree already carries directories the reference doc never listed (`sessions/`, `daemon/`, `chrome/`, `ide/`, `jobs/`) - the CLI grows state faster than a hand-written list tracks. Those appear under **Unlisted** with their real size and are never offered for deletion. Under-deleting is recoverable; over-deleting is not.
- **`projects/` is split, never deleted as one unit.** Clearing transcripts empties each `projects/<slug>/` around its `memory/`, and the Data group's reported size already excludes those bytes so it never promises space it will not free. A slug directory that still holds memory keeps its folder. Tick the memory group too and both go, including the slug folder left empty behind them.
- **Not reachable from a paired phone.** Both commands are absent from `COMPANION_ALLOWED_COMMANDS` (`src/services/hostInvoke.js`), and the menu row itself sits inside the header's `nativeWindow` block - same treatment as the statusline and profile rows, which also write this Mac's own config.

A per-entry failure is collected and reported, never raised: one unreadable directory does not abort a cleanup the user already confirmed. The modal shows the freed total and the first error.

## Sizes

Rust returns raw `u64` bytes and never formats. `src/utils/bytes.js` owns the presentation, including the one genuinely platform-dependent part: macOS Finder counts a GB as 1000³, Windows Explorer as 1024³ while still printing "GB". `detectByteBase()` follows the host so the number agrees with the user's own file manager. The app ships macOS-only, so the Windows branch is written but unreachable until a Windows bundle exists - shipping one is a build-target change, not a hunt for hardcoded `1000`s.

Sizing walks the tree with `symlink_metadata` and never follows a symlink, so a link pointing outside `~/.claude` can neither inflate a number nor drag the walk into an unrelated tree. Both commands are `async fn` + `spawn_blocking`: `projects/` and `file-history/` routinely hold tens of thousands of files, and that walk on the IPC dispatch thread would freeze the window.

## After a cleanup

Deleting the Account group removes the very files the app's own Claude Code usage monitor reads (`auth-cache.json`, `rate-limits-cache.json` - see `docs/arch/usage-claudecode.md`). The modal fires a manual refresh afterwards so the usage panel does not keep rendering quota numbers whose source file no longer exists.

## Verification status

`cargo test --lib` covers the catalogue shape (no absolute or `..` path, unique keys, no protected path reachable), memory being reachable only through its own single-entry group, and both directions of the `projects/` split end to end on a temp tree. A live `scan` against this machine's real `~/.claude` was run read-only and resolves correctly, including an existing-but-empty `memory/` reading as present at 0 B. **The destructive path has not been run against a real `~/.claude`** - that is user-triggered and is the one thing static reading cannot settle.
