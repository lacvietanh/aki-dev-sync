use serde::Serialize;
use std::collections::HashMap;
use std::time::UNIX_EPOCH;
use std::io::BufRead;
use std::io::BufReader;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::thread;
use tauri::{Emitter, Manager, Window};

use crate::projects::{validate_path_segment, validate_project, SyncProject};

static RSYNC_VERSIONS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

// Cached once on first command invocation (run_sync or check_sync_status).
// Tauri's app data dir is fixed for the lifetime of the process.
static APP_DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

fn get_rsync_versions() -> &'static Mutex<HashMap<String, String>> {
    RSYNC_VERSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

// Caches Tauri's appDataDir once per process so baseline_dir() (and anything else keyed off
// it) resolves correctly. Safe to call from any command that has an AppHandle - OnceLock::set
// is a no-op once the value is already populated.
fn ensure_app_data_dir(app: &tauri::AppHandle) {
    if APP_DATA_DIR.get().is_none() {
        if let Ok(dir) = app.path().app_data_dir() {
            let _ = APP_DATA_DIR.set(dir);
        }
    }
}

#[derive(Serialize, Clone)]
struct LogPayload {
    project_id: String,
    line: String,
}

fn emit_log(window: &Window, project_id: &str, line: String) {
    let _ = window.emit("sync-log", LogPayload { project_id: project_id.to_string(), line });
}

fn stream_reader<R: std::io::Read + Send + 'static>(
    reader: R,
    window: Window,
    project_id: String,
    prefix: &str,
) -> thread::JoinHandle<()> {
    let prefix = prefix.to_string();
    thread::spawn(move || {
        for line in BufReader::new(reader).lines().flatten() {
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
    // Null stdin so ssh/rsync can never block on an interactive prompt
    // (hostkey/password) - in a GUI app that would hang the sync silently.
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start {}: {}", label, e))?;

    let stdout = child.stdout.take().ok_or_else(|| format!("Failed to capture {} stdout", label))?;
    let stderr = child.stderr.take().ok_or_else(|| format!("Failed to capture {} stderr", label))?;

    let t_out = stream_reader(stdout, window.clone(), project_id.to_string(), "");
    let t_err = stream_reader(stderr, window.clone(), project_id.to_string(), "[ERR] ");
    let _ = t_out.join();
    let _ = t_err.join();

    let status = child.wait().map_err(|e| format!("Error waiting for {}: {}", label, e))?;
    if !status.success() {
        return Err(format!("{} exited with code: {}", label, status.code().unwrap_or(-1)));
    }
    Ok(())
}

fn execute_hook(
    window: &Window,
    project: &SyncProject,
    cmd: &str,
    dry_prefix: &str,
) -> Result<(), String> {
    emit_log(window, &project.id, format!("\n>>> {}Executing hook: {}\n", dry_prefix, cmd));
    let mut command = if project.hooks.run_hooks_on_remote {
        let mut c = crate::system::create_command("ssh");
        c.args([&project.remote_host, cmd]);
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
        emit_log(window, &project.id, format!("\n>>> {}Skipping {} hook\n", dry_prefix, phase_name));
        return Ok(());
    }
    if let Some(c) = cmd {
        if !c.trim().is_empty() {
            if let Err(e) = execute_hook(window, project, c, dry_prefix) {
                if project.hooks.ignore_hook_errors {
                    emit_log(window, &project.id, format!("[WARN] {} hook failed (ignored): {}\n", phase_name, e));
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
// Written after every full successful sync as HashMap<filename, mtime_secs>.
// On the next status check, both PUSH and PULL dry-run files are classified:
//   PULL side:  in baseline + missing locally  → local deleted it  → push_count
//               not in baseline                → remote created it → pull_count
//   PUSH side:  in baseline + local mtime UNCHANGED → remote deleted it → suppress push_count
//               in baseline + local mtime CHANGED   → user edited it  → keep in push_count
//               not in baseline                     → local created it → push_count
// The mtime comparison is key: it distinguishes "remote deleted the file" (mtime
// unchanged since last sync) from "user modified the file" (mtime changed), without
// requiring an extra SSH call. This eliminates the ambiguity in both directions.
// Baselines are stored in Tauri's appDataDir (set via APP_DATA_DIR on first
// command call). If APP_DATA_DIR is not yet set, the legacy ~/.aki path is used
// as a fallback so read_baseline can also find baselines written by old builds.

// Pre-appDataDir (<1.7.1) baseline location - sole source of truth for that path,
// used by baseline_dir()'s fallback, legacy_baseline_path(), and cleanup_legacy_baselines().
fn legacy_baseline_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".aki").join("devsync-baselines")
}

fn baseline_dir() -> PathBuf {
    if let Some(dir) = APP_DATA_DIR.get() {
        dir.join("baselines")
    } else {
        // Fallback: used if baseline_dir() is called before any Tauri command sets
        // APP_DATA_DIR (should not happen in practice).
        legacy_baseline_dir()
    }
}

fn baseline_path(project_id: &str) -> PathBuf {
    baseline_dir().join(format!("{}.json", project_id))
}

// Legacy path from pre-appDataDir builds - read_baseline checks this as fallback.
fn legacy_baseline_path(project_id: &str) -> PathBuf {
    legacy_baseline_dir().join(format!("{}.json", project_id))
}

/// Returns true if `rel` is, or is nested under, one of `dir_excludes` (entries
/// ending in `/`, e.g. `.git/`). Matches on path-component boundaries so `.wrangler/`
/// never matches a sibling like `.wrangler-backup`.
fn is_under_dir_exclude(rel: &str, dir_excludes: &[String]) -> bool {
    dir_excludes.iter().any(|e| {
        let trimmed = e.trim();
        // Only dir-entries (`/`-suffixed) carry push-only/exclude semantics here  - 
        // glob entries (`*.log`) never appear in the change list to reconcile against.
        if !trimmed.ends_with('/') {
            return false;
        }
        let name = trimmed.trim_end_matches('/');
        !name.is_empty() && (rel == name || rel.starts_with(&format!("{}/", name)))
    })
}

/// Selects the exclude list for a given transfer direction - push reads
/// `push_excludes`, pull reads `pull_excludes` (R1). Shared by `build_rsync_args`
/// (real push/pull) and `rsync_change_files` (status check) so both agree on what
/// "this direction will transfer" means. See CHANGELOG 1.13.1 (R2 revert).
fn direction_excludes(project: &SyncProject, is_push: bool) -> &Vec<String> {
    if is_push {
        &project.push_excludes
    } else {
        &project.pull_excludes
    }
}

/// Union of push_excludes and pull_excludes, deduped by trimmed value.
/// Still used by `write_baseline` (baseline must NOT track push-only-dir files  - 
/// see CHANGELOG 1.13.1 for why the baseline call site keeps the union while the
/// status-check call site was reverted to per-direction excludes).
fn union_excludes(project: &SyncProject) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for e in project.push_excludes.iter().chain(project.pull_excludes.iter()) {
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
            let mtime = path.metadata().ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            out.insert(rel, mtime);
        }
    }
}

fn write_baseline(local_path: &str, project_id: &str, dir_excludes: &[String]) -> Result<(), String> {
    let base = std::path::Path::new(local_path);
    let mut files: HashMap<String, u64> = HashMap::new();
    collect_local_files_with_mtime(base, base, dir_excludes, &mut files);

    let json = serde_json::to_string(&files)
        .map_err(|e| format!("baseline serialize: {}", e))?;

    let path = baseline_path(project_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("baseline mkdir: {}", e))?;
    }
    std::fs::write(&path, json)
        .map_err(|e| format!("baseline write: {}", e))?;
    Ok(())
}

/// One-shot migration off the pre-1.7.1 `~/.aki/devsync-baselines` path: copies any
/// baseline files not already present in appDataDir, then removes the legacy dir.
/// Frontend gates this behind a localStorage flag so it only runs once per install.
/// Losing an unmigrated baseline is non-destructive - the next full sync just rewrites it.
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
    // Old format (<1.7.1): Vec<String> - migrate with mtime=0 so suppression is disabled
    // for all entries until the next successful sync writes a new-format baseline.
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
    // Mirror mode (--delete ON): sender is fully authoritative → drop -u so the sender
    // can overwrite receiver-newer files. Keeping -u with --delete is incoherent: -u
    // protects receiver-newer files (e.g. dotfiles that survived rm -fR ./*) while
    // --delete intends an exact mirror, leaving a perpetual PULL/PUSH loop.
    // Merge mode (--delete OFF): -u is safe - keep receiver-newer files, add only what
    // the sender has that the receiver lacks.
    let is_mirror = (is_push && project.delete_on_push) || (!is_push && project.delete_on_pull);
    let base_flags = if is_mirror { "-avz" } else { "-avzu" };
    let mut args = vec![base_flags.to_string()];
    if dry_run {
        args.push("--dry-run".to_string());
    }

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
    tauri::async_runtime::spawn_blocking(move || {
        run_sync_blocking(window, project, direction, dry_run, specific_paths)
    })
    .await
    .map_err(|e| format!("Sync task error: {}", e))?
}

fn run_sync_blocking(
    window: Window,
    project: SyncProject,
    direction: String,
    dry_run: bool,
    specific_paths: Vec<String>,
) -> Result<(), String> {
    let is_push = direction == "push";
    let dry_prefix = if dry_run { "[DRY RUN] " } else { "" };

    // First log line arrives before any SSH work - closes the gap between
    // "START SYNC" (JS) and the first rsync output (which can take 1-3s).
    emit_log(&window, &project.id, format!(">>> {}Connecting to {}...\n", dry_prefix, project.remote_host));

    let pre_cmd = if is_push { &project.hooks.pre_push_cmd } else { &project.hooks.pre_pull_cmd };
    run_hook_phase(&window, &project, pre_cmd, dry_run, dry_prefix, "pre-sync")?;

    let local = format!("{}/", project.local_path.trim_end_matches('/'));
    let remote = project.remote_path.trim_end_matches('/');
    let remote_full = format!("{}:{}/", project.remote_host, remote);

    let (src, dest) = if is_push { (&local, &remote_full) } else { (&remote_full, &local) };

    if is_push {
        let remote_dir = expand_remote_tilde(&project.remote_path);
        if !dry_run {
            let mkdir_out = crate::system::create_command("ssh")
                .args([&project.remote_host, "mkdir", "-p", &remote_dir])
                .output()
                .map_err(|e| format!("Failed to create remote directory '{}': {}", remote_dir, e))?;
            if !mkdir_out.status.success() {
                return Err(format!(
                    "Remote mkdir failed for '{}': {}",
                    remote_dir,
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

    let local_v_str = {
        let mut map = versions_map.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(v) = map.get("local") {
            v.clone()
        } else {
            let v = if let Ok(out) = crate::system::create_command("rsync").arg("--version").output() {
                String::from_utf8_lossy(&out.stdout).lines().next().unwrap_or("unknown").to_string()
            } else {
                "unknown".to_string()
            };
            map.insert("local".to_string(), v.clone());
            v
        }
    };

    let remote_v_str = {
        let mut map = versions_map.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(v) = map.get(&project.remote_host) {
            v.clone()
        } else {
            let v = if let Ok(out) = crate::system::create_command("ssh").args([&project.remote_host, "rsync", "--version"]).output() {
                String::from_utf8_lossy(&out.stdout).lines().next().unwrap_or("unknown").to_string()
            } else {
                "unknown".to_string()
            };
            map.insert(project.remote_host.clone(), v.clone());
            v
        }
    };

    let log_str = format!(
        ">>> {}Local Rsync: {}\n>>> {}Remote Rsync: {}\n",
        dry_prefix, local_v_str.trim(), dry_prefix, remote_v_str.trim()
    );
    emit_log(&window, &project.id, log_str);

    emit_log(
        &window,
        &project.id,
        format!(">>> {}Executing command: rsync {}\n", dry_prefix, args.join(" ")),
    );

    let mut command = crate::system::create_command("rsync");
    if !specific_paths.is_empty() && is_push {
        command.current_dir(&project.local_path);
    }

    spawn_and_stream(&mut command.args(&args), &window, &project.id, "rsync")?;

    // Write baseline after a full (non-dry, non-partial) sync so the next status
    // check can classify PULL/PUSH files against the last-known-good state (EC-3).
    if !dry_run && specific_paths.is_empty() {
        ensure_app_data_dir(&window.app_handle());
        let local_path = project.local_path.clone();
        let project_id = project.id.clone();
        let dir_excludes = union_excludes(&project);
        if let Err(e) = write_baseline(&local_path, &project_id, &dir_excludes) {
            emit_log(&window, &project.id, format!("[WARN] Baseline write failed (non-fatal): {}\n", e));
        }
    }

    let post_cmd = if is_push { &project.hooks.post_push_cmd } else { &project.hooks.post_pull_cmd };
    run_hook_phase(&window, &project, post_cmd, dry_run, dry_prefix, "post-sync")?;

    emit_log(&window, &project.id, format!("\n>>> SYNC COMPLETED SUCCESSFULLY{}! <<<\n", dry_prefix));
    Ok(())
}

/// Pulls one named file from `host:remote_dir/filename` into `local_dir/filename` via rsync.
/// For one-off single-file syncs (e.g. REPORT.html) that don't need the full project
/// push/pull pipeline in `build_rsync_args`/`run_sync` (which only honors `specific_paths` on
/// push) - reuse this instead of hand-rolling another `create_command("rsync")` call site.
pub fn rsync_pull_file(host: &str, remote_dir: &str, filename: &str, local_dir: &str) -> Result<(), String> {
    let remote_src = format!("{}:{}/{}", host, remote_dir.trim_end_matches('/'), filename);
    let local_dest = format!("{}/{}", local_dir.trim_end_matches('/'), filename);
    let out = crate::system::create_command("rsync")
        .args(["-az", &remote_src, &local_dest])
        .output()
        .map_err(|e| format!("Failed to run rsync: {}", e))?;
    if !out.status.success() {
        return Err(format!("rsync failed: {}", String::from_utf8_lossy(&out.stderr)));
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

#[derive(Serialize, Clone)]
pub struct SyncStatusResult {
    pub has_local_changes: bool,
    pub has_remote_changes: bool,
    pub push_count: u32,
    pub pull_count: u32,
}

/// Returns the list of file paths that would be additively transferred in the given direction.
/// Status check always uses -avzu (no --delete) regardless of project settings:
///   • -u: only lists files where the SOURCE is newer - matches the button semantic
///         ("this side has something new to offer"). Without it, rsync lists receiver-newer
///         files too, causing both buttons to light when only one side was modified (EC-7).
///   • no --delete: additive content only. "deleting …" lines are the opposite direction's
///         signal and would inflate the wrong count (EC-2).
fn rsync_change_files(project: &SyncProject, is_push: bool) -> Result<Vec<String>, String> {
    let local = format!("{}/", project.local_path.trim_end_matches('/'));
    let remote = format!("{}:{}/", project.remote_host, project.remote_path.trim_end_matches('/'));
    let (src, dest) = if is_push {
        (local.as_str(), remote.as_str())
    } else {
        (remote.as_str(), local.as_str())
    };

    // Status check: per-direction excludes, same as real push/pull (R1). Badge for a
    // direction counts exactly what that direction would transfer - a push-only dir
    // (in pull_excludes, absent from push_excludes - e.g. `.git/`) IS counted on the
    // push side because push really does carry it. R2 (union excludes for both
    // directions, so push-only dirs never counted at all) shipped in 1.13.0 and was
    // reverted in 1.13.1 - see CHANGELOG and docs/plan/done/push-only-paths.md §9.
    let mut args: Vec<String> = vec!["-avzu".to_string(), "--dry-run".to_string()];
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
    let files: Vec<String> = stdout
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
        .collect();

    Ok(files)
}

/// Computes (push_count, pull_count) with full Tier-2 baseline reclassification.
///
/// PULL side (EC-3): file in pull_files + in baseline + missing locally
///   → local deleted it → reclassify to push_count (PUSH --delete propagates this)
///
/// PUSH side: file in push_files + in baseline + local mtime UNCHANGED since baseline
///   → remote deleted it (file not modified locally, so remote must have removed it)
///   → suppress from push_count.
///   If local mtime CHANGED → user modified the file → keep in push_count.
///   mtime == 0 means old-format baseline (pre-1.7.1) → don't suppress (conservative).
///
/// Falls back to raw counts when no baseline exists (first run or cleared).
fn compute_sync_counts(project: &SyncProject) -> Result<(u32, u32), String> {
    let push_files = rsync_change_files(project, true)?;
    let pull_files = rsync_change_files(project, false)?;

    let baseline = read_baseline(&project.id);

    // PULL side: Mac deleted file since last sync → should push the deletion, not pull
    let (reclassified_to_push, real_pull): (Vec<_>, Vec<_>) = pull_files.into_iter().partition(|f| {
        if let Some(ref bl) = baseline {
            let local_full = std::path::Path::new(&project.local_path).join(f);
            bl.contains_key(f) && !local_full.exists()
        } else {
            false
        }
    });

    // PUSH side: suppress only when local mtime matches baseline mtime, meaning the file
    // was NOT modified locally since last sync → remote deleted it (not a local edit).
    let (_, real_push): (Vec<_>, Vec<_>) = push_files.into_iter().partition(|f| {
        if let Some(ref bl) = baseline {
            if let Some(&baseline_mtime) = bl.get(f) {
                if baseline_mtime == 0 {
                    return false; // Old-format entry - conservative: don't suppress
                }
                let local_full = std::path::Path::new(&project.local_path).join(f);
                let current_mtime = local_full.metadata().ok()
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

    Ok((
        real_push.len() as u32 + reclassified_to_push.len() as u32,
        real_pull.len() as u32,
    ))
}

#[tauri::command]
pub async fn check_sync_status(app: tauri::AppHandle, project: SyncProject) -> Result<SyncStatusResult, String> {
    validate_project(&project)?;
    ensure_app_data_dir(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let (push_count, pull_count) = compute_sync_counts(&project)?;
        Ok(SyncStatusResult {
            has_local_changes: push_count > 0,
            has_remote_changes: pull_count > 0,
            push_count,
            pull_count,
        })
    })
    .await
    .map_err(|e| format!("check_sync_status task error: {}", e))?
}

/// Returns the list of paths that would be deleted on the destination side
/// if the given direction ran with --delete. Used by the JS confirm dialog to
/// show exactly what is at risk before the user commits to a destructive sync.
#[tauri::command]
pub async fn get_sync_delete_preview(
    project: SyncProject,
    direction: String,
) -> Result<Vec<String>, String> {
    validate_project(&project)?;
    tauri::async_runtime::spawn_blocking(move || {
        let is_push = direction == "push";
        let local = format!("{}/", project.local_path.trim_end_matches('/'));
        let remote = format!("{}:{}/", project.remote_host, project.remote_path.trim_end_matches('/'));
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
            // strip_prefix (not trim_start_matches) removes the "deleting " marker at most
            // once - trim_start_matches strips it repeatedly, so a real file whose own path
            // begins with "deleting " (rsync line: "deleting deleting me.txt") would have
            // BOTH occurrences stripped, corrupting the path fed into the delete-preview list.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::SyncHooks;

    // Local fixture builder - `projects.rs`'s test-only `make_project` is behind its own
    // `#[cfg(test)] mod tests` and does not cross the module boundary into sync.rs's tests,
    // so we build a minimal SyncProject here instead of making that helper public.
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
            tasks: vec![],
            notes: String::new(),
            dev_cmd_override: None,
            build_cmd_override: None,
        }
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
        // ".wrangler-backup" shares a prefix with ".wrangler/" but is a sibling
        // directory, not a nested path - a naive starts_with would wrongly match.
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
        // R2 revert (1.13.1): a dir present only in pull_excludes (e.g. `.git/`) must
        // NOT be excluded from the push-direction status check - push really transfers
        // it, so the badge must count it.
        let project = make_test_project(vec![], vec![".git/"]);
        let push_excludes = direction_excludes(&project, true);
        assert!(!push_excludes.contains(&".git/".to_string()));
    }

    #[test]
    fn direction_excludes_pull_only_dir_present_in_pull_direction() {
        // Same dir must still be excluded from the pull-direction status check  - 
        // pull never brings it back, so it must never count as a pull change.
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
        assert_eq!(result, vec!["a/".to_string(), "b/".to_string(), "c/".to_string(), "d/".to_string()]);
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
}
