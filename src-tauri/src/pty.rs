// In-app terminal — PTY backend (docs/plan/1.20.0-terminal-and-remote-sync.md §4, T-1..T-8).
//
// One shared PTY for the whole app process (T-3): the phone is a dumb terminal surface onto the SAME shell the Mac's own TerminalView drives, not a second independent session. `pty_spawn` is therefore idempotent — whichever screen (host or companion, via the `invoke` seam) opens the Terminal tab first actually spawns the shell; every later call is a no-op that returns the already-running session. `cwd` (T-8) is only honoured on that first spawn.
//
// BINARY-SAFE TRANSPORT: PTY output is not guaranteed valid UTF-8 at chunk boundaries (a multi-byte UTF-8 sequence, e.g. from `ls` on a filename with accented characters, can be split across two `read()`s). This module never treats PTY bytes as a Rust `String` — it carries them as base64 end-to-end (Tauri event payload AND the WS `pty_output`/`pty_input` frames), decoding only at the very edge (`pty_write`) or not at all (scrollback is stored as raw bytes and re-encoded to base64 on read). The frontend decodes base64 to a `Uint8Array` and feeds it to `xterm.js`'s `Terminal.write()`, which accepts binary and — per its own docs — maintains a stateful UTF-8 decoder across `write()` calls, so a sequence split at a chunk boundary is reassembled correctly by xterm itself. This is why nothing in this module or in `usePtyTerminal.js` needs its own split-sequence buffering.
//
// NEVER-BLOCK-THE-UI (CLAUDE.md ABSOLUTE, plan §4.3): `pty_spawn`/`pty_write`/`pty_resize`/`pty_get_scrollback` are all `async fn` wrapping their work in `spawn_blocking`, same as every other command in this app. The PTY READ LOOP is the one deliberate exception — it is a dedicated `std::thread::spawn`, started once from inside `pty_spawn`'s blocking closure, NOT `spawn_blocking` and NOT a tokio task. `spawn_blocking`'s pool is sized for bounded one-shot work; parking one of its threads forever in a `reader.read()` loop for the app's whole lifetime would starve every other blocking command (`resolve_remote_path`, `check_for_updates`, `get_git_info`, …) of a slot. `portable-pty`'s reader/writer are synchronous `Read`/`Write` trait objects, not `AsyncRead`, so a tokio task would block a worker thread just as badly — a raw OS thread is the only shape that is both correct and does not starve anything else. The same reasoning covers the flusher thread each read loop starts (see `flusher_loop`).
//
// State lives in a process-global `OnceLock`, same pattern as `web_server::RELAY` and `system::PROJECT_ICONS` — a struct of `std::sync::Mutex` fields rather than one big Mutex, since the PTY session (writer/master/child) and the scrollback ring buffer are locked independently and at different frequencies (every keystroke vs. every read).

// COMPILED AND TESTED ON MAC. This file once carried a "VERIFY ON MAC" caveat because `portable-pty = "0.9"`'s API was used here from knowledge of the crate rather than a resolved docs build; `cargo check` and `cargo test --lib` have since both passed on this machine, so every shape it listed as assumed is confirmed real: `SlavePty::spawn_command` returning `Box<dyn Child + Send + Sync>`, `MasterPty::get_size`, `CommandBuilder::cwd`/`arg`/`env`, and `Child::kill`/`wait`/`process_id`.
use base64::{engine::general_purpose::STANDARD, Engine as _};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// Scrollback ring-buffer cap (plan §4.3 / file table row 2): bounds memory for a long-running shared session (e.g. a `npm run build` left running for hours) without needing a UI-facing "clear scrollback" affordance in this MVP.
const SCROLLBACK_CAP: usize = 256 * 1024;

/// Read-loop coalescing thresholds (plan §4.4): flush on ~20ms elapsed OR ~16KB accumulated, whichever comes first, so a build's stdout firehose does not become one WS message per `read()` syscall.
const FLUSH_INTERVAL: Duration = Duration::from_millis(20);
const FLUSH_BYTES: usize = 16 * 1024;

/// The pending-output accumulator, shared by a reader thread and the flusher thread that serves it.
///
/// WHY IT IS SHARED STATE AND NOT A LOCAL (the "fast typing does not appear" bug): the coalescing test used to live entirely inside the read loop, so the ONLY thing that could ever flush was another `reader.read()` returning. Output arriving less than `FLUSH_INTERVAL` after the previous flush therefore failed the test, stayed in the accumulator, and the loop went straight back to blocking in `read()` — where nothing can time it out. On an idle shell "the next read" may be minutes away or never, so a keystroke echo landing within 20ms of the previous one was invisible until unrelated output happened to arrive. A deadline is only enforceable by something that is NOT parked in the blocking read, which is what `flusher_loop` is.
struct OutBuf {
    acc: Vec<u8>,
    /// When the last non-empty flush went out — the coalescing clock, now readable by both threads.
    last_flush: Instant,
    /// Set by the reader thread as it exits so its flusher does not outlive the session it serves.
    done: bool,
}

struct PtySession {
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    /// Kept so `pty_kill`/`pty_restart` can actually terminate the shell, and so dropping the session tears the child down rather than leaking an orphaned process.
    child: Box<dyn Child + Send + Sync>,
    /// Which spawn this session is — see `PtyState::generation`.
    generation: u64,
}

struct PtyState {
    session: StdMutex<Option<PtySession>>,
    scrollback: StdMutex<Vec<u8>>,
    /// Incremented on every real spawn. A reader thread only tears down `session` if the session currently in the slot is still the one IT was reading — otherwise a slow EOF from the shell killed by `pty_restart` would race in and null out the brand-new session that replaced it, leaving the terminal permanently dead with no error anywhere. This counter is the whole reason restart is safe to spam.
    generation: AtomicU64,
}

static PTY: OnceLock<PtyState> = OnceLock::new();

fn pty_state() -> &'static PtyState {
    PTY.get_or_init(|| PtyState {
        session: StdMutex::new(None),
        scrollback: StdMutex::new(Vec::new()),
        generation: AtomicU64::new(0),
    })
}

/// The user's own login shell, same resolution `Terminal.app`/a login shell would use. This app ships macOS-only (CLAUDE.md); `$SHELL` is set on every macOS account, `/bin/zsh` (the macOS default since Catalina) is the fallback for the rare case it is unset.
fn shell_bin() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string())
}

fn append_scrollback(bytes: &[u8]) {
    let mut buf = pty_state().scrollback.lock().unwrap();
    buf.extend_from_slice(bytes);
    if buf.len() > SCROLLBACK_CAP {
        let excess = buf.len() - SCROLLBACK_CAP;
        buf.drain(0..excess);
    }
}

#[derive(Serialize, Clone)]
struct PtyOutputPayload {
    /// base64 — see module doc comment "BINARY-SAFE TRANSPORT".
    data: String,
    /// `true` = "this payload replaces everything on screen", not "append it". Set by `pty_clear`/`pty_restart` so BOTH screens wipe in one step: the host's own TerminalView listens to this event directly, and `services/ptyBridge.js` forwards the same flag on the `pty_output` frame. Without it, clearing on one screen would leave the other showing a scrollback the host no longer has.
    reset: bool,
}

/// Emits accumulated bytes as one `pty-output` Tauri event and clears the accumulator. No-op on an empty accumulator (the read loop's final post-EOF call, if nothing was pending).
///
/// MUST BE CALLED WITH THE `OutBuf` MUTEX HELD — that is the whole ordering guarantee. Two threads emit through this function (the reader on the byte/interval fast path, the flusher on the deadline), and because each call drains the entire accumulator and emits it without ever releasing the lock in between, the sequence of `pty-output` payloads is exactly the sequence of bytes the PTY produced: no interleaving, no byte emitted twice, none dropped.
fn flush_locked(app: &AppHandle, buf: &mut OutBuf) {
    if buf.acc.is_empty() {
        return;
    }
    let payload = PtyOutputPayload { data: STANDARD.encode(&buf.acc[..]), reset: false };
    // AppHandle::emit is thread-safe and callable from a raw thread — same shape as the existing `sync-log` emit in src-tauri/src/sync.rs. `services/ptyBridge.js` (host-only) listens for this and relays it to companions as a `pty_output` WS frame; the host's own TerminalView listens for it directly for lowest latency (plan §4.4 wire-path diagram).
    let _ = app.emit("pty-output", payload);
    buf.acc.clear();
    buf.last_flush = Instant::now();
}

/// The deadline half of the coalescer, one raw thread per reader thread — see `OutBuf` for the bug this exists to fix.
///
/// It sleeps on the condvar while there is nothing pending (an idle shell costs zero wakeups), and the moment the reader parks a sub-threshold chunk it wakes, waits out only the remainder of the current `FLUSH_INTERVAL` window, and emits. Worst-case latency for any byte is therefore one `FLUSH_INTERVAL`, whether or not the shell ever produces another byte.
///
/// A RAW `std::thread` FOR THE SAME REASON THE READ LOOP IS ONE (module doc comment): it is parked for the session's whole lifetime, so putting it on `spawn_blocking`'s pool — sized for bounded one-shot work — would hold a slot hostage exactly as the reader would. It exits when its reader sets `done`, so the thread count stays 1:1 with live sessions no matter how hard RESTART is spammed.
fn flusher_loop(app: AppHandle, shared: Arc<(StdMutex<OutBuf>, Condvar)>) {
    let (lock, cv) = &*shared;
    let mut buf = lock.lock().unwrap();
    loop {
        while buf.acc.is_empty() && !buf.done {
            buf = cv.wait(buf).unwrap();
        }
        if buf.done {
            // The reader already flushed the tail before setting this; the call is a no-op unless it lost a race, in which case it is what keeps those last bytes from being dropped.
            flush_locked(&app, &mut buf);
            return;
        }
        // Wait out the REST of the current window before emitting. This has to be a loop, not a single `wait_timeout`: the reader calls `notify_one` for every sub-threshold chunk, so a single wait would be cut short by the very next keystroke or log line and emit at once — which would defeat coalescing exactly when there is something to coalesce, turning a chatty build back into one message per `read()` (the thing FLUSH_INTERVAL exists to prevent). Re-checking the deadline on each wake is the standard condvar predicate loop, and it also absorbs spurious wakeups for free.
        //
        // `wait_timeout` releases the lock while parked, so the reader keeps accumulating into the same window rather than stalling — blocking the reader here would apply backpressure to the shell itself.
        loop {
            if buf.done {
                break;
            }
            let since = buf.last_flush.elapsed();
            // Crossing FLUSH_BYTES mid-window is the reader's own fast path, but it can also happen while this thread holds the deadline; break out and emit rather than sitting on an oversized buffer.
            if since >= FLUSH_INTERVAL || buf.acc.len() >= FLUSH_BYTES {
                break;
            }
            buf = cv.wait_timeout(buf, FLUSH_INTERVAL - since).unwrap().0;
        }
        flush_locked(&app, &mut buf);
    }
}

/// Tells every screen "wipe what you have, this is the new whole content" (usually empty).
fn emit_reset(app: &AppHandle, bytes: &[u8]) {
    let _ = app.emit("pty-output", PtyOutputPayload { data: STANDARD.encode(bytes), reset: true });
}

/// End-of-session notice appended to the scrollback (so a screen that opens the tab later still sees WHY the terminal is idle) and rendered dim-red by the terminal itself via SGR. This is the fix for the 1.20.0 "ssh, exit, exit → the terminal just sits there dead" report: the shell exiting used to be completely invisible and unrecoverable.
const EXIT_NOTICE: &[u8] = b"\r\n\x1b[2m\x1b[31m[process exited - press any key or click RESTART to start a new shell]\x1b[0m\r\n";

/// The dedicated reader thread — see module doc comment for why this is a raw `std::thread`, not `spawn_blocking`. Runs for the PTY's whole lifetime; exits when the shell exits (EOF) or the pipe errors — at which point it TEARS DOWN the session slot (guarded by `generation`) and emits `pty-exit`, so the next `pty_spawn` really spawns instead of no-opping onto a corpse.
fn read_loop(app: AppHandle, mut reader: Box<dyn Read + Send>, generation: u64) {
    let mut read_buf = [0u8; 8192];
    let shared = Arc::new((
        StdMutex::new(OutBuf { acc: Vec::new(), last_flush: Instant::now(), done: false }),
        Condvar::new(),
    ));
    {
        let shared = Arc::clone(&shared);
        let app = app.clone();
        std::thread::spawn(move || flusher_loop(app, shared));
    }
    let (lock, cv) = &*shared;
    loop {
        match reader.read(&mut read_buf) {
            Ok(0) => break, // EOF — shell process exited
            Ok(n) => {
                // Outside the OutBuf lock on purpose: scrollback has its own mutex and its own (much cheaper) contention profile, and no path anywhere holds both at once.
                append_scrollback(&read_buf[..n]);
                let mut buf = lock.lock().unwrap();
                buf.acc.extend_from_slice(&read_buf[..n]);
                if buf.acc.len() >= FLUSH_BYTES || buf.last_flush.elapsed() >= FLUSH_INTERVAL {
                    // Fast path, unchanged: past the window already, so emit inline with zero added latency rather than paying a thread hop for it.
                    flush_locked(&app, &mut buf);
                } else {
                    // Inside the window — hand the tail to the flusher, which owns the deadline this thread cannot service once it is back inside a blocking `read()`.
                    cv.notify_one();
                }
            }
            Err(_) => break,
        }
    }
    // The reader emits its own tail rather than delegating it, so these bytes are guaranteed to go out BEFORE the EXIT_NOTICE emitted further down — a flusher racing on the same lock could otherwise put the notice ahead of the shell's last line.
    {
        let mut buf = lock.lock().unwrap();
        flush_locked(&app, &mut buf);
        buf.done = true;
    }
    cv.notify_one();

    // Only retire the slot if it still holds OUR session — see `PtyState::generation`.
    let state = pty_state();
    let mut guard = state.session.lock().unwrap();
    let is_ours = guard.as_ref().map(|s| s.generation == generation).unwrap_or(false);
    if !is_ours {
        return;
    }
    *guard = None;
    drop(guard);

    append_scrollback(EXIT_NOTICE);
    let payload = PtyOutputPayload { data: STANDARD.encode(EXIT_NOTICE), reset: false };
    let _ = app.emit("pty-output", payload);
    // Separate signal from the notice bytes: the frontend needs to flip its own alive state (to enable "type anything to respawn" and colour the tab), which it cannot infer from output.
    let _ = app.emit("pty-exit", ());
}

/// Spawns the one shared PTY if it does not already exist; a no-op returning `Ok(())` on every later call (T-3). `cwd` (T-8) is only honoured on the first, actual spawn — no UI passes it yet, but the signature is settled now so the DEV/BUILD-redirect follow-up (plan §7) never needs a breaking change. Initial size is a placeholder 80x24; the host's `usePtyTerminal.js` calls `pty_resize` immediately after mount once it knows the real fit (T-4).
#[tauri::command]
pub async fn pty_spawn(app: AppHandle, cwd: Option<String>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || spawn_if_absent(app, cwd))
        .await
        .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

/// The actual spawn, synchronous. Called only from inside a `spawn_blocking` closure. Takes the session lock itself and no-ops if a live session is already in the slot (T-3 idempotency).
fn spawn_if_absent(app: AppHandle, cwd: Option<String>) -> Result<(), String> {
    let state = pty_state();
    let mut guard = state.session.lock().unwrap();
    if guard.is_some() {
        return Ok(());
    }

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| format!("failed to open pty: {}", e))?;

    // `-l` (login shell): without it the shell skips .zprofile/.bash_profile, so nvm/rbenv/path_helper never run and the in-app terminal has a different PATH from every Terminal.app window the user has ever opened — the same class of surprise as the AG-over-ssh bug fixed in this release. A login shell is what "the terminal you are already using" means.
    let mut cmd = CommandBuilder::new(shell_bin());
    cmd.arg("-l");
    // Marks this shell for anything that cares (prompt customisation, statusline scripts) and matches what Terminal.app-launched shells see.
    cmd.env("TERM", "xterm-256color");
    if let Some(dir) = cwd {
        cmd.cwd(dir);
    }
    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("failed to spawn shell: {}", e))?;
    // The slave side is only needed to spawn the child; dropping it here (standard portable-pty usage) lets the master side see EOF correctly when the shell exits.
    drop(pair.slave);

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("failed to clone pty reader: {}", e))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|e| format!("failed to take pty writer: {}", e))?;

    let generation = state.generation.fetch_add(1, Ordering::SeqCst) + 1;
    *guard = Some(PtySession { writer, master: pair.master, child, generation });
    drop(guard); // release before handing `app` to the new thread

    std::thread::spawn(move || read_loop(app, reader, generation));
    Ok(())
}

/// SIGHUP → SIGKILL the shell's ENTIRE process group, not just the shell process itself.
///
/// WHY: `portable_pty`'s `Child::kill()` signals the direct child pid only — the login shell. Anything the user started INSIDE that shell (`ssh host`, a dev server, …) is a separate process in the shell's process group, and a signal aimed at the shell alone need not reach it. An `ssh` that survives is the expensive case: the client holds its TCP session open, so the REMOTE `sshd` session and whatever was left running under it (`agy`, `claude` — 300–500 MB of RSS each) stay alive on a machine this app is no longer talking to.
///
/// HOW MUCH THIS BUYS, HONESTLY — do not let the paragraph above oversell it. `killing_the_shell_takes_processes_started_inside_it_with_it` still passes with this function reduced to a plain `kill` on the shell, verified by mutation. When a session leader holding a controlling terminal dies, the kernel SIGHUPs the foreground process group by itself, so an ordinary foreground child dies either way. What `killpg` adds is coverage for what the kernel does not do: a process that has left the foreground group, and any teardown path where the ctty is not revoked. The load-bearing fix is the `RunEvent::Exit` hook (see `shutdown`), not this function.
///
/// THIS WAS *NOT* THE CAUSE OF THE ORPHANED `ssh` CLIENTS INVESTIGATED IN 1.20.0, and an earlier version of this comment wrongly claimed it was. Those were traced to hand-typed `ssh` commands in `Terminal.app` tabs that were later closed — matched to the second against `~/.zsh_history`, and conclusively excluded from this code by the fact that the then-installed app binary contained no PTY symbols at all. Kept here so the false attribution is not reconstructed from the code later.
///
/// `portable-pty` puts the child in its own session on unix (`setsid` + `TIOCSCTTY` — that is what makes it a controlling terminal), so the child's pid IS its process-group id and `killpg(pid, …)` reaches every descendant. SIGHUP first — the same signal closing a real terminal window sends, which `ssh` and virtually every CLI honours by exiting cleanly — then SIGKILL for anything still alive after the grace period. The grace loop polls with signal 0 (existence check, sends nothing) so the common case returns in ~25ms rather than always stalling the caller for the full budget; that matters because app-exit goes through here.
#[cfg(unix)]
fn kill_process_group(pid: u32) {
    let pgid = pid as libc::pid_t;
    unsafe { libc::killpg(pgid, libc::SIGHUP) };
    for _ in 0..12 {
        std::thread::sleep(Duration::from_millis(25));
        // Non-zero from a signal-0 probe means the group is gone — nothing left to escalate to.
        if unsafe { libc::killpg(pgid, 0) } != 0 {
            return;
        }
    }
    unsafe { libc::killpg(pgid, libc::SIGKILL) };
}

/// Kills the current shell (if any) and drops the session so the slot is free. The reader thread for that session will hit EOF shortly after and find the slot already empty / a newer generation in place — both handled, see `read_loop`'s tail.
fn kill_current() {
    let state = pty_state();
    let mut guard = state.session.lock().unwrap();
    if let Some(session) = guard.as_mut() {
        // Group first, then the child itself — see `kill_process_group`. `child.kill()` stays as the backstop for the non-unix path and for a child that somehow is not a group leader.
        #[cfg(unix)]
        if let Some(pid) = session.child.process_id() {
            kill_process_group(pid);
        }
        let _ = session.child.kill();
        let _ = session.child.wait();
    }
    *guard = None;
}

/// Tears the shared PTY down on app exit. Wired to `RunEvent::Exit` in `lib.rs`.
///
/// WHY THIS EXISTS: every other path into `kill_current` is a user gesture (`pty_kill`, `pty_restart`). Quitting the app ran nothing at all, so the whole process tree under the terminal — including any `ssh` the user left connected — survived the app that spawned it. That, not the usage-polling SSH (which is bounded and always passes a remote command), is what accumulated the orphans described in `kill_process_group`.
pub fn shutdown() {
    kill_current();
}

/// Explicit "close this shell". The frontend shows the same `[process exited]` state it would for a shell that exited on its own — nothing here is a special case downstream.
#[tauri::command]
pub async fn pty_kill(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        kill_current();
        append_scrollback(EXIT_NOTICE);
        let _ = app.emit("pty-output", PtyOutputPayload { data: STANDARD.encode(EXIT_NOTICE), reset: false });
        let _ = app.emit("pty-exit", ());
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))
}

/// Kill + wipe scrollback + spawn a fresh shell, as one atomic user gesture (the RESTART button, and the implicit respawn when someone types into an exited terminal). Emits a `reset` so every screen — host and companions alike — clears at the same moment instead of the phone keeping a dead shell's output above a live one's prompt.
#[tauri::command]
pub async fn pty_restart(app: AppHandle, cwd: Option<String>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        kill_current();
        pty_state().scrollback.lock().unwrap().clear();
        emit_reset(&app, b"");
        spawn_if_absent(app, cwd)
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

/// Wipes the scrollback ring buffer without touching the running shell, and tells every screen to clear. Distinct from the shell's own `clear`, which only scrolls the visible screen away and leaves the host's buffer (and therefore any phone that reconnects) full of the old output.
#[tauri::command]
pub async fn pty_clear(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        pty_state().scrollback.lock().unwrap().clear();
        emit_reset(&app, b"");
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))
}

/// The shell's CURRENT working directory — i.e. wherever the user has `cd`'d to, not where the shell started. Powers the "open in Terminal.app" button (VS Code's external-terminal action): the point is to hand off the exact directory you are standing in.
///
/// macOS has no `/proc`, so `lsof` is the supported way to read another process's cwd. Failure is not an error worth surfacing — the caller falls back to `$HOME`.
#[tauri::command]
pub async fn pty_cwd() -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(|| -> Option<String> {
        let pid = {
            let guard = pty_state().session.lock().unwrap();
            guard.as_ref().and_then(|s| s.child.process_id())?
        };
        // -Fn = machine-readable, one field per line prefixed by its type; `n` is the path.
        #[cfg(target_os = "macos")]
        let found = std::process::Command::new("/usr/sbin/lsof")
            .args(["-a", "-d", "cwd", "-p", &pid.to_string(), "-Fn"])
            .output()
            .ok()
            .and_then(|out| {
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .find(|l| l.starts_with('n'))
                    .map(|l| l[1..].to_string())
            })
            .filter(|p| !p.is_empty());
        #[cfg(not(target_os = "macos"))]
        let found = std::fs::read_link(format!("/proc/{}/cwd", pid))
            .ok()
            .map(|p| p.to_string_lossy().to_string());
        found
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))
}

/// Writes companion/host keystrokes into the PTY. `data` is base64 (see module doc comment).
#[tauri::command]
pub async fn pty_write(data: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let bytes = STANDARD.decode(&data).map_err(|e| format!("invalid base64 input: {}", e))?;
        let state = pty_state();
        let mut guard = state.session.lock().unwrap();
        let session = guard.as_mut().ok_or_else(|| "no PTY session — call pty_spawn first".to_string())?;
        session.writer.write_all(&bytes).map_err(|e| format!("pty write failed: {}", e))?;
        session.writer.flush().map_err(|e| format!("pty flush failed: {}", e))
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

/// T-4: the host is the SOLE caller of this command. A companion never invokes it — its xterm is resized only by the `pty_resize` echo (`FRAME_PTY_RESIZE`) the host broadcasts after calling this, from `usePtyTerminal.js`'s host branch. Enforcing that is a frontend-side discipline (nothing in this command can distinguish "called from the host's own UI" from "called via the companion invoke seam" — see final report for why that residual gap is accepted, not a bug).
#[tauri::command]
pub async fn pty_resize(cols: u16, rows: u16) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let state = pty_state();
        let guard = state.session.lock().unwrap();
        let session = guard.as_ref().ok_or_else(|| "no PTY session — call pty_spawn first".to_string())?;
        session
            .master
            .resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| format!("pty resize failed: {}", e))
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

#[derive(Serialize)]
pub struct PtyScrollback {
    /// base64 — see module doc comment "BINARY-SAFE TRANSPORT".
    data: String,
    /// The PTY's CURRENT size (defaults 80x24 if no session exists yet). Lets a screen that opens the Terminal tab mid-session (most commonly a companion joining after the host has already resized the shared PTY several times) apply the right `term.resize()` on hydrate instead of waiting for the next `pty_resize` echo, which only fires when the HOST's own window next resizes (T-4) — that could be minutes away or never in a session.
    cols: u16,
    rows: u16,
    /// Is there a live shell right now? A screen opening the tab needs this to decide between "normal terminal" and "exited — offer a restart"; it cannot be inferred from the scrollback bytes, and getting it wrong is exactly the 1.20.0 hang the user hit.
    alive: bool,
}

/// Returns the whole ring buffer plus the PTY's current size, so a screen opening (or rejoining) the Terminal tab can hydrate — both content and dimensions — before subscribing to live `pty-output`/`pty_output` frames.
#[tauri::command]
pub async fn pty_get_scrollback() -> Result<PtyScrollback, String> {
    tauri::async_runtime::spawn_blocking(|| -> Result<PtyScrollback, String> {
        let state = pty_state();
        let data = {
            let buf = state.scrollback.lock().unwrap();
            STANDARD.encode(&buf[..])
        };
        let (cols, rows, alive) = {
            let guard = state.session.lock().unwrap();
            let alive = guard.is_some();
            match guard.as_ref().map(|s| s.master.get_size()) {
                Some(Ok(size)) => (size.cols, size.rows, alive),
                _ => (80, 24, alive),
            }
        };
        Ok(PtyScrollback { data, cols, rows, alive })
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use portable_pty::native_pty_system;

    fn alive(pid: i32) -> bool {
        // Signal 0 sends nothing; it only asks "may I signal this pid". 0 = the process exists.
        unsafe { libc::kill(pid, 0) == 0 }
    }

    /// Direct children of `pid`, as reported by `ps`. Used to find the grandchild without the shell having to cooperate.
    fn children_of(pid: i32) -> Vec<i32> {
        let out = std::process::Command::new("/bin/ps").args(["-eo", "pid=,ppid="]).output().expect("ps");
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| {
                let mut it = l.split_whitespace();
                let c: i32 = it.next()?.parse().ok()?;
                let p: i32 = it.next()?.parse().ok()?;
                if p == pid { Some(c) } else { None }
            })
            .collect()
    }

    /// The whole point of `kill_process_group`, as an executable claim rather than a comment: a process the user started INSIDE the shell must die when the terminal is torn down.
    ///
    /// The grandchild must be a SEPARATE pid from the shell (hence `sleep 300` with no `exec`), because the shell's own pid is the only thing `portable_pty`'s `Child::kill()` reaches.
    ///
    /// WHAT THIS TEST DOES *NOT* PROVE, stated so nobody reads more into a green run than is there: it still passes with `killpg` replaced by a plain `kill` on the shell alone. That is not a flaw in the test, it is a fact about unix — when a session leader with a controlling terminal dies, the kernel SIGHUPs the foreground process group itself, so a well-behaved foreground child dies either way. Verified by mutation, not assumed. This test therefore guards the END-TO-END property ("after teardown, nothing the user started inside the terminal is left running"), which is the property that actually matters, and NOT the claim that `killpg` is what delivers it. `killpg` earns its place on the cases the kernel does not cover — a child that has left the foreground group, or one that outlives a teardown path where the ctty is not revoked — and this test does not exercise those.
    #[test]
    fn killing_the_shell_takes_processes_started_inside_it_with_it() {
        let pair = native_pty_system()
            .openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 })
            .expect("openpty");
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.env("TERM", "dumb");
        let child = pair.slave.spawn_command(cmd).expect("spawn shell");
        drop(pair.slave);
        let shell_pid = child.process_id().expect("shell pid") as i32;

        // Drain the master, or the shell can block writing its own echo into a full pipe.
        let mut reader = pair.master.try_clone_reader().expect("reader");
        std::thread::spawn(move || {
            let mut sink = [0u8; 4096];
            while reader.read(&mut sink).map(|n| n > 0).unwrap_or(false) {}
        });

        let mut writer = pair.master.take_writer().expect("writer");
        // NO `exec` — the shell must fork, so the sleep gets its own pid inside the shell's process group. That is the shape of the bug.
        writer.write_all(b"sleep 300\n").expect("write");
        writer.flush().expect("flush");

        let mut grandchild = None;
        for _ in 0..40 {
            std::thread::sleep(Duration::from_millis(50));
            if let Some(&c) = children_of(shell_pid).first() {
                grandchild = Some(c);
                break;
            }
        }
        let grandchild = grandchild.unwrap_or_else(|| panic!("setup failed: /bin/sh {} never forked a child", shell_pid));
        assert_ne!(grandchild, shell_pid, "setup failed: grandchild must be a distinct pid or the test proves nothing");
        assert!(alive(grandchild), "setup failed: grandchild is not running");

        kill_process_group(shell_pid as u32);

        // Reap the shell so nothing below can pass on a zombie, which `kill(pid, 0)` still reports as alive. The grandchild is reparented to init on its parent's death and reaped there, so it needs no wait of ours.
        let mut child = child;
        let _ = child.wait();

        for _ in 0..40 {
            if !alive(grandchild) {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        // Do not leave a stray `sleep 300` behind if the assertion is about to fail.
        unsafe { libc::kill(grandchild, libc::SIGKILL) };
        panic!("grandchild {} survived kill_process_group — the SSH/agy/claude leak is back", grandchild);
    }
}
