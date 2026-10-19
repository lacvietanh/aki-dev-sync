# PUSH sync froze the whole app: static diagnosis

Status: open, runtime checks R1-R3 pending on the Mac.

## 1. Start time and purpose

Started 2026-10-19 against HEAD `2d8567f` (release 1.33.0). Question: an owner note (`task-1791428905665`, pinned) records that a PUSH sync once froze the entire app, which breaks the "never block the UI" rule in `CLAUDE.md`. Which code path can freeze the window during a PUSH, and is the rule violated anywhere?

Constraints at the time: Linux dev box, Rust does not compile there, no Mac or phone, the original freeze was not reproduced or logged. Everything below is static reading unless marked otherwise.

## 2. Strategy and checklist

- Sweep every `#[tauri::command]` for the rule: a command that runs a subprocess, SSH or network call must be `async fn` with `spawn_blocking`.
- Trace the PUSH path end to end: click handler, `run_sync`, `spawn_and_stream`, `stream_reader`, event, listener, log store, render.
- Test five hypotheses (H1-H5) against the code.
- Change code only where a hypothesis has a concrete mechanism.

## 3. Result

### 3.1 Sweep

0 of 86 commands violate the `spawn_blocking` rule. 22 are plain `fn` commands (`save_projects` in `src-tauri/src/projects.rs:238`, the `src-tauri/src/ssh.rs` commands, `log_frontend`, `pty_write`, `pty_rename_tab`, `pty_set_tab_pinned`, `cleanup_legacy_baselines` in `src-tauri/src/sync.rs:684`, the `src-tauri/src/system.rs` string builders); none waits on a subprocess, SSH or the network.

### 3.2 Hypotheses

| # | Hypothesis | Verdict | Evidence |
| :-- | :-- | :-- | :-- |
| H1 | A plain `fn` command on the main thread waits on a lock held across blocking work | Not supported | Locks those commands take (`LOG_LOCK` `src-tauri/src/logger.rs:24,67`, `pty_state().inputs` `src-tauri/src/pty.rs:922`, `tab_meta` `src-tauri/src/pty.rs:216,509`) are held for one file append, one map get or one channel send. The locks held across blocking calls (`host_lock` `src-tauri/src/remote_shell.rs:46-47`, `RSYNC_VERSIONS` `src-tauri/src/sync.rs:897`) are taken only by worker or blocking-pool threads, never by a sync command. `SYNC_PROCS`, `SYNC_INFLIGHT`, `CANCELLED` (`src-tauri/src/sync.rs:180-262`) hold for map ops only; `spawn_and_stream` holds no lock across `child.wait()` (`src-tauri/src/sync.rs:402-421`) |
| H2 | A large push floods the webview with one `sync-log` event per line and the listener does O(buffer) work per event | Supported mechanism, impact unmeasured | `stream_reader` (`src-tauri/src/sync.rs:352-383`) emits per stdout/stderr line from two threads, no batching. The listener (`src/composables/useLogs.js`) called `appendLog` per event: `pushCapped` push + `splice` on a reactive array of up to `LOG_CAP` 2000 (`src/store/logStore.js:34-38`), `scrollConsole` forcing layout per event, `src/components/dock/LogStack.vue:40` re-patching 2000 index-keyed rows, and deep watchers in `src/services/mirror.js:259-266` re-traversing all retained lines. Only plausible for large transfers; a small push emits a handful of lines |
| H3 | Heavy synchronous frontend work on the click path | Low | The confirm dialog (`src/composables/useSync.js`) builds one `<pre>` from the at-risk list by linear string join; a one-shot cost on `--delete` or overwrite pushes, not a freeze for the duration of a push |
| H4 | An async command does blocking work before its `spawn_blocking`, or holds a std `MutexGuard` across `.await` | Not supported | Pre-spawn regions are validation, a momentary slot acquire and a cancel check (`src-tauri/src/sync.rs:784-795`); the SMB-prone `ensure_local_path_present` is inside `spawn_blocking` (`src-tauri/src/sync.rs:798`). No guard alive across an `.await`. No custom runtime or `max_blocking_threads`, so the default blocking pool is not exhausted by one push |
| H5 | Other: hooks, deploy-after-push, remote poll loop, boot work | Not supported | Hooks run inside the same `spawn_blocking` (`src-tauri/src/sync.rs:432-453`); deploy-after-push goes through async `pty_spawn` (`src-tauri/src/pty.rs:473-490`); the `src-tauri/src/remote_shell.rs:81-107` poll loop is bounded and not on the PUSH path; boot migrations (`src-tauri/src/lib.rs:25-38`) are launch-only |

Side finding, no UI effect: the `RSYNC_VERSIONS` lock is held across an ssh call of up to 10 s, which serializes other sync and status tasks on a cold or dead host. Pool threads only.

### 3.3 Verification

Static reading and greps only. After the change below, `npm run build` passed (`built in ~11s`). No runtime evidence exists for either the freeze or the fix. Not verified: whether Tauri `emit` queues main-thread work at thousands of events per second (assumption), and whether the original freeze was a large push.

Corroborating links: `CLAUDE.md` § NEVER BLOCK THE UI; `src-tauri/src/git.rs` `get_git_info` as the canonical correct pattern.

## 4. Change made

`src/composables/useLogs.js`: `sync-log` lines are queued per project and flushed every 50 ms (`SYNC_LOG_FLUSH_MS`) through one `appendProjectLogLines` call and one scroll, so store update, splice, scroll and deep-watch run at most about 20 times per second. `appendLog` flushes pending lines first so a direct log line cannot jump ahead of rsync output; Clear drops the active project's pending lines. A `setTimeout` is used rather than `requestAnimationFrame` because rAF stops while the window is hidden and the buffer would grow unbounded.

Not changed: the Rust event contract, `src/store/logStore.js`, and `src/components/dock/LogStack.vue` (rows are still index-keyed, so each flush repatches the rows; cost is per flush instead of per line). Keying rows by a monotonic line id remains available if R2 shows the render path is still too slow.

## 5. Runtime checks for the Mac

| # | Check | Settles |
| :-- | :-- | :-- |
| R1 | `npm run tauri dev`, Safari Web Inspector on the `localhost` target (not the inspector's own `Main.html`), Timelines recording during a PUSH of at least 1000 changed files, then a PUSH of about 10 files. Before the batching: main-thread bands tied to `sync-log` handlers on the large push only. After: the large push streams smoothly, console stays scrolled to the bottom, Clear mid-push empties it and later lines still appear | H2 and the fix |
| R2 | Temporarily time the listener body with `performance.now()`; mean above about 4 ms at 2000 retained lines means the render path still needs id-keyed rows | Remaining render cost |
| R3 | Only if R1 shows no frontend cost: during a freeze run `sample "Aki Dev Sync" 3` (or Activity Monitor Sample Process); a stack in a `Mutex::lock` or `recv` frame names the lock | H1 |

The owner note stays open until R1 passes.

## 6. Decision

Action: batching landed in `src/composables/useLogs.js` (changelog `[Unreleased]`); the backlog row #7 in `docs/plan/backlog.md` tracks R1. No claim that the freeze is fixed until R1 is run.

### UNIDOC lesson (proposal for the Mac-side UNIDOC Tauri, not applied)

> Event floods are the second UI-freeze class, beside blocking commands: a streaming subprocess must not emit one IPC event per output line into a listener that does O(buffer) work per event. Batch on the receiving side (one flush per timer tick into a capped list) or on the emitting side (coalesce lines for about 50 ms). Review rule: for every `emit` inside a read loop, name the per-event cost of its listener and the maximum events per second.

Cross-references: `docs/plan/backlog.md` #7, `CHANGELOG.md` `[Unreleased]`.
