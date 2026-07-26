// In-app terminal — PTY backend (docs/plan/1.20.0-terminal-and-remote-sync.md §4, T-1..T-8).
//
// One shared PTY for the whole app process (T-3): the phone is a dumb terminal surface onto the SAME shell the Mac's own TerminalView drives, not a second independent session. `pty_spawn` is therefore idempotent — whichever screen (host or companion, via the `invoke` seam) opens the Terminal tab first actually spawns the shell; every later call is a no-op that returns the already-running session. `cwd` (T-8) is only honoured on that first spawn.
//
// BINARY-SAFE TRANSPORT: PTY output is not guaranteed valid UTF-8 at chunk boundaries (a multi-byte UTF-8 sequence, e.g. from `ls` on a filename with accented characters, can be split across two `read()`s). This module never treats PTY bytes as a Rust `String` — it carries them as base64 end-to-end (Tauri event payload AND the WS `pty_output`/`pty_input` frames), decoding only at the very edge (`pty_write`) or not at all (scrollback is stored as raw bytes and re-encoded to base64 on read). The frontend decodes base64 to a `Uint8Array` and feeds it to `xterm.js`'s `Terminal.write()`, which accepts binary and — per its own docs — maintains a stateful UTF-8 decoder across `write()` calls, so a sequence split at a chunk boundary is reassembled correctly by xterm itself. This is why nothing in this module or in `usePtyTerminal.js` needs its own split-sequence buffering.
//
// NEVER-BLOCK-THE-UI (CLAUDE.md ABSOLUTE, plan §4.3): `pty_spawn`/`pty_resize`/`pty_get_scrollback` are all `async fn` wrapping their work in `spawn_blocking`, same as every other command in this app. `pty_write` is the one command that is deliberately synchronous, because after the writer-thread rework it no longer waits on anything (it decodes base64 and enqueues) and because being synchronous is what makes keystroke ORDER a structural property rather than a race — see its own doc comment, which is where that argument belongs in full. The PTY READ LOOP is the other deliberate exception — it is a dedicated `std::thread::spawn`, started once from inside `pty_spawn`'s blocking closure, NOT `spawn_blocking` and NOT a tokio task. `spawn_blocking`'s pool is sized for bounded one-shot work; parking one of its threads forever in a `reader.read()` loop for the app's whole lifetime would starve every other blocking command (`resolve_remote_path`, `check_for_updates`, `get_git_info`, …) of a slot. `portable-pty`'s reader/writer are synchronous `Read`/`Write` trait objects, not `AsyncRead`, so a tokio task would block a worker thread just as badly — a raw OS thread is the only shape that is both correct and does not starve anything else. The same reasoning covers the flusher thread each read loop starts (see `flusher_loop`).
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

/// The input side of one session: a queue and the thread that owns the PTY writer.
///
/// WHY A QUEUE AND NOT A MUTEX AROUND THE WRITER (the ordering bug this replaces): `pty_write` used
/// to lock the session, write, and unlock. Two keystrokes are two independent Tauri commands, so
/// which one reached the mutex first was a race — the mutex guaranteed that the two writes did not
/// interleave, and guaranteed nothing whatsoever about their ORDER. Typing `ls` could put `sl` into
/// the shell. Unobservable at human speed, certain under a paste or a companion replaying input.
///
/// Ordering cannot be fixed by locking harder; it has to stop being a contest. So there is one
/// consumer — this thread — fed by an mpsc, and the wire order is decided once, at enqueue time, by
/// a channel that is FIFO by construction. Nothing downstream can reorder because nothing
/// downstream is concurrent. See `pty_write` for the other half (why the command is synchronous).
struct InputChannel {
    /// The session this queue belongs to, so a writer thread that fails can only retire ITS OWN
    /// channel and never the replacement a `pty_restart` has already installed — the same identity
    /// discipline `read_loop` uses on the session slot.
    generation: u64,
    tx: std::sync::mpsc::Sender<Vec<u8>>,
}

struct PtySession {
    master: Box<dyn MasterPty + Send>,
    /// Kept so `pty_kill`/`pty_restart` can actually terminate the shell, and so dropping the session tears the child down rather than leaking an orphaned process.
    child: Box<dyn Child + Send + Sync>,
    /// Which spawn this session is — see `PtyState::generation`.
    generation: u64,
}

struct PtyState {
    session: StdMutex<Option<PtySession>>,
    /// The current session's input queue — see `InputChannel`. Its own mutex rather than a field on
    /// `PtySession` on purpose: keystrokes must not queue behind whatever else holds the session
    /// lock (a resize, a `pty_cwd`, a spawn), and this one is only ever taken for the microseconds
    /// it costs to clone a channel handle.
    ///
    /// LOCK ORDER, module-wide: `session` → `input`. Every path that needs both (`spawn_if_absent`,
    /// `kill_current`) takes them in that order; `pty_write` takes only `input`. Stated because the
    /// module's other invariant — no path holds both the `OutBuf` lock and the `scrollback` lock —
    /// is only auditable if every lock pair in the file has a written rule.
    input: StdMutex<Option<InputChannel>>,
    scrollback: StdMutex<Vec<u8>>,
    /// Incremented on every real spawn. A reader thread only tears down `session` if the session currently in the slot is still the one IT was reading — otherwise a slow EOF from the shell killed by `pty_restart` would race in and null out the brand-new session that replaced it, leaving the terminal permanently dead with no error anywhere. This counter is the whole reason restart is safe to spam.
    generation: AtomicU64,
    /// The oldest session generation whose bytes may still reach the scrollback and the screens. Bumped past a session the moment that session is killed (`kill_current`), which is what makes the *output* side of a RESTART as safe as the session slot already was.
    ///
    /// THE BUG IT FIXES: killing a shell does not stop its reader thread instantly. Up to one `FLUSH_INTERVAL` of already-read bytes can still be sitting in that reader's accumulator, and its final `read()` can return more. Those bytes used to be appended and emitted unconditionally, so on a RESTART they landed AFTER `pty_restart` had cleared the ring buffer and emitted its `reset` — i.e. above the fresh prompt, inside a scrollback that was supposed to be empty. The generation counter already protected the session slot from exactly this class of race; this extends the same identity check to the byte path instead of adding a second, different mechanism.
    min_accepted_generation: AtomicU64,
}

static PTY: OnceLock<PtyState> = OnceLock::new();

fn pty_state() -> &'static PtyState {
    PTY.get_or_init(|| PtyState {
        session: StdMutex::new(None),
        input: StdMutex::new(None),
        scrollback: StdMutex::new(Vec::new()),
        generation: AtomicU64::new(0),
        min_accepted_generation: AtomicU64::new(0),
    })
}

/// The user's own login shell, same resolution `Terminal.app`/a login shell would use. This app ships macOS-only (CLAUDE.md); `$SHELL` is set on every macOS account, `/bin/zsh` (the macOS default since Catalina) is the fallback for the rare case it is unset.
fn shell_bin() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string())
}

/// Appends to the ring buffer, dropping anything a retired session produced.
///
/// `generation`: `Some(g)` = "these bytes came out of session `g`" and they are discarded once that session has been retired (see `PtyState::min_accepted_generation`). `None` = "this text is not session output" — the `[process exited]` notice `pty_kill` writes belongs to no session and must always land.
///
/// THE STALENESS TEST IS TAKEN UNDER THE SCROLLBACK LOCK, which is what makes it airtight rather than merely likely: `retire_generation` publishes the new floor while holding the same lock, so an appender either finishes entirely before the retirement (and has its bytes cleared by the `pty_restart` that follows) or sees the new floor and drops them. There is no interleaving in which a check passes and the append then lands after the clear.
fn append_scrollback(bytes: &[u8], generation: Option<u64>) {
    let state = pty_state();
    let mut buf = state.scrollback.lock().unwrap();
    if let Some(g) = generation {
        if g < state.min_accepted_generation.load(Ordering::SeqCst) {
            return;
        }
    }
    buf.extend_from_slice(bytes);
    if buf.len() > SCROLLBACK_CAP {
        let excess = buf.len() - SCROLLBACK_CAP;
        buf.drain(0..excess);
    }
}

/// Declares every session up to and including `generation` finished: nothing they produce from here on may reach the scrollback or the screens. Called from `kill_current` only — a session that ends by itself (EOF) has no late writer to fence off, since its own reader is the thread doing the ending.
fn retire_generation(generation: u64) {
    let state = pty_state();
    // Held for the store, not for anything inside it — see `append_scrollback` for why the lock is the ordering guarantee.
    let _buf = state.scrollback.lock().unwrap();
    state.min_accepted_generation.store(generation + 1, Ordering::SeqCst);
}

/// Cheap read of the same floor for the emit path. Deliberately lock-free: `flush_locked` runs holding the `OutBuf` mutex, and taking the scrollback mutex there would create an `OutBuf` → `scrollback` lock order that no other path has, breaking the module's "no path holds both at once" invariant to close a race measured in nanoseconds. The scrollback — the thing a late-joining screen hydrates from, and therefore the durable damage — is fenced exactly.
fn generation_accepted(generation: u64) -> bool {
    generation >= pty_state().min_accepted_generation.load(Ordering::SeqCst)
}

#[derive(Serialize, Clone)]
struct PtyOutputPayload {
    /// base64 — see module doc comment "BINARY-SAFE TRANSPORT".
    data: String,
    /// `true` = "this payload replaces everything on screen", not "append it". Set by `pty_clear`/`pty_restart` so BOTH screens wipe in one step: the host's own TerminalView listens to this event directly, and `services/ptyBridge.js` forwards the same flag on the `pty_output` frame. Without it, clearing on one screen would leave the other showing a scrollback the host no longer has.
    reset: bool,
    /// LIVENESS, CARRIED ON THE OUTPUT CHANNEL. `Some(x)` = "the shell is/was x at the moment this payload was produced"; `None` = "this payload says nothing about liveness, leave your current belief alone" (and, thanks to `skip_serializing_if`, the field is absent from the JSON entirely, so the frontend's `typeof alive === 'boolean'` test is the whole protocol).
    ///
    /// WHY IT IS OPTIONAL RATHER THAN ALWAYS PRESENT: the ordinary byte path (`flush_locked`) runs on every read and must not take the session mutex — the hot path would then contend with every keystroke's `pty_write` for no benefit. Liveness only ever *changes* at a spawn, a kill or an EOF, so it is stamped exactly on those three payloads.
    ///
    /// WHY IT EXISTS AT ALL: before this field there was no "the shell came back" signal anywhere on the wire. A screen that had seen the shell exit stayed convinced it was dead until it happened to re-hydrate, and one keystroke on that screen then tore down the live shell the *other* screen had just restarted. See `emit_alive`.
    #[serde(skip_serializing_if = "Option::is_none")]
    alive: Option<bool>,
}

/// Emits accumulated bytes as one `pty-output` Tauri event and clears the accumulator. No-op on an empty accumulator (the read loop's final post-EOF call, if nothing was pending).
///
/// MUST BE CALLED WITH THE `OutBuf` MUTEX HELD — that is the whole ordering guarantee. Two threads emit through this function (the reader on the byte/interval fast path, the flusher on the deadline), and because each call drains the entire accumulator and emits it without ever releasing the lock in between, the sequence of `pty-output` payloads is exactly the sequence of bytes the PTY produced: no interleaving, no byte emitted twice, none dropped.
///
/// `generation` is the session these bytes came from: once that session has been retired the accumulator is dropped rather than emitted, so a killed shell's last chunk cannot paint itself over the fresh prompt a `pty_restart` just produced (the screen half of the same fix `append_scrollback` applies to the ring buffer).
fn flush_locked(app: &AppHandle, buf: &mut OutBuf, generation: u64) {
    if buf.acc.is_empty() {
        return;
    }
    if !generation_accepted(generation) {
        buf.acc.clear();
        return;
    }
    let payload = PtyOutputPayload { data: STANDARD.encode(&buf.acc[..]), reset: false, alive: None };
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
fn flusher_loop(app: AppHandle, shared: Arc<(StdMutex<OutBuf>, Condvar)>, generation: u64) {
    let (lock, cv) = &*shared;
    let mut buf = lock.lock().unwrap();
    loop {
        while buf.acc.is_empty() && !buf.done {
            buf = cv.wait(buf).unwrap();
        }
        if buf.done {
            // The reader already flushed the tail before setting this; the call is a no-op unless it lost a race, in which case it is what keeps those last bytes from being dropped.
            flush_locked(&app, &mut buf, generation);
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
        flush_locked(&app, &mut buf, generation);
    }
}

/// Reads the one true answer to "is there a shell right now" straight off the session slot. Never inferred from output bytes anywhere.
fn is_alive() -> bool {
    pty_state().session.lock().unwrap().is_some()
}

/// Tells every screen "wipe what you have, this is the new whole content" (usually empty), stamped with the liveness at that instant.
fn emit_reset(app: &AppHandle, bytes: &[u8]) {
    let alive = is_alive();
    let _ = app.emit("pty-output", PtyOutputPayload { data: STANDARD.encode(bytes), reset: true, alive: Some(alive) });
}

/// "The shell came back" — the event that did not exist before, and whose absence let one screen keep believing a restarted shell was dead (plan §2.4).
///
/// It carries no bytes and no `reset`, so it can never disturb what is on screen; it only corrects a stale belief. That is exactly why it is emitted AFTER the spawn rather than folding liveness into the `reset` that precedes it: the reset has to go out BEFORE the new shell starts writing, or it would wipe the fresh prompt off both screens, while the liveness has to be read AFTER the spawn or it is stale by construction. One payload cannot satisfy both orderings — so it is two payloads, and the ordering of each is forced rather than chosen.
fn emit_alive(app: &AppHandle) {
    let alive = is_alive();
    let _ = app.emit("pty-output", PtyOutputPayload { data: String::new(), reset: false, alive: Some(alive) });
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
        std::thread::spawn(move || flusher_loop(app, shared, generation));
    }
    let (lock, cv) = &*shared;
    loop {
        match reader.read(&mut read_buf) {
            Ok(0) => break, // EOF — shell process exited
            Ok(n) => {
                // Outside the OutBuf lock on purpose: scrollback has its own mutex and its own (much cheaper) contention profile, and no path anywhere holds both at once.
                // Stamped with THIS reader's generation: after a RESTART these bytes belong to a shell that no longer exists, and appending them would put a dead shell's tail above the new prompt in a ring buffer that was just cleared for it.
                append_scrollback(&read_buf[..n], Some(generation));
                let mut buf = lock.lock().unwrap();
                buf.acc.extend_from_slice(&read_buf[..n]);
                if buf.acc.len() >= FLUSH_BYTES || buf.last_flush.elapsed() >= FLUSH_INTERVAL {
                    // Fast path, unchanged: past the window already, so emit inline with zero added latency rather than paying a thread hop for it.
                    flush_locked(&app, &mut buf, generation);
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
        flush_locked(&app, &mut buf, generation);
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

    // Reached only when the slot still held OUR session (the check above), so this generation is by construction the current one — the argument documents the provenance rather than adding a second gate.
    append_scrollback(EXIT_NOTICE, Some(generation));
    // `alive: false` rides the notice as well as the separate `pty-exit` signal below: they travel different paths to a companion (`pty_output` vs `pty_exit` frames), and a screen must never be able to render the "[process exited]" line while still believing it has a live shell.
    let payload = PtyOutputPayload { data: STANDARD.encode(EXIT_NOTICE), reset: false, alive: Some(false) };
    let _ = app.emit("pty-output", payload);
    // Separate signal from the notice bytes: the frontend needs to flip its own alive state (to enable "type anything to respawn" and colour the tab), which it cannot infer from output.
    let _ = app.emit("pty-exit", ());
}

/// The pure half of the writer thread: pull chunks off the queue and write them, in order, until
/// the queue's last sender is dropped (a normal session teardown) or the PTY refuses a write.
///
/// Separated from `writer_loop` so the ordering guarantee can be tested against an ordinary sink
/// without touching the process-global `PtyState` — the module's tests run in parallel and anything
/// that drives the globals races the one test that already owns them.
fn drain_input_queue(rx: std::sync::mpsc::Receiver<Vec<u8>>, writer: &mut (impl Write + ?Sized)) -> std::io::Result<()> {
    // `recv()` blocks, which is exactly right: this thread exists to be parked.
    while let Ok(chunk) = rx.recv() {
        writer.write_all(&chunk)?;
        // Flushed per chunk, not per batch: a chunk is a keystroke or a paste, and the shell must
        // see it now — coalescing input would trade the responsiveness this terminal is judged on
        // for a throughput nobody is asking for on a human-typed stream.
        writer.flush()?;
    }
    Ok(())
}

/// The dedicated writer thread — a raw `std::thread` for the same reason `read_loop` and
/// `flusher_loop` are (module doc comment): it is parked in a blocking `recv()` for the session's
/// whole life, and parking a `spawn_blocking` slot forever starves every other blocking command.
///
/// One per session. Exits when `kill_current`/`spawn_if_absent` drops the sender, so the thread
/// count stays 1:1 with live sessions however hard RESTART is spammed.
fn writer_loop(rx: std::sync::mpsc::Receiver<Vec<u8>>, mut writer: Box<dyn Write + Send>, generation: u64) {
    if let Err(e) = drain_input_queue(rx, writer.as_mut()) {
        eprintln!("[pty] writing to the shell failed (session {}): {}", generation, e);
        // Retire OUR channel only. Without this a dead writer would keep accepting keystrokes into
        // a queue nobody drains — every key silently swallowed, which is worse than an error the
        // frontend can show. The generation test is what keeps this from stealing the channel a
        // `pty_restart` may already have installed in the meantime.
        let state = pty_state();
        let mut guard = state.input.lock().unwrap();
        if guard.as_ref().map(|c| c.generation) == Some(generation) {
            *guard = None;
        }
    }
}

/// Spawns the one shared PTY if it does not already exist; a no-op returning `Ok(())` on every later call (T-3). `cwd` (T-8) is only honoured on the first, actual spawn — no UI passes it yet, but the signature is settled now so the DEV/BUILD-redirect follow-up (plan §7) never needs a breaking change. Initial size is a placeholder 80x24; the host's `usePtyTerminal.js` calls `pty_resize` immediately after mount once it knows the real fit (T-4).
#[tauri::command]
pub async fn pty_spawn(app: AppHandle, cwd: Option<String>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let result = spawn_if_absent(app.clone(), cwd);
        // Broadcast liveness even on the no-op path: the caller is not the only screen, and the OTHER screen may still believe the shell is dead (it is the screen that did not press the button). Announcing the state rather than the event is what keeps the two in step regardless of who acted.
        emit_alive(&app);
        result
    })
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
    // Installed while the session lock is held (lock order `session` → `input`, see `PtyState`), so
    // no window exists in which a session is live but its input queue is still the dead one's.
    let (input_tx, input_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    *state.input.lock().unwrap() = Some(InputChannel { generation, tx: input_tx });
    *guard = Some(PtySession { master: pair.master, child, generation });
    drop(guard); // release before handing `app` to the new thread

    std::thread::spawn(move || writer_loop(input_rx, writer, generation));
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
    let killed = guard.as_ref().map(|s| s.generation);
    if let Some(session) = guard.as_mut() {
        // Group first, then the child itself — see `kill_process_group`. `child.kill()` stays as the backstop for the non-unix path and for a child that somehow is not a group leader.
        #[cfg(unix)]
        if let Some(pid) = session.child.process_id() {
            kill_process_group(pid);
        }
        let _ = session.child.kill();
        let _ = session.child.wait();
    }
    // Dropping the sender is what ends this session's writer thread (its `recv()` returns Err), and
    // it is also what makes a `pty_write` arriving after the kill fail loudly with "no PTY session"
    // instead of queueing keystrokes for a shell that no longer exists.
    *state.input.lock().unwrap() = None;
    *guard = None;
    drop(guard);
    // Its reader thread is still alive for a moment longer, holding bytes it has already read. Fence them off HERE — before `pty_restart` clears the buffer and spawns the replacement — so nothing that shell produced can reach a screen or the ring buffer again.
    if let Some(g) = killed {
        retire_generation(g);
    }
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
        // `None`: the notice is the app speaking, not the dead shell — it must land even though the session that prompted it has just been retired.
        append_scrollback(EXIT_NOTICE, None);
        let _ = app.emit("pty-output", PtyOutputPayload { data: STANDARD.encode(EXIT_NOTICE), reset: false, alive: Some(false) });
        let _ = app.emit("pty-exit", ());
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))
}

/// Kill + wipe scrollback + spawn a fresh shell, as one atomic user gesture — now the EXPLICIT RESTART button only. (Typing into an exited terminal used to come through here too; it goes to the idempotent `pty_spawn` instead, see `usePtyTerminal.js`'s `respawn`.) Emits a `reset` so every screen — host and companions alike — clears at the same moment instead of the phone keeping a dead shell's output above a live one's prompt.
///
/// THE ORDER OF THE THREE EMITS IS THE FIX, not decoration (plan §2.4): `reset` goes out while the slot is empty, so it is the last thing every screen sees before the new shell's first byte and cannot wipe the fresh prompt; `emit_alive` goes out after the spawn, so the value it reports is the new session's, not the corpse's. Emitting one combined payload either way round is wrong in one direction or the other.
#[tauri::command]
pub async fn pty_restart(app: AppHandle, cwd: Option<String>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        kill_current();
        pty_state().scrollback.lock().unwrap().clear();
        emit_reset(&app, b"");
        let result = spawn_if_absent(app.clone(), cwd);
        // Also emitted when the spawn failed — `emit_alive` reports what IS, and a screen that thinks it has a shell when the spawn just failed is the same class of lie this fix exists to remove.
        emit_alive(&app);
        result
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

/// Queues companion/host keystrokes for the PTY. `data` is base64 (see module doc comment).
///
/// SYNCHRONOUS ON PURPOSE — and this is the load-bearing half of the ordering fix, not an oversight
/// against CLAUDE.md's never-block-the-UI rule. That rule governs commands that WAIT: a subprocess,
/// an SSH round trip, a network call. This one does a base64 decode and an unbounded channel send;
/// it cannot block on anything, because the thing that used to block — the actual `write_all` into
/// the PTY — now happens on the writer thread (`writer_loop`).
///
/// Making it `async` would REINTRODUCE the bug it exists to fix. Tauri hands an async command to the
/// runtime, so two commands dispatched back to back can be polled concurrently on two workers, and
/// which of them reaches the enqueue first is a coin toss — the identical race the old session mutex
/// had, merely relocated. A synchronous command runs inline on the IPC dispatch thread, in the order
/// the webview posted the calls, so `l` then `s` enqueues as `l` then `s`, and the single-consumer
/// queue carries that order all the way to the shell. Ordering is a property of the structure here,
/// not of who wins a lock.
///
/// WHAT THIS GIVES UP, stated plainly: a write error can no longer be returned to the caller — the
/// write happens after this call has returned. It surfaces instead as the writer thread retiring the
/// channel (so the NEXT keystroke returns "no PTY session") plus the reader hitting EOF and emitting
/// `pty-exit`. That is the right trade: a per-keystroke error return nobody rendered, exchanged for
/// input that arrives in the order it was typed.
#[tauri::command]
pub fn pty_write(data: String) -> Result<(), String> {
    let bytes = STANDARD.decode(&data).map_err(|e| format!("invalid base64 input: {}", e))?;
    let state = pty_state();
    let guard = state.input.lock().unwrap();
    let channel = guard.as_ref().ok_or_else(|| "no PTY session — call pty_spawn first".to_string())?;
    channel
        .tx
        .send(bytes)
        .map_err(|_| "the PTY writer thread has gone — call pty_spawn first".to_string())
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

    /// Reads the ring buffer as a lossy string — every fixture below is ASCII, so this is only a convenience for the assertion messages.
    fn scrollback_text() -> String {
        String::from_utf8_lossy(&pty_state().scrollback.lock().unwrap()[..]).to_string()
    }

    /// The RESTART poisoning bug (plan §3.16), as an executable claim: a shell killed by RESTART keeps a reader thread alive for up to one `FLUSH_INTERVAL`, and the bytes it flushes in that window used to land in the ring buffer that `pty_restart` had just cleared for the NEW shell — i.e. above the fresh prompt, in a scrollback every screen then hydrates from.
    ///
    /// ONE TEST, NOT FOUR, ON PURPOSE: `pty_state()` is a process-global `OnceLock` and `cargo test` runs test functions on parallel threads, so two tests both driving the generation counters would race each other rather than the code under test. The scenario is therefore played out as one sequence.
    #[test]
    fn bytes_from_a_retired_session_never_reach_the_scrollback() {
        let state = pty_state();
        // Stand in for "session 7 is running" without spawning a real shell — this test is about the byte-path fence, not about the PTY.
        state.generation.store(7, Ordering::SeqCst);
        state.min_accepted_generation.store(0, Ordering::SeqCst);
        state.scrollback.lock().unwrap().clear();

        append_scrollback(b"output from the old shell\n", Some(7));
        assert!(scrollback_text().contains("old shell"), "a live session's own bytes must be appended");
        assert!(generation_accepted(7), "a live session's bytes must also be emittable");

        // What RESTART does, in order: kill (which retires the session), then wipe the buffer, then spawn.
        retire_generation(7);
        state.scrollback.lock().unwrap().clear();

        // The killed shell's reader thread, waking up ~20ms late with bytes it had already read.
        append_scrollback(b"TRAILING BYTES FROM THE CORPSE", Some(7));
        assert_eq!(scrollback_text(), "", "a retired session's trailing bytes must be dropped, not appended above the new prompt");
        assert!(!generation_accepted(7), "a retired session's bytes must not be emitted to any screen either");

        // The replacement shell writes into the same buffer and must be entirely unaffected.
        state.generation.store(8, Ordering::SeqCst);
        append_scrollback(b"new prompt", Some(8));
        assert_eq!(scrollback_text(), "new prompt", "the new session must own the freshly cleared buffer");
        assert!(generation_accepted(8));

        // The exit notice belongs to no session (`pty_kill` writes it right after retiring one), so it must land regardless of the fence.
        append_scrollback(EXIT_NOTICE, None);
        assert!(scrollback_text().contains("process exited"), "an unattributed notice must never be fenced off");

        // The fence must not have cost the ring buffer its cap.
        state.scrollback.lock().unwrap().clear();
        let chunk = vec![b'x'; SCROLLBACK_CAP / 2];
        for _ in 0..3 {
            append_scrollback(&chunk, Some(8));
        }
        assert_eq!(pty_state().scrollback.lock().unwrap().len(), SCROLLBACK_CAP, "SCROLLBACK_CAP must still bound the buffer");

        state.scrollback.lock().unwrap().clear();
        state.min_accepted_generation.store(0, Ordering::SeqCst);
    }

    /// A sink that records exactly what it was handed, in the order it was handed it — the observable
    /// the ordering guarantee is about.
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

    /// The ordering claim, executable: whatever `pty_write` enqueues reaches the shell in enqueue
    /// order, no matter how the producer and the writer are scheduled against each other.
    ///
    /// The old shape could not make this claim — each keystroke was an independent command racing
    /// for the session mutex, so the mutex serialised the writes without ordering them. The fixture
    /// deliberately keeps producing while the writer is already draining (no handshake between the
    /// two), which is exactly the interleaving that used to be a coin toss.
    #[test]
    fn queued_input_reaches_the_shell_in_the_order_it_was_enqueued() {
        let sink = Arc::new(StdMutex::new(Vec::new()));
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();

        let writer_sink = Arc::clone(&sink);
        let writer = std::thread::spawn(move || {
            let mut w = RecordingSink(writer_sink);
            drain_input_queue(rx, &mut w).expect("a recording sink never fails a write");
        });

        // 4096 distinct chunks: enough that any reordering shows up, and enough that the writer is
        // draining concurrently with the producer rather than starting after it.
        let mut expected = Vec::new();
        for i in 0..4096u32 {
            let chunk = format!("<{}>", i).into_bytes();
            expected.extend_from_slice(&chunk);
            tx.send(chunk).expect("writer thread is alive");
        }
        drop(tx); // last sender gone → drain_input_queue returns, exactly as a session teardown does
        writer.join().expect("writer thread must not panic");

        let written = sink.lock().unwrap().clone();
        assert_eq!(written, expected, "the byte stream handed to the PTY must be the enqueue order, unaltered");
    }

    /// A teardown must end the writer thread rather than leave it parked forever — one thread per
    /// live session, no matter how often RESTART is pressed.
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

    /// A PTY that refuses writes (the shell died between the enqueue and the write) must surface as
    /// an error the writer thread can act on, not as silently swallowed keystrokes.
    #[test]
    fn a_failing_pty_stops_the_writer_instead_of_swallowing_input() {
        struct DeadPty;
        impl Write for DeadPty {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "shell is gone"))
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
