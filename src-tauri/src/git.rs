use serde::Serialize;
use std::path::Path;
use std::process::Command;

#[derive(Serialize)]
pub struct GitInfo {
    pub status: String,
    pub remote_url: String,
    pub log: String,
}

/// Runs a git command in `path` and returns trimmed stdout, or None on failure.
fn git_capture(path: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git").current_dir(path).args(args).output().ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        None
    }
}

#[tauri::command]
pub fn get_git_info(local_path: String) -> Result<GitInfo, String> {
    let path = Path::new(&local_path);
    if !path.join(".git").exists() {
        return Ok(GitInfo {
            status: "No Git".to_string(),
            remote_url: String::new(),
            log: "Not a git repository.".to_string(),
        });
    }

    let porcelain = git_capture(path, &["status", "--porcelain"]);
    let status = match porcelain.as_deref() {
        None => "Git Error".to_string(),
        Some(s) if s.is_empty() => {
            // Clean — check if ahead of remote
            let sb = git_capture(path, &["status", "-sb"]).unwrap_or_default();
            if sb.contains("[ahead ") { "Ahead".to_string() } else { "Clean".to_string() }
        }
        Some(_) => "Dirty".to_string(),
    };

    let remote_url = git_capture(path, &["remote", "get-url", "origin"]).unwrap_or_default();

    let mut log = git_capture(path, &["status"]).unwrap_or_default();
    log.push_str("\n\n--- Recent Commits ---\n");
    if let Some(commits) = git_capture(path, &["log", "-n", "10", "--oneline"]) {
        log.push_str(&commits);
    }

    Ok(GitInfo { status, remote_url, log })
}

/// Returns modified/untracked files from `git status --porcelain` for the special push modal.
/// Returns an empty list if the path is not a git repo or git fails.
#[tauri::command]
pub fn get_project_files(local_path: String, sync_git: bool) -> Result<Vec<String>, String> {
    let path = Path::new(&local_path);
    if !path.exists() || !path.join(".git").exists() {
        return Ok(vec![]);
    }
    let out = Command::new("git")
        .current_dir(path)
        // core.quotepath=false → git emits real UTF-8 paths instead of
        // octal-escaped (`"\303\251"`), so non-ASCII filenames (Vietnamese,
        // emoji) display correctly. Quotes are still stripped below.
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
}
