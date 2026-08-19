// Usage-flow structured logger.
//
// Three levels - production default is ERROR-only:
//   error  → always:     file + stderr
//   info   → debug-only: file + stderr
//   debug  → debug-only: file + stderr
//
// Enable info/debug with --debug flag or AKI_DEBUG=1 env var.
// Log file: {app_data_dir}/usage.log  (same directory as projects.json)
// File is trimmed to the most recent 512 KB whenever it exceeds 1 MB - checked at startup AND as the file grows, because a long `--debug` session never restarts and used to grow without bound.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static DEBUG_MODE: AtomicBool = AtomicBool::new(false);
static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Serializes append + rotate.
/// Without it, rotation (read file, write tail) can drop lines appended concurrently by other worker threads.
static LOG_LOCK: Mutex<()> = Mutex::new(());

/// Bytes appended since the last size check.
/// Counter-based trigger avoids stat() syscall on hot path in debug mode; 64 KB drift vs 1 MB ceiling is irrelevant.
static BYTES_SINCE_CHECK: AtomicU64 = AtomicU64::new(0);
const CHECK_EVERY_BYTES: u64 = 65_536;

pub fn init(_handle: &tauri::AppHandle) {
    let debug = std::env::args().any(|a| a == "--debug")
        || std::env::var("AKI_DEBUG").map(|v| !v.is_empty()).unwrap_or(false);
    DEBUG_MODE.store(debug, Ordering::Relaxed);

    let path = crate::app_paths::app_data_dir()
        .map(|d| d.join("usage.log"))
        .unwrap_or_else(|_| PathBuf::from("usage.log"));

    LOG_PATH.get_or_init(|| path.clone());

    if debug {
        eprintln!("[AKI] debug mode → log: {}", path.display());
    }

    maybe_truncate_log(&path);

    // STARTUP is always written to file as a session boundary marker. stderr only in debug mode (end users don't see stderr in production).
    let ts = now_human();
    let msg = format!("aki-dev-sync started debug={} log={}", debug, path.display());
    let line = format!("[{}][STARTUP] {}\n", ts, msg);
    append_line(&path, &line);
    if debug {
        eprint!("{}", line);
    }
}

/// Appends one line, then rotates if the file has grown past the ceiling since the last check.
/// Every write in this module goes through here, so rotation can never again be tied to startup.
fn append_line(path: &PathBuf, line: &str) {
    let _guard = LOG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(line.as_bytes());
    }
    if tally(&BYTES_SINCE_CHECK, line.len() as u64) {
        maybe_truncate_log(path);
    }
}

/// Accumulates bytes and reports (resetting) whenever [`CHECK_EVERY_BYTES`] has been passed.
/// Separated out so the trigger is testable without touching the real log file.
fn tally(counter: &AtomicU64, added: u64) -> bool {
    let total = counter.fetch_add(added, Ordering::Relaxed) + added;
    if total >= CHECK_EVERY_BYTES {
        counter.store(0, Ordering::Relaxed);
        return true;
    }
    false
}

/// Trim the log file to the most recent 512 KB when it exceeds 1 MB.
/// Finds a clean newline boundary so no partial lines are left.
fn maybe_truncate_log(path: &PathBuf) {
    const MAX_BYTES: u64 = 1_048_576;  // 1 MB
    const KEEP_BYTES: usize = 524_288; // keep newest 512 KB
    truncate_to_tail(path, MAX_BYTES, KEEP_BYTES);
}

fn truncate_to_tail(path: &PathBuf, max_bytes: u64, keep_bytes: usize) {
    let size = match std::fs::metadata(path) {
        Ok(m) => m.len(),
        Err(_) => return,
    };
    if size <= max_bytes {
        return;
    }

    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(_) => return,
    };

    let trim_start = data.len().saturating_sub(keep_bytes);
    let line_start = data[trim_start..]
        .iter()
        .position(|&b| b == b'\n')
        .map(|i| trim_start + i + 1)
        .unwrap_or(trim_start);

    let _ = std::fs::write(path, &data[line_start..]);
}

pub fn is_debug() -> bool {
    DEBUG_MODE.load(Ordering::Relaxed)
}

fn log_path() -> PathBuf {
    LOG_PATH.get().cloned().unwrap_or_else(|| PathBuf::from("usage.log"))
}

/// Format UTC datetime as `YYYYMMDD.HHMMSS.mmm` (compact, optimised for high-volume log lines).
fn now_human() -> String {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    let ms = dur.subsec_millis();

    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;

    let mut days = secs / 86400;
    let mut year = 1970u64;
    loop {
        let dy = if is_leap(year) { 366 } else { 365 };
        if days < dy { break; }
        days -= dy;
        year += 1;
    }
    let month_len: [u64; 12] = [31, if is_leap(year) { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 1u64;
    for &md in &month_len {
        if days < md { break; }
        days -= md;
        month += 1;
    }
    let day = days + 1;

    format!("{:04}{:02}{:02}.{:02}{:02}{:02}.{:03}", year, month, day, h, m, s, ms)
}

fn is_leap(y: u64) -> bool {
    (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400)
}

/// Internal: write one line to file and stderr. Only called when the level gate passes.
fn write_line(tag: &str, msg: &str) {
    let ts = now_human();
    let line = format!("[{}][{}] {}\n", ts, tag, msg);
    append_line(&log_path(), &line);
    eprint!("{}", line);
}

/// Always written: file + stderr. Use for failures, unexpected states, data loss risk.
pub fn error(tag: &str, msg: &str) {
    write_line(tag, msg);
}

/// Written only when --debug / AKI_DEBUG=1. Use for key lifecycle events (start, done, STALE_RESET, force-sync outcome).
pub fn info(tag: &str, msg: &str) {
    if is_debug() {
        write_line(tag, msg);
    }
}

/// Written only when --debug / AKI_DEBUG=1. Use for per-poll detail, parse internals, shell output lines (verbose).
pub fn debug(tag: &str, msg: &str) {
    if is_debug() {
        write_line(tag, msg);
    }
}

#[tauri::command]
pub fn is_debug_mode() -> bool {
    is_debug()
}

#[tauri::command]
pub fn get_log_path() -> String {
    log_path().display().to_string()
}

/// Routes frontend log entry through backend pipeline (usage.log + stderr).
/// Only info/debug are gated by debug mode; "error" is always written (matching three-level contract).
#[tauri::command]
pub fn log_frontend(level: String, tag: String, msg: String) {
    match level.as_str() {
        "error" => error(&tag, &msg),
        "info"  => info(&tag, &msg),
        _       => debug(&tag, &msg),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("aki-logger-test-{}-{}.log", std::process::id(), name));
        let _ = std::fs::remove_file(&p);
        p
    }

    /// The rotation trigger. Uses a local counter so parallel tests in the same process do not perturb assertions.
    #[test]
    fn size_check_fires_once_per_threshold_then_resets() {
        let c = AtomicU64::new(0);
        assert!(!tally(&c, CHECK_EVERY_BYTES - 1), "fired before the threshold");
        assert!(tally(&c, 1), "did not fire on reaching the threshold");
        assert!(!tally(&c, 1), "counter was not reset after firing");
    }

    #[test]
    fn rotation_keeps_the_tail_and_cuts_on_a_line_boundary() {
        let path = scratch("rotate");
        let body: String = (0..500).map(|i| format!("line-{:04} padding padding padding\n", i)).collect();
        std::fs::write(&path, &body).unwrap();

        truncate_to_tail(&path, 1_000, 2_000);
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(after.len() <= 2_000, "kept more than requested: {}", after.len());
        assert!(!after.is_empty(), "everything was thrown away");
        assert!(after.starts_with("line-"), "cut mid-line: {:?}", &after[..20.min(after.len())]);
        assert!(after.ends_with("line-0499 padding padding padding\n"), "the newest line was lost");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn rotation_is_a_noop_below_the_ceiling() {
        let path = scratch("noop");
        std::fs::write(&path, "short\n").unwrap();
        truncate_to_tail(&path, 1_000, 500);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "short\n");
        let _ = std::fs::remove_file(&path);
    }

    /// The actual §3.21 defect: rotation was reachable only from `init`, so a long `--debug` session grew forever. Every append must trigger it.
    #[test]
    fn appending_past_the_ceiling_rotates_without_a_restart() {
        let path = scratch("grow");
        let line = format!("{}\n", "x".repeat(255));
        for _ in 0..8_000 {
            append_line(&path, &line);
        }
        let size = std::fs::metadata(&path).unwrap().len();
        // 2 MB was written; anything near that means rotation never ran. The ceiling plus one check window is the honest bound.
        assert!(size < 1_048_576 + CHECK_EVERY_BYTES, "log grew unchecked: {} bytes", size);
        let _ = std::fs::remove_file(&path);
    }
}
