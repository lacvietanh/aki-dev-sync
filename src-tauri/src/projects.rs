use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;

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

/// A single per-project task: created/mutated on frontend (JS `Date.now()` timestamps), Rust only persists via save_projects.
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
    // Host the last sync action ran against. A project may point to different remotes over time (remote_host is editable), so record it per action.
    #[serde(default)]
    pub last_sync_host: Option<String>,
    #[serde(default = "default_true")]
    pub dry_run: bool,
    // DEPRECATED (1.13.0, push-only-paths plan): superseded by exclude-list semantics. Kept for migration (useProjectConfig.js migratePushOnlyPaths); skip_serializing_if prevents re-materialization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_git: Option<bool>,
    // When true, PULL includes --delete (mirror remote). Opt-out to preserve local-only files.
    #[serde(default = "default_true")]
    pub delete_on_pull: bool,
    #[serde(default)]
    pub delete_on_push: bool,
    #[serde(default)]
    pub last_sync_status: Option<String>,
    // DEPRECATED (1.22.0, docs/plan/done/1.22.0-notes-json-ssot.md): notes moved to <local_path>/.akidevsync/notes.json. Kept for 1 release for JS migration (useProjectNotes.js); removed in 1.23.0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tasks: Option<Vec<ProjectTask>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default)]
    pub dev_cmd_override: Option<String>,
    #[serde(default)]
    pub build_cmd_override: Option<String>,
    // Skips this project in background sync/git polling only; manual actions are unaffected - see docs/feat/background-refresh.md.
    #[serde(default)]
    pub disabled: bool,
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

/// Validates local_path/remote_path: prevents sync from mirroring root on empty paths (format!("{}/", "") -> "/" and remote_path: "" -> host:/). Host is checked separately in validate_project.
pub fn validate_project_paths(project: &SyncProject) -> Result<(), String> {
    validate_path_segment("local_path", &project.local_path)?;
    validate_path_segment("remote_path", &project.remote_path)?;
    if project.local_path.trim().is_empty() {
        return Err("local_path cannot be empty".to_string());
    }
    // An unexpanded `~/...` never worked either: commands are spawned without a shell, so the tilde would reach rsync literally. Requiring an absolute path makes that failure explicit.
    if !PathBuf::from(&project.local_path).is_absolute() {
        return Err(format!(
            "local_path must be an absolute path (got '{}')",
            project.local_path
        ));
    }
    if project.remote_path.trim().is_empty() {
        return Err("remote_path cannot be empty".to_string());
    }
    Ok(())
}

/// Validates persisted project fields at the system boundary before any shell execution.
pub fn validate_project(project: &SyncProject) -> Result<(), String> {
    validate_project_paths(project)?;
    if project.remote_host.is_empty() {
        return Err("remote_host cannot be empty".to_string());
    }
    // Host becomes argv for ssh/rsync: leading '-' parsed as ssh option (e.g. -oProxyCommand=... executes commands on host). Shared validator prevents bypass.
    crate::system::validate_remote_host(&project.remote_host)?;
    Ok(())
}

pub fn get_projects_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(get_app_data_dir(app)?.join("projects.json"))
}

/// Returns the app data directory. `app` is unused (the dir no longer derives from the Tauri app handle - see `app_paths::app_data_dir`) but kept so `ssh.rs`/`web_server.rs` call sites do not need to change.
pub fn get_app_data_dir(_app: &AppHandle) -> Result<PathBuf, String> {
    crate::app_paths::app_data_dir()
}

/// Synchronous load_projects body: stats up to 7 icon paths per project (reading ≤250KB each); external mounts can stall in kernel metadata().
/// Callers in spawn_blocking call this directly; Tauri command load_projects wraps it to keep IPC thread non-blocking.
pub fn load_projects_blocking(app: AppHandle) -> Result<Vec<SyncProject>, String> {
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
pub async fn load_projects(app: AppHandle) -> Result<Vec<SyncProject>, String> {
    tauri::async_runtime::spawn_blocking(move || load_projects_blocking(app))
        .await
        .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

#[tauri::command]
pub fn save_projects(app: AppHandle, projects: Vec<SyncProject>) -> Result<(), String> {
    // Last line of defense (untrusted frontend): empty local_path becomes rsync --delete / (root mirror). Write rejection is recoverable; destructive sync is not.
    for p in &projects {
        validate_project_paths(p)
            .map_err(|e| format!("Cannot save project '{}': {}", p.name, e))?;
        // Host can be empty at rest (half-configured project), but if present must be safe immediately to prevent persisting malicious options.
        if !p.remote_host.is_empty() {
            crate::system::validate_remote_host(&p.remote_host)
                .map_err(|e| format!("Cannot save project '{}': {}", p.name, e))?;
        }
    }
    let path = get_projects_path(&app)?;
    let content = serde_json::to_string_pretty(&projects)
        .map_err(|e| format!("Failed to serialize projects: {}", e))?;
    // Atomic: this one file holds every project's config, tasks and notes. A truncated write loses all of them at once, and `load_projects` rejects a half-written file wholesale.
    crate::system::write_atomic(&path, &content)
        .map_err(|e| format!("Failed to write projects: {}", e))?;
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
            sync_git: None,
            delete_on_pull: false,
            delete_on_push: false,
            last_sync_status: None,
            tasks: None,
            notes: None,
            dev_cmd_override: None,
            build_cmd_override: None,
            disabled: false,
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
    fn validate_rejects_empty_local_path() {
        let p = make_project("", "~/app", "server");
        assert!(validate_project(&p).is_err());
        assert!(validate_project_paths(&p).is_err());
    }

    #[test]
    fn validate_rejects_whitespace_only_local_path() {
        let p = make_project("   ", "~/app", "server");
        assert!(validate_project(&p).is_err());
    }

    #[test]
    fn validate_rejects_relative_local_path() {
        let p = make_project("dev/app", "~/app", "server");
        assert!(validate_project(&p).is_err());
    }

    #[test]
    fn validate_rejects_tilde_local_path() {
        // `~` is never expanded: commands are spawned without a shell.
        let p = make_project("~/dev/app", "~/app", "server");
        assert!(validate_project(&p).is_err());
    }

    #[test]
    fn validate_rejects_empty_remote_path() {
        let p = make_project("/home/user/app", "", "server");
        assert!(validate_project(&p).is_err());
        assert!(validate_project_paths(&p).is_err());
    }

    #[test]
    fn validate_project_paths_ignores_empty_remote_host() {
        // A draft with no host yet is incomplete, not destructive - save must still work.
        let p = make_project("/home/user/app", "~/app", "");
        assert!(validate_project_paths(&p).is_ok());
        assert!(validate_project(&p).is_err());
    }

    #[test]
    fn validate_accepts_valid_project() {
        let p = make_project("/home/user/myproject/", "~/sites/myproject", "myserver");
        assert!(validate_project(&p).is_ok());
    }

    #[test]
    fn validate_rejects_a_host_that_ssh_would_read_as_an_option() {
        // A companion device can write a project record directly, so this is reachable without ever touching the host's own UI. `ssh -oProxyCommand=…` runs that command on THIS Mac.
        let p = make_project("/home/user/app", "~/app", "-oProxyCommand=touch /tmp/pwned");
        assert!(validate_project(&p).is_err());

        let p = make_project("/home/user/app", "~/app", "host; rm -rf /");
        assert!(validate_project(&p).is_err());

        // An ordinary host with dashes in it is not what this guards against.
        let p = make_project("/home/user/app", "~/app", "deploy@build-01.example.com");
        assert!(validate_project(&p).is_ok());
    }

    #[test]
    fn validate_accepts_a_path_on_an_unmounted_volume() {
        // Regression guard: validates path SHAPE, not existence. Unmounted external volumes must still load/save (checked at sync time in sync.rs::ensure_local_path_present).
        let p = make_project("/Volumes/NotMountedRightNow/app", "~/app", "vps01");
        assert!(validate_project_paths(&p).is_ok());
        assert!(validate_project(&p).is_ok());
    }

    #[test]
    fn validate_accepts_tilde_paths() {
        let p = make_project("/Users/aki/dev/app/", "~/apps/myapp", "vps01");
        assert!(validate_project(&p).is_ok());
    }
}
