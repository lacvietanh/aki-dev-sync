// In-app terminal PTY backend. Architecture and protocol: docs/plan/done/1.20.0-terminal-and-remote-sync.md §4 (T-1..T-8).
//
// Constraints: one shared PTY per `TabId`; missing IDs target tab 0 for companion compatibility
// (except destructive `pty_close_tab`); PTY bytes remain base64/raw until xterm decoding; blocking PTY
// work stays off the async runtime. Lock order is `sessions` → `inputs`, `scrollbacks` →
// `min_accepted`, `sessions` → `spawn_inflight`, and `OutBuf` → `min_accepted`; `min_accepted`, `spawn_inflight` and `tab_meta` are leaf locks,
// and no path may hold both `OutBuf` and `scrollbacks`. macOS builds and library tests verify the
// `portable-pty` API used here.
use base64::{engine::general_purpose::STANDARD, Engine as _};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// `u32`, not a newtype: a plain JSON number in Tauri command args, event payloads and the WS relay frames alike.
pub type TabId = u32;

/// The tab every id-less caller lands on — see the module doc comment's backward-compatibility note.
const DEFAULT_TAB: TabId = 0;

/// Per-tab ring cap; also what a joining companion receives (~175 KB base64 per tab). Each mounted xterm keeps its own 5000-line scrollback.
pub(crate) const SCROLLBACK_CAP: usize = 128 * 1024;

/// Read-loop coalescing thresholds (plan §4.4): flush on ~20ms elapsed OR ~16KB accumulated, whichever comes first, so a build's stdout firehose does not become one WS message per `read()` syscall.
const FLUSH_INTERVAL: Duration = Duration::from_millis(20);
const FLUSH_BYTES: usize = 16 * 1024;

/// Pending output shared by one session's reader and flusher: docs/arch/terminal-stack.md § Output: coalescing with a deadline.
struct OutBuf {
    acc: Vec<u8>,
    /// When the last non-empty flush went out — the coalescing clock, now readable by both threads.
    last_flush: Instant,
    /// Set by the reader thread as it exits so its flusher does not outlive the session it serves.
    done: bool,
}

/// One tab's input queue and the thread that owns its PTY writer: docs/arch/terminal-stack.md § Input: one queue, a synchronous `pty_write`.
struct InputChannel {
    /// Lets a failing writer retire only its own channel, never the one a `pty_restart` already installed.
    generation: u64,
    tx: std::sync::mpsc::Sender<Vec<u8>>,
}

struct PtySession {
    /// Redundant with the map key; kept so a session passed by value still knows its tab.
    #[allow(dead_code)]
    tab_id: TabId,
    master: Box<dyn MasterPty + Send>,
    /// Kept so `pty_kill`/`pty_restart` can actually terminate the shell, and so dropping the session tears the child down rather than leaking an orphaned process.
    child: Box<dyn Child + Send + Sync>,
    /// Which spawn this session is — see `PtyState::generation`.
    generation: u64,
}

/// Per-tab facts that outlive the shell and a webview reload: docs/arch/terminal-stack.md § Tab metadata survives a reload.
#[derive(Default, Clone)]
struct TabMeta {
    /// `None` = global scope. Set once, at the tab's first `pty_spawn`, and never reassigned after —
    /// nothing in this app's frontend ever moves a tab to a different project.
    project_id: Option<String>,
    /// `None` = no title recorded yet (frontend falls back to its own default, e.g. "Shell {id}").
    title: Option<String>,
    pinned: bool,
}

struct PtyState {
    /// Live sessions, keyed by tab. Absent key = "that tab has no shell right now" — the exact meaning `Option<PtySession>` carried before tabs existed.
    sessions: StdMutex<HashMap<TabId, PtySession>>,
    tab_meta: StdMutex<HashMap<TabId, TabMeta>>,
    /// Its own mutex, not a `PtySession` field, so a keystroke never waits behind a resize, `pty_cwd` or spawn holding `sessions`.
    inputs: StdMutex<HashMap<TabId, InputChannel>>,
    /// Outlives the session so the exit notice stays readable; only `drop_tab_state` removes an entry.
    scrollbacks: StdMutex<HashMap<TabId, Vec<u8>>>,
    /// Global across tabs, bumped per real spawn; a reader tears down its slot only if it still holds its generation, which makes RESTART safe to spam.
    generation: AtomicU64,
    /// Per-tab generation floor, a leaf lock: docs/arch/terminal-stack.md § Generations fence bytes, not only the slot.
    min_accepted: StdMutex<HashMap<TabId, u64>>,
    /// Serializes spawns and is held across `spawn_command`, so `sessions` need not be; a second `pty_spawn` for the tab waits here, then no-ops.
    spawn_gate: StdMutex<()>,
    /// Tab being spawned + whether `kill_session` retired it (the commit's cancel signal); not `min_accepted`, also `u64::MAX` for a reused id. Leaf lock.
    spawn_inflight: StdMutex<Option<(TabId, bool)>>,
}

static PTY: OnceLock<PtyState> = OnceLock::new();

fn pty_state() -> &'static PtyState {
    PTY.get_or_init(|| PtyState {
        sessions: StdMutex::new(HashMap::new()),
        tab_meta: StdMutex::new(HashMap::new()),
        inputs: StdMutex::new(HashMap::new()),
        scrollbacks: StdMutex::new(HashMap::new()),
        generation: AtomicU64::new(0),
        min_accepted: StdMutex::new(HashMap::new()),
        spawn_gate: StdMutex::new(()),
        spawn_inflight: StdMutex::new(None),
    })
}

/// The user's login shell; `/bin/zsh`, the macOS default, when `$SHELL` is unset.
fn shell_bin() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string())
}

/// Appends to one tab's ring: `Some(g)` bytes of a retired session are dropped, `None` (not session output) always lands. The floor is read under `scrollbacks`.
fn append_scrollback(tab_id: TabId, bytes: &[u8], generation: Option<u64>) {
    let state = pty_state();
    let mut all = state.scrollbacks.lock().unwrap();
    if let Some(g) = generation {
        // Lock order `scrollbacks` → `min_accepted`; the leaf is released before the append.
        let floor = state
            .min_accepted
            .lock()
            .unwrap()
            .get(&tab_id)
            .copied()
            .unwrap_or(0);
        if g < floor {
            return;
        }
    }
    let buf = all.entry(tab_id).or_default();
    buf.extend_from_slice(bytes);
    if buf.len() > SCROLLBACK_CAP {
        let excess = buf.len() - SCROLLBACK_CAP;
        buf.drain(0..excess);
    }
}

/// Fences one tab's sessions up to `generation`. Only `kill_session` calls it: a shell ending by EOF has no late writer to fence.
fn retire_generation(tab_id: TabId, generation: u64) {
    let state = pty_state();
    // Held for the store, not for anything inside it — see `append_scrollback` for why the lock is the ordering guarantee.
    let _all = state.scrollbacks.lock().unwrap();
    state
        .min_accepted
        .lock()
        .unwrap()
        .insert(tab_id, generation + 1);
}

/// Reads one tab's floor through the leaf lock only, so it is legal inside `flush_locked`, which holds `OutBuf`.
fn generation_accepted(tab_id: TabId, generation: u64) -> bool {
    let floor = pty_state()
        .min_accepted
        .lock()
        .unwrap()
        .get(&tab_id)
        .copied()
        .unwrap_or(0);
    generation >= floor
}

/// Forgets one tab (session, input, scrollback, floor), kills nothing. The floor goes to `u64::MAX` so late bytes cannot resurrect it; `admit_generation` lifts it.
fn drop_tab_state(tab_id: TabId) {
    let state = pty_state();
    state.sessions.lock().unwrap().remove(&tab_id);
    state.inputs.lock().unwrap().remove(&tab_id);
    state.scrollbacks.lock().unwrap().remove(&tab_id);
    state.min_accepted.lock().unwrap().insert(tab_id, u64::MAX);
    state.tab_meta.lock().unwrap().remove(&tab_id);
}

/// Raises a new session's tab floor to its own generation: lifts a reused id's close fence while every older session stays rejected.
fn admit_generation(tab_id: TabId, generation: u64) {
    pty_state()
        .min_accepted
        .lock()
        .unwrap()
        .insert(tab_id, generation);
}

/// Patches one tab's metadata: `None` keeps a field as recorded, so an older companion calling `pty_spawn` without ownership cannot erase it.
fn upsert_tab_meta(tab_id: TabId, project_id: Option<String>, title: Option<String>) {
    let mut all = pty_state().tab_meta.lock().unwrap();
    let entry = all.entry(tab_id).or_default();
    if project_id.is_some() {
        entry.project_id = project_id;
    }
    if title.is_some() {
        entry.title = title;
    }
}

#[derive(Serialize, Clone)]
struct PtyOutputPayload {
    /// Mandatory: every `TerminalView` filters on it, so a missing value would write one tab's output into another's xterm.
    tab_id: TabId,
    /// base64 — see module doc comment "BINARY-SAFE TRANSPORT".
    data: String,
    /// Replace, not append: `pty_clear`/`pty_restart` wipe this tab on every screen at once.
    reset: bool,
    /// Set only where liveness changes (spawn, kill, EOF), absent otherwise: docs/arch/terminal-stack.md § Liveness rides the output channel.
    #[serde(skip_serializing_if = "Option::is_none")]
    alive: Option<bool>,
}

/// The `pty-exit` payload. It used to be `()`; with tabs it MUST say which tab ended, or a screen would mark whichever tab it happens to be showing as dead on any other tab's exit.
#[derive(Serialize, Clone)]
struct PtyExitPayload {
    tab_id: TabId,
}

/// Emits and clears the accumulator, or drops it once `generation` is retired. Runs with the `OutBuf` mutex held: that lock is the byte-order guarantee.
fn flush_locked(app: &AppHandle, tab_id: TabId, buf: &mut OutBuf, generation: u64) {
    if buf.acc.is_empty() {
        return;
    }
    if !generation_accepted(tab_id, generation) {
        buf.acc.clear();
        return;
    }
    let payload = PtyOutputPayload {
        tab_id,
        data: STANDARD.encode(&buf.acc[..]),
        reset: false,
        alive: None,
    };
    // `services/ptyBridge.js` relays this to companions as `pty_output`; the host's `TerminalView` listens directly.
    let _ = app.emit("pty-output", payload);
    buf.acc.clear();
    buf.last_flush = Instant::now();
}

/// The coalescer's deadline half: a raw thread, parked for the session's life, that exits when its reader sets `done`.
fn flusher_loop(
    app: AppHandle,
    tab_id: TabId,
    shared: Arc<(StdMutex<OutBuf>, Condvar)>,
    generation: u64,
) {
    let (lock, cv) = &*shared;
    let mut buf = lock.lock().unwrap();
    loop {
        while buf.acc.is_empty() && !buf.done {
            buf = cv.wait(buf).unwrap();
        }
        if buf.done {
            // The reader already flushed the tail before setting this; the call is a no-op unless it lost a race, in which case it is what keeps those last bytes from being dropped.
            flush_locked(&app, tab_id, &mut buf, generation);
            return;
        }
        // A predicate loop, not one `wait_timeout`: each chunk's `notify_one` would cut a single wait short and defeat coalescing.
        loop {
            if buf.done {
                break;
            }
            let since = buf.last_flush.elapsed();
            if since >= FLUSH_INTERVAL || buf.acc.len() >= FLUSH_BYTES {
                break;
            }
            buf = cv.wait_timeout(buf, FLUSH_INTERVAL - since).unwrap().0;
        }
        flush_locked(&app, tab_id, &mut buf, generation);
    }
}

/// Reads the one true answer to "is there a shell on THIS TAB right now" straight off the session map. Never inferred from output bytes anywhere.
fn is_alive(tab_id: TabId) -> bool {
    pty_state().sessions.lock().unwrap().contains_key(&tab_id)
}

/// Tells every screen "wipe what you have on this tab, this is the new whole content" (usually empty), stamped with that tab's liveness at that instant.
fn emit_reset(app: &AppHandle, tab_id: TabId, bytes: &[u8]) {
    let alive = is_alive(tab_id);
    let _ = app.emit(
        "pty-output",
        PtyOutputPayload {
            tab_id,
            data: STANDARD.encode(bytes),
            reset: true,
            alive: Some(alive),
        },
    );
}

/// "This tab's shell came back": no bytes, no `reset`, sent after the spawn (docs/arch/terminal-stack.md § Liveness rides the output channel).
fn emit_alive(app: &AppHandle, tab_id: TabId) {
    let alive = is_alive(tab_id);
    let _ = app.emit(
        "pty-output",
        PtyOutputPayload {
            tab_id,
            data: String::new(),
            reset: false,
            alive: Some(alive),
        },
    );
}

/// Written into the scrollback so a later screen sees why the terminal is idle; it names only a control that is on screen.
const EXIT_NOTICE: &[u8] =
    b"\r\n\x1b[2m\x1b[31m[process exited. Press any key to start a new shell]\x1b[0m\r\n";

/// One tab's reader, a raw thread for the PTY's life. On EOF it frees the slot if it still holds its generation and emits `pty-exit`, so the next spawn is real.
fn read_loop(app: AppHandle, tab_id: TabId, mut reader: Box<dyn Read + Send>, generation: u64) {
    let mut read_buf = [0u8; 8192];
    let shared = Arc::new((
        StdMutex::new(OutBuf {
            acc: Vec::new(),
            last_flush: Instant::now(),
            done: false,
        }),
        Condvar::new(),
    ));
    {
        let shared = Arc::clone(&shared);
        let app = app.clone();
        std::thread::spawn(move || flusher_loop(app, tab_id, shared, generation));
    }
    let (lock, cv) = &*shared;
    loop {
        match reader.read(&mut read_buf) {
            Ok(0) => break, // EOF — shell process exited
            Ok(n) => {
                // Stamped with this reader's generation, so a restarted tab's dead shell cannot append above the new prompt.
                append_scrollback(tab_id, &read_buf[..n], Some(generation));
                let mut buf = lock.lock().unwrap();
                buf.acc.extend_from_slice(&read_buf[..n]);
                if buf.acc.len() >= FLUSH_BYTES || buf.last_flush.elapsed() >= FLUSH_INTERVAL {
                    // Fast path, unchanged: past the window already, so emit inline with zero added latency rather than paying a thread hop for it.
                    flush_locked(&app, tab_id, &mut buf, generation);
                } else {
                    // Inside the window — hand the tail to the flusher, which owns the deadline this thread cannot service once it is back inside a blocking `read()`.
                    cv.notify_one();
                }
            }
            Err(_) => break,
        }
    }
    // The reader emits its own tail so the shell's last line precedes the exit notice.
    {
        let mut buf = lock.lock().unwrap();
        flush_locked(&app, tab_id, &mut buf, generation);
        buf.done = true;
    }
    cv.notify_one();

    // Tear down only if the slot still holds our generation; a closed tab has no entry, so nothing to tear down.
    let state = pty_state();
    let mut guard = state.sessions.lock().unwrap();
    let is_ours = guard
        .get(&tab_id)
        .map(|s| s.generation == generation)
        .unwrap_or(false);
    if !is_ours {
        return;
    }
    guard.remove(&tab_id);
    drop(guard);

    append_scrollback(tab_id, EXIT_NOTICE, Some(generation));
    // `alive: false` rides the notice too: it and `pty-exit` reach a companion on different frames.
    let payload = PtyOutputPayload {
        tab_id,
        data: STANDARD.encode(EXIT_NOTICE),
        reset: false,
        alive: Some(false),
    };
    let _ = app.emit("pty-output", payload);
    // Separate signal from the notice bytes: the frontend needs to flip its own alive state (to enable "type anything to respawn" and colour the tab), which it cannot infer from output.
    let _ = app.emit("pty-exit", PtyExitPayload { tab_id });
}

/// The writer's pure half, apart from `writer_loop` so ordering is testable on a plain sink without the process-global `PtyState`.
fn drain_input_queue(
    rx: std::sync::mpsc::Receiver<Vec<u8>>,
    writer: &mut (impl Write + ?Sized),
) -> std::io::Result<()> {
    // `recv()` blocks, which is exactly right: this thread exists to be parked.
    while let Ok(chunk) = rx.recv() {
        writer.write_all(&chunk)?;
        // Per chunk: a chunk is a keystroke or a paste, and the shell must see it now.
        writer.flush()?;
    }
    Ok(())
}

/// One per session, a raw thread parked in `recv()`; it ends when `kill_session`/`spawn_if_absent` drops the sender.
fn writer_loop(
    rx: std::sync::mpsc::Receiver<Vec<u8>>,
    mut writer: Box<dyn Write + Send>,
    tab_id: TabId,
    generation: u64,
) {
    if let Err(e) = drain_input_queue(rx, writer.as_mut()) {
        eprintln!(
            "[pty] writing to the shell failed (tab {}, session {}): {}",
            tab_id, generation, e
        );
        // Retire only our tab's channel at our generation, never one a `pty_restart` installed; a dead writer must not swallow keys silently.
        let state = pty_state();
        let mut guard = state.inputs.lock().unwrap();
        if guard.get(&tab_id).map(|c| c.generation) == Some(generation) {
            guard.remove(&tab_id);
        }
    }
}

/// Spawns one tab's PTY unless it has one (no-op); `cwd` applies to the first real spawn only. `project_id`/`title` patch `TabMeta` on every call.
#[tauri::command]
pub async fn pty_spawn(
    app: AppHandle,
    tab_id: Option<u32>,
    cwd: Option<String>,
    project_id: Option<String>,
    title: Option<String>,
) -> Result<(), String> {
    let tab_id = tab_id.unwrap_or(DEFAULT_TAB);
    upsert_tab_meta(tab_id, project_id, title);
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let result = spawn_if_absent(app.clone(), tab_id, cwd);
        // Broadcast even on the no-op path: the other screen may still believe this shell is dead.
        emit_alive(&app, tab_id);
        result
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

/// Renames one tab's durable title (mirrors `renameTerminalTab`); a map insert, so a plain `fn`.
#[tauri::command]
pub fn pty_rename_tab(tab_id: u32, title: String) {
    upsert_tab_meta(tab_id, None, Some(title));
}

/// Sets one tab's durable pin (mirrors `toggleTabPinned`); a map insert, so a plain `fn`.
#[tauri::command]
pub fn pty_set_tab_pinned(tab_id: u32, pinned: bool) {
    let mut all = pty_state().tab_meta.lock().unwrap();
    all.entry(tab_id).or_default().pinned = pinned;
}

/// What one attempt to create a tab's shell came to. Separated from the thread wiring so the gate/commit/cancel logic can be driven by tests without an `AppHandle`.
enum Spawned {
    AlreadyLive,
    /// Committed to `sessions`; the caller owns starting the writer and reader threads.
    Installed {
        generation: u64,
        input_rx: std::sync::mpsc::Receiver<Vec<u8>>,
        writer: Box<dyn Write + Send>,
        reader: Box<dyn Read + Send>,
    },
    /// Retired while being created (close, kill, restart, app exit); the shell was reaped and nothing was installed.
    #[cfg_attr(not(test), allow(dead_code))]
    Cancelled {
        reaped_pid: Option<u32>,
    },
}

/// Synchronous, run inside `spawn_blocking`; no-op on a live session. `sessions` is held for the presence check and the commit only, never the spawn.
fn spawn_if_absent(app: AppHandle, tab_id: TabId, cwd: Option<String>) -> Result<(), String> {
    if let Spawned::Installed {
        generation,
        input_rx,
        writer,
        reader,
    } = spawn_shell(tab_id, cwd)?
    {
        std::thread::spawn(move || writer_loop(input_rx, writer, tab_id, generation));
        std::thread::spawn(move || read_loop(app, tab_id, reader, generation));
    }
    Ok(())
}

fn spawn_shell(tab_id: TabId, cwd: Option<String>) -> Result<Spawned, String> {
    let state = pty_state();
    let _gate = state.spawn_gate.lock().unwrap_or_else(|e| e.into_inner());
    *state.spawn_inflight.lock().unwrap() = Some((tab_id, false));
    struct ClearInflight;
    impl Drop for ClearInflight {
        fn drop(&mut self) {
            *pty_state().spawn_inflight.lock().unwrap() = None;
        }
    }
    let _clear = ClearInflight;
    if state.sessions.lock().unwrap().contains_key(&tab_id) {
        return Ok(Spawned::AlreadyLive);
    }

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("failed to open pty: {}", e))?;

    // Login shell, so `.zprofile` (nvm, rbenv, path_helper) gives the same PATH as Terminal.app.
    let mut cmd = CommandBuilder::new(shell_bin());
    cmd.arg("-l");
    // Marks this shell for anything that cares (prompt customisation, statusline scripts) and matches what Terminal.app-launched shells see.
    cmd.env("TERM", "xterm-256color");
    // Inherited from a parent Claude Code session, it would stop `claude` in a tab from saving transcripts.
    cmd.env_remove("CLAUDE_CODE_CHILD_SESSION");
    // Seed macOS install dirs before rc sourcing (cold-start PATH race, R-1 in docs/plan/done/dev-build-in-app-launch.md); nvm-managed node not covered.
    if let Ok(home) = std::env::var("HOME") {
        let seeded = format!(
            "{home}/.local/bin:/opt/homebrew/bin:/usr/local/bin:{home}/.claude/local:{}",
            std::env::var("PATH").unwrap_or_default()
        );
        cmd.env("PATH", seeded);
    }
    if let Some(dir) = cwd {
        cmd.cwd(dir);
    }
    let mut child = pair
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

    let mut guard = state.sessions.lock().unwrap();
    // `kill_session` sets the flag under this same lock, so a kill either lands before this check (cancelled here) or after the insert below (finds and kills the session) — never in between.
    let retired = matches!(*state.spawn_inflight.lock().unwrap(), Some((t, true)) if t == tab_id);
    if retired {
        drop(guard);
        let reaped_pid = child.process_id();
        terminate_child(&mut child);
        return Ok(Spawned::Cancelled { reaped_pid });
    }

    let generation = state.generation.fetch_add(1, Ordering::SeqCst) + 1;
    admit_generation(tab_id, generation);
    // Installed under `sessions` (`sessions` → `inputs`), so a live session never pairs with a dead input queue.
    let (input_tx, input_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    state.inputs.lock().unwrap().insert(
        tab_id,
        InputChannel {
            generation,
            tx: input_tx,
        },
    );
    guard.insert(
        tab_id,
        PtySession {
            tab_id,
            master: pair.master,
            child,
            generation,
        },
    );
    Ok(Spawned::Installed {
        generation,
        input_rx,
        writer,
        reader,
    })
}

/// SIGHUP then SIGKILL the shell's whole process group: docs/arch/terminal-stack.md § Killing the shell means killing its process group.
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

/// Group kill, then `child.kill()` as backstop, then `wait`. Blocks up to the grace period: never call it holding a session lock.
fn terminate_child(child: &mut Box<dyn Child + Send + Sync>) {
    #[cfg(unix)]
    if let Some(pid) = child.process_id() {
        kill_process_group(pid);
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Marks the in-flight spawn so its commit reaps the shell; `None` = whichever is in flight (app exit). The caller holds `sessions`.
fn retire_inflight_spawn(only_tab: Option<TabId>) {
    if let Some((t, retired)) = pty_state().spawn_inflight.lock().unwrap().as_mut() {
        if only_tab.is_none_or(|id| id == *t) {
            *retired = true;
        }
    }
}

/// Kills one tab's shell, drops its session and input queue; the scrollback stays so the exit notice is readable (`pty_restart` clears it itself).
fn kill_session(tab_id: TabId) {
    let state = pty_state();
    let mut guard = state.sessions.lock().unwrap();
    // Remove under the lock, kill after releasing it: the group kill can take 300 ms and must not stall other tabs.
    let session = guard.remove(&tab_id);
    let killed = session.as_ref().map(|s| s.generation);
    retire_inflight_spawn(Some(tab_id));
    // Dropping the sender ends the writer and makes a late `pty_write` fail instead of queueing for a dead shell.
    state.inputs.lock().unwrap().remove(&tab_id);
    drop(guard);
    // Kill/grace loop OUTSIDE the sessions lock — see comment above.
    if let Some(mut s) = session {
        terminate_child(&mut s.child);
    }
    // Fence the dying reader's late bytes before `pty_restart` clears the buffer and spawns the replacement.
    if let Some(g) = killed {
        retire_generation(tab_id, g);
    }
}

/// Kills every tab's shell. Only `shutdown` (app exit) may call it; closing one tab goes through `kill_session` + `drop_tab_state`.
fn kill_all_sessions() {
    // Snapshot ids and retire the in-flight spawn under one lock: a shell being created is in the snapshot or reaped by its own commit.
    let ids: Vec<TabId> = {
        let sessions = pty_state().sessions.lock().unwrap();
        retire_inflight_spawn(None);
        sessions.keys().copied().collect()
    };
    for id in ids {
        kill_session(id);
    }
}

/// Tears every tab's PTY down on app exit (`RunEvent::Exit` in `lib.rs`); every other kill path is a user gesture.
pub fn shutdown() {
    kill_all_sessions();
}

/// Kills one tab's shell and leaves the tab open, in the same `[process exited]` state as a shell that exited by itself.
#[tauri::command]
pub async fn pty_kill(app: AppHandle, tab_id: Option<u32>) -> Result<(), String> {
    let tab_id = tab_id.unwrap_or(DEFAULT_TAB);
    tauri::async_runtime::spawn_blocking(move || {
        kill_session(tab_id);
        // `None`: the notice is the app speaking, not the dead shell — it must land even though the session that prompted it has just been retired.
        append_scrollback(tab_id, EXIT_NOTICE, None);
        let _ = app.emit(
            "pty-output",
            PtyOutputPayload {
                tab_id,
                data: STANDARD.encode(EXIT_NOTICE),
                reset: false,
                alive: Some(false),
            },
        );
        let _ = app.emit("pty-exit", PtyExitPayload { tab_id });
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))
}

/// Closes one tab for good. `tab_id` is required, unlike every other command here: a destructive call fails without a target, never defaults to tab 0.
#[tauri::command]
pub async fn pty_close_tab(app: AppHandle, tab_id: u32) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        kill_session(tab_id);
        drop_tab_state(tab_id);
        // Liveness half only; the tab's removal from the list travels on the `terminalTabsStore` mirror.
        let _ = app.emit("pty-exit", PtyExitPayload { tab_id });
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))
}

/// One row of `pty_list_tabs`; `project_id`/`title`/`pinned` are what `adoptTabs` rebuilds from after a reload.
#[derive(Serialize)]
pub struct PtyTabInfo {
    id: TabId,
    alive: bool,
    project_id: Option<String>,
    title: Option<String>,
    pinned: bool,
}

/// Every tab with a session or a scrollback, by id: the companion replay and the post-reload re-adopt both need exited tabs too.
#[tauri::command]
pub async fn pty_list_tabs() -> Result<Vec<PtyTabInfo>, String> {
    tauri::async_runtime::spawn_blocking(|| -> Vec<PtyTabInfo> {
        let state = pty_state();
        let live: Vec<TabId> = state.sessions.lock().unwrap().keys().copied().collect();
        let mut ids: Vec<TabId> = state.scrollbacks.lock().unwrap().keys().copied().collect();
        for id in &live {
            if !ids.contains(id) {
                ids.push(*id);
            }
        }
        ids.sort_unstable();
        // Per-id lock keeps `tab_meta` a leaf.
        ids.into_iter()
            .map(|id| {
                let meta = state
                    .tab_meta
                    .lock()
                    .unwrap()
                    .get(&id)
                    .cloned()
                    .unwrap_or_default();
                PtyTabInfo {
                    id,
                    alive: live.contains(&id),
                    project_id: meta.project_id,
                    title: meta.title,
                    pinned: meta.pinned,
                }
            })
            .collect()
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))
}

/// Kill, wipe this tab's scrollback, spawn: the RESTART button only. The emit order is the fix: docs/arch/terminal-stack.md § Liveness rides the output channel.
#[tauri::command]
pub async fn pty_restart(
    app: AppHandle,
    tab_id: Option<u32>,
    cwd: Option<String>,
) -> Result<(), String> {
    let tab_id = tab_id.unwrap_or(DEFAULT_TAB);
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        kill_session(tab_id);
        clear_scrollback(tab_id);
        emit_reset(&app, tab_id, b"");
        let result = spawn_if_absent(app.clone(), tab_id, cwd);
        // Also emitted when the spawn failed — `emit_alive` reports what IS, and a screen that thinks it has a shell when the spawn just failed is the same class of lie this fix exists to remove.
        emit_alive(&app, tab_id);
        result
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

/// Empties ONE tab's ring buffer without removing the tab from the map — the entry stays so `pty_list_tabs` keeps reporting a tab whose scrollback the user just cleared.
fn clear_scrollback(tab_id: TabId) {
    pty_state()
        .scrollbacks
        .lock()
        .unwrap()
        .entry(tab_id)
        .or_default()
        .clear();
}

/// Empties one tab's ring and clears it on every screen; the shell's own `clear` leaves the buffer full for the next phone that reconnects.
#[tauri::command]
pub async fn pty_clear(app: AppHandle, tab_id: Option<u32>) -> Result<(), String> {
    let tab_id = tab_id.unwrap_or(DEFAULT_TAB);
    tauri::async_runtime::spawn_blocking(move || {
        clear_scrollback(tab_id);
        emit_reset(&app, tab_id, b"");
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))
}

/// The shell's current directory for "open in Terminal.app", read via `lsof` (no `/proc` on macOS); on failure the caller falls back to `$HOME`.
#[tauri::command]
pub async fn pty_cwd(tab_id: Option<u32>) -> Result<Option<String>, String> {
    let tab_id = tab_id.unwrap_or(DEFAULT_TAB);
    tauri::async_runtime::spawn_blocking(move || -> Option<String> {
        let pid = {
            let guard = pty_state().sessions.lock().unwrap();
            guard.get(&tab_id).and_then(|s| s.child.process_id())?
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

/// Queues base64 keystrokes for one tab. Synchronous on purpose, async would reorder input: docs/arch/terminal-stack.md § Input: one queue, a synchronous `pty_write`.
#[tauri::command]
pub fn pty_write(tab_id: Option<u32>, data: String) -> Result<(), String> {
    let tab_id = tab_id.unwrap_or(DEFAULT_TAB);
    let bytes = STANDARD
        .decode(&data)
        .map_err(|e| format!("invalid base64 input: {}", e))?;
    let state = pty_state();
    let guard = state.inputs.lock().unwrap();
    let channel = guard
        .get(&tab_id)
        .ok_or_else(|| format!("no PTY session on tab {} — call pty_spawn first", tab_id))?;
    channel.tx.send(bytes).map_err(|_| {
        format!(
            "the PTY writer thread for tab {} has gone — call pty_spawn first",
            tab_id
        )
    })
}

/// The host is the only caller; a companion is resized by the echoed `FRAME_PTY_RESIZE`, a frontend discipline this command cannot enforce.
#[tauri::command]
pub async fn pty_resize(tab_id: Option<u32>, cols: u16, rows: u16) -> Result<(), String> {
    let tab_id = tab_id.unwrap_or(DEFAULT_TAB);
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let state = pty_state();
        let guard = state.sessions.lock().unwrap();
        let session = guard
            .get(&tab_id)
            .ok_or_else(|| format!("no PTY session on tab {} — call pty_spawn first", tab_id))?;
        session
            .master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("pty resize failed: {}", e))
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

#[derive(Serialize)]
pub struct PtyScrollback {
    /// base64 — see module doc comment "BINARY-SAFE TRANSPORT".
    data: String,
    /// Current size (80x24 before a session), so a screen joining mid-session applies it without waiting for the next host resize.
    cols: u16,
    rows: u16,
    /// Live or exited: not inferable from the scrollback bytes, and needed to offer a restart.
    alive: bool,
}

/// One tab's ring buffer with its size and liveness, so a screen hydrates before subscribing to live output.
#[tauri::command]
pub async fn pty_get_scrollback(tab_id: Option<u32>) -> Result<PtyScrollback, String> {
    let tab_id = tab_id.unwrap_or(DEFAULT_TAB);
    tauri::async_runtime::spawn_blocking(move || -> Result<PtyScrollback, String> {
        let state = pty_state();
        let data = {
            let all = state.scrollbacks.lock().unwrap();
            match all.get(&tab_id) {
                Some(buf) => STANDARD.encode(&buf[..]),
                None => String::new(),
            }
        };
        let (cols, rows, alive) = {
            let guard = state.sessions.lock().unwrap();
            let session = guard.get(&tab_id);
            let alive = session.is_some();
            match session.map(|s| s.master.get_size()) {
                Some(Ok(size)) => (size.cols, size.rows, alive),
                _ => (80, 24, alive),
            }
        };
        Ok(PtyScrollback {
            data,
            cols,
            rows,
            alive,
        })
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

#[cfg(test)]
mod constant_guards {
    use super::*;

    /// `SCROLLBACK_CAP` is Rust-only state, but its size is what a joining phone must receive — so a change here is a change to `web_server.rs`'s budget, not a local tuning knob.
    #[test]
    fn scrollback_cap_is_the_size_the_companion_budget_was_derived_from() {
        assert_eq!(
            SCROLLBACK_CAP,
            128 * 1024,
            "SCROLLBACK_CAP changed. Re-derive COMPANION_QUEUE_LIMIT_BYTES in web_server.rs (INVARIANT R) \
             and update docs/arch/terminal-stack.md's PTY-backend contract in the same commit."
        );
    }
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
        let out = std::process::Command::new("/bin/ps")
            .args(["-eo", "pid=,ppid="])
            .output()
            .expect("ps");
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| {
                let mut it = l.split_whitespace();
                let c: i32 = it.next()?.parse().ok()?;
                let p: i32 = it.next()?.parse().ok()?;
                if p == pid {
                    Some(c)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Reads ONE tab's ring buffer as a lossy string — every fixture below is ASCII, so this is only a convenience for the assertion messages.
    fn scrollback_text(tab_id: TabId) -> String {
        let all = pty_state().scrollbacks.lock().unwrap();
        String::from_utf8_lossy(all.get(&tab_id).map(|b| &b[..]).unwrap_or(&[])).to_string()
    }

    /// A RESTARTed shell's late flush must not land in the buffer cleared for the new shell. One sequence: it drives the process-global `generation`.
    #[test]
    fn bytes_from_a_retired_session_never_reach_the_scrollback() {
        let state = pty_state();
        const TAB: TabId = 0;
        // Stand in for "session 7 is running on tab 0" without spawning a real shell — this test is about the byte-path fence, not about the PTY.
        state.generation.store(7, Ordering::SeqCst);
        state.min_accepted.lock().unwrap().insert(TAB, 0);
        state.scrollbacks.lock().unwrap().remove(&TAB);

        append_scrollback(TAB, b"output from the old shell\n", Some(7));
        assert!(
            scrollback_text(TAB).contains("old shell"),
            "a live session's own bytes must be appended"
        );
        assert!(
            generation_accepted(TAB, 7),
            "a live session's bytes must also be emittable"
        );

        // What RESTART does, in order: kill (which retires the session), then wipe the buffer, then spawn.
        retire_generation(TAB, 7);
        clear_scrollback(TAB);

        // The killed shell's reader thread, waking up ~20ms late with bytes it had already read.
        append_scrollback(TAB, b"TRAILING BYTES FROM THE CORPSE", Some(7));
        assert_eq!(
            scrollback_text(TAB),
            "",
            "a retired session's trailing bytes must be dropped, not appended above the new prompt"
        );
        assert!(
            !generation_accepted(TAB, 7),
            "a retired session's bytes must not be emitted to any screen either"
        );

        // The replacement shell writes into the same buffer and must be entirely unaffected.
        state.generation.store(8, Ordering::SeqCst);
        append_scrollback(TAB, b"new prompt", Some(8));
        assert_eq!(
            scrollback_text(TAB),
            "new prompt",
            "the new session must own the freshly cleared buffer"
        );
        assert!(generation_accepted(TAB, 8));

        // The exit notice belongs to no session (`pty_kill` writes it right after retiring one), so it must land regardless of the fence.
        append_scrollback(TAB, EXIT_NOTICE, None);
        assert!(
            scrollback_text(TAB).contains("process exited"),
            "an unattributed notice must never be fenced off"
        );

        // The fence must not have cost the ring buffer its cap.
        clear_scrollback(TAB);
        let chunk = vec![b'x'; SCROLLBACK_CAP / 2];
        for _ in 0..3 {
            append_scrollback(TAB, &chunk, Some(8));
        }
        assert_eq!(
            pty_state()
                .scrollbacks
                .lock()
                .unwrap()
                .get(&TAB)
                .map(|b| b.len()),
            Some(SCROLLBACK_CAP),
            "SCROLLBACK_CAP must still bound the buffer"
        );

        drop_tab_state(TAB);
    }

    /// `spawn_inflight` is one process-wide slot, so every test that drives it or a real spawn takes turns.
    static SPAWN_SLOT: StdMutex<()> = StdMutex::new(());

    fn wait_until(what: &str, mut ready: impl FnMut() -> bool) {
        for _ in 0..500 {
            if ready() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("timed out waiting for {what}");
    }

    fn spawn_in_flight_for(tab: TabId) -> bool {
        matches!(*pty_state().spawn_inflight.lock().unwrap(), Some((t, _)) if t == tab)
    }

    fn session_pid(tab: TabId) -> Option<u32> {
        pty_state()
            .sessions
            .lock()
            .unwrap()
            .get(&tab)
            .and_then(|s| s.child.process_id())
    }

    /// Runs `spawn_shell` on its own thread, as `pty_spawn`'s `spawn_blocking` does.
    fn spawn_on_thread(tab: TabId) -> std::thread::JoinHandle<Result<Spawned, String>> {
        std::thread::spawn(move || spawn_shell(tab, None))
    }

    /// Drains the shell's output the way `read_loop` does: a shell killed with undrained pty output can hang in the kernel while exiting, which would make `wait` in these tests block.
    fn installed(outcome: Spawned) -> bool {
        let Spawned::Installed { mut reader, .. } = outcome else {
            return false;
        };
        std::thread::spawn(move || {
            let mut sink = [0u8; 4096];
            while matches!(reader.read(&mut sink), Ok(n) if n > 0) {}
        });
        true
    }

    #[test]
    fn a_tab_closed_while_its_shell_is_being_created_leaves_no_session_and_no_process() {
        const TAB: TabId = 40;
        let _serial = SPAWN_SLOT.lock().unwrap_or_else(|e| e.into_inner());
        let state = pty_state();

        let commit_blocked = state.sessions.lock().unwrap();
        let spawn = spawn_on_thread(TAB);
        wait_until("the spawn to take the gate", || spawn_in_flight_for(TAB));
        retire_inflight_spawn(Some(TAB));
        drop(commit_blocked);

        match spawn.join().unwrap().unwrap() {
            Spawned::Cancelled {
                reaped_pid: Some(pid),
            } => assert!(!alive(pid as i32), "the cancelled shell must be reaped"),
            _ => panic!("a retired spawn must be cancelled with a reaped shell"),
        }
        assert!(session_pid(TAB).is_none());
        assert!(!state.inputs.lock().unwrap().contains_key(&TAB));
    }

    #[test]
    fn a_restart_during_a_spawn_ends_with_exactly_one_live_shell() {
        const TAB: TabId = 41;
        let _serial = SPAWN_SLOT.lock().unwrap_or_else(|e| e.into_inner());

        let commit_blocked = pty_state().sessions.lock().unwrap();
        let first = spawn_on_thread(TAB);
        wait_until("the first spawn to take the gate", || {
            spawn_in_flight_for(TAB)
        });
        retire_inflight_spawn(Some(TAB));
        let replacement = spawn_on_thread(TAB);
        drop(commit_blocked);

        let Spawned::Cancelled {
            reaped_pid: Some(old_pid),
        } = first.join().unwrap().unwrap()
        else {
            panic!("the spawn a restart retired must be cancelled");
        };
        assert!(installed(replacement.join().unwrap().unwrap()));
        let new_pid = session_pid(TAB).expect("the replacement is the tab's session");
        assert_ne!(old_pid, new_pid);
        assert!(!alive(old_pid as i32));
        assert!(alive(new_pid as i32));
        kill_session(TAB);
        assert!(!alive(new_pid as i32));
    }

    #[test]
    fn concurrent_opens_of_one_tab_make_one_shell_and_of_many_tabs_make_many() {
        const SAME: TabId = 42;
        let _serial = SPAWN_SLOT.lock().unwrap_or_else(|e| e.into_inner());

        let same_tab: Vec<_> = (0..8).map(|_| spawn_on_thread(SAME)).collect();
        let created = same_tab
            .into_iter()
            .map(|h| installed(h.join().unwrap().unwrap()))
            .filter(|&made_one| made_one)
            .count();
        assert_eq!(created, 1, "one tab, one shell");

        let many: Vec<_> = (43..53).map(|t| (t, spawn_on_thread(t))).collect();
        for (tab, h) in many {
            assert!(installed(h.join().unwrap().unwrap()), "tab {tab} must open");
        }
        for tab in 42..53 {
            assert!(session_pid(tab).is_some(), "tab {tab} has a session");
            kill_session(tab);
            assert!(session_pid(tab).is_none());
        }
    }

    #[test]
    fn killing_a_committed_session_still_reaps_its_shell() {
        const TAB: TabId = 53;
        let _serial = SPAWN_SLOT.lock().unwrap_or_else(|e| e.into_inner());

        assert!(installed(spawn_on_thread(TAB).join().unwrap().unwrap()));
        let pid = session_pid(TAB).unwrap();
        assert!(alive(pid as i32));
        kill_session(TAB);
        assert!(!alive(pid as i32));
        assert!(session_pid(TAB).is_none());
    }

    #[test]
    fn a_spawn_that_never_returns_cannot_stall_the_tabs_that_already_exist() {
        let _serial = SPAWN_SLOT.lock().unwrap_or_else(|e| e.into_inner());
        let _stuck_spawn = pty_state()
            .spawn_gate
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        let (done, finished) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            kill_session(54);
            let _ = pty_state().sessions.lock().unwrap().contains_key(&55);
            let _ = done.send(());
        });
        finished
            .recv_timeout(Duration::from_secs(2))
            .expect("session-map operations must not wait on the spawn gate");
    }

    #[test]
    fn a_kill_retires_only_the_spawn_in_flight_for_that_tab() {
        const SPAWNING: TabId = 30;
        const OTHER: TabId = 31;
        let _serial = SPAWN_SLOT.lock().unwrap_or_else(|e| e.into_inner());
        let state = pty_state();
        let retired = || matches!(*state.spawn_inflight.lock().unwrap(), Some((_, true)));

        *state.spawn_inflight.lock().unwrap() = Some((SPAWNING, false));
        kill_session(OTHER);
        assert!(!retired(), "killing another tab must not cancel this spawn");
        kill_session(SPAWNING);
        assert!(
            retired(),
            "killing the tab being spawned must cancel its commit"
        );

        *state.spawn_inflight.lock().unwrap() = Some((SPAWNING, false));
        retire_inflight_spawn(None);
        assert!(
            retired(),
            "app exit must cancel whichever spawn is in flight"
        );
        *state.spawn_inflight.lock().unwrap() = None;
    }

    /// Generations are global, so a per-tab floor must keep tab 10's retirement from muting tab 11's older, live session.
    #[test]
    fn retiring_one_tab_does_not_fence_another() {
        const A: TabId = 10; // the tab being retired
        const B: TabId = 11; // the innocent bystander, on an OLDER generation
        let state = pty_state();
        state.min_accepted.lock().unwrap().remove(&A);
        state.min_accepted.lock().unwrap().remove(&B);
        state.scrollbacks.lock().unwrap().remove(&A);
        state.scrollbacks.lock().unwrap().remove(&B);

        // B spawned first (lower generation), A second — the ordering that makes a global floor lethal.
        let gen_b: u64 = 40;
        let gen_a: u64 = 41;

        append_scrollback(B, b"B is alive", Some(gen_b));
        append_scrollback(A, b"A is alive", Some(gen_a));
        assert_eq!(scrollback_text(B), "B is alive");
        assert_eq!(scrollback_text(A), "A is alive");

        // Close A: its floor goes to gen_a + 1, which is ABOVE B's live generation.
        retire_generation(A, gen_a);

        assert!(
            !generation_accepted(A, gen_a),
            "A's own retired session must be fenced off"
        );
        append_scrollback(A, b" trailing corpse bytes", Some(gen_a));
        assert_eq!(
            scrollback_text(A),
            "A is alive",
            "A's retired session must not append after retirement"
        );

        // The whole point: B is untouched by a retirement that was not about B.
        assert!(
            generation_accepted(B, gen_b),
            "retiring another tab must not fence off this tab's live session"
        );
        append_scrollback(B, b" and still writing", Some(gen_b));
        assert_eq!(
            scrollback_text(B),
            "B is alive and still writing",
            "a live tab must keep appending after a SIBLING tab was retired"
        );

        drop_tab_state(A);
        drop_tab_state(B);
    }

    /// Multi-entity guard: close the middle of three tabs, and both neighbours' state must survive byte-identical.
    #[test]
    fn closing_one_tab_leaves_other_tabs_scrollback_intact() {
        const KEEP_LOW: TabId = 12;
        const CLOSED: TabId = 13;
        const KEEP_HIGH: TabId = 14;
        for t in [KEEP_LOW, CLOSED, KEEP_HIGH] {
            pty_state().scrollbacks.lock().unwrap().remove(&t);
        }

        append_scrollback(KEEP_LOW, b"tab 12 output", Some(50));
        append_scrollback(CLOSED, b"tab 13 output", Some(51));
        append_scrollback(KEEP_HIGH, b"tab 14 output", Some(52));

        // What `pty_close_tab` does to state, minus the kill (there is no real shell here).
        drop_tab_state(CLOSED);

        assert_eq!(
            scrollback_text(KEEP_LOW),
            "tab 12 output",
            "closing tab 13 must not touch tab 12"
        );
        assert_eq!(
            scrollback_text(KEEP_HIGH),
            "tab 14 output",
            "closing tab 13 must not touch tab 14"
        );
        assert!(
            !pty_state()
                .scrollbacks
                .lock()
                .unwrap()
                .contains_key(&CLOSED),
            "the closed tab's scrollback must actually be gone, not merely emptied"
        );

        // And the surviving tabs must still be WRITABLE — a fence left behind by the close would make them look intact here and go mute a moment later.
        append_scrollback(KEEP_LOW, b" +more", Some(50));
        append_scrollback(KEEP_HIGH, b" +more", Some(52));
        assert_eq!(scrollback_text(KEEP_LOW), "tab 12 output +more");
        assert_eq!(scrollback_text(KEEP_HIGH), "tab 14 output +more");

        drop_tab_state(KEEP_LOW);
        drop_tab_state(KEEP_HIGH);
    }

    /// P2-1 resurrection guard: a late `append_scrollback` from a dying reader after
    /// A tab id reused after its previous tab was closed (the frontend allocates `max(id)+1`) must accept its new session's bytes and still reject the old session's.
    #[test]
    fn reused_tab_id_accepts_the_new_session_only() {
        const TAB: TabId = 16;
        let state = pty_state();
        state.scrollbacks.lock().unwrap().remove(&TAB);
        state.min_accepted.lock().unwrap().insert(TAB, 0);

        drop_tab_state(TAB);
        assert!(
            !generation_accepted(TAB, 70),
            "closed tab must reject its old session"
        );

        admit_generation(TAB, 71);
        append_scrollback(TAB, b"new shell prompt", Some(71));
        assert!(
            generation_accepted(TAB, 71),
            "the reused id's new session must be accepted"
        );
        assert!(
            !generation_accepted(TAB, 70),
            "the old session must stay rejected"
        );
        assert_eq!(scrollback_text(TAB), "new shell prompt");
        drop_tab_state(TAB);
    }

    /// The `u64::MAX` floor must stop a late write recreating the closed tab's scrollback key (`pty_list_tabs` reads `sessions` ∪ `scrollbacks`).
    #[test]
    fn late_write_after_close_does_not_resurrect_tab() {
        const TAB: TabId = 15;
        let state = pty_state();
        // Explicit clean slate: overrides any floor left by a previous run in this process.
        state.sessions.lock().unwrap().remove(&TAB);
        state.scrollbacks.lock().unwrap().remove(&TAB);
        state.min_accepted.lock().unwrap().insert(TAB, 0);
        state.tab_meta.lock().unwrap().remove(&TAB);

        // Simulate a live session at generation 60.
        let gen: u64 = 60;
        append_scrollback(TAB, b"live output", Some(gen));
        assert!(
            state.scrollbacks.lock().unwrap().contains_key(&TAB),
            "live session must produce a scrollback entry"
        );

        // What `pty_close_tab` does to state: kill_session has already run, now drop_tab_state.
        drop_tab_state(TAB);

        assert!(
            !state.scrollbacks.lock().unwrap().contains_key(&TAB),
            "drop_tab_state must remove the scrollback entry"
        );
        assert!(
            !state.tab_meta.lock().unwrap().contains_key(&TAB),
            "drop_tab_state must remove the tab metadata"
        );
        assert!(
            !state.sessions.lock().unwrap().contains_key(&TAB),
            "closed tab must be absent from sessions"
        );

        // Late reader: the OS had already buffered these bytes when the close fired.
        append_scrollback(TAB, b"zombie bytes from dying reader", Some(gen));

        // The scrollback entry must NOT have been recreated — that is the resurrection bug.
        assert!(
            !state.scrollbacks.lock().unwrap().contains_key(&TAB),
            "a late write after close must not recreate the scrollback entry"
        );

        // The fence also blocks the emit path for every realistic generation.
        assert!(
            !generation_accepted(TAB, gen),
            "closed tab must reject the session's own generation"
        );
        assert!(
            !generation_accepted(TAB, 0),
            "closed tab must reject generation 0"
        );
        assert!(
            !generation_accepted(TAB, u64::MAX - 1),
            "closed tab must reject even a near-maximum generation — reopen is structurally blocked"
        );
    }

    /// A sink that records exactly what it was handed, in the order it was handed it — the observable the ordering guarantee is about.
    struct RecordingSink(Arc<StdMutex<Vec<u8>>>);
    impl Write for RecordingSink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Enqueue order is shell order, with the producer still writing while the writer drains.
    #[test]
    fn queued_input_reaches_the_shell_in_the_order_it_was_enqueued() {
        let sink = Arc::new(StdMutex::new(Vec::new()));
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();

        let writer_sink = Arc::clone(&sink);
        let writer = std::thread::spawn(move || {
            let mut w = RecordingSink(writer_sink);
            drain_input_queue(rx, &mut w).expect("a recording sink never fails a write");
        });

        // 4096 distinct chunks: enough that any reordering shows up, and enough that the writer is draining concurrently with the producer rather than starting after it.
        let mut expected = Vec::new();
        for i in 0..4096u32 {
            let chunk = format!("<{}>", i).into_bytes();
            expected.extend_from_slice(&chunk);
            tx.send(chunk).expect("writer thread is alive");
        }
        drop(tx); // last sender gone → drain_input_queue returns, exactly as a session teardown does
        writer.join().expect("writer thread must not panic");

        let written = sink.lock().unwrap().clone();
        assert_eq!(
            written, expected,
            "the byte stream handed to the PTY must be the enqueue order, unaltered"
        );
    }

    /// A teardown must end the writer thread rather than leave it parked forever — one thread per live session, no matter how often RESTART is pressed.
    #[test]
    fn dropping_the_queues_last_sender_ends_the_writer() {
        let sink = Arc::new(StdMutex::new(Vec::new()));
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        tx.send(b"pending work".to_vec()).unwrap();
        drop(tx);

        let mut w = RecordingSink(Arc::clone(&sink));
        drain_input_queue(rx, &mut w).expect("clean shutdown is not an error");
        assert_eq!(
            &sink.lock().unwrap()[..],
            b"pending work",
            "work already queued when the session ended must still be written, not discarded on the floor"
        );
    }

    /// A PTY that refuses writes (the shell died between the enqueue and the write) must surface as an error the writer thread can act on, not as silently swallowed keystrokes.
    #[test]
    fn a_failing_pty_stops_the_writer_instead_of_swallowing_input() {
        struct DeadPty;
        impl Write for DeadPty {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "shell is gone",
                ))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        tx.send(b"ls\n".to_vec()).unwrap();
        let mut w = DeadPty;
        let err = drain_input_queue(rx, &mut w).expect_err("a broken pipe must propagate");
        assert_eq!(err.kind(), std::io::ErrorKind::BrokenPipe);
    }

    /// Nothing started in the shell (`sleep 300`, no `exec`: a separate pid) survives teardown. A plain `kill` passes too: terminal-stack.md § Killing the shell.
    #[test]
    fn killing_the_shell_takes_processes_started_inside_it_with_it() {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
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
        let grandchild = grandchild
            .unwrap_or_else(|| panic!("setup failed: /bin/sh {} never forked a child", shell_pid));
        assert_ne!(
            grandchild, shell_pid,
            "setup failed: grandchild must be a distinct pid or the test proves nothing"
        );
        assert!(alive(grandchild), "setup failed: grandchild is not running");

        kill_process_group(shell_pid as u32);

        // Reap the shell: `kill(pid, 0)` reports a zombie as alive. The orphaned grandchild is reaped by init.
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
        panic!(
            "grandchild {} survived kill_process_group — the SSH/agy/claude leak is back",
            grandchild
        );
    }
}
