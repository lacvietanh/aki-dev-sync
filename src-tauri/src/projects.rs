use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

pub fn default_true() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SyncHooks {
    pub pre_pull_cmd: Option<String>,
    pub post_pull_cmd: Option<String>,
    pub pre_push_cmd: Option<String>,
    pub post_push_cmd: Option<String>,
    pub run_hooks_on_remote: bool,
    #[serde(default)]
    pub ignore_hook_errors: bool,
}

/// A single per-project task. Created and mutated entirely on the frontend
/// (timestamps come from JS `Date.now()`); Rust only persists it via save_projects.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProjectTask {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub pin: bool,
    #[serde(default)]
    pub wish: bool,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub updated_at: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SyncProject {
    pub id: String,
    pub name: String,
    pub local_path: String,
    pub remote_host: String,
    pub remote_path: String,
    pub production_url: Option<String>,
    pub pull_excludes: Vec<String>,
    pub push_excludes: Vec<String>,
    pub hooks: SyncHooks,
    pub last_sync_action: Option<String>,
    pub last_sync_time: Option<u64>,
    // Host the last sync action ran against. A project may point to different
    // remotes over time (remote_host is editable), so record it per action.
    #[serde(default)]
    pub last_sync_host: Option<String>,
    #[serde(default = "default_true")]
    pub dry_run: bool,
    #[serde(default = "default_true")]
    pub sync_git: bool,
    // When true, PULL includes --delete (mirror remote). Opt-out to preserve local-only files.
    #[serde(default = "default_true")]
    pub delete_on_pull: bool,
    #[serde(default)]
    pub delete_on_push: bool,
    #[serde(default)]
    pub last_sync_status: Option<String>,
    #[serde(default)]
    pub tasks: Vec<ProjectTask>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub dev_cmd_override: Option<String>,
    #[serde(default)]
    pub build_cmd_override: Option<String>,
}

/// Validates that a single path segment contains no traversal or control characters.
pub fn validate_path_segment(label: &str, s: &str) -> Result<(), String> {
    if s.contains("..") {
        return Err(format!("Invalid {label}: directory traversal not allowed"));
    }
    if s.chars().any(|c| c.is_control()) {
        return Err(format!("Invalid {label}: contains control characters"));
    }
    Ok(())
}

/// Validates persisted project fields at the system boundary before any shell execution.
pub fn validate_project(project: &SyncProject) -> Result<(), String> {
    validate_path_segment("local_path", &project.local_path)?;
    validate_path_segment("remote_path", &project.remote_path)?;
    if project.remote_host.is_empty() {
        return Err("remote_host cannot be empty".to_string());
    }
    Ok(())
}

pub fn get_projects_path(app: &AppHandle) -> Result<PathBuf, String> {
    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Cannot get app data dir: {}", e))?;
    if !app_dir.exists() {
        fs::create_dir_all(&app_dir)
            .map_err(|e| format!("Failed to create app data dir: {}", e))?;
    }
    Ok(app_dir.join("projects.json"))
}

/// Returns the app data directory — avoids repeated `parent().unwrap()` at call sites.
pub fn get_app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    get_projects_path(app).and_then(|p| {
        p.parent()
            .map(|d| d.to_path_buf())
            .ok_or_else(|| "Cannot determine app data directory".to_string())
    })
}

#[tauri::command]
pub fn load_projects(app: AppHandle) -> Result<Vec<SyncProject>, String> {
    let path = get_projects_path(&app)?;
    let mut projects = vec![];
    if path.exists() {
        let content =
            fs::read_to_string(&path).map_err(|e| format!("Failed to read projects: {}", e))?;
        projects = serde_json::from_str::<Vec<SyncProject>>(&content)
            .map_err(|e| format!("projects.json is corrupt or invalid: {}", e))?;
    }
    crate::system::load_and_cache_project_icons(&projects);
    Ok(projects)
}

#[tauri::command]
pub fn save_projects(app: AppHandle, projects: Vec<SyncProject>) -> Result<(), String> {
    let path = get_projects_path(&app)?;
    let content = serde_json::to_string_pretty(&projects)
        .map_err(|e| format!("Failed to serialize projects: {}", e))?;
    fs::write(&path, content).map_err(|e| format!("Failed to write projects: {}", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_project(local_path: &str, remote_path: &str, remote_host: &str) -> SyncProject {
        SyncProject {
            id: "test".to_string(),
            name: "Test".to_string(),
            local_path: local_path.to_string(),
            remote_host: remote_host.to_string(),
            remote_path: remote_path.to_string(),
            production_url: None,
            pull_excludes: vec![],
            push_excludes: vec![],
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
            sync_git: false,
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
    fn validate_path_segment_rejects_traversal() {
        assert!(validate_path_segment("path", "/home/../etc").is_err());
    }

    #[test]
    fn validate_path_segment_rejects_control_chars() {
        assert!(validate_path_segment("path", "/home/user\x00app").is_err());
    }

    #[test]
    fn validate_path_segment_accepts_valid() {
        assert!(validate_path_segment("path", "/home/user/myproject/").is_ok());
    }

    #[test]
    fn validate_rejects_traversal_in_local_path() {
        let p = make_project("/home/user/../etc/passwd", "~/app", "server");
        assert!(validate_project(&p).is_err());
    }

    #[test]
    fn validate_rejects_traversal_in_remote_path() {
        let p = make_project("/home/user/app", "~/app/../../../etc", "server");
        assert!(validate_project(&p).is_err());
    }

    #[test]
    fn validate_rejects_control_chars_in_local_path() {
        let p = make_project("/home/user/app\x00", "~/app", "server");
        assert!(validate_project(&p).is_err());
    }

    #[test]
    fn validate_rejects_empty_remote_host() {
        let p = make_project("/home/user/app", "~/app", "");
        assert!(validate_project(&p).is_err());
    }

    #[test]
    fn validate_accepts_valid_project() {
        let p = make_project("/home/user/myproject/", "~/sites/myproject", "myserver");
        assert!(validate_project(&p).is_ok());
    }

    #[test]
    fn validate_accepts_tilde_paths() {
        let p = make_project("/Users/aki/dev/app/", "~/apps/myapp", "vps01");
        assert!(validate_project(&p).is_ok());
    }
}
