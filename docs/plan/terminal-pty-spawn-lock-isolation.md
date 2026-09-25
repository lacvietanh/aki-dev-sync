# Plan: Terminal PTY spawn must not hold the `sessions` lock

Status: implemented in `src-tauri/src/pty.rs` (2026-09-25); runtime checks in §5 still open.

## 1. Problem, as verified against the code

`spawn_if_absent` used to hold `PtyState::sessions` across `openpty()` and `spawn_command()`. Every other operation on existing tabs (`pty_resize`, `pty_cwd`, `pty_list_tabs`, `kill_session`, `pty_write`'s session lookup) queues behind that lock for as long as the spawn takes. `kill_session` already releases the lock before its 300ms kill loop for the same reason.

Verified facts:
- `portable-pty 0.9` `spawn_command` installs a `pre_exec` hook (`unix.rs:238`), so Rust std forks instead of `posix_spawn`, and the hook calls `close_random_fds()` which allocates (`read_dir`, `Vec`, `unix.rs:152`). Strictly this is not async-signal-safe.
- Nothing in the repo shows the deadlock ever happening. The "malloc heap-lock deadlock after fork" story is the generic POSIX hazard; on macOS, libmalloc installs fork handlers, and `portable-pty` is what WezTerm ships. Treat it as theoretical, not as the motivation.

The real, defensible reason for the change is lock hygiene: never hold a shared map lock across fork/exec, whose duration the app does not control.

## 2. Rejected from the first draft (Gemini report)

| Draft item | Why it is wrong |
| :--- | :--- |
| Cancel-on-close detected by `min_accepted[tab] == u64::MAX` | That value also means "closed id awaiting reuse" (the close fence, lifted only by `admit_generation` at commit). Reusing a closed id — the normal ⌘T after closing the last tab — would read as "closed mid-spawn" and kill the fresh shell. The draft's own EC-4 contradicts its check order. |
| `spawning` set whose second caller returns `Ok(())` immediately | The second caller returns before any shell exists; DEV/BUILD writes into the shell right after `pty_spawn` returns (comment in `spawn_if_absent`), so it would hit "no PTY session". Previously it blocked on the lock and then no-op'd with a live shell. |
| `kill_process_group` in the commit step while holding `sessions` | Up to 300ms under the very lock the plan exists to free; also no `wait`, leaving a zombie. |
| 10 s `tokio::time::timeout` around `spawn_blocking` | Does not cancel the blocking thread. The IPC would report an error while the shell is still created later, and the tab is stuck if the spawn never returns. The UI is not blocked by a pending async command anyway. |
| "Unit test: concurrent `spawn_if_absent` yields one session" | `spawn_if_absent` needs an `AppHandle`; the module's tests deliberately avoid spawning through it. Not testable there without a mock runtime. |
| The 7-roster council / "corroborated" claim | Not verifiable from the repo; dropped. |

## 3. Design that was kept

- `spawn_gate: Mutex<()>` — serializes spawns. Held across `spawn_command` deliberately; a second `pty_spawn` for the same tab waits, then finds the session and no-ops, preserving the old semantics. Different tabs serialize for milliseconds, which is acceptable; the point is that `sessions` stays free.
- `sessions` is taken only for the presence check and for the commit.
- `spawn_inflight: Mutex<Option<(TabId, bool)>>` — the tab being created and a "retired" flag. `kill_session` sets it while holding `sessions`; the commit reads it while holding `sessions`. A kill therefore either lands before the check (the new shell is reaped by `terminate_child`, outside the lock) or after the insert (finds and kills the session). Set in `kill_session`, not only `pty_close_tab`, so `pty_kill`/`pty_restart` mid-spawn are covered too, and the id-reuse fence is untouched.
- Lock order gains `sessions` → `spawn_inflight` (leaf); documented in the module header.
- `terminate_child` extracted from `kill_session` (second use).

## 4. Edge cases

| Case | Outcome |
| :--- | :--- |
| Close/kill/restart while spawning | Commit sees the retired flag, reaps the child, installs nothing. |
| Two spawns for one tab | Second waits on the gate, then no-ops on the live session. |
| Reused closed tab id | Flag is per in-flight spawn and starts `false`; the fence is not consulted, so the shell commits and `admit_generation` lifts the fence as before. |
| App exit while a shell is being created | `kill_all_sessions` retires the in-flight spawn in the same critical section as its snapshot, so the shell is either in the snapshot or reaped by its own commit (before this, the lock made exit wait for the spawn; without this the new shell would outlive the app — the 1.20 orphan class). |
| Restart while the tab is spawning | `pty_restart`'s `kill_session` retires the first spawn; the restart's own spawn waits on the gate and installs the replacement. |
| `openpty`/`spawn_command` error or panic | `ClearInflight` resets the slot; the gate is taken with poison recovery so one panic cannot disable all spawning. |

## 5. Verification

- Done: `cargo test --lib pty::` (11 pass, including `reused_tab_id_accepts_the_new_session_only` and the new `a_kill_retires_only_the_spawn_in_flight_for_that_tab`); `pty.rs` clean under `cargo clippy --all-targets -- -D warnings`. The lint gate itself still fails on unrelated pre-existing findings in `web_server.rs`, `statusline.rs`, `system.rs`, `agent_usage/antigravity_payload.rs`.
- Open (needs a running app): open a tab and close it within ~50ms, then confirm no orphaned shell for it in `ps`; open several tabs rapidly and confirm all appear; while a tab is opening, resize/type in an existing tab.
