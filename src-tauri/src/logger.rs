// Usage-flow structured logger.
//
// Three levels — production default is ERROR-only:
//   error  → always:     file + stderr
//   info   → debug-only: file + stderr
//   debug  → debug-only: file + stderr
//
// Enable info/debug with --debug flag or AKI_DEBUG=1 env var.
// Log file: {app_data_dir}/usage.log  (same directory as projects.json)
// File is trimmed to the most recent 512 KB whenever it exceeds 1 MB on startup.

use tauri::Manager;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static DEBUG_MODE: AtomicBool = AtomicBool::new(false);
static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn init(handle: &tauri::AppHandle) {
    let debug = std::env::args().any(|a| a == "--debug")
        || std::env::var("AKI_DEBUG").map(|v| !v.is_empty()).unwrap_or(false);
    DEBUG_MODE.store(debug, Ordering::Relaxed);

    let path = handle
        .path()
        .app_data_dir()
        .map(|d| d.join("usage.log"))
        .unwrap_or_else(|_| PathBuf::from("usage.log"));

    LOG_PATH.get_or_init(|| path.clone());

    if debug {
        eprintln!("[AKI] debug mode → log: {}", path.display());
    }

    maybe_truncate_log(&path);

    // STARTUP is always written to file as a session boundary marker.
    // stderr only in debug mode (end users don't see stderr in production).
    let ts = now_human();
    let msg = format!("aki-dev-sync started debug={} log={}", debug, path.display());
    let line = format!("[{}][STARTUP] {}\n", ts, msg);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(line.as_bytes());
    }
    if debug {
        eprint!("{}", line);
    }
}

/// Trim the log file to the most recent 512 KB when it exceeds 1 MB.
/// Finds a clean newline boundary so no partial lines are left.
fn maybe_truncate_log(path: &PathBuf) {
    const MAX_BYTES: u64 = 1_048_576;  // 1 MB
    const KEEP_BYTES: usize = 524_288; // keep newest 512 KB

    let size = match std::fs::metadata(path) {
        Ok(m) => m.len(),
        Err(_) => return,
    };
    if size <= MAX_BYTES {
        return;
    }

    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(_) => return,
    };

    let trim_start = data.len().saturating_sub(KEEP_BYTES);
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

/// Format UTC datetime as `YYYY-MM-DD HH:MM:SS.mmm` without external crates.
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

    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}", year, month, day, h, m, s, ms)
}

fn is_leap(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Internal: write one line to file and stderr. Only called when the level gate passes.
fn write_line(tag: &str, msg: &str) {
    let ts = now_human();
    let line = format!("[{}][{}] {}\n", ts, tag, msg);
    let path = log_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(line.as_bytes());
    }
    eprint!("{}", line);
}

/// Always written: file + stderr. Use for failures, unexpected states, data loss risk.
pub fn error(tag: &str, msg: &str) {
    write_line(tag, msg);
}

/// Written only when --debug / AKI_DEBUG=1. Use for key lifecycle events
/// (start, done, STALE_RESET, force-sync outcome).
pub fn info(tag: &str, msg: &str) {
    if is_debug() {
        write_line(tag, msg);
    }
}

/// Written only when --debug / AKI_DEBUG=1. Use for per-poll detail, parse internals,
/// shell output lines — anything too verbose for normal operation.
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
