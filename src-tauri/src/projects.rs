use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;

pub fn default_true() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SyncHooks {
    #[serde(default)]
    pub pre_pull_cmd: Option<String>,
    #[serde(default)]
    pub post_pull_cmd: Option<String>,
    #[serde(default)]
    pub pre_push_cmd: Option<String>,
    #[serde(default)]
    pub post_push_cmd: Option<String>,
    #[serde(default)]
    pub run_hooks_on_remote: bool,
    #[serde(default)]
    pub ignore_hook_errors: bool,
}

/// `SyncProject.deploy` (docs/plan/done/deploy-action.md § Design, amended 2026-09-28): where a deploy runs and
/// whether a push offers it. A remote deploy names its OWN `host`/`path` - never the active sync host
/// implicitly, because that is a table dropdown one click from any other box (docs/research/sync-host-safety.md).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ProjectDeploy {
    /// `"local"` | `"remote"`. Empty/unset is treated as "local" by the frontend (deploy plan default),
    /// but stored as-is here - this struct only carries the on-disk shape.
    #[serde(default)]
    pub run_on: String,
    #[serde(default)]
    pub on_push: bool,
    /// SSH alias a remote deploy runs on. Empty with `run_on: "remote"` = not runnable (the frontend disables it).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub host: String,
    /// Remote working directory; empty = the project's `remote_path`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
}

/// One remembered remote (docs/research/akidevsync-project-config-scope-2.md § Field placement): `targets.<host>`.
/// `hooks` are host-specific facts (real hooks embed that host's paths), so switching the active `remote_host` restores this entry instead of losing the
/// previous host's settings (F5).
///
/// `remote_path` is DEPRECATED here (docs/plan/done/settings-and-state-layout.md § Amendments, 1.32.0): a
/// project has exactly ONE remote directory regardless of which host serves it - storing it per-host made
/// the first sync to a new host demand re-entering a path that was never actually different, which no real
/// project in this app's own registry has ever needed. Kept only as a one-release migration source
/// (`sync_state::migrate_settings_and_state` lifts it onto `SyncProject.remote_path`); app code never writes
/// it again, and `skip_serializing_if` drops the key from a `Target` the moment it is cleared.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Target {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub remote_path: String,
    #[serde(default)]
    pub hooks: Option<SyncHooks>,
}

/// `SyncHooks::default()` comparison target for `hooks`' `skip_serializing_if` below.
fn is_default_hooks(h: &SyncHooks) -> bool {
    *h == SyncHooks::default()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SyncProject {
    pub id: String,
    // name/production_url/pull_excludes/push_excludes/commands.* are project-owned (project_config.rs) as
    // of 1.32.0; these six registry fields are load-bearing only as a migration source and a plain (not
    // Option) type so no read site needs an unwrap. Owner, wiring and the save-funnel strip: docs/arch/settings-and-state.md.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub local_path: String,
    pub remote_host: String,
    // The project's one remote directory - fixed across every host that serves it, never per-host (1.32.0
    // fix; briefly lived in `targets.<host>.remote_path` in 1.32.0, see `Target`'s doc comment). Always the
    // single, authoritative copy; `targets.<remote_host>` never holds it going forward.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub remote_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub production_url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pull_excludes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub push_excludes: Vec<String>,
    #[serde(default, skip_serializing_if = "is_default_hooks")]
    pub hooks: SyncHooks,
    // DEPRECATED (1.32.0, docs/plan/done/settings-and-state-layout.md): moved to
    // `~/.aki/devsync/state/<id>/<host>/last_sync.json` (a target · state fact, not project-on-machine config).
    // Kept as Option + skip_serializing_if for one release so the migration can read an old value once and
    // then never re-materializes it (1.13.0 sync_git lesson) - see sync_state::migrate_settings_and_state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sync_action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sync_time: Option<u64>,
    // Host the last sync action ran against. A project may point to different remotes over time (remote_host is editable), so record it per action.
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sync_status: Option<String>,
    // DEPRECATED as a registry field (1.32.0, § B1): owned by `project.json`'s `commands.dev`. Same skip_serializing_if reasoning as `name` above.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dev_cmd_override: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_cmd_override: Option<String>,
    // Skips this project in background sync/git polling only; manual actions are unaffected - see docs/feat/background-refresh.md.
    #[serde(default)]
    pub disabled: bool,
    // Per-host remembered remote (target · config, see Target doc comment); keyed by SSH alias. `#[serde(default)]` lets pre-1.32.0 projects.json entries load with an empty map, migrated once by sync_state::migrate_settings_and_state.
    #[serde(default)]
    pub targets: BTreeMap<String, Target>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deploy: Option<ProjectDeploy>,
    // DEPRECATED (1.22.0, docs/plan/done/1.22.0-notes-json-ssot.md): moved to `<local_path>/.akidevsync/notes.json`.
    // Kept opaque (not the old typed list) so an unmigrated legacy record survives load -> save until
    // useProjectNotes.js's migrateLegacyProjectNotes clears it; skip_serializing_if stops these keys from
    // being re-materialized once gone (1.13.0 sync_git lesson).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tasks: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<serde_json::Value>,
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
    validate_path_segment("remote_path", &project.remote_path)?;
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

/// Pure parse of `projects.json` - no icon I/O, no `local_path` access of any kind. Safe to call before any
/// window exists (setup()-time migration, S-b: `load_projects_blocking`'s icon stat pass must not run there).
pub fn load_projects_raw(app: &AppHandle) -> Result<Vec<SyncProject>, String> {
    let path = get_projects_path(app)?;
    if !path.exists() {
        return Ok(vec![]);
    }
    let content =
        fs::read_to_string(&path).map_err(|e| format!("Failed to read projects: {}", e))?;
    serde_json::from_str::<Vec<SyncProject>>(&content)
        .map_err(|e| format!("projects.json is corrupt or invalid: {}", e))
}

/// Synchronous load_projects body: stats up to 7 icon paths per project (reading ≤250KB each); external mounts can stall in kernel metadata().
/// Callers in spawn_blocking call this directly; Tauri command load_projects wraps it to keep IPC thread non-blocking.
pub fn load_projects_blocking(app: AppHandle) -> Result<Vec<SyncProject>, String> {
    let projects = load_projects_raw(&app)?;
    crate::system::load_and_cache_project_icons(&projects);
    Ok(projects)
}

#[tauri::command]
pub async fn load_projects(app: AppHandle) -> Result<Vec<SyncProject>, String> {
    tauri::async_runtime::spawn_blocking(move || load_projects_blocking(app))
        .await
        .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

/// Re-reads projects.json from disk and rebuilds the icon cache in place — the only way to pick up
/// an icon added/replaced after boot, since `load_and_cache_project_icons` otherwise only runs once at startup.
#[tauri::command]
pub async fn reload_project_icons(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || load_projects_blocking(app).map(|_| ()))
        .await
        .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

/// `name` is project-owned (project_config.rs) and often empty in the registry payload now, so an
/// error keyed on it read as "Cannot save project ''". Identify by id and the local folder's basename instead.
fn project_error_label(p: &SyncProject) -> String {
    let basename = PathBuf::from(&p.local_path)
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| p.local_path.clone());
    format!("{} ({})", p.id, basename)
}

#[tauri::command]
pub fn save_projects(app: AppHandle, projects: Vec<SyncProject>) -> Result<(), String> {
    let path = get_projects_path(&app)?;
    // Last line of defense (untrusted frontend): empty local_path becomes rsync --delete / (root mirror). Write rejection is recoverable; destructive sync is not.
    for p in &projects {
        validate_project_paths(p)
            .map_err(|e| format!("Cannot save project {}: {}", project_error_label(p), e))?;
        // Host can be empty at rest (half-configured project), but if present must be safe immediately to prevent persisting malicious options.
        if !p.remote_host.is_empty() {
            crate::system::validate_remote_host(&p.remote_host)
                .map_err(|e| format!("Cannot save project {}: {}", project_error_label(p), e))?;
        }
    }
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

    /// B1 (docs/plan/done/settings-and-state-layout.md § B): name/production_url/pull_excludes/push_excludes/
    /// dev_cmd_override/build_cmd_override are project-owned (`project.json`) from 1.32.0. A legacy entry
    /// still carrying them must load without error, and once the frontend's save funnel strips them to
    /// their empty/None equivalents, `skip_serializing_if` must keep them out of the saved registry for
    /// good (1.13.0 sync_git lesson) — never re-drifting back in on a later load -> save.
    #[test]
    fn project_owned_fields_load_from_legacy_and_never_reappear_once_stripped() {
        let mut legacy = make_project("/home/user/app", "~/app", "server");
        legacy.name = "Legacy Name".into();
        legacy.production_url = Some("https://example.com".into());
        legacy.pull_excludes = vec![".git/".into()];
        legacy.push_excludes = vec![".git/".into()];
        legacy.dev_cmd_override = Some("npm run dev".into());
        legacy.build_cmd_override = Some("npm run build".into());

        let json = serde_json::to_string(&legacy).unwrap();
        let reloaded: SyncProject = serde_json::from_str(&json).unwrap();
        assert_eq!(reloaded.name, "Legacy Name", "an old registry entry must still load its project-owned fields");

        // Simulate the frontend's save funnel stripping these six fields once project.json owns them.
        let mut stripped = legacy.clone();
        stripped.name = String::new();
        stripped.production_url = None;
        stripped.pull_excludes = vec![];
        stripped.push_excludes = vec![];
        stripped.dev_cmd_override = None;
        stripped.build_cmd_override = None;

        let stripped_json = serde_json::to_string(&stripped).unwrap();
        for key in [
            "\"name\"",
            "\"production_url\"",
            "\"pull_excludes\"",
            "\"push_excludes\"",
            "\"dev_cmd_override\"",
            "\"build_cmd_override\"",
        ] {
            assert!(!stripped_json.contains(key), "{} must never be persisted once stripped", key);
        }
        let reloaded_stripped: SyncProject = serde_json::from_str(&stripped_json).unwrap();
        assert_eq!(reloaded_stripped.name, "");
        assert!(reloaded_stripped.production_url.is_none());
        assert!(reloaded_stripped.pull_excludes.is_empty());
        assert!(reloaded_stripped.dev_cmd_override.is_none());
    }

    /// 1.32.0 fix: `remote_path` is a project fact, not a per-host one - it must keep serializing at the
    /// top level regardless of whether a `targets.<host>` entry exists, and must never be read back FROM a
    /// target. `hooks` stays the opposite: still stripped from the top level once a target holds it (it
    /// really is host-specific). CLAUDE.md multi-entity guard: round-tripping project A must never affect
    /// project B's own target entry.
    #[test]
    fn top_level_remote_path_always_persists_regardless_of_targets() {
        let mut a = make_project("/local/a", "~/app-a", "hostA");
        a.targets.insert("hostA".into(), Target { remote_path: String::new(), hooks: None });
        let mut b = make_project("/local/b", "~/app-b", "hostB");
        b.targets.insert("hostB".into(), Target { remote_path: String::new(), hooks: None });
        let all = vec![a, b];

        let json = serde_json::to_string(&all).unwrap();
        let values: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert_eq!(values[0].get("remote_path").and_then(|v| v.as_str()), Some("~/app-a"));
        assert_eq!(values[1].get("remote_path").and_then(|v| v.as_str()), Some("~/app-b"));
        for v in &values {
            assert!(v.get("hooks").is_none(), "default hooks must not be persisted at the top level");
        }

        let reloaded: Vec<SyncProject> = serde_json::from_str(&json).unwrap();
        assert_eq!(reloaded[0].remote_path, "~/app-a", "project A's own path survives untouched by project B");
        assert_eq!(reloaded[1].remote_path, "~/app-b", "project B's own path survives untouched by project A");
    }

    /// A stale `targets.<host>.remote_path` left over from a pre-fix registry (the field is now
    /// migration-only, see `Target`'s doc comment) must never leak back onto the project once a real
    /// top-level value exists - only the one-shot migration in `sync_state.rs` is allowed to lift it.
    #[test]
    fn stale_target_remote_path_never_overrides_the_top_level_value() {
        let mut p = make_project("/local/half-configured", "~/real-path", "hostA");
        p.targets.insert("hostA".into(), Target { remote_path: "~/stale-old-path".into(), hooks: None });

        let json = serde_json::to_string(&p).unwrap();
        let reloaded: SyncProject = serde_json::from_str(&json).unwrap();
        assert_eq!(reloaded.remote_path, "~/real-path");
    }

    /// An empty remote_host has no target key that could ever hold hooks (`targets[""]` is never created) -
    /// the top-level `hooks` field is the ONLY copy there, so it must keep serializing regardless.
    #[test]
    fn empty_remote_host_legacy_entry_round_trips_hooks_intact() {
        let mut p = make_project("/local/half-configured", "~/still-here", "");
        p.hooks.pre_pull_cmd = Some("echo hi".into());

        let json = serde_json::to_string(&p).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(value.get("hooks").is_some(), "non-default hooks with no target to hold them must still serialize");
        assert!(!p.targets.contains_key(""), "an empty remote_host must never gain a targets[\"\"] entry");

        let reloaded: SyncProject = serde_json::from_str(&json).unwrap();
        assert_eq!(reloaded.remote_path, "~/still-here");
        assert_eq!(reloaded.hooks.pre_pull_cmd.as_deref(), Some("echo hi"));
    }

    /// B2 last-sync end-to-end (docs/plan/done/settings-and-state-layout.md § B2): `useSync.js` now calls
    /// `write_last_sync` (state/<id>/<host>/last_sync.json, sync_state.rs) instead of leaving these fields
    /// Some on the project object for `save_projects` to persist. This is the same contract as B1's
    /// project-owned fields: `skip_serializing_if` only drops a None/empty value - it is the frontend's
    /// save funnel (`saveProjectsList`) that resets these four fields to null in the payload before every
    /// save, same as `name`/`production_url` above. Proves the funnel's stripped payload never re-persists
    /// them, even starting from a hydrated Some (mirrors `project_owned_fields_load_from_legacy_...` above).
    #[test]
    fn last_sync_fields_never_persist_once_the_save_funnel_strips_them() {
        let mut hydrated = make_project("/home/user/app", "~/app", "server");
        hydrated.last_sync_action = Some("PUSH".into());
        hydrated.last_sync_time = Some(12345);
        hydrated.last_sync_host = Some("server".into());
        hydrated.last_sync_status = Some("success".into());

        // A Some value straight from hydration DOES persist if sent as-is - this is what makes the frontend's stripping in saveProjectsList load-bearing, not optional.
        let unstripped_json = serde_json::to_string(&vec![hydrated.clone()]).unwrap();
        assert!(unstripped_json.contains("last_sync_action"), "sanity: Some values do serialize when sent");

        // Simulate saveProjectsList's payload construction (useProjectConfig.js): reset to null before invoke.
        let mut stripped = hydrated.clone();
        stripped.last_sync_action = None;
        stripped.last_sync_time = None;
        stripped.last_sync_host = None;
        stripped.last_sync_status = None;

        let json = serde_json::to_string(&vec![stripped]).unwrap();
        for key in ["last_sync_action", "last_sync_time", "last_sync_host", "last_sync_status"] {
            assert!(!json.contains(key), "{} must never be persisted once the save funnel strips it", key);
        }
    }

    /// B4 (docs/plan/done/settings-and-state-layout.md § C): `tasks`/`notes` are deprecated but opaque, not
    /// deleted - an unmigrated legacy record must survive a load -> save round trip so
    /// useProjectNotes.js's migrateLegacyProjectNotes still sees it, while a project that never had one
    /// must never gain the key back (1.13.0 sync_git lesson).
    #[test]
    fn deprecated_tasks_and_notes_round_trip_some_and_never_reintroduce_none() {
        let mut with_legacy = make_project("/home/user/app", "~/app", "server");
        with_legacy.tasks = Some(serde_json::json!([{"id": "t1", "done": false}]));
        with_legacy.notes = Some(serde_json::json!("legacy note"));

        let json = serde_json::to_string(&with_legacy).unwrap();
        let reloaded: SyncProject = serde_json::from_str(&json).unwrap();
        assert_eq!(reloaded.tasks, with_legacy.tasks, "a Some legacy value must survive load -> save");
        assert_eq!(reloaded.notes, with_legacy.notes);

        let without_legacy = make_project("/home/user/app2", "~/app2", "server");
        assert!(without_legacy.tasks.is_none());
        let json2 = serde_json::to_string(&without_legacy).unwrap();
        assert!(!json2.contains("\"tasks\""), "a project with no legacy value must never gain the key back");
        assert!(!json2.contains("\"notes\""));
        let reloaded2: SyncProject = serde_json::from_str(&json2).unwrap();
        assert!(reloaded2.tasks.is_none());
    }

    /// deploy plan: an old projects.json entry with no `deploy` key must load as None, not error.
    #[test]
    fn project_deploy_defaults_to_none_when_absent() {
        let p: SyncProject = serde_json::from_str(r#"{"id":"a","local_path":"/l","remote_host":"h"}"#).unwrap();
        assert!(p.deploy.is_none());
    }

    /// Round trip + CLAUDE.md multi-entity guard: project A's deploy (with its own host) must not leak into
    /// project B, and an empty `host`/`path` is omitted on disk rather than written as "".
    #[test]
    fn project_deploy_round_trips_and_leaves_other_projects_untouched() {
        let mut a = make_project("/local/a", "~/app-a", "hostA");
        a.deploy = Some(ProjectDeploy { run_on: "remote".into(), on_push: true, host: "prod".into(), path: String::new() });
        let b = make_project("/local/b", "~/app-b", "hostB");
        let json = serde_json::to_string(&vec![a, b]).unwrap();
        assert!(!json.contains("\"path\":\"\""), "empty deploy path must be omitted");
        let reloaded: Vec<SyncProject> = serde_json::from_str(&json).unwrap();
        let a_deploy = reloaded[0].deploy.clone().unwrap();
        assert_eq!((a_deploy.run_on.as_str(), a_deploy.on_push, a_deploy.host.as_str()), ("remote", true, "prod"));
        assert!(reloaded[1].deploy.is_none(), "project B must stay untouched by project A's deploy config");
    }
}
