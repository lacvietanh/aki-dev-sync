use serde::Serialize;
use std::path::{Path, PathBuf};
use crate::system::create_command;

#[derive(Serialize)]
pub struct GitInfo {
    pub status: String,
    pub remote_url: String,
    pub log: String,
}

#[derive(Serialize)]
pub struct FileConflictInfo {
    pub rel_path: String,
    pub local_mtime: i64,
    pub local_mtime_fmt: String,
    pub remote_exists: bool,
    pub remote_mtime: i64,
    pub remote_mtime_fmt: String,
}

/// Runs a git command in `path` and returns trimmed stdout, or None on failure.
fn git_capture(path: &Path, args: &[&str]) -> Option<String> {
    let out = create_command("git").current_dir(path).args(args).output().ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        None
    }
}

fn fmt_epoch(secs: i64) -> String {
    if secs <= 0 { return "—".to_string() }
    // Simple UTC formatting: seconds since epoch → "YYYY-MM-DD HH:MM"
    let secs = secs as u64;
    let s_in_day = secs % 86400;
    let days = secs / 86400;
    let h = s_in_day / 3600;
    let m = (s_in_day % 3600) / 60;
    // Day 0 = 1970-01-01
    let (y, mo, d) = days_to_ymd(days);
    format!("{:04}-{:02}-{:02} {:02}:{:02}", y, mo, d, h, m)
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut y = 1970u64;
    loop {
        let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
        let dy = if leap { 366 } else { 365 };
        if days < dy { break }
        days -= dy;
        y += 1;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let months = if leap {
        [31u64,29,31,30,31,30,31,31,30,31,30,31]
    } else {
        [31u64,28,31,30,31,30,31,31,30,31,30,31]
    };
    let mut mo = 1u64;
    for dm in &months {
        if days < *dm { break }
        days -= dm;
        mo += 1;
    }
    (y, mo, days + 1)
}

#[tauri::command]
pub async fn get_git_info(local_path: String) -> Result<GitInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = Path::new(&local_path);
        if !path.join(".git").exists() {
            return Ok(GitInfo {
                status: "No Git".to_string(),
                remote_url: String::new(),
                log: "Not a git repository.".to_string(),
            });
        }

        let porcelain = git_capture(path, &["-c", "core.quotepath=false", "status", "--porcelain"]);
        let status = match porcelain.as_deref() {
            None => "Git Error".to_string(),
            Some(s) if s.is_empty() => {
                let sb = git_capture(path, &["status", "-sb"]).unwrap_or_default();
                if sb.contains("[ahead ") { "Ahead".to_string() } else { "Clean".to_string() }
            }
            Some(_) => "Dirty".to_string(),
        };

        let remote_url = git_capture(path, &["remote", "get-url", "origin"]).unwrap_or_default();

        let mut log = git_capture(path, &["-c", "color.status=always", "-c", "core.quotepath=false", "status"]).unwrap_or_default();
        log.push_str("\n\n--- Recent Commits ---\n");
        if let Some(commits) = git_capture(path, &["log", "-n", "10", "--oneline", "--color=always"]) {
            log.push_str(&commits);
        }

        Ok(GitInfo { status, remote_url, log })
    }).await.map_err(|e| format!("Task error: {}", e))?
}

/// Returns modified/untracked files from `git status --porcelain` for the special push modal.
#[tauri::command]
pub async fn get_project_files(local_path: String, sync_git: bool) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = Path::new(&local_path);
        if !path.exists() || !path.join(".git").exists() {
            return Ok(vec![]);
        }
        let out = create_command("git")
            .current_dir(path)
            .args(["-c", "core.quotepath=false", "status", "--porcelain"])
            .output()
            .map_err(|e| format!("Failed to execute git status: {}", e))?;
        if !out.status.success() {
            return Ok(vec![]);
        }
        let mut files: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter(|s| !s.trim().is_empty())
            .filter_map(|s| {
                if s.len() <= 3 { return None; }
                let file_path = &s[3..];
                Some(if file_path.starts_with('"') && file_path.ends_with('"') {
                    file_path[1..file_path.len() - 1].to_string()
                } else {
                    file_path.to_string()
                })
            })
            .collect();

        if sync_git && path.join(".git").exists() {
            files.insert(0, ".git/".to_string());
        }

        Ok(files)
    }).await.map_err(|e| format!("Task error: {}", e))?
}

#[tauri::command]
pub async fn run_git_command(local_path: String, args: Vec<String>) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = Path::new(&local_path);
        if !path.exists() {
            return Err("Path does not exist".to_string());
        }
        let out = create_command("git")
            .current_dir(path)
            .args(&args)
            .output()
            .map_err(|e| format!("Failed to execute git: {}", e))?;

        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();

        if out.status.success() {
            Ok(if stdout.trim().is_empty() { stderr } else { stdout })
        } else {
            Err(if stderr.trim().is_empty() { stdout } else { stderr })
        }
    }).await.map_err(|e| format!("Task error: {}", e))?
}

/// Checks local and remote mtime for a list of relative file paths.
/// Used by the SELECT (native file picker) to warn about conflicts before pushing.
#[tauri::command]
pub async fn get_file_conflict_info(
    local_path: String,
    remote_host: String,
    remote_path: String,
    rel_paths: Vec<String>,
) -> Result<Vec<FileConflictInfo>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let local_base = PathBuf::from(&local_path);

        // Collect local mtimes
        let mut results: Vec<FileConflictInfo> = rel_paths.iter().map(|rel| {
            let abs = local_base.join(rel);
            let (local_mtime, local_mtime_fmt) = match std::fs::metadata(&abs)
                .and_then(|m| m.modified())
            {
                Ok(t) => {
                    let secs = t.duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    (secs, fmt_epoch(secs))
                }
                Err(_) => (0, "—".to_string()),
            };
            FileConflictInfo {
                rel_path: rel.clone(),
                local_mtime,
                local_mtime_fmt,
                remote_exists: false,
                remote_mtime: 0,
                remote_mtime_fmt: "—".to_string(),
            }
        }).collect();

        if remote_host.is_empty() || rel_paths.is_empty() {
            return Ok(results);
        }

        // Expand tilde in remote path
        let expanded_remote = if remote_path.starts_with("~/") {
            format!("$HOME/{}", &remote_path[2..])
        } else if remote_path == "~" {
            "$HOME".to_string()
        } else {
            remote_path.clone()
        };

        // Build SSH command: for each file print "STAT {mtime} {rel_path}" or "MISS {rel_path}"
        // Use double quotes around the cd path so $HOME expands on the remote shell.
        let safe_remote = expanded_remote.replace('"', "\\\"");
        let checks: Vec<String> = rel_paths.iter().map(|f| {
            // shell-escape single quotes in filename
            let safe = f.replace('\'', "'\"'\"'");
            format!(
                "if [ -e '{safe}' ]; then printf 'STAT %s %s\\n' \"$(stat -c '%Y' '{safe}' 2>/dev/null)\" '{safe}'; else printf 'MISS %s\\n' '{safe}'; fi"
            )
        }).collect();

        let script = format!("cd \"{safe_remote}\" && {}", checks.join("; "));

        let out = create_command("ssh")
            .args([&remote_host, &script])
            .output();

        if let Ok(out) = out {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                for line in stdout.lines() {
                    if let Some(rest) = line.strip_prefix("STAT ") {
                        // rest = "{mtime} {rel_path}"
                        if let Some((mtime_str, rel)) = rest.split_once(' ') {
                            if let Ok(mtime) = mtime_str.trim().parse::<i64>() {
                                if let Some(entry) = results.iter_mut().find(|e| e.rel_path == rel) {
                                    entry.remote_exists = true;
                                    entry.remote_mtime = mtime;
                                    entry.remote_mtime_fmt = fmt_epoch(mtime);
                                }
                            }
                        }
                    }
                    // MISS lines: remote_exists stays false (default)
                }
            }
        }

        Ok(results)
    }).await.map_err(|e| format!("Task error: {}", e))?
}
