# Terminal session restore across quit/relaunch: the restorable boundary

## Start time

2026-10-27 01:39 (Asia/Saigon), against `900d4e5` (1.35.0).

## Initial purpose

Backlog #5 / note `task-1789604852739`: quitting the app and opening it again loses every in-app terminal tab. Question: what can be restored after a relaunch, what cannot, and where the restored record lives so it does not become a second source of truth.

Context at the time:
- `TabMeta` (`project_id`, `title`, `pinned`) already outlives a webview reload, in Rust memory only (`src-tauri/src/pty.rs:61-67`); `adoptTabs` (`src/store/terminalTabsStore.js:129`) rebuilds the tab list from `pty_list_tabs` (`pty.rs:687`). Background: `research/terminal-tab-ownership-reload-loss.md`.
- Quitting runs `pty::shutdown` (`pty.rs:635`) → `kill_all_sessions` (`pty.rs:622`), which kills every tab's process group on purpose (`docs/arch/terminal-stack.md` § Killing the shell means killing its process group).
- `pty_spawn` is idempotent and applies `cwd` only on a tab's first real spawn (`pty.rs:403-410`); a mounted terminal calls it (`src/composables/usePtyTerminal.js:133`).

## Strategy

Static read of the PTY backend, the tab store and the arch doc; sort each piece of tab state by whether it survives the process boundary, and pick one writer and one reader for whatever does.

## Checklist

- [x] List the state a tab carries (record, metadata, process, scrollback, run command)
- [x] Check what `shutdown` destroys and in what order
- [x] Check the existing reload-adoption path for a reader to reuse
- [x] Decide the boundary and the single source

## Result

| State | Restorable | Why |
|---|---|---|
| Tab exists, its scope (`project_id`), title, pin | yes | already the durable `TabMeta`; only its lifetime ends at process exit |
| Last working directory | yes, best effort | not in `TabMeta` today; must be read while the shell still lives, so inside `shutdown` before `kill_all_sessions`, one batched `lsof` like `system.rs`'s terminal scan; on failure the spawn falls back to the scope's default directory |
| Active tab per scope | yes | screen navigation (`lastTabByScope`); restoring "the tab with the highest id per scope" is enough, no new field |
| The running process (shell, `ssh`, `claude`, dev server) | **no** | `shutdown` kills the process group by design; keeping it alive would mean a daemon outliving the app, the exact orphan class 1.20.0 investigated |
| Scrollback | no | 128 KiB per tab of output from a dead process, possibly holding secrets, written on every quit; no reader needs it once the shell is fresh |
| DEV/BUILD `pendingCmd`, `runKind` | no | re-running a command on launch is an action the user did not take |
| A Claude Code / agy conversation running in the tab | not by the app | the app records no session id (0 hits for `--continue`, `--resume`, `session_id` in `src/` and `src-tauri/src/`); the CLI keeps its own transcript, and restoring the directory is what lets the user's own `claude --continue` find the latest conversation of that folder (ASSUMPTION: vendor CLI behavior, not re-read in this research) |

Not every quit runs `shutdown`: `npm run build:app` ends the app with `killall` (`scripts/install-desktop.sh:106`). So the record must already be on disk before the quit; only the `cwd` snapshot depends on `shutdown` running.

A restored tab is a record with no session: `adoptTabs` shows it, and its first activation spawns a fresh login shell in the restored directory through the existing idempotent `pty_spawn`.

### Verification

Static reading only, at `900d4e5`. Not verified: that an adopted tab with no session and no scrollback spawns cleanly on first activation (today `pty_list_tabs` lists only tabs with a session or a scrollback, so this path does not exist yet), and the cost of one batched `lsof` at quit with many tabs. Both are checks for the implementation, with `npm run dev:agent` + `npm run ctl` (`docs/ref/agent-control.md`).

### Corroborating links

- `research/terminal-tab-ownership-reload-loss.md` — the reload fix this extends; rejected `localStorage` for the same reason as below
- `docs/arch/terminal-stack.md` § Tab metadata survives a reload, § Killing the shell means killing its process group, § PTY backend contract

## Decision

**Follow-up (implementation, not in this research):** persist `TabMeta` plus a `cwd` field to one file in the app data dir (`~/.aki/devsync/`, `docs/arch/settings-and-state.md`), written by `pty.rs` alone: on every `TabMeta` change, on `drop_tab_state` (one entry removed, never a whole-map clear) and once in `shutdown` with the `cwd` snapshot before the kill. On launch `pty.rs` loads it, `pty_list_tabs` includes those tabs with `alive: false`, and `adoptTabs` stays the only reader. The first real spawn of a restored id takes the restored `cwd` when the caller passes none. New fields carry `#[serde(default)]`.

Rejected:
- Frontend persistence (`localStorage` or a store ref): a second source beside `TabMeta`, per-webview, and `mirror.js` would ship it to companions.
- Restoring processes: contradicts the documented teardown.
- Restoring scrollback: see the table. Reopen if the owner wants the last screen of output back.
- Auto-typing `claude --continue` or a DEV/BUILD command into a restored tab: an action on the user's behalf.
- A quit confirmation while shells are live, instead of restore: it keeps nothing after a quit the user meant, and `killall` (`install-desktop.sh:106`) never shows it.

Reopen if the owner wants only pinned tabs restored, or wants scrollback.

Cross-references: `docs/plan/backlog.md` #5, `docs/arch/terminal-stack.md` (gains a section when this ships), `.akidevsync/notes.json` `task-1789604852739`.
