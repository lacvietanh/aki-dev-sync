use serde::Serialize;
use std::collections::HashMap;
use std::io::BufRead;
use std::io::BufReader;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::thread;
use tauri::{Emitter, Window};

use crate::projects::{validate_path_segment, validate_project, SyncProject};

static RSYNC_VERSIONS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

fn get_rsync_versions() -> &'static Mutex<HashMap<String, String>> {
    RSYNC_VERSIONS.get_or_init(|| Mutex::new(HashMap::new()))
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
    let mut child = command
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

fn build_rsync_args(
    project: &SyncProject,
    is_push: bool,
    dry_run: bool,
    specific_paths: &[String],
    sync_git: bool,
    src: &str,
    dest: &str,
) -> Vec<String> {
    let mut args = vec!["-avzu".to_string()];
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
        let excludes = if is_push { &project.push_excludes } else { &project.pull_excludes };
        let sync_git_on_push = sync_git && is_push;

        for e in excludes {
            if !e.trim().is_empty() {
                if sync_git_on_push && e.trim() == ".git/" {
                    continue;
                }
                args.push(format!("--exclude={}", e));
            }
        }
        if !sync_git_on_push && !excludes.iter().any(|x| x.trim() == ".git/") {
            args.push("--exclude=.git/".to_string());
        }
        if (!is_push && project.delete_on_pull) || (is_push && project.delete_on_push) {
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
    sync_git: bool,
) -> Result<(), String> {
    validate_project(&project)?;
    validate_specific_paths(&specific_paths)?;
    tauri::async_runtime::spawn_blocking(move || {
        run_sync_blocking(window, project, direction, dry_run, specific_paths, sync_git)
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
    sync_git: bool,
) -> Result<(), String> {
    let is_push = direction == "push";
    let dry_prefix = if dry_run { "[DRY RUN] " } else { "" };

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

    let args = build_rsync_args(&project, is_push, dry_run, &specific_paths, sync_git, src, dest);

    let versions_map = get_rsync_versions();

    let local_v_str = {
        let mut map = versions_map.lock().unwrap();
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
        let mut map = versions_map.lock().unwrap();
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

    let post_cmd = if is_push { &project.hooks.post_push_cmd } else { &project.hooks.post_pull_cmd };
    run_hook_phase(&window, &project, post_cmd, dry_run, dry_prefix, "post-sync")?;

    emit_log(&window, &project.id, format!("\n>>> SYNC COMPLETED SUCCESSFULLY{}! <<<\n", dry_prefix));
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
}

fn count_rsync_changes(project: &SyncProject, is_push: bool) -> Result<usize, String> {
    let local = format!("{}/", project.local_path.trim_end_matches('/'));
    let remote = format!("{}:{}/", project.remote_host, project.remote_path.trim_end_matches('/'));
    let (src, dest) = if is_push {
        (local.as_str(), remote.as_str())
    } else {
        (remote.as_str(), local.as_str())
    };

    let sync_git = is_push && project.sync_git;
    let mut args = build_rsync_args(project, is_push, true, &[], sync_git, src, dest);
    // Insert before the trailing src/dest pair to handle sub-second mtime differences
    // between local APFS (nanosecond) and remote ext4/FAT (second precision).
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
    let count = stdout
        .lines()
        .filter(|line| {
            let l = line.trim();
            !l.is_empty()
                && !l.starts_with("sending ")
                && !l.starts_with("receiving ")
                && !l.starts_with("sent ")
                && !l.starts_with("received ")
                && !l.starts_with("total size")
                && !l.starts_with("Number of")
                && !l.starts_with("building file list")
                && !l.starts_with("Transfer starting:")
                && !l.starts_with("Skip newer ")
                && !l.ends_with('/')  // directory mtime churn (e.g. .git/ after git status)
        })
        .count();

    Ok(count)
}

#[tauri::command]
pub async fn check_sync_status(project: SyncProject) -> Result<SyncStatusResult, String> {
    validate_project(&project)?;
    tauri::async_runtime::spawn_blocking(move || {
        let push_count = count_rsync_changes(&project, true)?;
        let pull_count = count_rsync_changes(&project, false)?;
        Ok(SyncStatusResult {
            has_local_changes: push_count > 0,
            has_remote_changes: pull_count > 0,
        })
    })
    .await
    .map_err(|e| format!("check_sync_status task error: {}", e))?
}

#[cfg(test)]
mod tests {
    use super::*;

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
