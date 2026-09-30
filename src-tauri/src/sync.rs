use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::BufRead;
use std::io::BufReader;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::UNIX_EPOCH;
use tauri::{Emitter, Window};

use crate::conflict::{self, ClassifyInput, ConflictEntry, FileClass, RemoteFileMeta};
use crate::projects::{validate_path_segment, validate_project, SyncProject};

static RSYNC_VERSIONS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

// Cached once on first command invocation (run_sync or check_sync_status).
// The app data dir is fixed for the lifetime of the process.
static APP_DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

fn get_rsync_versions() -> &'static Mutex<HashMap<String, String>> {
    RSYNC_VERSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

// Caches the app data dir once per process so baseline_dir() resolves correctly; safe to call repeatedly from any command.
fn ensure_app_data_dir(_app: &tauri::AppHandle) {
    if APP_DATA_DIR.get().is_none() {
        if let Ok(dir) = crate::app_paths::app_data_dir() {
            let _ = APP_DATA_DIR.set(dir);
        }
    }
}

// ─── Sync transport bounds ────────────────────────────────────────────────────
// Transport timeouts bound how long a dead or silent host can hang a sync session.

// One answer in the whole app to "how long before we call a host dead" - the same value the usage poller uses (`agent_usage.rs::polling_ssh`).
const SSH_CONNECT_TIMEOUT: &str = "ConnectTimeout=10";

// Rsync stall timeout (120s): generous to allow slow large files without risking premature aborts on active transfers.
const RSYNC_IO_TIMEOUT: &str = "--timeout=120";

/// `ssh` with shared ConnectTimeout and host pre-applied; callers append remote arguments (options must precede host).
fn sync_ssh(host: &str) -> Command {
    let mut c = crate::system::create_command("ssh");
    c.args(["-o", SSH_CONNECT_TIMEOUT]);
    c.arg(host);
    c
}

/// Appends transport timeout flags (`-e ssh -o ConnectTimeout=10` and `--timeout=120`) so rsync's internal ssh transfer cannot hang indefinitely on a dead host.
fn push_transport_args(args: &mut Vec<String>) {
    args.push(RSYNC_IO_TIMEOUT.to_string());
    args.push("-e".to_string());
    args.push(format!("ssh -o {}", SSH_CONNECT_TIMEOUT));
}

/// POSIX single-quote escaping: wrap in `'…'` with embedded `'` replaced by `'\''`. Local to sync.rs to avoid coupling with terminal escapers.
fn shell_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Quotes a remote path for remote shell re-parsing; substitutes leading `~` with `"$HOME"` before single-quoting the rest so tilde expansion works with spaces.
fn quote_remote_path(path: &str) -> String {
    if path == "~" {
        return "\"$HOME\"".to_string();
    }
    match path.strip_prefix("~/") {
        Some("") => "\"$HOME\"".to_string(),
        Some(rest) => format!("\"$HOME\"/{}", shell_single_quote(rest)),
        None => shell_single_quote(path),
    }
}

// ─── Remote path → rsync argument ─────────────────────────────────────────────
// Rsync >=3.2.4 auto-escapes `host:path` chars on remote shells (manual quoting double-escapes). Paths pass unquoted; local rsync <3.2.4 is refused if path has shell-active chars.

/// Shell-active characters escaped by rsync >=3.2.4; `~` and wildcards are omitted so valid expansions and patterns remain untouched across rsync versions.
const REMOTE_SHELL_ACTIVE: &[char] = &[
    ' ', ';', '&', '|', '<', '>', '(', ')', '$', '`', '\\', '\'', '"', '{', '}', '#', '!',
];

fn first_shell_active_char(path: &str) -> Option<char> {
    path.chars().find(|c| REMOTE_SHELL_ACTIVE.contains(c))
}

fn version_triple(v: &str) -> (u32, u32, u32) {
    let mut parts = v.split('.').map(|p| {
        p.chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse::<u32>()
            .unwrap_or(0)
    });
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

/// True when rsync >= 3.2.4 (auto-protects remote path args). Unrecognized version strings (e.g. macOS stock openrsync) fail closed as unprotected.
fn rsync_protects_remote_args(version_line: &str) -> bool {
    let line = version_line.trim();
    if !line.starts_with("rsync") {
        return false;
    }
    match line
        .split_whitespace()
        .skip_while(|t| *t != "version")
        .nth(1)
    {
        Some(v) => version_triple(v) >= (3, 2, 4),
        None => false,
    }
}

/// First line of the local `rsync --version`, cached for the process (same map the sync log reads).
fn local_rsync_version_line() -> String {
    let mut map = get_rsync_versions()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(v) = map.get("local") {
        return v.clone();
    }
    let v = if let Ok(out) = crate::system::create_command("rsync")
        .arg("--version")
        .output()
    {
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .next()
            .unwrap_or("unknown")
            .to_string()
    } else {
        "unknown".to_string()
    };
    map.insert("local".to_string(), v.clone());
    v
}

/// `~/x` → `x`, `~` → `.`: rsync over SSH resolves a relative remote path from the login home - the directory
/// `~` names - but a literal `~` is only expanded when the remote shell parses the arg, which `--files-from`
/// and `--secluded-args` bypass (`change_dir "/home/u/~/x"`). Never returns "" - callers append `/`, and
/// `host:/` is the remote root.
fn home_relative(remote_path: &str) -> &str {
    let rest = if remote_path == "~" {
        ""
    } else if let Some(r) = remote_path.strip_prefix("~/") {
        r.trim_start_matches('/')
    } else {
        return remote_path;
    };
    if rest.is_empty() {
        "."
    } else {
        rest
    }
}

/// Single funnel for `host:path` rsync args; validates local rsync version only when `remote_path` contains shell-active characters.
fn remote_rsync_arg(host: &str, remote_path: &str) -> Result<String, String> {
    let remote_path = home_relative(remote_path);
    if let Some(c) = first_shell_active_char(remote_path) {
        let version = local_rsync_version_line();
        if !rsync_protects_remote_args(&version) {
            return Err(format!(
                "Remote path '{}' contains the shell character '{}', and the local rsync ({}) predates 3.2.4, so it would hand that path to the remote host's shell unprotected - the path would be split apart or executed instead of used as a directory. Install rsync 3.2.4 or newer (`brew install rsync`), or remove that character from the remote path.",
                remote_path,
                c,
                version.trim()
            ));
        }
    }
    Ok(format!("{}:{}", host, remote_path))
}

// ─── Running-sync registry (cancel + exit cleanup) ────────────────────────────
// Tracks process groups by project id so cancel and exit kill the entire process tree (rsync, ssh, hooks) via process groups (ref: `pty.rs::kill_process_group`).
static SYNC_PROCS: OnceLock<Mutex<HashMap<String, Vec<u32>>>> = OnceLock::new();

// Project IDs cancelled by user, allowing `run_sync` to distinguish user STOP from unexpected non-zero rsync failures.
static CANCELLED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn sync_procs() -> &'static Mutex<HashMap<String, Vec<u32>>> {
    SYNC_PROCS.get_or_init(|| Mutex::new(HashMap::new()))
}

// ─── In-flight sync guard (one sync per project) ──────────────────────────────
// Prevents concurrent syncs on the same project (GUI and companion relay); claimed on entry to `run_sync` and released via RAII `Drop`.
static SYNC_INFLIGHT: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn sync_inflight() -> &'static Mutex<HashSet<String>> {
    SYNC_INFLIGHT.get_or_init(|| Mutex::new(HashSet::new()))
}

struct SyncSlot(String);

impl SyncSlot {
    /// Claims the slot for `project_id`, or `Err` naming the project if a sync is already running.
    fn acquire(project_id: &str) -> Result<Self, String> {
        let mut set = sync_inflight().lock().unwrap_or_else(|e| e.into_inner());
        if !set.insert(project_id.to_string()) {
            return Err("A sync is already running for this project.".to_string());
        }
        Ok(SyncSlot(project_id.to_string()))
    }
}

impl Drop for SyncSlot {
    fn drop(&mut self) {
        sync_inflight()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.0);
    }
}

fn cancelled_ids() -> &'static Mutex<HashSet<String>> {
    CANCELLED.get_or_init(|| Mutex::new(HashSet::new()))
}

fn register_child(project_id: &str, pid: u32) {
    let mut map = sync_procs().lock().unwrap_or_else(|e| e.into_inner());
    map.entry(project_id.to_string()).or_default().push(pid);
}

fn unregister_child(project_id: &str, pid: u32) {
    let mut map = sync_procs().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(pids) = map.get_mut(project_id) {
        pids.retain(|p| *p != pid);
        if pids.is_empty() {
            map.remove(project_id);
        }
    }
}

/// Removes and returns the pids of ONE project's sync. Scoped to that project by construction -
/// stopping one sync must never touch another project's running transfer.
fn take_children(project_id: &str) -> Vec<u32> {
    let mut map = sync_procs().lock().unwrap_or_else(|e| e.into_inner());
    map.remove(project_id).unwrap_or_default()
}

/// Removes and returns every registered pid. Only for app exit, where "everything" IS the scope.
fn take_all_children() -> Vec<u32> {
    let mut map = sync_procs().lock().unwrap_or_else(|e| e.into_inner());
    std::mem::take(&mut *map).into_values().flatten().collect()
}

fn mark_cancelled(project_id: &str) {
    cancelled_ids()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(project_id.to_string());
}

fn consume_cancelled(project_id: &str) -> bool {
    cancelled_ids()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(project_id)
}

/// Signals the child process group (SIGTERM then SIGKILL escalation with signal-0 polling) to terminate rsync, spawned ssh, and remote transfers cleanly.
#[cfg(unix)]
fn kill_process_group(pid: u32) {
    let pgid = pid as libc::pid_t;
    unsafe { libc::killpg(pgid, libc::SIGTERM) };
    for _ in 0..12 {
        std::thread::sleep(std::time::Duration::from_millis(25));
        // Non-zero from the signal-0 probe means the group is gone - nothing left to escalate to.
        if unsafe { libc::killpg(pgid, 0) } != 0 {
            return;
        }
    }
    unsafe { libc::killpg(pgid, libc::SIGKILL) };
}

#[cfg(not(unix))]
fn kill_process_group(_pid: u32) {}

/// Places the spawned child in its own process group so `kill_process_group` does not signal the parent application process.
#[cfg(unix)]
fn detach_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    // SAFETY: `setpgid` is async-signal-safe and mutates only the freshly-forked child's own process group - within what `pre_exec` permits between fork and exec.
    unsafe {
        command.pre_exec(|| {
            libc::setpgid(0, 0);
            Ok(())
        });
    }
}

#[cfg(not(unix))]
fn detach_process_group(_command: &mut Command) {}

/// Stops all processes for `project_id`'s sync; returns true if killed. Async + spawn_blocking to avoid UI freezes during kill escalation sleeps (NEVER BLOCK THE UI).
#[tauri::command]
pub async fn cancel_sync(project_id: String) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let pids = take_children(&project_id);
        if pids.is_empty() {
            return false;
        }
        mark_cancelled(&project_id);
        for pid in pids {
            kill_process_group(pid);
        }
        true
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))
}

/// Kills all running sync processes unconditionally on app exit (wired to `RunEvent::Exit` in `lib.rs` alongside `pty::shutdown`).
pub fn shutdown() {
    for pid in take_all_children() {
        kill_process_group(pid);
    }
}

/// Fails if `local_path` is missing (e.g. unmounted volume) before file ops run, preventing destructive `--delete` mirror wipes while allowing project load/save.
fn ensure_local_path_present(local_path: &str) -> Result<(), String> {
    if std::path::Path::new(local_path).is_dir() {
        Ok(())
    } else {
        Err(format!(
            "Local path not found: '{}'. If it lives on an external or network volume, mount it and try again.",
            local_path
        ))
    }
}

#[derive(Serialize, Clone)]
struct LogPayload {
    project_id: String,
    line: String,
}

fn emit_log(window: &Window, project_id: &str, line: String) {
    let _ = window.emit(
        "sync-log",
        LogPayload {
            project_id: project_id.to_string(),
            line,
        },
    );
}

fn stream_reader<R: std::io::Read + Send + 'static>(
    reader: R,
    window: Window,
    project_id: String,
    prefix: &str,
) -> thread::JoinHandle<()> {
    let prefix = prefix.to_string();
    thread::spawn(move || {
        // Reads raw bytes and decodes lossily per line so non-UTF-8 filenames are visible with replacement characters instead of being dropped by `lines().flatten()`.
        let mut reader = BufReader::new(reader);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) => break,
                Ok(_) => {}
                Err(_) => break,
            }
            while matches!(buf.last(), Some(b'\n') | Some(b'\r')) {
                buf.pop();
            }
            let line = String::from_utf8_lossy(&buf);
            let _ = window.emit(
                "sync-log",
                LogPayload {
                    project_id: project_id.clone(),
                    line: format!("{}{}", prefix, line),
                },
            );
        }
    })
}

/// Spawns `command` with piped stdout/stderr, streams both to the sync-log event,
/// waits for exit, and returns Err if the process exits non-zero.
fn spawn_and_stream(
    command: &mut Command,
    window: &Window,
    project_id: &str,
    label: &str,
) -> Result<(), String> {
    // Null stdin prevents interactive prompt hangs; detached process group allows clean cancellation of child trees.
    detach_process_group(command);
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start {}: {}", label, e))?;

    let pid = child.id();
    register_child(project_id, pid);

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("Failed to capture {} stdout", label))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| format!("Failed to capture {} stderr", label))?;

    let t_out = stream_reader(stdout, window.clone(), project_id.to_string(), "");
    let t_err = stream_reader(stderr, window.clone(), project_id.to_string(), "[ERR] ");
    let _ = t_out.join();
    let _ = t_err.join();

    let wait_result = child.wait();
    unregister_child(project_id, pid);
    let status = wait_result.map_err(|e| format!("Error waiting for {}: {}", label, e))?;
    if !status.success() {
        return Err(format!(
            "{} exited with code: {}",
            label,
            status.code().unwrap_or(-1)
        ));
    }
    Ok(())
}

fn execute_hook(
    window: &Window,
    project: &SyncProject,
    cmd: &str,
    dry_prefix: &str,
) -> Result<(), String> {
    emit_log(
        window,
        &project.id,
        format!("\n>>> {}Executing hook: {}\n", dry_prefix, cmd),
    );
    let mut command = if project.hooks.run_hooks_on_remote {
        let mut c = sync_ssh(&project.remote_host);
        c.arg(cmd);
        c
    } else {
        let mut c = crate::system::create_command("sh");
        c.args(["-c", cmd]);
        c
    };
    spawn_and_stream(&mut command, window, &project.id, "hook")
}

fn run_hook_phase(
    window: &Window,
    project: &SyncProject,
    cmd: &Option<String>,
    dry_run: bool,
    dry_prefix: &str,
    phase_name: &str,
) -> Result<(), String> {
    if dry_run {
        emit_log(
            window,
            &project.id,
            format!("\n>>> {}Skipping {} hook\n", dry_prefix, phase_name),
        );
        return Ok(());
    }
    if let Some(c) = cmd {
        if !c.trim().is_empty() {
            if let Err(e) = execute_hook(window, project, c, dry_prefix) {
                if project.hooks.ignore_hook_errors {
                    emit_log(
                        window,
                        &project.id,
                        format!("[WARN] {} hook failed (ignored): {}\n", phase_name, e),
                    );
                } else {
                    return Err(e);
                }
            }
        }
    }
    Ok(())
}

fn validate_specific_paths(paths: &[String]) -> Result<(), String> {
    for p in paths {
        validate_path_segment("specific_path", p)?;
    }
    Ok(())
}

// ─── Tier 2 Baseline Manifest ─────────────────────────────────────────────────
// Written on full sync as HashMap<filename, mtime_secs> in appDataDir (with legacy ~/.aki fallback).
// * PULL: in baseline + missing locally -> local deleted (push_count); not in baseline -> remote created (pull_count).
// * PUSH: in baseline + mtime unchanged -> remote deleted (suppress); mtime changed -> local edit (push_count).
// * Baseline mtime comparison avoids extra SSH roundtrips to resolve deletion vs edit ambiguity.

// Test-only path injection (S5): so a test exercising the pre-1.32.0 baseline formats never resolves a path under the real home directory. Both are set/cleared together by sync_state.rs's `Scratch` guard.
#[cfg(test)]
pub(crate) static TEST_APP_DATA_DIR: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);
#[cfg(test)]
pub(crate) static TEST_LEGACY_BASELINE_DIR: std::sync::Mutex<Option<PathBuf>> =
    std::sync::Mutex::new(None);

#[cfg(test)]
pub(crate) fn set_test_dirs(app_data: Option<PathBuf>, legacy: Option<PathBuf>) {
    *TEST_APP_DATA_DIR.lock().unwrap_or_else(|e| e.into_inner()) = app_data;
    *TEST_LEGACY_BASELINE_DIR.lock().unwrap_or_else(|e| e.into_inner()) = legacy;
}

// Pre-appDataDir (<1.7.1) baseline location - sole source of truth for that path, used by baseline_dir()'s fallback, legacy_baseline_path(), and cleanup_legacy_baselines().
fn legacy_baseline_dir() -> PathBuf {
    #[cfg(test)]
    {
        if let Some(dir) = TEST_LEGACY_BASELINE_DIR.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            return dir;
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".aki").join("devsync-baselines")
}

fn baseline_dir() -> PathBuf {
    if let Some(dir) = APP_DATA_DIR.get() {
        return dir.join("baselines");
    }
    #[cfg(test)]
    {
        if let Some(dir) = TEST_APP_DATA_DIR.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            return dir.join("baselines");
        }
    }
    // Deterministic fallback (docs/plan/settings-and-state-layout.md § Migration, B3): resolve app-data
    // directly instead of the pre-1.7.1 legacy dir, so this is correct even when called before any Tauri
    // command has primed APP_DATA_DIR - e.g. the setup()-time boot migration, which runs before any IPC
    // command exists to serve that cache.
    crate::app_paths::app_data_dir()
        .map(|d| d.join("baselines"))
        .unwrap_or_else(|_| legacy_baseline_dir())
}

fn baseline_path(project_id: &str) -> PathBuf {
    baseline_dir().join(format!("{}.json", project_id))
}

// Legacy path from pre-appDataDir builds - read_baseline checks this as fallback.
fn legacy_baseline_path(project_id: &str) -> PathBuf {
    legacy_baseline_dir().join(format!("{}.json", project_id))
}

/// Test-only accessor so sync_state.rs's migration tests can seed a pre-1.32.0 per-project baseline file
/// at the exact path `legacy_flat_baseline`/`remove_legacy_flat_baseline` resolve, without duplicating
/// this module's private path logic (S5/S7).
#[cfg(test)]
pub(crate) fn baseline_path_for_test(project_id: &str) -> PathBuf {
    baseline_path(project_id)
}

/// True if `rel` matches or is nested under a `dir_excludes` entry (`/`-suffixed), matching path-component boundaries so sibling dirs (e.g. `.wrangler-backup`) do not match.
fn is_under_dir_exclude(rel: &str, dir_excludes: &[String]) -> bool {
    dir_excludes.iter().any(|e| {
        let trimmed = e.trim();
        // Only dir-entries (`/`-suffixed) carry push-only/exclude semantics here  - glob entries (`*.log`) never appear in the change list to reconcile against.
        if !trimmed.ends_with('/') {
            return false;
        }
        let name = trimmed.trim_end_matches('/');
        !name.is_empty() && (rel == name || rel.starts_with(&format!("{}/", name)))
    })
}

/// Selects excludes for direction (push -> `push_excludes`, pull -> `pull_excludes`; R1). Shared by `build_rsync_args` and `rsync_change_files` (ref: CHANGELOG 1.13.1 / R2 revert).
fn direction_excludes(project: &SyncProject, is_push: bool) -> &Vec<String> {
    if is_push {
        &project.push_excludes
    } else {
        &project.pull_excludes
    }
}

/// Deduped union of push and pull excludes; used by `write_baseline` so baseline does not track push-only-dir files (ref: CHANGELOG 1.13.1 on baseline vs status-check excludes).
fn union_excludes(project: &SyncProject) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for e in project
        .push_excludes
        .iter()
        .chain(project.pull_excludes.iter())
    {
        let key = e.trim().to_string();
        if key.is_empty() || !seen.insert(key) {
            continue;
        }
        out.push(e.clone());
    }
    out
}

fn collect_local_files_with_mtime(
    base: &std::path::Path,
    current: &std::path::Path,
    dir_excludes: &[String],
    out: &mut HashMap<String, u64>,
) {
    let entries = match std::fs::read_dir(current) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let rel = match path.strip_prefix(base) {
            Ok(r) => r.to_string_lossy().to_string(),
            Err(_) => continue,
        };
        if is_under_dir_exclude(&rel, dir_excludes) {
            continue;
        }
        if path.is_dir() {
            collect_local_files_with_mtime(base, &path, dir_excludes, out);
        } else {
            let mtime = path
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            out.insert(rel, mtime);
        }
    }
}

/// Applies F2 (docs/plan/settings-and-state-layout.md § A): after a push without `--delete`, a file
/// deleted locally has not reached the remote, so it is not common yet - carry its previous baseline
/// entry forward instead of letting the fresh local walk silently drop it (which the next status check
/// would otherwise misread as "remote created" -> pull). Mirror pushes (deletion propagated) and every
/// pull (a merge pull restores the file, a mirror pull matches the remote) need no carry-over.
/// S1 (docs/plan/settings-and-state-layout.md: "a baseline whose recorded remote_path differs from the
/// target's current one counts as NO baseline - the ancestor is someone else's"). The ONE place this
/// filter is spelled, shared by the post-sync write path and the status-check read path - previously
/// duplicated with the write path missing the filter entirely (S1).
fn baseline_for_target(
    baseline: Option<crate::sync_state::Baseline>,
    remote_path: &str,
) -> Option<crate::sync_state::Baseline> {
    baseline.filter(|b| b.remote_path == remote_path)
}

fn carry_forward_local_deletions(
    current: &mut HashMap<String, u64>,
    previous: Option<&HashMap<String, u64>>,
    is_push: bool,
    is_mirror: bool,
) {
    if !is_push || is_mirror {
        return;
    }
    let Some(previous) = previous else { return };
    for (path, mtime) in previous {
        if !current.contains_key(path) {
            current.insert(path.clone(), *mtime);
        }
    }
}

/// Pre-1.32.0 flat baseline file (one per project, no host) - read-only now, kept solely so
/// `sync_state::migrate_settings_and_state` can move its contents into the per-host store once.
pub(crate) fn legacy_flat_baseline(project_id: &str) -> Option<HashMap<String, u64>> {
    read_baseline(project_id)
}

/// Deletes the pre-1.32.0 flat baseline file after its contents have been migrated.
pub(crate) fn remove_legacy_flat_baseline(project_id: &str) {
    let _ = std::fs::remove_file(baseline_path(project_id));
    let _ = std::fs::remove_file(legacy_baseline_path(project_id));
}

/// One-shot migration copying pre-1.7.1 baselines from `~/.aki/devsync-baselines` into appDataDir, then cleaning legacy dir (non-destructive; rewritable on next full sync).
#[tauri::command]
pub fn cleanup_legacy_baselines(app: tauri::AppHandle) -> Result<bool, String> {
    ensure_app_data_dir(&app);

    let legacy_dir = legacy_baseline_dir();
    if !legacy_dir.exists() {
        return Ok(false);
    }

    let new_dir = baseline_dir();
    std::fs::create_dir_all(&new_dir).map_err(|e| format!("baseline mkdir: {}", e))?;

    if let Ok(entries) = std::fs::read_dir(&legacy_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Some(name) = path.file_name() {
                let dest = new_dir.join(name);
                if !dest.exists() {
                    let _ = std::fs::copy(&path, &dest);
                }
            }
        }
    }

    std::fs::remove_dir_all(&legacy_dir).map_err(|e| format!("legacy baseline cleanup: {}", e))?;
    Ok(true)
}

fn read_baseline(project_id: &str) -> Option<HashMap<String, u64>> {
    let content = std::fs::read_to_string(baseline_path(project_id))
        .or_else(|_| std::fs::read_to_string(legacy_baseline_path(project_id)))
        .ok()?;
    // New format (≥1.7.1): HashMap<String, u64> - filename → mtime_secs
    if let Ok(map) = serde_json::from_str::<HashMap<String, u64>>(&content) {
        return Some(map);
    }
    // Old format (<1.7.1): Vec<String> - migrate with mtime=0 so suppression is disabled for all entries until the next successful sync writes a new-format baseline.
    if let Ok(files) = serde_json::from_str::<Vec<String>>(&content) {
        return Some(files.into_iter().map(|f| (f, 0u64)).collect());
    }
    None
}

fn build_rsync_args(
    project: &SyncProject,
    is_push: bool,
    dry_run: bool,
    specific_paths: &[String],
    src: &str,
    dest: &str,
) -> Vec<String> {
    // Mirror mode (--delete ON) drops `-u` (-avz) so sender overwrites receiver-newer files; merge mode (--delete OFF) keeps `-u` (-avzu) to preserve receiver-newer files.
    let is_mirror = (is_push && project.delete_on_push) || (!is_push && project.delete_on_pull);
    let base_flags = if is_mirror { "-avz" } else { "-avzu" };
    let mut args = vec![base_flags.to_string()];
    if dry_run {
        args.push("--dry-run".to_string());
    }
    push_transport_args(&mut args);

    if !specific_paths.is_empty() && is_push {
        args.push("-R".to_string());
        for p in specific_paths {
            args.push(p.clone());
        }
        args.push(dest.to_string());
    } else {
        let excludes = direction_excludes(project, is_push);

        for e in excludes {
            if !e.trim().is_empty() {
                args.push(format!("--exclude={}", e));
            }
        }
        if is_mirror {
            args.push("--delete".to_string());
            // Protect the receiver's task list from a mirror wipe when the sender never had it
            // (e.g. a PULL before the project's first PUSH) - --delete would otherwise remove it.
            args.push("--filter=P .akidevsync/".to_string());
        }

        args.push(src.to_string());
        args.push(dest.to_string());
    }

    args
}

// Async so Tauri IPC returns a Promise to JS immediately (no observable UI freeze).
// All blocking subprocess work runs inside spawn_blocking to avoid starving the async executor.
#[tauri::command]
pub async fn run_sync(
    window: Window,
    project: SyncProject,
    direction: String,
    dry_run: bool,
    specific_paths: Vec<String>,
) -> Result<(), String> {
    validate_project(&project)?;
    validate_specific_paths(&specific_paths)?;
    let project_id = project.id.clone();
    let audit_head = format!(
        "{} {}{} {} <-> {}:{}{}",
        project.id, direction, if dry_run { " (dry)" } else { "" }, project.local_path, project.remote_host, project.remote_path,
        if specific_paths.is_empty() { String::new() } else { format!(" [{} path(s)]", specific_paths.len()) },
    );
    // Held until this command returns, so a second invoke (companion seam included) is refused rather than run concurrently against the same tree.
    let _slot = SyncSlot::acquire(&project_id)?;
    // Drop any flag left by a previous run so this sync's outcome is judged on its own.
    consume_cancelled(&project_id);
    let result = tauri::async_runtime::spawn_blocking(move || {
        // Existence check runs inside spawn_blocking closure to avoid UI thread freezes on unmounted SMB/NFS kernel stalls (stack-tauri A1).
        ensure_local_path_present(&project.local_path)?;
        run_sync_blocking(window, project, direction, dry_run, specific_paths)
    })
    .await
    .map_err(|e| format!("Sync task error: {}", e))?;

    // A cancelled sync fails as "rsync exited with code: -1" (killed by a signal). Say what actually happened instead - the user pressed STOP; that is not an error they need to debug.
    let result = match result {
        Err(_) if consume_cancelled(&project_id) => Err("Sync stopped".to_string()),
        other => other,
    };
    crate::logger::audit("sync", &match &result {
        Ok(()) => format!("{audit_head} => ok"),
        Err(e) => format!("{audit_head} => {e}"),
    });
    result
}

fn run_sync_blocking(
    window: Window,
    project: SyncProject,
    direction: String,
    dry_run: bool,
    specific_paths: Vec<String>,
) -> Result<(), String> {
    let mut project = project;
    // Excludes always come from project.json read fresh here, never the JS-passed object - refuses rather than syncing on stale or empty excludes (docs/plan/settings-and-state-layout.md).
    let (pull_excludes, push_excludes) =
        crate::project_config::require_ok_excludes(&project.local_path)?;
    project.pull_excludes = pull_excludes;
    project.push_excludes = push_excludes;

    let is_push = direction == "push";
    let dry_prefix = if dry_run { "[DRY RUN] " } else { "" };

    // First log line emits before SSH work to close the latency gap between UI click and rsync output.
    emit_log(
        &window,
        &project.id,
        format!(
            ">>> {}Connecting to {}...\n",
            dry_prefix, project.remote_host
        ),
    );

    let pre_cmd = if is_push {
        &project.hooks.pre_push_cmd
    } else {
        &project.hooks.pre_pull_cmd
    };
    run_hook_phase(&window, &project, pre_cmd, dry_run, dry_prefix, "pre-sync")?;

    let local = format!("{}/", project.local_path.trim_end_matches('/'));
    let remote = project.remote_path.trim_end_matches('/');
    // Refused here, before the remote mkdir and before rsync spawns, if this rsync cannot protect the path from the remote shell.
    let remote_full = format!("{}/", remote_rsync_arg(&project.remote_host, remote)?);

    let (src, dest) = if is_push {
        (&local, &remote_full)
    } else {
        (&remote_full, &local)
    };

    if is_push {
        if !dry_run {
            // Remote path is quoted via quote_remote_path with leading `~` expanded as $HOME so remote shell space parsing cannot split directories.
            let mkdir_out = sync_ssh(&project.remote_host)
                .arg(format!(
                    "mkdir -p {}",
                    quote_remote_path(&project.remote_path)
                ))
                .output()
                .map_err(|e| {
                    format!(
                        "Failed to create remote directory '{}': {}",
                        project.remote_path, e
                    )
                })?;
            if !mkdir_out.status.success() {
                return Err(format!(
                    "Remote mkdir failed for '{}': {}",
                    project.remote_path,
                    String::from_utf8_lossy(&mkdir_out.stderr)
                ));
            }
        }
    } else {
        std::fs::create_dir_all(&project.local_path)
            .map_err(|e| format!("Failed to create local directory: {}", e))?;
    }

    let args = build_rsync_args(&project, is_push, dry_run, &specific_paths, src, dest);

    let versions_map = get_rsync_versions();

    // Same cache entry the remote-path guard reads (`local_rsync_version_line`).
    let local_v_str = local_rsync_version_line();

    let remote_v_str = {
        let mut map = versions_map.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(v) = map.get(&project.remote_host) {
            v.clone()
        } else {
            let v = if let Ok(out) = sync_ssh(&project.remote_host)
                .args(["rsync", "--version"])
                .output()
            {
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .next()
                    .unwrap_or("unknown")
                    .to_string()
            } else {
                "unknown".to_string()
            };
            map.insert(project.remote_host.clone(), v.clone());
            v
        }
    };

    let log_str = format!(
        ">>> {}Local Rsync: {}\n>>> {}Remote Rsync: {}\n",
        dry_prefix,
        local_v_str.trim(),
        dry_prefix,
        remote_v_str.trim()
    );
    emit_log(&window, &project.id, log_str);

    emit_log(
        &window,
        &project.id,
        format!(
            ">>> {}Executing command: rsync {}\n",
            dry_prefix,
            args.join(" ")
        ),
    );

    let mut command = crate::system::create_command("rsync");
    if !specific_paths.is_empty() && is_push {
        command.current_dir(&project.local_path);
    }

    spawn_and_stream(command.args(&args), &window, &project.id, "rsync")?;

    // Write baseline after a full (non-dry, non-partial) sync so the next status check can classify PULL/PUSH files against the last-known-good state for THIS (project, host) pair (EC-3).
    if !dry_run && specific_paths.is_empty() {
        let dir_excludes = union_excludes(&project);
        let mut files: HashMap<String, u64> = HashMap::new();
        collect_local_files_with_mtime(
            std::path::Path::new(&project.local_path),
            std::path::Path::new(&project.local_path),
            &dir_excludes,
            &mut files,
        );
        let is_mirror = (is_push && project.delete_on_push) || (!is_push && project.delete_on_pull);
        let previous = baseline_for_target(
            crate::sync_state::read_baseline(&project.id, &project.remote_host),
            &project.remote_path,
        );
        carry_forward_local_deletions(
            &mut files,
            previous.as_ref().map(|b| &b.files),
            is_push,
            is_mirror,
        );
        let baseline = crate::sync_state::Baseline {
            remote_path: project.remote_path.clone(),
            files,
        };
        if let Err(e) = crate::sync_state::write_baseline(&project.id, &project.remote_host, &baseline) {
            emit_log(
                &window,
                &project.id,
                format!("[WARN] Baseline write failed (non-fatal): {}\n", e),
            );
        }
    }

    let post_cmd = if is_push {
        &project.hooks.post_push_cmd
    } else {
        &project.hooks.post_pull_cmd
    };
    run_hook_phase(
        &window,
        &project,
        post_cmd,
        dry_run,
        dry_prefix,
        "post-sync",
    )?;

    emit_log(
        &window,
        &project.id,
        format!("\n>>> SYNC COMPLETED SUCCESSFULLY{}! <<<\n", dry_prefix),
    );
    Ok(())
}

/// Pulls a single named file (e.g. REPORT.html) via rsync without invoking full push/pull pipeline; guards remote path against shell injection.
pub fn rsync_pull_file(
    host: &str,
    remote_dir: &str,
    filename: &str,
    local_dir: &str,
) -> Result<(), String> {
    // Directory and filename are one remote-shell word each - guard the joined path, since the filename reaches the same shell the directory does.
    let remote_src = remote_rsync_arg(
        host,
        &format!("{}/{}", remote_dir.trim_end_matches('/'), filename),
    )?;
    let local_dest = format!("{}/{}", local_dir.trim_end_matches('/'), filename);
    let mut args = vec!["-az".to_string()];
    push_transport_args(&mut args);
    args.push(remote_src);
    args.push(local_dest);
    let out = crate::system::create_command("rsync")
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to run rsync: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "rsync failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(())
}

/// Expands `~/` or `~` to `$HOME` for use in remote shell contexts.
pub fn expand_remote_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        path.replacen("~/", "$HOME/", 1)
    } else if path == "~" {
        "$HOME".to_string()
    } else {
        path.to_string()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SyncStatusResult {
    pub has_local_changes: bool,
    pub has_remote_changes: bool,
    pub push_count: u32,
    pub pull_count: u32,
    /// Files classified as a real (or unverified, above the checksum cap) conflict - already excluded from
    /// `push_count`/`pull_count` (docs/plan/conflict-detection-and-agy-report.md §4).
    #[serde(default)]
    pub conflicts: Vec<ConflictEntry>,
    /// How many of `push_count + pull_count` are under `.git/` - tooltip breakdown only, never subtracted.
    #[serde(default)]
    pub git_count: u32,
    /// At least one push file was classified `PushStaleRemote` (R < B - the remote regressed relative to
    /// the common ancestor) - the plan's "whether the remote is behind" signal (§4/§5), surfaced instead of
    /// being folded silently into `push_count`.
    #[serde(default)]
    pub remote_behind: bool,
    /// Per-top-directory, per-class breakdown of both lists (§4: "per-class, per-top-directory counts for
    /// both lists") - keyed by the path's first component, or `"."` for a root-level file.
    #[serde(default)]
    pub by_top_dir: HashMap<String, TopDirCounts>,
}

/// One top-level directory's class breakdown (`SyncStatusResult.by_top_dir`'s value). `.git` files land in
/// `git` only, even though a `.git` file is ALSO folded into the top-level `push_count`/`pull_count` on
/// `SyncStatusResult` (so `push + pull + git + stale_remote` here is NOT the same total as
/// `push_count + pull_count + git_count`, whose `.git` share is double-counted across two fields there).
/// `stale_remote` counts a `PushStaleRemote` file instead of `push`, so it is also excluded from `push` here.
/// Conflicts are not represented in this struct at all - they live only in `SyncStatusResult.conflicts`.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct TopDirCounts {
    #[serde(default)]
    pub push: u32,
    #[serde(default)]
    pub pull: u32,
    #[serde(default)]
    pub git: u32,
    #[serde(default)]
    pub stale_remote: u32,
}

/// The path's first `/`-separated component, or `"."` for a root-level file - the grouping key for
/// `SyncStatusResult.by_top_dir`.
fn top_dir_of(path: &str) -> String {
    match path.split_once('/') {
        Some((first, _)) if !first.is_empty() => first.to_string(),
        _ => ".".to_string(),
    }
}

/// Lists additively transferable files. Always uses -avzu without --delete so `-u` limits counts to source-newer files (EC-7) and avoids deletion count inflation (EC-2).
fn rsync_change_files(project: &SyncProject, is_push: bool) -> Result<Vec<String>, String> {
    let local = format!("{}/", project.local_path.trim_end_matches('/'));
    let remote = format!(
        "{}/",
        remote_rsync_arg(
            &project.remote_host,
            project.remote_path.trim_end_matches('/')
        )?
    );
    let (src, dest) = if is_push {
        (local.as_str(), remote.as_str())
    } else {
        (remote.as_str(), local.as_str())
    };

    // Per-direction excludes (R1): status badge counts exact files transferred (e.g. push-only dir counted on push; ref: CHANGELOG 1.13.1 & docs/plan/done/push-only-paths.md §9).
    let mut args: Vec<String> = vec!["-avzu".to_string(), "--dry-run".to_string()];
    push_transport_args(&mut args);
    for e in direction_excludes(project, is_push) {
        if !e.trim().is_empty() {
            args.push(format!("--exclude={}", e));
        }
    }
    args.push(src.to_string());
    args.push(dest.to_string());

    // Tolerate APFS (ns) vs ext4 (1s) mtime precision gap.
    let insert_pos = args.len().saturating_sub(2);
    args.insert(insert_pos, "--modify-window=2".to_string());

    let output = crate::system::create_command("rsync")
        .args(&args)
        .output()
        .map_err(|e| format!("rsync status check failed: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("rsync exited non-zero: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(filter_file_change_lines(&stdout))
}

/// Lines a raw (non `--out-format`) rsync dry-run stdout carries that are summary/status text, never an
/// actual file path - shared by every place that reads plain rsync dry-run output for file names
/// (`rsync_change_files`'s push side and `rsync_checksum_residue`'s `-c` residue check).
fn filter_file_change_lines(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter_map(|line| {
            let l = line.trim();
            if l.is_empty()
                || l.starts_with("deleting ")
                || l.starts_with("sending ")
                || l.starts_with("receiving ")
                || l.starts_with("sent ")
                || l.starts_with("received ")
                || l.starts_with("total size")
                || l.starts_with("Number of")
                || l.starts_with("building file list")
                || l.starts_with("Transfer starting:")
                || l.starts_with("Skip newer ")
                || l.ends_with('/')
            {
                None
            } else {
                Some(l.to_string())
            }
        })
        .collect()
}

/// The baseline reclassification (EC-3): a PULL entry the baseline already knew about but that no longer
/// exists locally is a LOCAL deletion that has not reached the remote yet - push_count, never pull_count
/// (N-b: decomposed out of `compute_sync_counts` purely so this is testable without a real rsync/SSH round
/// trip). A PUSH entry whose local mtime still matches the baseline was never touched locally, so it is a
/// REMOTE deletion and is suppressed rather than pushed.
fn classify_sync_counts(
    push_files: Vec<String>,
    pull_files: Vec<String>,
    baseline: Option<&HashMap<String, u64>>,
    local_path: &str,
) -> (u32, u32) {
    // PULL side: Mac deleted file since last sync → should push the deletion, not pull
    let (reclassified_to_push, real_pull): (Vec<_>, Vec<_>) =
        pull_files.into_iter().partition(|f| {
            if let Some(bl) = baseline {
                let local_full = std::path::Path::new(local_path).join(f);
                bl.contains_key(f) && !local_full.exists()
            } else {
                false
            }
        });

    // PUSH side: suppress only when local mtime matches baseline mtime, meaning the file was NOT modified locally since last sync → remote deleted it (not a local edit).
    let (_, real_push): (Vec<_>, Vec<_>) = push_files.into_iter().partition(|f| {
        if let Some(bl) = baseline {
            if let Some(&baseline_mtime) = bl.get(f) {
                if baseline_mtime == 0 {
                    return false; // Old-format entry - conservative: don't suppress
                }
                let local_full = std::path::Path::new(local_path).join(f);
                let current_mtime = local_full
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                current_mtime == baseline_mtime
            } else {
                false
            }
        } else {
            false
        }
    });

    (
        real_push.len() as u32 + reclassified_to_push.len() as u32,
        real_pull.len() as u32,
    )
}

/// Computes (push_count, pull_count) with Tier-2 baseline reclassification (EC-3: local deletion -> push_count; remote deletion without local edit -> suppress push_count).
/// Takes an already-fetched `push_files` list instead of re-running the
/// push dry-run - the degrade paths in `compute_sync_status_full` always already have one, so reusing it
/// halves the redundant SSH round-trips on a degrade ("degraded path reuses outputs already
/// fetched").
fn compute_sync_counts_with_push_files(
    project: &SyncProject,
    push_files: Vec<String>,
) -> Result<(u32, u32), String> {
    let pull_files = rsync_change_files(project, false)?;

    // A baseline recorded against a different remote_path is someone else's ancestor - counts as no
    // baseline at all (docs/plan/settings-and-state-layout.md: "the path changed, so the ancestor is
    // someone else's"), never a guessed classification against the wrong tree.
    let baseline = baseline_for_target(
        crate::sync_state::read_baseline(&project.id, &project.remote_host),
        &project.remote_path,
    )
    .map(|b| b.files);

    Ok(classify_sync_counts(push_files, pull_files, baseline.as_ref(), &project.local_path))
}

/// The pull-side dry-run for conflict detection (docs/plan/conflict-detection-and-agy-report.md §1):
/// drops `-u` (so every differing file is listed, not just remote-newer ones) and adds `--out-format` to
/// get the remote's size and mtime from the one existing SSH round-trip - the `-u` filter is reapplied in
/// Rust (`conflict::select_pull_after_u_filter`) so `pull_count` keeps meaning "remote is newer."
fn rsync_pull_diff_with_metadata(project: &SyncProject) -> Result<conflict::ParsedRemoteDiff, String> {
    let local = format!("{}/", project.local_path.trim_end_matches('/'));
    let remote = format!(
        "{}/",
        remote_rsync_arg(&project.remote_host, project.remote_path.trim_end_matches('/'))?
    );

    let mut args: Vec<String> = vec![
        "-avz".to_string(),
        "--dry-run".to_string(),
        "--out-format=%n\t%l\t%M".to_string(),
    ];
    push_transport_args(&mut args);
    for e in direction_excludes(project, false) {
        if !e.trim().is_empty() {
            args.push(format!("--exclude={}", e));
        }
    }
    args.push(remote);
    args.push(local);
    let insert_pos = args.len().saturating_sub(2);
    args.insert(insert_pos, "--modify-window=2".to_string());

    let output = crate::system::create_command("rsync")
        .args(&args)
        .output()
        .map_err(|e| format!("rsync conflict-detect pull dry-run failed: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("rsync exited non-zero: {}", stderr.trim()));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(conflict::parse_out_format_output(&stdout, &conflict::local_utc_offset_secs_at))
}

/// Local mtime (secs) and size for one project-relative path, or `(None, None)` when it does not exist -
/// the "L absent" input to `conflict::classify_file`.
fn local_file_stat(local_path: &str, rel: &str) -> (Option<u64>, Option<u64>) {
    let full = std::path::Path::new(local_path).join(rel);
    match full.metadata() {
        Ok(m) if m.is_file() => {
            let mtime = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs());
            (mtime, Some(m.len()))
        }
        _ => (None, None),
    }
}

/// Checksums exactly the given residue (§3: same-size files that both changed since the baseline) with one
/// `rsync -c --dry-run --files-from=<residue>` call, and returns the subset it still lists as differing
/// (real content conflicts) - an empty residue makes no rsync call at all.
fn rsync_checksum_residue(project: &SyncProject, residue: &[String]) -> Result<HashSet<String>, String> {
    if residue.is_empty() {
        return Ok(HashSet::new());
    }
    let local = format!("{}/", project.local_path.trim_end_matches('/'));
    let remote = format!(
        "{}/",
        remote_rsync_arg(&project.remote_host, project.remote_path.trim_end_matches('/'))?
    );
    let list_path = std::env::temp_dir().join(format!(
        "aki-devsync-checksum-{}-{}.txt",
        project.id,
        std::process::id()
    ));
    std::fs::write(&list_path, residue.join("\n"))
        .map_err(|e| format!("checksum residue list write: {}", e))?;

    let mut args: Vec<String> = vec![
        "-avzc".to_string(),
        "--dry-run".to_string(),
        format!("--files-from={}", list_path.display()),
    ];
    push_transport_args(&mut args);
    args.push(remote);
    args.push(local);
    let insert_pos = args.len().saturating_sub(2);
    args.insert(insert_pos, "--modify-window=2".to_string());

    let output = crate::system::create_command("rsync").args(&args).output();
    let _ = std::fs::remove_file(&list_path);
    let output = output.map_err(|e| format!("rsync checksum residue failed: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("rsync exited non-zero: {}", stderr.trim()));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(filter_file_change_lines(&stdout).into_iter().collect())
}

/// Cache key for a same-size checksum residue result: (project, host, path, local mtime, remote mtime,
/// size). Any change to L/R/size naturally misses the cache and re-checksums - correctness never depends on
/// eviction. "converged checksum results cached in memory ... so they don't re-run every poll."
type ChecksumCacheKey = (String, String, String, u64, u64, u64);

static CHECKSUM_CACHE: OnceLock<Mutex<HashMap<ChecksumCacheKey, bool>>> = OnceLock::new();

fn checksum_cache() -> &'static Mutex<HashMap<ChecksumCacheKey, bool>> {
    CHECKSUM_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Last status-check anomaly logged per (project, host, kind): one poll can fail in several independent steps
/// with different messages, so a single slot per host would be overwritten by the sibling step and every poll
/// would log both again. An unchanged failure of one kind is logged once, not once a minute.
static LAST_STATUS_ANOMALY: OnceLock<Mutex<HashMap<(String, String, &'static str), String>>> = OnceLock::new();

fn status_anomalies() -> &'static Mutex<HashMap<(String, String, &'static str), String>> {
    LAST_STATUS_ANOMALY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// True when `msg` differs from the last one recorded for this project, host and failure kind.
fn record_status_anomaly(project: &SyncProject, kind: &'static str, msg: &str) -> bool {
    let key = (project.id.clone(), project.remote_host.clone(), kind);
    let mut seen = status_anomalies().lock().unwrap_or_else(|e| e.into_inner());
    if seen.get(&key).map(String::as_str) == Some(msg) {
        return false;
    }
    seen.insert(key, msg.to_string());
    true
}

fn log_status_anomaly(project: &SyncProject, kind: &'static str, msg: &str) {
    if record_status_anomaly(project, kind, msg) {
        crate::logger::error(
            "sync",
            &format!("status check {} ({}) @ {}: {}", project.id, project.local_path, project.remote_host, msg),
        );
    }
}

fn clear_status_anomaly(project: &SyncProject) {
    status_anomalies()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .retain(|(id, host, _), _| !(id == &project.id && host == &project.remote_host));
}

/// Pure output of `classify_candidates` - everything `compute_sync_status_full` needs before the checksum
/// residue round-trip. Extracted as its own function (no rsync/SSH call inside) so the classification,
/// direction-fallback, and remote-deletion-suppression logic is unit-testable without a real
/// rsync/SSH round-trip - `compute_sync_status_full` remains the impure caller that gathers the inputs.
#[derive(Debug, Default)]
struct ClassificationOutcome {
    push_count: u32,
    pull_count: u32,
    git_count: u32,
    remote_behind: bool,
    by_top_dir: HashMap<String, TopDirCounts>,
    conflicts: Vec<ConflictEntry>,
    needs_checksum: Vec<String>,
    needs_checksum_meta: HashMap<String, (u64, u64, u64)>,
}

/// Classifies every candidate path via `conflict::classify_file` and accumulates counts, the per-top-directory
/// breakdown, the remote-behind signal, and the residue that still needs a checksum. Pure: no I/O, no rsync,
/// no SSH - `local_stats`/`remote_by_path`/`baseline_files`/`push_set`/`pull_after_u_set` are all pre-gathered
/// by the caller.
fn classify_candidates(
    candidates: &[String],
    local_stats: &HashMap<String, (Option<u64>, Option<u64>)>,
    remote_by_path: &HashMap<&str, &RemoteFileMeta>,
    baseline_files: Option<&HashMap<String, u64>>,
    has_baseline: bool,
    push_set: &HashSet<&str>,
    pull_after_u_set: &HashSet<&str>,
) -> ClassificationOutcome {
    let mut out = ClassificationOutcome::default();

    for path in candidates {
        let is_git = path.starts_with(".git/") || path.contains("/.git/");
        let (local_mtime, local_size) = local_stats.get(path).copied().unwrap_or((None, None));
        let remote_meta = remote_by_path.get(path.as_str()).copied();
        let baseline_mtime = baseline_files.and_then(|f| f.get(path)).copied();

        let input = ClassifyInput {
            is_git,
            has_baseline,
            baseline_mtime,
            local_mtime,
            remote_mtime: remote_meta.map(|m| m.mtime),
            local_size,
            remote_size: remote_meta.map(|m| m.size),
        };

        // Direction for a file the classifier did not resolve to push/pull itself: ONLY whichever raw
        // dry-run actually reported it - a file reported by neither side (e.g. excluded from push and
        // excluded from the reapplied pull `-u` because local is strictly newer) is dropped entirely rather
        // than defaulted into push, since nothing would actually transfer it in either direction (
        // "push counting comes only from the push dry-run set").
        let is_push_side = push_set.contains(path.as_str());
        let is_pull_side = !is_push_side && pull_after_u_set.contains(path.as_str());
        let dir = top_dir_of(path);

        match conflict::classify_file(&input) {
            FileClass::GitGroup => {
                out.git_count += 1;
                let entry = out.by_top_dir.entry(dir).or_default();
                entry.git += 1;
                if is_push_side {
                    out.push_count += 1;
                } else if is_pull_side {
                    out.pull_count += 1;
                }
            }
            FileClass::Push => {
                out.push_count += 1;
                out.by_top_dir.entry(dir).or_default().push += 1;
            }
            FileClass::PushStaleRemote => {
                out.push_count += 1;
                out.remote_behind = true;
                out.by_top_dir.entry(dir).or_default().stale_remote += 1;
            }
            FileClass::Pull => {
                out.pull_count += 1;
                out.by_top_dir.entry(dir).or_default().pull += 1;
            }
            FileClass::Suppressed => {} // remote-deleted, unedited locally - drops from every count.
            FileClass::Unclassified => {
                if is_push_side {
                    out.push_count += 1;
                    out.by_top_dir.entry(dir).or_default().push += 1;
                } else if is_pull_side {
                    out.pull_count += 1;
                    out.by_top_dir.entry(dir).or_default().pull += 1;
                }
            }
            FileClass::Conflict => out.conflicts.push(ConflictEntry {
                path: path.clone(),
                local_mtime: local_mtime.unwrap_or(0),
                remote_mtime: remote_meta.map(|m| m.mtime).unwrap_or(0),
                local_size: local_size.unwrap_or(0),
                remote_size: remote_meta.map(|m| m.size).unwrap_or(0),
                verified: true,
            }),
            FileClass::NeedsChecksum => {
                if let (Some(l), Some(r), Some(s)) = (local_mtime, remote_meta.map(|m| m.mtime), local_size) {
                    out.needs_checksum_meta.insert(path.clone(), (l, r, s));
                }
                out.needs_checksum.push(path.clone());
            }
        }
    }

    out
}

/// Full pipeline (docs/plan/conflict-detection-and-agy-report.md §§1-4): gathers (L, R, B, sizes) for every
/// differing file, classifies each with `conflict::classify_file`, checksums the same-size residue, and
/// returns push/pull counts with conflicts already excluded. Degrades to the pre-conflict-detection counts
/// (`compute_sync_counts_with_push_files`, reusing the push dry-run already fetched here) when the pull
/// dry-run's `--out-format` did not parse, OR when the metadata call itself exits non-zero (e.g. this
/// rsync build rejects `--out-format` outright) - either way, never a guessed classification
/// (§ Execution steps). A genuine connectivity failure surfaces again from the degrade path's own calls.
fn compute_sync_status_full(project: &SyncProject) -> Result<SyncStatusResult, String> {
    let push_files = rsync_change_files(project, true)?;
    let parsed = match rsync_pull_diff_with_metadata(project) {
        Ok(p) => p,
        Err(e) => {
            // Unlike `parsed.degraded` below (an rsync build that never understands `--out-format`), a
            // failing metadata call is an anomaly - silent conflict-detection loss is the bug class of
            // CHANGELOG "Delete preview error silently swallowed".
            log_status_anomaly(
                project,
                "metadata",
                &format!("conflict-detect metadata pull failed, no conflicts reported this poll: {}", e),
            );
            let (push_count, pull_count) = compute_sync_counts_with_push_files(project, push_files)?;
            return Ok(SyncStatusResult {
                has_local_changes: push_count > 0,
                has_remote_changes: pull_count > 0,
                push_count,
                pull_count,
                ..Default::default()
            });
        }
    };

    if parsed.degraded {
        clear_status_anomaly(project);
        let (push_count, pull_count) = compute_sync_counts_with_push_files(project, push_files)?;
        return Ok(SyncStatusResult {
            has_local_changes: push_count > 0,
            has_remote_changes: pull_count > 0,
            push_count,
            pull_count,
            ..Default::default()
        });
    }

    let baseline_opt = baseline_for_target(
        crate::sync_state::read_baseline(&project.id, &project.remote_host),
        &project.remote_path,
    );
    let has_baseline = baseline_opt.is_some();
    let baseline_files = baseline_opt.as_ref().map(|b| &b.files);

    let remote_by_path: HashMap<&str, &RemoteFileMeta> =
        parsed.files.iter().map(|m| (m.path.as_str(), m)).collect();
    let push_set: HashSet<&str> = push_files.iter().map(|s| s.as_str()).collect();

    // Candidate set: every path either dry-run saw differing. In practice the pull-side diff (no `-u`)
    // already lists every differing file regardless of direction; the union guards against a push-only
    // edge case (e.g. an exclude that differs between directions) rather than assuming that always holds.
    let mut candidates: Vec<String> = parsed.files.iter().map(|m| m.path.clone()).collect();
    for f in &push_files {
        if !candidates.iter().any(|c| c == f) {
            candidates.push(f.clone());
        }
    }

    // Local stat is walked once per candidate and reused both for the -u reapplication below and for classification, instead of stat-ing the same file twice.
    let local_stats: HashMap<String, (Option<u64>, Option<u64>)> = candidates
        .iter()
        .map(|p| (p.clone(), local_file_stat(&project.local_path, p)))
        .collect();
    let local_mtimes: HashMap<String, Option<u64>> =
        local_stats.iter().map(|(p, (mtime, _))| (p.clone(), *mtime)).collect();
    // Reapplies the pull-side `-u` meaning (§ Execution steps) since the metadata call above dropped it -
    // used only as the direction fallback for files the classifier could not attribute to push or pull
    // (GitGroup/Unclassified with no baseline to compare against).
    let pull_after_u_set: HashSet<&str> = conflict::select_pull_after_u_filter(&parsed.files, &local_mtimes)
        .into_iter()
        .map(|m| m.path.as_str())
        .collect();

    let outcome = classify_candidates(
        &candidates,
        &local_stats,
        &remote_by_path,
        baseline_files,
        has_baseline,
        &push_set,
        &pull_after_u_set,
    );
    let ClassificationOutcome {
        push_count: final_push_count,
        pull_count: final_pull_count,
        git_count,
        remote_behind,
        by_top_dir,
        mut conflicts,
        needs_checksum,
        needs_checksum_meta,
    } = outcome;

    let (checked, unverified) = conflict::split_checksum_residue(needs_checksum, conflict::CHECKSUM_CAP);

    // Cache lookup: a same-size residue file whose (L, R, size) is unchanged since a previous poll already checksummed it - skip the rsync -c call for it entirely.
    let mut still_need_check: Vec<String> = Vec::new();
    let mut cached_differs: HashSet<String> = HashSet::new();
    {
        let cache = checksum_cache().lock().unwrap_or_else(|e| e.into_inner());
        for path in &checked {
            let hit = needs_checksum_meta.get(path).and_then(|(l, r, s)| {
                let key = (project.id.clone(), project.remote_host.clone(), path.clone(), *l, *r, *s);
                cache.get(&key).copied()
            });
            match hit {
                Some(differs) => {
                    if differs {
                        cached_differs.insert(path.clone());
                    }
                }
                None => still_need_check.push(path.clone()),
            }
        }
    }

    // A failed checksum leaves these files "both changed, same size, content unknown" - reported as
    // unverified conflicts, the same honest answer the cap gives, never an Err that sinks the whole status
    // (which froze every badge on the last good poll) and never "converged" (a guess).
    let mut unverified = unverified;
    let mut checked = checked;
    let mut checksum_failed = false;
    let freshly_differing = match rsync_checksum_residue(project, &still_need_check) {
        Ok(differing) => {
            let mut cache = checksum_cache().lock().unwrap_or_else(|e| e.into_inner());
            for path in &still_need_check {
                if let Some((l, r, s)) = needs_checksum_meta.get(path) {
                    let key = (project.id.clone(), project.remote_host.clone(), path.clone(), *l, *r, *s);
                    cache.insert(key, differing.contains(path));
                }
            }
            differing
        }
        Err(e) => {
            log_status_anomaly(
                project,
                "checksum",
                &format!(
                    "checksum residue failed, {} file(s) reported as unverified conflicts: {}",
                    still_need_check.len(),
                    e
                ),
            );
            checksum_failed = true;
            let failed: HashSet<&String> = still_need_check.iter().collect();
            checked.retain(|p| !failed.contains(p));
            unverified.extend(still_need_check.iter().cloned());
            HashSet::new()
        }
    };

    let mut differing = freshly_differing;
    differing.extend(cached_differs);
    let (_, real_conflicts) = conflict::classify_checksum_result(&checked, &differing);

    let conflict_entry_for = |path: String, verified: bool| {
        let (local_mtime, local_size) = local_file_stat(&project.local_path, &path);
        let remote_meta = remote_by_path.get(path.as_str()).copied();
        ConflictEntry {
            path,
            local_mtime: local_mtime.unwrap_or(0),
            remote_mtime: remote_meta.map(|m| m.mtime).unwrap_or(0),
            local_size: local_size.unwrap_or(0),
            remote_size: remote_meta.map(|m| m.size).unwrap_or(0),
            verified,
        }
    };
    for path in real_conflicts {
        conflicts.push(conflict_entry_for(path, true));
    }
    for path in unverified {
        conflicts.push(conflict_entry_for(path, false));
    }
    if !checksum_failed {
        clear_status_anomaly(project);
    }

    Ok(SyncStatusResult {
        has_local_changes: final_push_count > 0,
        has_remote_changes: final_pull_count > 0,
        push_count: final_push_count,
        pull_count: final_pull_count,
        conflicts,
        git_count,
        remote_behind,
        by_top_dir,
    })
}

#[tauri::command]
pub async fn check_sync_status(
    app: tauri::AppHandle,
    project: SyncProject,
) -> Result<SyncStatusResult, String> {
    validate_project(&project)?;
    ensure_app_data_dir(&app);
    tauri::async_runtime::spawn_blocking(move || {
        // Inside the closure, not before it: this command is on the periodic refresh path, so an unmounted network volume whose `is_dir()` stalls would freeze the UI on every tick.
        ensure_local_path_present(&project.local_path)?;
        // Same excludes-from-disk floor as run_sync - a background status check must never light a push/pull badge computed against stale or stripped excludes.
        let mut project = project;
        let (pull_excludes, push_excludes) =
            crate::project_config::require_ok_excludes(&project.local_path)?;
        project.pull_excludes = pull_excludes;
        project.push_excludes = push_excludes;
        compute_sync_status_full(&project).inspect_err(|e| log_status_anomaly(&project, "status", e))
    })
    .await
    .map_err(|e| format!("check_sync_status task error: {}", e))?
}

/// Returns paths that would be deleted on destination if run with --delete; used by JS confirm dialog to preview destructive sync risk.
#[tauri::command]
pub async fn get_sync_delete_preview(
    project: SyncProject,
    direction: String,
) -> Result<Vec<String>, String> {
    validate_project(&project)?;
    tauri::async_runtime::spawn_blocking(move || {
        // Inside closure so stalled network-volume stats do not block IPC dispatch thread (see run_sync).
        ensure_local_path_present(&project.local_path)?;
        // Same excludes-from-disk floor as run_sync/check_sync_status - a delete preview must reflect the real excludes, not a stale or stripped in-memory copy.
        let mut project = project;
        let (pull_excludes, push_excludes) =
            crate::project_config::require_ok_excludes(&project.local_path)?;
        project.pull_excludes = pull_excludes;
        project.push_excludes = push_excludes;
        let is_push = direction == "push";
        let local = format!("{}/", project.local_path.trim_end_matches('/'));
        let remote = format!(
            "{}/",
            remote_rsync_arg(
                &project.remote_host,
                project.remote_path.trim_end_matches('/')
            )?
        );
        let (src, dest) = if is_push {
            (local.as_str(), remote.as_str())
        } else {
            (remote.as_str(), local.as_str())
        };

        let mut args = build_rsync_args(&project, is_push, true, &[], src, dest);
        let insert_pos = args.len().saturating_sub(2);
        args.insert(insert_pos, "--modify-window=2".to_string());

        let output = crate::system::create_command("rsync")
            .args(&args)
            .output()
            .map_err(|e| format!("rsync delete preview failed: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("rsync exited non-zero: {}", stderr.trim()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let deletes: Vec<String> = stdout
            .lines()
            .filter(|l| l.trim().starts_with("deleting "))
            // `strip_prefix` removes "deleting " at most once (avoiding corruption if a path begins with "deleting ").
            .map(|l| {
                let t = l.trim();
                t.strip_prefix("deleting ").unwrap_or(t).to_string()
            })
            .collect();

        Ok(deletes)
    })
    .await
    .map_err(|e| format!("get_sync_delete_preview task error: {}", e))?
}

/// Files a mirror-mode sync (this direction has `delete_on_push`/`delete_on_pull` on, so `build_rsync_args`
/// drops `-u`) would transfer even though the destination already holds a newer-or-equal copy - the
/// destructive effect `get_sync_delete_preview` never sees, since that one only catches destination-only
/// files, never destination-newer ones. Pure: the caller has already run both dry-runs.
fn files_overwritten_by_mirror(mirror_files: &[String], update_only_files: &[String]) -> Vec<String> {
    let update_only: HashSet<&str> = update_only_files.iter().map(String::as_str).collect();
    mirror_files
        .iter()
        .filter(|f| !update_only.contains(f.as_str()))
        .cloned()
        .collect()
}

/// Returns paths a mirror sync would silently overwrite on the destination (source is NOT newer there),
/// used by the JS confirm dialog alongside `get_sync_delete_preview` before a destructive `--delete` run.
/// Merge-mode directions (no `--delete`) already keep `-u` for real, so nothing here is ever silently
/// overwritten - returns empty without spending a second rsync round-trip.
#[tauri::command]
pub async fn get_sync_overwrite_preview(
    project: SyncProject,
    direction: String,
) -> Result<Vec<String>, String> {
    validate_project(&project)?;
    tauri::async_runtime::spawn_blocking(move || {
        ensure_local_path_present(&project.local_path)?;
        // Same excludes-from-disk floor as every other preview/status call.
        let mut project = project;
        let (pull_excludes, push_excludes) =
            crate::project_config::require_ok_excludes(&project.local_path)?;
        project.pull_excludes = pull_excludes;
        project.push_excludes = push_excludes;

        let is_push = direction == "push";
        let is_mirror = (is_push && project.delete_on_push) || (!is_push && project.delete_on_pull);
        if !is_mirror {
            return Ok(Vec::new());
        }

        let local = format!("{}/", project.local_path.trim_end_matches('/'));
        let remote = format!(
            "{}/",
            remote_rsync_arg(
                &project.remote_host,
                project.remote_path.trim_end_matches('/')
            )?
        );
        let (src, dest) = if is_push {
            (local.as_str(), remote.as_str())
        } else {
            (remote.as_str(), local.as_str())
        };

        // The real mirror dry-run - same shape `run_sync` and `get_sync_delete_preview` build for the actual sync, so this preview cannot diverge from what mirror would really send.
        let mut mirror_args = build_rsync_args(&project, is_push, true, &[], src, dest);
        let insert_pos = mirror_args.len().saturating_sub(2);
        mirror_args.insert(insert_pos, "--modify-window=2".to_string());
        let mirror_output = crate::system::create_command("rsync")
            .args(&mirror_args)
            .output()
            .map_err(|e| format!("rsync overwrite preview (mirror) failed: {}", e))?;
        if !mirror_output.status.success() {
            let stderr = String::from_utf8_lossy(&mirror_output.stderr);
            return Err(format!("rsync exited non-zero: {}", stderr.trim()));
        }
        let mirror_files = filter_file_change_lines(&String::from_utf8_lossy(&mirror_output.stdout));

        // The same transfer under real `-u` semantics (`rsync_change_files` always forces -avzu) - whatever mirror sends that this list omits is destination-newer-or-equal, silently overwritten.
        let update_only_files = rsync_change_files(&project, is_push)?;

        Ok(files_overwritten_by_mirror(&mirror_files, &update_only_files))
    })
    .await
    .map_err(|e| format!("get_sync_overwrite_preview task error: {}", e))?
}

// ─── agy explanation (Explain button, docs/plan/conflict-detection-and-agy-report.md § agy explanation) ──

const EXPLAIN_DIFF_LINE_CAP: usize = 400;

/// Real filesystem/PATH resolution wrapping `conflict::resolve_agy_bin_with`'s pure candidate order
/// (stack-tauri.A2's cold-start pattern) - `command -v` is the last resort, never the first.
fn resolve_agy_bin() -> Option<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    conflict::resolve_agy_bin_with(&home, |p| std::path::Path::new(p).is_file()).or_else(|| {
        crate::system::create_command("sh")
            .args(["-c", "command -v agy"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
    })
}

/// Cheap pre-flight check the frontend calls to disable Explain with its reason before any click, instead of
/// only surfacing "agy is not installed" after a failed invoke. Wraps the same `command -v`
/// subprocess fallback `resolve_agy_bin` may take, so per stack-tauri.A1 this stays async + `spawn_blocking`
/// even though the static-candidate fast path usually settles it with no subprocess at all.
#[tauri::command]
pub async fn check_agy_available() -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(|| resolve_agy_bin().is_some())
        .await
        .map_err(|e| format!("check_agy_available task error: {}", e))
}

/// First `EXPLAIN_DIFF_LINE_CAP` lines of a local file, or `None` for a binary file (a NUL byte in the
/// first 8000 bytes) - binaries send metadata only (§ agy explanation).
fn read_local_excerpt(local_path: &str, rel: &str) -> Option<String> {
    let bytes = std::fs::read(std::path::Path::new(local_path).join(rel)).ok()?;
    if bytes.iter().take(8000).any(|b| *b == 0) {
        return None;
    }
    let text = String::from_utf8_lossy(&bytes);
    Some(text.lines().take(EXPLAIN_DIFF_LINE_CAP).collect::<Vec<_>>().join("\n"))
}

/// Fetches one conflict file to a throwaway temp dir and reads its excerpt the same way as the local side;
/// `None` on any fetch/read failure or a binary file - metadata-only is always a safe degrade here.
fn fetch_remote_excerpt(host: &str, remote_path: &str, rel: &str) -> Option<String> {
    let tmp_dir = std::env::temp_dir().join(format!(
        "aki-devsync-explain-{}-{}",
        std::process::id(),
        rel.replace(['/', '\\'], "_")
    ));
    let dest = tmp_dir.join(rel);
    std::fs::create_dir_all(dest.parent()?).ok()?;
    let remote_full = format!("{}/{}", remote_path.trim_end_matches('/'), rel);
    let remote_src = remote_rsync_arg(host, &remote_full).ok()?;
    let mut args = vec!["-az".to_string()];
    push_transport_args(&mut args);
    args.push(remote_src);
    args.push(dest.to_string_lossy().to_string());
    let out = crate::system::create_command("rsync").args(&args).output().ok()?;
    let result = if out.status.success() {
        read_local_excerpt(tmp_dir.to_str()?, rel)
    } else {
        None
    };
    let _ = std::fs::remove_dir_all(&tmp_dir);
    result
}

/// agy needs an explicit `--model` slug (`agy models`); it carries its thinking tier in the slug, so there is no separate effort dial.
/// Facts: https://github.com/lacvietanh/akidevrule/blob/79fb6695a64254df91fd61e1318b3b8ec5d5eac3/skills/akiflow/references/harness-facts.md#model-tiers (read 2026-09-29).
/// The value reaches argv (no shell) shell-quoted, so the only hazard is it being read as a flag.
fn validate_agy_model(model: &str) -> Result<(), String> {
    let ok = !model.is_empty()
        && !model.starts_with('-')
        && model.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    if ok {
        Ok(())
    } else {
        Err(format!("invalid agy model: {:?}", model))
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct AgyModel {
    id: String,
    label: String,
}

/// `agy models` prints a header line, then one `id<TAB>label` row per model.
fn parse_agy_models(stdout: &str) -> Vec<AgyModel> {
    stdout
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(id, label)| AgyModel { id: id.trim().to_string(), label: label.trim().to_string() })
        .collect()
}

#[tauri::command]
pub async fn list_agy_models() -> Result<Vec<AgyModel>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let agy_bin = resolve_agy_bin().ok_or_else(|| "agy is not installed".to_string())?;
        let out = crate::system::create_command(&agy_bin)
            .arg("models")
            .output()
            .map_err(|e| format!("agy models failed: {}", e))?;
        if !out.status.success() {
            return Err(format!("agy models exited non-zero: {}", String::from_utf8_lossy(&out.stderr).trim()));
        }
        Ok(parse_agy_models(&String::from_utf8_lossy(&out.stdout)))
    })
    .await
    .map_err(|e| format!("list_agy_models task join error: {}", e))?
}

#[derive(Serialize, Clone, Debug)]
struct ExplainConflictPayload {
    path: String,
    /// `local_root/path` and `remote_root/path`, spelled out so nothing has to be looked up.
    local_full: String,
    remote_full: String,
    local_size: u64,
    remote_size: u64,
    local_mtime: u64,
    remote_mtime: u64,
    verified: bool,
    /// Unified diff (`conflict::unified_diff`) of the local excerpt against the remote excerpt - `None` for
    /// a binary file, a secret-named file, or a fetch failure on either side (§5: "unified diff of local vs
    /// remote"; metadata-only degrade stays the same safe fallback either way).
    #[serde(skip_serializing_if = "Option::is_none")]
    diff: Option<String>,
}

/// The owner edits the prompt in AI Settings; this only bounds it so a runaway paste cannot reach the command line.
const EXPLAIN_PROMPT_MAX_BYTES: usize = 8000;

#[derive(Serialize, Clone, Debug)]
struct ExplainPayload {
    /// Unix seconds, so relative ages ("edited 3 days ago") can be stated without the model guessing the date.
    now: u64,
    project: String,
    /// Absolute local project directory.
    local_root: String,
    /// `host:/absolute/remote/dir` - the other side of every path below.
    remote_root: String,
    host: String,
    push_count: u32,
    pull_count: u32,
    git_count: u32,
    /// "Whether the remote is behind" (§5) - at least one push file was stale relative to the baseline.
    remote_behind: bool,
    /// Per-top-directory, per-class counts for both lists (§5's "whole picture", not only conflicts).
    by_top_dir: HashMap<String, TopDirCounts>,
    last_sync: Option<crate::sync_state::LastSync>,
    conflicts: Vec<ExplainConflictPayload>,
}

/// On-demand only (never on the 60s poll, § agy explanation): builds the whole-picture payload behind both
/// lit badges into a temp prompt file and returns the shell command that opens interactive `agy --mode plan` (read-only) on it; the frontend
/// runs that command in an in-app terminal tab, so the owner watches agy work and stream its answer, and can steer or quit it.
/// Read-only - no rsync/git mutation. Resolving `agy` and the SSH fetches for conflict excerpts run inside
/// `spawn_blocking` so a slow host never freezes the window (stack-tauri.A1).
#[tauri::command]
pub async fn explain_sync_status(
    project: SyncProject,
    status: SyncStatusResult,
    model: String,
    prompt: String,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        validate_agy_model(&model)?;
        if prompt.trim().is_empty() || prompt.len() > EXPLAIN_PROMPT_MAX_BYTES {
            return Err(format!("explain prompt must be 1..{} bytes", EXPLAIN_PROMPT_MAX_BYTES));
        }
        let agy_bin = resolve_agy_bin()
            .ok_or_else(|| "agy is not installed - no ~/.local/bin/agy and no agy on PATH".to_string())?;

        for c in &status.conflicts {
            conflict::validate_conflict_rel_path(&c.path)?;
        }

        let last_sync = crate::sync_state::read_last_sync(&project.id, &project.remote_host);
        let conflicts: Vec<ExplainConflictPayload> = status
            .conflicts
            .iter()
            .map(|c| {
                let diff = if conflict::is_secret_named(&c.path) {
                    None
                } else {
                    let local_excerpt = read_local_excerpt(&project.local_path, &c.path);
                    let remote_excerpt =
                        fetch_remote_excerpt(&project.remote_host, &project.remote_path, &c.path);
                    match (local_excerpt, remote_excerpt) {
                        (None, None) => None, // binary or unreadable on both sides - metadata only
                        (local, remote) => Some(conflict::unified_diff(
                            &local.unwrap_or_default(),
                            &remote.unwrap_or_default(),
                        )),
                    }
                };
                ExplainConflictPayload {
                    path: c.path.clone(),
                    local_full: format!("{}/{}", project.local_path.trim_end_matches('/'), c.path),
                    remote_full: format!("{}:{}/{}", project.remote_host, project.remote_path.trim_end_matches('/'), c.path),
                    local_size: c.local_size,
                    remote_size: c.remote_size,
                    local_mtime: c.local_mtime,
                    remote_mtime: c.remote_mtime,
                    verified: c.verified,
                    diff,
                }
            })
            .collect();

        let payload = ExplainPayload {
            now: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
            project: project.name.clone(),
            local_root: project.local_path.clone(),
            remote_root: format!("{}:{}", project.remote_host, project.remote_path),
            host: project.remote_host.clone(),
            push_count: status.push_count,
            pull_count: status.pull_count,
            git_count: status.git_count,
            remote_behind: status.remote_behind,
            by_top_dir: status.by_top_dir.clone(),
            last_sync,
            conflicts,
        };
        let payload_json =
            serde_json::to_string(&payload).map_err(|e| format!("explain payload serialize: {}", e))?;

        let prompt_file = std::env::temp_dir().join(format!(
            "aki-devsync-explain-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
        ));
        std::fs::write(&prompt_file, format!("{}\n\n{}", prompt, payload_json))
            .map_err(|e| format!("explain prompt write failed: {}", e))?;
        let file_q = crate::system::shell_quote(&prompt_file.to_string_lossy());
        // `-i` opens agy's own TUI with the prompt as its first turn (the owner watches it work and can steer); `--mode plan` keeps it read-only by mechanism. The prompt is the last token because `-i` takes the next one as its value, and the file is read and removed in the same substitution so the temp file never outlives the launch.
        // Facts: https://github.com/lacvietanh/akidevrule/blob/79fb6695a64254df91fd61e1318b3b8ec5d5eac3/skills/akiflow/references/harness-facts.md#cross-cli-worker-claude-code-lead--agy-headless (read 2026-09-29); `-i` = `--prompt-interactive` per `agy --help`.
        let command = format!(
            "{} --model {} --mode plan -i \"$(cat {}; rm -f {})\"",
            crate::system::shell_quote(&agy_bin),
            crate::system::shell_quote(&model),
            file_q,
            file_q
        );
        Ok(command)
    })
    .await
    .map_err(|e| format!("explain_sync_status task join error: {}", e))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::SyncHooks;

    #[test]
    fn parse_agy_models_skips_the_header_and_splits_id_from_label() {
        let m = parse_agy_models("Fetching available models...\ngemini-3.8-flash-high\tGemini 3.8 Flash (High)\n");
        assert_eq!(m.len(), 1);
        assert_eq!((m[0].id.as_str(), m[0].label.as_str()), ("gemini-3.8-flash-high", "Gemini 3.8 Flash (High)"));
    }

    #[test]
    fn agy_model_must_be_a_plain_slug_never_empty_or_flag_shaped() {
        assert!(validate_agy_model("gemini-3.8-flash-high").is_ok());
        assert!(validate_agy_model("").is_err());
        assert!(validate_agy_model("--dangerously-skip-permissions").is_err());
        assert!(validate_agy_model("a b").is_err());
    }

    #[test]
    fn two_failing_steps_of_one_poll_are_each_logged_once_across_polls() {
        let mut p = make_test_project(vec![], vec![]);
        p.id = "anomaly-two-kinds".to_string();
        for _poll in 0..3 {
            let logged = [
                record_status_anomaly(&p, "metadata", "metadata pull failed: reset"),
                record_status_anomaly(&p, "status", "reset"),
            ];
            if _poll == 0 {
                assert_eq!(logged, [true, true]);
            } else {
                assert_eq!(logged, [false, false], "an unchanged failure must not be logged again");
            }
        }
        assert!(record_status_anomaly(&p, "status", "connection closed"), "a changed message logs again");
    }

    #[test]
    fn clearing_a_projects_anomalies_forgets_every_kind_for_that_host_only() {
        let mut p = make_test_project(vec![], vec![]);
        p.id = "anomaly-clear".to_string();
        let mut other = p.clone();
        other.remote_host = "other-host".to_string();
        record_status_anomaly(&p, "metadata", "a");
        record_status_anomaly(&p, "status", "b");
        record_status_anomaly(&other, "status", "b");
        clear_status_anomaly(&p);
        assert!(record_status_anomaly(&p, "metadata", "a"));
        assert!(record_status_anomaly(&p, "status", "b"));
        assert!(!record_status_anomaly(&other, "status", "b"), "another host's slot survives");
    }

    // Test fixture builder: constructs a minimal SyncProject without making `projects.rs` test helper public.
    fn make_test_project(push_excludes: Vec<&str>, pull_excludes: Vec<&str>) -> SyncProject {
        SyncProject {
            id: "test".to_string(),
            name: "Test".to_string(),
            local_path: "/local".to_string(),
            remote_host: "host".to_string(),
            remote_path: "/remote".to_string(),
            production_url: None,
            pull_excludes: pull_excludes.into_iter().map(String::from).collect(),
            push_excludes: push_excludes.into_iter().map(String::from).collect(),
            hooks: SyncHooks {
                pre_pull_cmd: None,
                post_pull_cmd: None,
                pre_push_cmd: None,
                post_push_cmd: None,
                run_hooks_on_remote: false,
                ignore_hook_errors: false,
            },
            last_sync_action: None,
            last_sync_time: None,
            last_sync_host: None,
            dry_run: true,
            sync_git: None,
            delete_on_pull: false,
            delete_on_push: false,
            last_sync_status: None,
            dev_cmd_override: None,
            build_cmd_override: None,
            disabled: false,
            targets: std::collections::BTreeMap::new(),
            deploy: None,
            tasks: None,
            notes: None,
        }
    }

    #[test]
    fn files_overwritten_by_mirror_keeps_only_the_destination_newer_residue() {
        let mirror = vec!["a.txt".to_string(), "b.txt".to_string(), "c.txt".to_string()];
        // -u would still send a.txt and c.txt (source is newer there) - only b.txt is mirror-only, meaning destination held a newer/equal copy that -u would have protected.
        let update_only = vec!["a.txt".to_string(), "c.txt".to_string()];
        assert_eq!(
            files_overwritten_by_mirror(&mirror, &update_only),
            vec!["b.txt".to_string()]
        );
    }

    #[test]
    fn files_overwritten_by_mirror_is_empty_when_every_file_is_source_newer() {
        let mirror = vec!["a.txt".to_string()];
        let update_only = vec!["a.txt".to_string()];
        assert!(files_overwritten_by_mirror(&mirror, &update_only).is_empty());
    }

    #[test]
    fn is_under_dir_exclude_matches_exact_dir() {
        let excludes = vec![".git/".to_string()];
        assert!(is_under_dir_exclude(".git", &excludes));
    }

    #[test]
    fn is_under_dir_exclude_matches_nested_path() {
        let excludes = vec![".git/".to_string()];
        assert!(is_under_dir_exclude(".git/objects/ab/cdef", &excludes));
    }

    #[test]
    fn is_under_dir_exclude_respects_component_boundary() {
        // ".wrangler-backup" shares a prefix with ".wrangler/" but is a sibling directory, not a nested path - a naive starts_with would wrongly match.
        let excludes = vec![".wrangler/".to_string()];
        assert!(!is_under_dir_exclude(".wrangler-backup/foo", &excludes));
    }

    #[test]
    fn is_under_dir_exclude_does_not_match_sibling_with_shared_prefix() {
        let excludes = vec![".git/".to_string()];
        assert!(!is_under_dir_exclude(".gitignore", &excludes));
    }

    #[test]
    fn is_under_dir_exclude_ignores_glob_entries() {
        let excludes = vec!["*.log".to_string()];
        assert!(!is_under_dir_exclude("app.log", &excludes));
    }

    #[test]
    fn is_under_dir_exclude_trims_whitespace() {
        let excludes = vec!["  .git/  ".to_string()];
        assert!(is_under_dir_exclude(".git", &excludes));
    }

    #[test]
    fn is_under_dir_exclude_degenerate_root_slash_matches_nothing() {
        let excludes = vec!["/".to_string()];
        assert!(!is_under_dir_exclude("anything", &excludes));
        assert!(!is_under_dir_exclude("", &excludes));
    }

    #[test]
    fn is_under_dir_exclude_degenerate_empty_string_matches_nothing() {
        let excludes = vec!["".to_string()];
        assert!(!is_under_dir_exclude("anything", &excludes));
    }

    #[test]
    fn is_under_dir_exclude_empty_list_is_false() {
        let excludes: Vec<String> = vec![];
        assert!(!is_under_dir_exclude(".git", &excludes));
    }

    #[test]
    fn direction_excludes_pull_only_dir_absent_from_push_direction() {
        // R2 revert (1.13.1): pull-only exclude (e.g. `.git/`) is NOT excluded on push status check since push transfers it.
        let project = make_test_project(vec![], vec![".git/"]);
        let push_excludes = direction_excludes(&project, true);
        assert!(!push_excludes.contains(&".git/".to_string()));
    }

    #[test]
    fn direction_excludes_pull_only_dir_present_in_pull_direction() {
        // Same dir must still be excluded from the pull-direction status check  - pull never brings it back, so it must never count as a pull change.
        let project = make_test_project(vec![], vec![".git/"]);
        let pull_excludes = direction_excludes(&project, false);
        assert!(pull_excludes.contains(&".git/".to_string()));
    }

    #[test]
    fn direction_excludes_push_only_dir_absent_from_pull_direction() {
        let project = make_test_project(vec!["push_only/"], vec![]);
        let pull_excludes = direction_excludes(&project, false);
        assert!(!pull_excludes.contains(&"push_only/".to_string()));
    }

    #[test]
    fn direction_excludes_push_only_dir_present_in_push_direction() {
        let project = make_test_project(vec!["push_only/"], vec![]);
        let push_excludes = direction_excludes(&project, true);
        assert!(push_excludes.contains(&"push_only/".to_string()));
    }

    #[test]
    fn union_excludes_includes_entries_unique_to_each_side() {
        let project = make_test_project(vec!["push_only/"], vec!["pull_only/"]);
        let result = union_excludes(&project);
        assert!(result.contains(&"push_only/".to_string()));
        assert!(result.contains(&"pull_only/".to_string()));
    }

    #[test]
    fn union_excludes_dedups_entry_present_in_both_lists() {
        let project = make_test_project(vec![".git/"], vec![".git/"]);
        let result = union_excludes(&project);
        assert_eq!(result.iter().filter(|e| e.as_str() == ".git/").count(), 1);
    }

    #[test]
    fn union_excludes_dedups_on_trimmed_value() {
        let project = make_test_project(vec![".git/"], vec![" .git/ "]);
        let result = union_excludes(&project);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn union_excludes_drops_empty_and_whitespace_only_entries() {
        let project = make_test_project(vec!["", "  ", "real/"], vec!["   "]);
        let result = union_excludes(&project);
        assert_eq!(result, vec!["real/".to_string()]);
    }

    #[test]
    fn union_excludes_push_entries_precede_pull_entries() {
        let project = make_test_project(vec!["a/", "b/"], vec!["c/", "d/"]);
        let result = union_excludes(&project);
        assert_eq!(
            result,
            vec![
                "a/".to_string(),
                "b/".to_string(),
                "c/".to_string(),
                "d/".to_string()
            ]
        );
    }

    #[test]
    fn expand_tilde_prefix() {
        assert_eq!(expand_remote_tilde("~/app"), "$HOME/app");
    }

    #[test]
    fn expand_tilde_alone() {
        assert_eq!(expand_remote_tilde("~"), "$HOME");
    }

    #[test]
    fn expand_tilde_no_op_absolute() {
        assert_eq!(expand_remote_tilde("/var/www/app"), "/var/www/app");
    }

    #[test]
    fn expand_tilde_no_op_relative() {
        assert_eq!(expand_remote_tilde("relative/path"), "relative/path");
    }

    #[test]
    fn expand_tilde_only_replaces_leading() {
        assert_eq!(expand_remote_tilde("~/a/~/b"), "$HOME/a/~/b");
    }

    #[test]
    fn validate_specific_paths_rejects_traversal() {
        let paths = vec!["../../etc/passwd".to_string()];
        assert!(validate_specific_paths(&paths).is_err());
    }

    #[test]
    fn validate_specific_paths_rejects_control_chars() {
        let paths = vec!["file\x01.txt".to_string()];
        assert!(validate_specific_paths(&paths).is_err());
    }

    #[test]
    fn validate_specific_paths_accepts_valid() {
        let paths = vec!["src/main.rs".to_string(), "README.md".to_string()];
        assert!(validate_specific_paths(&paths).is_ok());
    }

    #[test]
    fn validate_specific_paths_accepts_empty() {
        assert!(validate_specific_paths(&[]).is_ok());
    }

    // ─── §3.5 remote-path quoting ─────────────────────────────────────────────

    #[test]
    fn quote_remote_path_wraps_a_path_with_a_space() {
        // The actual bug: `mkdir -p ~/my app` created two directories on the remote.
        assert_eq!(quote_remote_path("~/my app"), "\"$HOME\"/'my app'");
    }

    #[test]
    fn quote_remote_path_keeps_tilde_expandable() {
        // A quoted `~` would be a literal directory named `~`, so it must be substituted first.
        assert!(!quote_remote_path("~/app").contains('~'));
        assert_eq!(quote_remote_path("~"), "\"$HOME\"");
        assert_eq!(quote_remote_path("~/"), "\"$HOME\"");
    }

    #[test]
    fn quote_remote_path_quotes_an_absolute_path_whole() {
        assert_eq!(quote_remote_path("/var/www/my app"), "'/var/www/my app'");
    }

    #[test]
    fn quote_remote_path_escapes_embedded_single_quote() {
        assert_eq!(quote_remote_path("/srv/it's"), "'/srv/it'\\''s'");
    }

    #[test]
    fn shell_single_quote_neutralises_shell_metacharacters() {
        for raw in ["$(id)", "`id`", "a; rm -rf /", "a\\b", "a\"b", "tài liệu"] {
            let q = shell_single_quote(raw);
            assert!(q.starts_with('\'') && q.ends_with('\''), "not quoted: {q}");
            // Nothing but the escape sequence may ever end the quoting early.
            assert_eq!(
                q.matches('\'').count(),
                2 + raw.matches('\'').count() * 2,
                "{q}"
            );
        }
    }

    #[test]
    fn shell_single_quote_handles_consecutive_quotes() {
        assert_eq!(shell_single_quote("a''b"), "'a'\\'''\\''b'");
    }

    // ─── §3.5 rsync remote-path argument ──────────────────────────────────────

    #[test]
    fn first_shell_active_char_flags_every_injection_shape() {
        for (raw, expected) in [
            ("/srv/it's", '\''),
            ("/srv/a\"b", '"'),
            ("/srv/$(id)", '$'),
            ("/srv/`id`", '`'),
            ("/srv/a\\b", '\\'),
            ("/srv/my app", ' '),
            ("/srv/x; curl http://e | sh", ';'),
            ("/srv/a&b", '&'),
            ("/srv/a|b", '|'),
            ("/srv/a>b", '>'),
            ("/srv/{a}", '{'),
            ("#/srv/a", '#'),
        ] {
            assert_eq!(first_shell_active_char(raw), Some(expected), "{raw}");
        }
    }

    #[test]
    fn first_shell_active_char_leaves_ordinary_paths_alone() {
        // Non-ASCII is not shell-active, and `~` / wildcards mean the same thing on old and new rsync - flagging any of these would refuse a config that works today for no gain.
        for raw in [
            "/var/www/app",
            "~",
            "~/app",
            "~/a/~/b",
            "~user/app",
            "/srv/tàiliệu",
            "/srv/日本語",
            "/srv/app*",
            "/srv/log[0-9]",
            "/srv/a?b",
        ] {
            assert_eq!(first_shell_active_char(raw), None, "{raw}");
        }
    }

    #[test]
    fn rsync_protects_remote_args_only_from_3_2_4() {
        assert!(rsync_protects_remote_args(
            "rsync  version 3.4.1  protocol version 32"
        ));
        assert!(rsync_protects_remote_args(
            "rsync  version 3.2.4  protocol version 31"
        ));
        assert!(!rsync_protects_remote_args(
            "rsync  version 3.2.3  protocol version 31"
        ));
        assert!(!rsync_protects_remote_args(
            "rsync  version 2.6.9  protocol version 29"
        ));
    }

    #[test]
    fn rsync_protects_remote_args_fails_closed_on_an_unknown_banner() {
        // macOS's stock /usr/bin/rsync is openrsync; its first line carries a "version 29" token that must never be read as a version number.
        assert!(!rsync_protects_remote_args(
            "openrsync: protocol version 29"
        ));
        assert!(!rsync_protects_remote_args("unknown"));
        assert!(!rsync_protects_remote_args(""));
    }

    #[test]
    fn remote_rsync_arg_passes_an_ordinary_path_through_unchanged() {
        assert_eq!(
            remote_rsync_arg("host", "/var/www/app").unwrap(),
            "host:/var/www/app"
        );
        assert_eq!(
            remote_rsync_arg("host", "/srv/日本語").unwrap(),
            "host:/srv/日本語"
        );
    }

    #[test]
    fn remote_rsync_arg_makes_a_leading_tilde_home_relative() {
        assert_eq!(remote_rsync_arg("host", "~/app").unwrap(), "host:app");
        assert_eq!(remote_rsync_arg("host", "~/a/~/b").unwrap(), "host:a/~/b");
        assert_eq!(remote_rsync_arg("host", "~").unwrap(), "host:.");
        assert_eq!(remote_rsync_arg("host", "~user/app").unwrap(), "host:~user/app");
    }

    #[test]
    fn home_relative_never_yields_an_empty_or_root_anchored_path() {
        // Callers append `/`: an empty result would become `host:/`, the remote root.
        assert_eq!(home_relative("~/"), ".");
        assert_eq!(home_relative("~//app"), "app");
        assert_eq!(home_relative("~//"), ".");
        assert_eq!(home_relative("relative/path"), "relative/path");
    }

    // ─── §3.7 transport timeouts ──────────────────────────────────────────────

    #[test]
    fn rsync_args_carry_the_io_timeout_and_ssh_connect_timeout() {
        let project = make_test_project(vec![], vec![]);
        let args = build_rsync_args(&project, true, false, &[], "/local/", "host:/remote/");
        assert!(args.contains(&"--timeout=120".to_string()), "{args:?}");
        let e = args
            .iter()
            .position(|a| a == "-e")
            .expect("no -e in {args:?}");
        assert_eq!(args[e + 1], "ssh -o ConnectTimeout=10");
    }

    #[test]
    fn rsync_args_keep_src_and_dest_last() {
        // get_sync_delete_preview inserts --modify-window at len-2, so the transport flags must never displace src/dest from the tail.
        let project = make_test_project(vec!["x/"], vec![]);
        let args = build_rsync_args(&project, true, true, &[], "/local/", "host:/remote/");
        assert_eq!(args[args.len() - 2], "/local/");
        assert_eq!(args[args.len() - 1], "host:/remote/");
    }

    #[test]
    fn rsync_args_keep_specific_paths_before_dest() {
        let project = make_test_project(vec![], vec![]);
        let paths = vec!["a.txt".to_string(), "b.txt".to_string()];
        let args = build_rsync_args(&project, true, false, &paths, "/local/", "host:/remote/");
        let r = args.iter().position(|a| a == "-R").unwrap();
        assert_eq!(args[r + 1], "a.txt");
        assert_eq!(args[r + 2], "b.txt");
        assert_eq!(args[args.len() - 1], "host:/remote/");
    }

    // ─── notes protected from a mirror wipe ───────────────────────────────────

    const NOTES_PROTECT_FILTER: &str = "--filter=P .akidevsync/";

    #[test]
    fn mirror_push_protects_the_task_list_from_delete() {
        let mut project = make_test_project(vec![], vec![]);
        project.delete_on_push = true;
        let args = build_rsync_args(&project, true, false, &[], "/local/", "host:/remote/");
        assert!(args.contains(&"--delete".to_string()), "{args:?}");
        assert!(args.contains(&NOTES_PROTECT_FILTER.to_string()), "{args:?}");
    }

    #[test]
    fn mirror_pull_protects_the_task_list_from_delete() {
        // The direction that actually caused the risk: a PULL before the project's first PUSH finds no .akidevsync/ on the remote, so --delete alone would erase the local one.
        let mut project = make_test_project(vec![], vec![]);
        project.delete_on_pull = true;
        let args = build_rsync_args(&project, false, false, &[], "host:/remote/", "/local/");
        assert!(args.contains(&"--delete".to_string()), "{args:?}");
        assert!(args.contains(&NOTES_PROTECT_FILTER.to_string()), "{args:?}");
    }

    #[test]
    fn a_non_mirror_transfer_carries_neither_delete_nor_the_protect_filter() {
        let project = make_test_project(vec![], vec![]);
        let args = build_rsync_args(&project, true, false, &[], "/local/", "host:/remote/");
        assert!(!args.contains(&"--delete".to_string()), "{args:?}");
        assert!(
            !args.contains(&NOTES_PROTECT_FILTER.to_string()),
            "{args:?}"
        );
    }

    // ─── §3.22 missing local path ─────────────────────────────────────────────

    #[test]
    fn ensure_local_path_present_rejects_a_missing_directory() {
        let err = ensure_local_path_present("/Volumes/definitely-not-mounted-xyz/app").unwrap_err();
        assert!(
            err.contains("/Volumes/definitely-not-mounted-xyz/app"),
            "{err}"
        );
        // The message must point at the real cause, not read as a validation rejection.
        assert!(err.to_lowercase().contains("mount"), "{err}");
    }

    #[test]
    fn ensure_local_path_present_accepts_an_existing_directory() {
        assert!(ensure_local_path_present(std::env::temp_dir().to_str().unwrap()).is_ok());
    }

    // ─── §3.6 cancel registry ─────────────────────────────────────────────────

    #[test]
    fn take_children_is_scoped_to_one_project() {
        // Multi-entity guard: stopping one sync must leave every other project's pids intact.
        register_child("proj-a", 111);
        register_child("proj-a", 112);
        register_child("proj-b", 222);

        let a = take_children("proj-a");
        assert_eq!(a, vec![111, 112]);
        assert!(take_children("proj-a").is_empty());
        assert_eq!(take_children("proj-b"), vec![222]);
    }

    #[test]
    fn unregister_child_removes_only_that_pid() {
        register_child("proj-c", 331);
        register_child("proj-c", 332);
        unregister_child("proj-c", 331);
        assert_eq!(take_children("proj-c"), vec![332]);
    }

    #[test]
    fn consume_cancelled_is_one_shot_and_per_project() {
        mark_cancelled("proj-d");
        assert!(!consume_cancelled("proj-e"));
        assert!(consume_cancelled("proj-d"));
        assert!(!consume_cancelled("proj-d"));
    }

    // F2 (docs/plan/settings-and-state-layout.md § A): "delete a file locally -> merge push -> next
    // status check still classifies it as local deleted, not remote created." `carry_forward_local_deletions`
    // is the pure function that makes this true - tested directly, no SSH/rsync needed.

    #[test]
    fn f2_merge_push_carries_forward_a_file_missing_from_the_fresh_local_walk() {
        let mut current: HashMap<String, u64> = HashMap::new();
        current.insert("kept.txt".to_string(), 1);
        let mut previous = HashMap::new();
        previous.insert("kept.txt".to_string(), 1);
        previous.insert("deleted-locally.txt".to_string(), 2);

        carry_forward_local_deletions(&mut current, Some(&previous), true, false);

        assert_eq!(
            current.get("deleted-locally.txt"),
            Some(&2u64),
            "a merge push must carry the missing file forward so it is not misread as remote-created"
        );
    }

    #[test]
    fn f2_mirror_push_needs_no_carry_over() {
        let mut current: HashMap<String, u64> = HashMap::new();
        let mut previous = HashMap::new();
        previous.insert("deleted-locally.txt".to_string(), 2);

        carry_forward_local_deletions(&mut current, Some(&previous), true, true);

        assert!(
            !current.contains_key("deleted-locally.txt"),
            "a mirror push propagates the deletion to the remote, so nothing needs carrying forward"
        );
    }

    #[test]
    fn f2_pull_needs_no_carry_over() {
        let mut current: HashMap<String, u64> = HashMap::new();
        let mut previous = HashMap::new();
        previous.insert("deleted-locally.txt".to_string(), 2);

        carry_forward_local_deletions(&mut current, Some(&previous), false, false);

        assert!(
            !current.contains_key("deleted-locally.txt"),
            "a pull (merge or mirror) either restores or matches the remote - never carries over"
        );
    }

    // A baseline recorded against a different remote_path is someone else's ancestor - counts as no
    // baseline at all. `baseline_for_target` is the one place this filter is spelled (previously
    // duplicated, with the write path missing it entirely).

    #[test]
    fn s1_baseline_for_same_remote_path_is_kept() {
        let baseline = crate::sync_state::Baseline {
            remote_path: "~/app".to_string(),
            files: HashMap::new(),
        };
        assert!(baseline_for_target(Some(baseline), "~/app").is_some());
    }

    #[test]
    fn s1_baseline_for_a_changed_remote_path_counts_as_no_baseline() {
        let baseline = crate::sync_state::Baseline {
            remote_path: "~/old-path".to_string(),
            files: HashMap::new(),
        };
        assert!(
            baseline_for_target(Some(baseline), "~/new-path").is_none(),
            "a baseline recorded against a different remote_path must never be used as this target's ancestor"
        );
    }

    #[test]
    fn s1_no_baseline_stays_none() {
        assert!(baseline_for_target(None, "~/app").is_none());
    }

    // F2's own scenario ("delete a file locally -> merge push -> next status check still classifies
    // it as local deleted, not remote created") exercised through the actual status classification function
    // `classify_sync_counts` uses, not only the pure carry-forward helper.

    #[test]
    fn n_b_a_carried_baseline_entry_missing_locally_classifies_as_local_deleted_not_remote_created() {
        let scratch = std::env::temp_dir().join(format!("aki-sync-n-b-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).unwrap();

        // Baseline still remembers the file (it was common before); it was rsync-reported on the pull side
        // (the fresh remote listing still has it) but is genuinely absent from the local walk, exactly as
        // F2's carry-forward would have left it after a merge push.
        let mut baseline = HashMap::new();
        baseline.insert("deleted-locally.txt".to_string(), 100u64);

        let (push_count, pull_count) = classify_sync_counts(
            vec![],
            vec!["deleted-locally.txt".to_string()],
            Some(&baseline),
            scratch.to_str().unwrap(),
        );

        assert_eq!(push_count, 1, "a baseline entry missing locally must classify as local-deleted -> push");
        assert_eq!(pull_count, 0, "it must never also count as remote-created -> pull");

        let _ = std::fs::remove_dir_all(&scratch);
    }

    // `classify_candidates` is the pure core `compute_sync_status_full`
    // calls - exercised directly here without a real rsync/SSH round-trip, per the brief's own escape hatch
    // ("the pure core it calls - extract a pure seam if needed"). Covers push, pull, conflict, .git, and the
    // restored remote-deletion suppression; the degraded path itself is exercised at the parser boundary that
    // actually triggers it (`conflict::tests::parse_output_degrades_on_a_plain_filename_line_no_out_format_support`).

    fn candidate_meta(path: &str, size: u64, mtime: u64) -> (String, RemoteFileMeta) {
        (path.to_string(), RemoteFileMeta { path: path.to_string(), size, mtime })
    }

    #[test]
    fn classify_candidates_remote_deleted_unedited_locally_is_suppressed_not_push() {
        let candidates = vec!["deleted-on-remote.txt".to_string()];
        let local_stats: HashMap<String, (Option<u64>, Option<u64>)> =
            [("deleted-on-remote.txt".to_string(), (Some(100), Some(10)))].into();
        let remote_by_path: HashMap<&str, &RemoteFileMeta> = HashMap::new();
        let mut baseline_files = HashMap::new();
        baseline_files.insert("deleted-on-remote.txt".to_string(), 100u64); // same as local mtime -> unedited since baseline
        let push_set: HashSet<&str> = ["deleted-on-remote.txt"].into();
        let pull_after_u_set: HashSet<&str> = HashSet::new();

        let out = classify_candidates(
            &candidates,
            &local_stats,
            &remote_by_path,
            Some(&baseline_files),
            true,
            &push_set,
            &pull_after_u_set,
        );

        assert_eq!(out.push_count, 0, "remote-deleted + locally unedited must be suppressed, never counted as push");
        assert_eq!(out.pull_count, 0);
        assert!(out.conflicts.is_empty());
    }

    #[test]
    fn classify_candidates_remote_deleted_but_locally_edited_is_a_real_push() {
        let candidates = vec!["edited-then-remote-deleted.txt".to_string()];
        let local_stats: HashMap<String, (Option<u64>, Option<u64>)> =
            [("edited-then-remote-deleted.txt".to_string(), (Some(200), Some(10)))].into();
        let remote_by_path: HashMap<&str, &RemoteFileMeta> = HashMap::new();
        let mut baseline_files = HashMap::new();
        baseline_files.insert("edited-then-remote-deleted.txt".to_string(), 100u64); // local moved past baseline -> real edit
        let push_set: HashSet<&str> = ["edited-then-remote-deleted.txt"].into();
        let pull_after_u_set: HashSet<&str> = HashSet::new();

        let out = classify_candidates(
            &candidates,
            &local_stats,
            &remote_by_path,
            Some(&baseline_files),
            true,
            &push_set,
            &pull_after_u_set,
        );

        assert_eq!(out.push_count, 1, "a real local edit past the baseline must still count as push even though remote lacks the file");
    }

    #[test]
    fn classify_candidates_push_excluded_file_reported_only_on_pull_side_never_counts_as_push() {
        // "push counting comes only from the push dry-run set" - a file excluded from push but
        // reported by the pull-side diff (no baseline, so the classifier itself can't resolve a direction)
        // must be attributed by set membership only, never defaulted into push.
        let (path, meta) = candidate_meta("push-excluded.txt", 10, 500);
        let candidates = vec![path.clone()];
        let local_stats: HashMap<String, (Option<u64>, Option<u64>)> =
            [(path.clone(), (Some(600), Some(10)))].into();
        let remote_by_path: HashMap<&str, &RemoteFileMeta> = [(path.as_str(), &meta)].into();
        let push_set: HashSet<&str> = HashSet::new(); // excluded from push
        let pull_after_u_set: HashSet<&str> = [path.as_str()].into();

        let out = classify_candidates(
            &candidates,
            &local_stats,
            &remote_by_path,
            None,
            false,
            &push_set,
            &pull_after_u_set,
        );

        assert_eq!(out.push_count, 0, "a push-excluded file must never be counted as push");
        assert_eq!(out.pull_count, 1);
    }

    #[test]
    fn classify_candidates_unattributable_file_in_neither_set_is_dropped_entirely() {
        let (path, meta) = candidate_meta("neither-side.txt", 10, 500);
        let candidates = vec![path.clone()];
        let local_stats: HashMap<String, (Option<u64>, Option<u64>)> =
            [(path.clone(), (Some(600), Some(10)))].into();
        let remote_by_path: HashMap<&str, &RemoteFileMeta> = [(path.as_str(), &meta)].into();
        let push_set: HashSet<&str> = HashSet::new();
        let pull_after_u_set: HashSet<&str> = HashSet::new();

        let out = classify_candidates(
            &candidates,
            &local_stats,
            &remote_by_path,
            None,
            false,
            &push_set,
            &pull_after_u_set,
        );

        assert_eq!(out.push_count, 0);
        assert_eq!(out.pull_count, 0);
    }

    #[test]
    fn classify_candidates_git_path_counts_git_and_direction_by_set_membership() {
        let candidates = vec![".git/HEAD".to_string()];
        let local_stats: HashMap<String, (Option<u64>, Option<u64>)> =
            [(".git/HEAD".to_string(), (Some(100), Some(10)))].into();
        let remote_by_path: HashMap<&str, &RemoteFileMeta> = HashMap::new();
        let push_set: HashSet<&str> = [".git/HEAD"].into();
        let pull_after_u_set: HashSet<&str> = HashSet::new();

        let out = classify_candidates(
            &candidates,
            &local_stats,
            &remote_by_path,
            None,
            false,
            &push_set,
            &pull_after_u_set,
        );

        assert_eq!(out.git_count, 1);
        assert_eq!(out.push_count, 1);
        assert_eq!(out.by_top_dir.get(".git").map(|c| c.git), Some(1));
    }

    #[test]
    fn classify_candidates_size_mismatch_past_baseline_is_a_conflict() {
        let (path, meta) = candidate_meta("both-edited.txt", 20, 500);
        let candidates = vec![path.clone()];
        let local_stats: HashMap<String, (Option<u64>, Option<u64>)> =
            [(path.clone(), (Some(600), Some(10)))].into();
        let remote_by_path: HashMap<&str, &RemoteFileMeta> = [(path.as_str(), &meta)].into();
        let mut baseline_files = HashMap::new();
        baseline_files.insert(path.clone(), 100u64);
        let push_set: HashSet<&str> = [path.as_str()].into();
        let pull_after_u_set: HashSet<&str> = HashSet::new();

        let out = classify_candidates(
            &candidates,
            &local_stats,
            &remote_by_path,
            Some(&baseline_files),
            true,
            &push_set,
            &pull_after_u_set,
        );

        assert_eq!(out.conflicts.len(), 1);
        assert_eq!(out.push_count, 0);
        assert_eq!(out.pull_count, 0);
    }

    // real fixture running the local `rsync` binary this app
    // resolves, asserting `%l`/`%M` report the SENDER's size/mtime - not the receiver's - in a dry-run pull's
    // `--out-format` output. The receiver holds a DIFFERENT-size, DIFFERENT-mtime copy of the same path so a
    // parser that (wrongly) reported the receiver's attributes would fail this assertion instead of passing
    // it vacuously (an empty receiver could not distinguish the two). Flags
    // mirror `rsync_pull_diff_with_metadata`'s own pull dry-run: `-avz --dry-run --out-format=... --modify-window=2`,
    // no `-u`. Skips gracefully (does not fail) if `rsync` is absent from PATH - this box's rsync is GNU 3.2.7
    // at `/usr/bin/rsync` (not openrsync; the Mac check for openrsync's own `--out-format` shape is separate,
    // docs/plan/conflict-detection-and-agy-report.md § Mac checks).
    #[test]
    fn real_rsync_out_format_reports_the_senders_size_and_mtime_in_a_dry_run_pull() {
        if Command::new("rsync").arg("--version").output().is_err() {
            eprintln!("skipping: rsync not on PATH");
            return;
        }

        let root = std::env::temp_dir().join(format!("aki-sync-rsync-fixture-{}", std::process::id()));
        let sender = root.join("sender");
        let receiver = root.join("receiver");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&sender).unwrap();
        std::fs::create_dir_all(&receiver).unwrap();

        let sender_file = sender.join("known.txt");
        let sender_contents = b"exactly seventeen"; // 17 bytes, a known, checkable size
        std::fs::write(&sender_file, sender_contents).unwrap();
        let sender_mtime_unix: u64 = 1_700_000_000; // 2023-11-14T22:13:20Z, arbitrary but fixed
        std::fs::File::open(&sender_file)
            .unwrap()
            .set_modified(UNIX_EPOCH + std::time::Duration::from_secs(sender_mtime_unix))
            .expect("std::fs::File::set_modified must be supported on this filesystem for the fixture to mean anything");

        // Same path already exists on the receiver with a DIFFERENT size and a DIFFERENT mtime (well beyond
        // the 2s modify-window), so the classifier's own "both sides differ" shape is what this fixture
        // actually exercises - the exact case #1 flagged as untested.
        let receiver_file = receiver.join("known.txt");
        let receiver_contents = b"a shorter one"; // 13 bytes - deliberately not 17
        std::fs::write(&receiver_file, receiver_contents).unwrap();
        let receiver_mtime_unix: u64 = sender_mtime_unix - 100_000; // >2s away in either direction
        std::fs::File::open(&receiver_file)
            .unwrap()
            .set_modified(UNIX_EPOCH + std::time::Duration::from_secs(receiver_mtime_unix))
            .expect("std::fs::File::set_modified must be supported on this filesystem for the fixture to mean anything");

        let output = Command::new("rsync")
            .args([
                "-avz",
                "--dry-run",
                "--out-format=%n\t%l\t%M",
                "--modify-window=2",
                &format!("{}/", sender.to_str().unwrap()),
                &format!("{}/", receiver.to_str().unwrap()),
            ])
            .output()
            .expect("rsync was confirmed present above");

        let _ = std::fs::remove_dir_all(&root);

        assert!(output.status.success(), "local-to-local dry-run rsync must succeed: {}", String::from_utf8_lossy(&output.stderr));
        let stdout = String::from_utf8_lossy(&output.stdout);

        let parsed = conflict::parse_out_format_output(&stdout, &conflict::local_utc_offset_secs_at);
        assert!(!parsed.degraded, "this GNU rsync's --out-format output must parse, not degrade: {stdout:?}");
        let meta = parsed
            .files
            .iter()
            .find(|m| m.path == "known.txt")
            .unwrap_or_else(|| panic!("known.txt missing from parsed output: {stdout:?}"));

        assert_eq!(
            meta.size,
            sender_contents.len() as u64,
            "parsed %l must equal the SENDER's real file size, not the receiver's differing {} bytes",
            receiver_contents.len()
        );
        assert_eq!(
            meta.mtime, sender_mtime_unix,
            "parsed %M, converted via this process's own local UTC offset, must equal the SENDER's real mtime, not the receiver's differing mtime ({receiver_mtime_unix})"
        );
    }
}
