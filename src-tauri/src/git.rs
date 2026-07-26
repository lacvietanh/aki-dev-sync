use serde::Serialize;
use std::path::{Path, PathBuf};
use crate::system::create_command;

#[derive(Serialize)]
pub struct GitInfo {
    pub status: String,
    pub remote_url: String,
    pub log: String,
    pub changed_count: usize,
    /// True when `local_path` itself is absent - typically an external or network volume that is
    /// not mounted. Reported separately from the `status` string because "the folder is gone" and
    /// "the folder has no .git" used to look identical, while the user's next move differs
    /// completely: mount the drive vs run `git init` (contract C-4).
    pub local_path_missing: bool,
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
    if secs <= 0 { return " - ".to_string() }
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
        if !path.is_dir() {
            return Ok(GitInfo {
                status: "No Git".to_string(),
                remote_url: String::new(),
                log: format!(
                    "Local path not found: {}\nIf it lives on an external or network volume, mount it and refresh.",
                    local_path
                ),
                changed_count: 0,
                local_path_missing: true,
            });
        }
        if !path.join(".git").exists() {
            return Ok(GitInfo {
                status: "No Git".to_string(),
                remote_url: String::new(),
                log: "Not a git repository.".to_string(),
                changed_count: 0,
                local_path_missing: false,
            });
        }

        let porcelain = git_capture(path, &["-c", "core.quotepath=false", "status", "--porcelain"]);
        let changed_count = porcelain
            .as_deref()
            .map(|s| s.lines().filter(|l| !l.trim().is_empty()).count())
            .unwrap_or(0);
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

        Ok(GitInfo { status, remote_url, log, changed_count, local_path_missing: false })
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

/// Folds the remote stat script's stdout into `results`.
///
/// Fail-closed by construction: every entry must be answered by exactly one `STAT`/`MISS` line,
/// and a `STAT` line whose mtime cannot be parsed is an error rather than a skipped entry. A
/// skipped entry would leave `remote_exists: false` / `remote_mtime: 0`, which every caller reads
/// as "the remote does not have this file" - i.e. "safe to overwrite" and "safe to auto-approve
/// for deletion". Unknown must never collapse into that answer.
fn apply_remote_stat_output(
    stdout: &str,
    remote_host: &str,
    results: &mut [FileConflictInfo],
) -> Result<(), String> {
    let mut answered: Vec<&str> = Vec::with_capacity(results.len());

    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("STAT ") {
            // rest = "{mtime} {rel_path}"
            let (mtime_str, rel) = rest.split_once(' ').ok_or_else(|| {
                format!("Unreadable reply from '{}' while checking remote files: {}", remote_host, line)
            })?;
            let mtime = mtime_str.trim().parse::<i64>().map_err(|_| {
                format!(
                    "Cannot read the modification time of '{}' on '{}' (the remote reported '{}'). Refusing to guess whether the remote copy is newer.",
                    rel,
                    remote_host,
                    mtime_str.trim()
                )
            })?;
            if let Some(entry) = results.iter_mut().find(|e| e.rel_path == rel) {
                entry.remote_exists = true;
                entry.remote_mtime = mtime;
                entry.remote_mtime_fmt = fmt_epoch(mtime);
                answered.push(rel);
            }
        } else if let Some(rel) = line.strip_prefix("MISS ") {
            // The remote genuinely does not have this file - remote_exists stays false.
            answered.push(rel);
        }
    }

    if let Some(missing) = results.iter().find(|e| !answered.contains(&e.rel_path.as_str())) {
        return Err(format!(
            "'{}' gave no answer for '{}' while checking remote files. Refusing to continue without knowing what is on the remote.",
            remote_host, missing.rel_path
        ));
    }
    Ok(())
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
                Err(_) => (0, " - ".to_string()),
            };
            FileConflictInfo {
                rel_path: rel.clone(),
                local_mtime,
                local_mtime_fmt,
                remote_exists: false,
                remote_mtime: 0,
                remote_mtime_fmt: " - ".to_string(),
            }
        }).collect();

        if rel_paths.is_empty() {
            return Ok(results);
        }
        if remote_host.is_empty() {
            return Err(
                "Cannot check the remote files: this project has no remote host configured."
                    .to_string(),
            );
        }
        // The host lands in ssh's argv below. It arrives here straight from a project record,
        // which a companion device can write directly - so it is checked at this boundary too,
        // not only where projects are saved.
        crate::system::validate_remote_host(&remote_host)?;

        // Build SSH command: for each file print "STAT {mtime} {rel_path}" or "MISS {rel_path}"
        //
        // The `cd` target goes through the app's ONE remote-path quoter, which keeps a leading
        // `~`/`$HOME` expandable by the remote shell while quoting everything after it literally.
        // The hand-rolled escaping this replaced only escaped `"` and then embedded the result in
        // a double-quoted segment - and `"…"` does not suppress `$(…)` or backticks, so a remote
        // path of `$(curl …|sh)` ran on the remote host the moment a pre-upload conflict check
        // fired. That is the same defect fixed everywhere else in 1.20.0; this call site was
        // simply missed, which is exactly why the quoting lives in one shared function now.
        //
        // mtime is read portably: GNU coreutils first (the common Linux case), BSD/macOS as the
        // fallback. Hardcoding `stat -c` made every file on a BSD remote look non-existent, and
        // ssh still exited 0 - a silent wrong answer, which is the failure mode this whole
        // command must never produce.
        let safe_remote = crate::system::shell_quote_remote_path(&remote_path);
        let checks: Vec<String> = rel_paths.iter().map(|f| {
            // shell-escape single quotes in filename
            let safe = f.replace('\'', "'\"'\"'");
            format!(
                "if [ -e '{safe}' ]; then printf 'STAT %s %s\\n' \"$(stat -c '%Y' '{safe}' 2>/dev/null || stat -f '%m' '{safe}' 2>/dev/null)\" '{safe}'; else printf 'MISS %s\\n' '{safe}'; fi"
            )
        }).collect();

        let script = format!("cd {safe_remote} && {}", checks.join("; "));

        // ConnectTimeout matches every other ssh hop in the sync path (sync.rs) and the usage
        // poller, so the app has one answer to "how long before we call a host dead". Without it
        // a blackholed host holds the pre-upload conflict check open for the kernel's TCP
        // timeout - minutes - with the user waiting on a file picker they already answered.
        let out = create_command("ssh")
            .args(["-o", "ConnectTimeout=10", &remote_host, &script])
            .output()
            .map_err(|e| format!("Cannot reach '{}': failed to start ssh ({})", remote_host, e))?;

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let detail = stderr.trim();
            let code = out
                .status
                .code()
                .map(|c| format!("exit {}", c))
                .unwrap_or_else(|| "killed by a signal".to_string());
            return Err(format!(
                "Cannot reach '{}' to check the remote files ({}): {}",
                remote_host,
                code,
                if detail.is_empty() { "no error output from ssh" } else { detail }
            ));
        }

        apply_remote_stat_output(&String::from_utf8_lossy(&out.stdout), &remote_host, &mut results)?;

        Ok(results)
    }).await.map_err(|e| format!("Task error: {}", e))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(rels: &[&str]) -> Vec<FileConflictInfo> {
        rels.iter()
            .map(|r| FileConflictInfo {
                rel_path: r.to_string(),
                local_mtime: 100,
                local_mtime_fmt: fmt_epoch(100),
                remote_exists: false,
                remote_mtime: 0,
                remote_mtime_fmt: " - ".to_string(),
            })
            .collect()
    }

    #[test]
    fn stat_output_marks_existing_and_missing_files() {
        let mut r = entries(&["a.txt", "b.txt"]);
        let out = "STAT 1700000000 a.txt\nMISS b.txt\n";
        apply_remote_stat_output(out, "vps01", &mut r).unwrap();
        assert!(r[0].remote_exists);
        assert_eq!(r[0].remote_mtime, 1700000000);
        assert!(!r[1].remote_exists);
        assert_eq!(r[1].remote_mtime, 0);
    }

    #[test]
    fn stat_output_handles_paths_with_spaces() {
        let mut r = entries(&["my dir/my file.txt"]);
        apply_remote_stat_output("STAT 42 my dir/my file.txt\n", "vps01", &mut r).unwrap();
        assert!(r[0].remote_exists);
        assert_eq!(r[0].remote_mtime, 42);
    }

    #[test]
    fn stat_output_errors_on_unparsable_mtime() {
        // What a BSD remote produced before the portable `stat -f` fallback: an empty mtime.
        let mut r = entries(&["a.txt"]);
        let err = apply_remote_stat_output("STAT  a.txt\n", "vps01", &mut r).unwrap_err();
        assert!(err.contains("a.txt"), "error must name the file: {err}");
        assert!(!r[0].remote_exists);
    }

    #[test]
    fn stat_output_errors_when_a_file_is_unanswered() {
        let mut r = entries(&["a.txt", "b.txt"]);
        let err = apply_remote_stat_output("STAT 5 a.txt\n", "vps01", &mut r).unwrap_err();
        assert!(err.contains("b.txt"), "error must name the unanswered file: {err}");
    }

    #[test]
    fn stat_output_ignores_unrelated_noise_lines() {
        let mut r = entries(&["a.txt"]);
        let out = "Warning: Permanently added host to known hosts.\nSTAT 7 a.txt\n";
        apply_remote_stat_output(out, "vps01", &mut r).unwrap();
        assert!(r[0].remote_exists);
    }
}
