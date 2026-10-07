//! Per-project settings, stored **inside the project's own working directory**.
//!
//! THE DECISION THIS FILE IMPLEMENTS (docs/research/akidevsync-project-config-scope-2.md § Field
//! placement): `name`, `production_url`, `pull_excludes`/`push_excludes` and `commands.{dev,build}` are
//! properties of the *project*, the same on every machine that clones it - so they travel with the
//! project (rsync, git) at `<local_path>/.akidevsync/project.json`, exactly like `notes.json` (1.22.0).
//!
//! Everything about that file - path, shape, atomic write - is owned here, mirroring `project_notes.rs`'s
//! tagged-status read contract and read-modify-write write contract (see `write_blocking`).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
pub struct ProjectConfigCommands {
    #[serde(default)]
    pub dev: String,
    #[serde(default)]
    pub build: String,
    /// `commands.deploy` (docs/plan/done/deploy-action.md): same shape as `dev`/`build` - no `cd`, no machine
    /// path, the working directory is supplied by the app at run time.
    #[serde(default)]
    pub deploy: String,
    /// Preserves any `commands.x` key the dialog doesn't know about (a hand-added key, or a newer build's
    /// field this one predates) - `write_blocking` never lets the dialog's own submission replace this map.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// On-disk shape: serde(default) on every field lets forward-compatible or hand-trimmed files load
/// without migration, same as `ProjectNotesFile`. Unlike `ProjectNotesFile`, this file's own research doc
/// explicitly allows hand edits, so `extra` (and `commands.extra`) round-trip any key this struct doesn't
/// name - see `write_blocking`'s merge, which is what actually preserves them across a save.
#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
pub struct ProjectConfigFile {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub production_url: String,
    #[serde(default)]
    pub pull_excludes: Vec<String>,
    #[serde(default)]
    pub push_excludes: Vec<String>,
    #[serde(default)]
    pub commands: ProjectConfigCommands,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectConfigStatus {
    /// File read and parsed. Writable.
    Ok,
    /// local_path is a readable directory but project.json is missing (fresh clone/new project). Writable.
    Missing,
    /// local_path is not a directory (unmounted volume/perms) or the file is unreadable. Not writable.
    Unavailable,
    /// File exists but contains invalid JSON (e.g. a git merge conflict). Not writable, never defaulted.
    Corrupt,
}

#[derive(Serialize, Clone, Debug)]
pub struct ProjectConfigRead {
    pub status: ProjectConfigStatus,
    /// Some only when status == Ok: UI cannot read content from unwritable states.
    pub file: Option<ProjectConfigFile>,
    pub error: Option<String>,
}

impl ProjectConfigRead {
    fn ok(file: ProjectConfigFile) -> Self {
        Self { status: ProjectConfigStatus::Ok, file: Some(file), error: None }
    }
    fn missing() -> Self {
        Self { status: ProjectConfigStatus::Missing, file: None, error: None }
    }
    fn unavailable(e: impl std::fmt::Display) -> Self {
        Self { status: ProjectConfigStatus::Unavailable, file: None, error: Some(e.to_string()) }
    }
    fn corrupt(e: impl std::fmt::Display) -> Self {
        Self { status: ProjectConfigStatus::Corrupt, file: None, error: Some(e.to_string()) }
    }
}

/// One entry of the boot-time batch read.
#[derive(Deserialize, Clone, Debug)]
pub struct ProjectConfigTarget {
    pub id: String,
    pub local_path: String,
}

/// The ONE place `.akidevsync/project.json` is spelled in the Rust tree.
fn config_path(local_path: &str) -> PathBuf {
    Path::new(local_path).join(".akidevsync").join("project.json")
}

/// Validates local_path segment against traversal/control chars (untrusted input from projects.json / paired companion).
fn check_path(local_path: &str) -> Result<(), String> {
    crate::projects::validate_path_segment("local_path", local_path)
}

/// Synchronous read: returns structured statuses rather than Err for missing/corrupt/unreachable states
/// (same reasoning as `project_notes::read_blocking` - a missing project would otherwise be indistinguishable
/// from an unmounted volume, and only the first is safe to write over).
pub(crate) fn read_blocking(local_path: &str) -> ProjectConfigRead {
    if let Err(e) = check_path(local_path) {
        return ProjectConfigRead::unavailable(e);
    }
    let dir = Path::new(local_path);
    if !dir.is_dir() {
        return ProjectConfigRead::unavailable(format!(
            "'{}' is not a readable directory right now",
            local_path
        ));
    }
    let path = config_path(local_path);
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return ProjectConfigRead::missing(),
        Err(e) => return ProjectConfigRead::unavailable(e),
    };
    match serde_json::from_str::<ProjectConfigFile>(&raw) {
        Ok(file) => ProjectConfigRead::ok(file),
        Err(e) => ProjectConfigRead::corrupt(e),
    }
}

/// Reads one project's config file.
/// Uses spawn_blocking because local_path can live on network/external mounts that stall in kernel metadata/read calls.
#[tauri::command]
pub async fn read_project_config(local_path: String) -> Result<ProjectConfigRead, String> {
    tauri::async_runtime::spawn_blocking(move || read_blocking(&local_path))
        .await
        .map_err(|e| format!("read_project_config task join error: {}", e))
}

/// Batch read for app boot: one IPC round-trip for all projects; failures on unmounted volumes are isolated per target.
#[tauri::command]
pub async fn read_project_config_map(
    targets: Vec<ProjectConfigTarget>,
) -> Result<HashMap<String, ProjectConfigRead>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        targets
            .into_iter()
            .map(|t| (t.id, read_blocking(&t.local_path)))
            .collect()
    })
    .await
    .map_err(|e| format!("read_project_config_map task join error: {}", e))
}

/// T1 (docs/plan/done/settings-and-state-layout.md): the ONE place `run_sync`/`check_sync_status`/
/// `get_sync_delete_preview` obtain rsync excludes - read fresh from `project.json` at call time, never
/// trusted from the JS-passed `SyncProject` (which may be stale mid-Refresh, or already stripped by the
/// save funnel). Refuses rather than falling back to empty/stale excludes whenever the file is not `Ok`.
pub(crate) fn require_ok_excludes(local_path: &str) -> Result<(Vec<String>, Vec<String>), String> {
    let read = read_blocking(local_path);
    match read.status {
        ProjectConfigStatus::Ok => {
            let file = read.file.unwrap_or_default();
            Ok((file.pull_excludes, file.push_excludes))
        }
        other => Err(format!(
            "Refusing to sync '{}': .akidevsync/project.json is {:?}, not Ok",
            local_path, other
        )),
    }
}

/// True read-modify-write, mirroring `project_notes::write_blocking`'s contract: the current status is read
/// first and a Corrupt/Unavailable file refuses the write rather than being blindly overwritten (a
/// git-conflicted `project.json` would otherwise lose both sides of the conflict). Unlike a whole-file
/// overwrite, only the five fields the dialog actually edits are replaced - `extra` (and `commands.extra`)
/// are always carried forward from the file already on disk, since the dialog never round-trips a key it
/// does not know about (a hand-added key, or a newer build's field this one predates).
fn write_blocking(local_path: &str, incoming: ProjectConfigFile) -> Result<ProjectConfigFile, String> {
    check_path(local_path)?;
    if !Path::new(local_path).is_dir() {
        return Err(format!(
            "'{}' is not a readable directory right now",
            local_path
        ));
    }
    let path = config_path(local_path);
    let current = read_blocking(local_path);
    let mut file = match current.status {
        ProjectConfigStatus::Ok => current.file.unwrap_or_default(),
        ProjectConfigStatus::Missing => ProjectConfigFile::default(),
        ProjectConfigStatus::Corrupt => {
            return Err(format!(
                "'{}' is not valid JSON — resolve it before saving",
                path.display()
            ));
        }
        ProjectConfigStatus::Unavailable => {
            return Err(format!("'{}' is not writable right now", path.display()));
        }
    };

    file.name = incoming.name;
    file.production_url = incoming.production_url;
    file.pull_excludes = incoming.pull_excludes;
    file.push_excludes = incoming.push_excludes;
    file.commands.dev = incoming.commands.dev;
    file.commands.build = incoming.commands.build;
    file.commands.deploy = incoming.commands.deploy;
    // file.extra / file.commands.extra: intentionally NOT overwritten by `incoming` - they stay whatever was already on disk, since the dialog's submission never carries a meaningful value for either.

    // Pretty-printed: this file is meant to be diffed in git, and a one-line JSON blob makes every edit a whole-file diff.
    let json = serde_json::to_string_pretty(&file)
        .map_err(|e| format!("Failed to serialize project config: {}", e))?;
    crate::system::write_atomic(&path, &json)?;
    Ok(file)
}

#[tauri::command]
pub async fn write_project_config(
    local_path: String,
    config: ProjectConfigFile,
) -> Result<ProjectConfigFile, String> {
    tauri::async_runtime::spawn_blocking(move || write_blocking(&local_path, config))
        .await
        .map_err(|e| format!("write_project_config task join error: {}", e))?
}

/// True when every legacy-registry field this seed would write is empty - a project that never had any of
/// these values set (a brand-new, still-unconfigured project) must not gain an all-empty `project.json`
/// just from being loaded once; there is nothing there yet to seed.
///
/// `pub(crate)` (not `pub`) so `sync_state.rs`'s migration rehearsal can predict the JS seeding step's
/// count from the same rule instead of duplicating it (docs/plan/done/1.32.0-mac-handoff.md § 2) - the seeding
/// itself still only runs from JS (`write_project_configs_if_missing`'s per-project caller).
pub(crate) fn is_all_empty(file: &ProjectConfigFile) -> bool {
    file.name.trim().is_empty()
        && file.production_url.trim().is_empty()
        && file.pull_excludes.is_empty()
        && file.push_excludes.is_empty()
        && file.commands.dev.trim().is_empty()
        && file.commands.build.trim().is_empty()
}

/// One project's config as seeded from the pre-1.32.0 registry fields, for the load-time migration.
#[derive(Deserialize, Clone, Debug)]
pub struct ProjectConfigSeed {
    pub id: String,
    pub local_path: String,
    pub file: ProjectConfigFile,
}

/// Per-project result of one seed attempt, so the caller knows exactly which projects are now owned by
/// `project.json` (vs already were, vs still have nothing to seed from, vs could not be reached at all).
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SeedOutcome {
    /// `project.json` did not exist and now does, seeded from the legacy registry values.
    Written,
    /// `project.json` already existed - untouched, the existing file already wins.
    AlreadyOwned,
    /// Every legacy value was empty - refused to write an all-empty file; still Missing, retried next launch.
    NothingToSeed,
    /// Folder unreadable (unmounted volume, permissions) or the write itself failed - retried next launch.
    Unavailable,
}

fn seed_one(local_path: &str, file: ProjectConfigFile) -> SeedOutcome {
    match read_blocking(local_path).status {
        ProjectConfigStatus::Missing => {
            if is_all_empty(&file) {
                SeedOutcome::NothingToSeed
            } else if write_blocking(local_path, file).is_ok() {
                SeedOutcome::Written
            } else {
                SeedOutcome::Unavailable
            }
        }
        ProjectConfigStatus::Ok => SeedOutcome::AlreadyOwned,
        ProjectConfigStatus::Unavailable | ProjectConfigStatus::Corrupt => SeedOutcome::Unavailable,
    }
}

/// Migration entry point (docs/plan/done/settings-and-state-layout.md § C): writes `project.json` for every seed
/// whose file is still missing, has a readable folder, and has at least one non-empty legacy value - never
/// an all-empty file. Returns each project's outcome keyed by id, so the caller (JS) knows exactly which
/// projects are now owned by `project.json` versus still need retrying or have nothing to seed at all.
#[tauri::command]
pub async fn write_project_configs_if_missing(
    seeds: Vec<ProjectConfigSeed>,
) -> Result<HashMap<String, SeedOutcome>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        seeds
            .into_iter()
            .map(|s| {
                let outcome = seed_one(&s.local_path, s.file);
                (s.id, outcome)
            })
            .collect()
    })
    .await
    .map_err(|e| format!("write_project_configs_if_missing task join error: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct Scratch(PathBuf);
    impl Scratch {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("aki-project-config-test-{}-{}", tag, std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }
        fn path(&self) -> String {
            self.0.to_string_lossy().to_string()
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write_raw(dir: &Scratch, body: &str) {
        let p = config_path(&dir.path());
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
    }

    #[test]
    fn missing_dir_reads_unavailable_not_missing() {
        let r = read_blocking("/definitely/not/a/real/mount/point");
        assert_eq!(r.status, ProjectConfigStatus::Unavailable);
    }

    #[test]
    fn missing_file_in_a_real_dir_reads_missing() {
        let d = Scratch::new("missing");
        let r = read_blocking(&d.path());
        assert_eq!(r.status, ProjectConfigStatus::Missing);
    }

    #[test]
    fn invalid_json_reads_corrupt_and_never_defaults() {
        let d = Scratch::new("corrupt");
        write_raw(&d, "<<<<<<< HEAD\n{\"name\":\"mine\"}\n=======\n");
        let r = read_blocking(&d.path());
        assert_eq!(r.status, ProjectConfigStatus::Corrupt);
        assert!(r.file.is_none());
    }

    #[test]
    fn a_file_with_only_one_key_still_loads() {
        let d = Scratch::new("partial");
        write_raw(&d, r#"{"name":"x"}"#);
        let r = read_blocking(&d.path());
        assert_eq!(r.status, ProjectConfigStatus::Ok);
        let f = r.file.unwrap();
        assert_eq!(f.name, "x");
        assert!(f.pull_excludes.is_empty());
    }

    #[test]
    fn write_then_read_round_trips() {
        let d = Scratch::new("roundtrip");
        let file = ProjectConfigFile {
            name: "demo".into(),
            production_url: "https://demo.example.com".into(),
            pull_excludes: vec![".git/".into()],
            push_excludes: vec![".git/".into()],
            commands: ProjectConfigCommands { dev: "npm run dev".into(), build: "npm run build".into(), ..Default::default() },
            ..Default::default()
        };
        write_blocking(&d.path(), file.clone()).unwrap();
        let r = read_blocking(&d.path());
        assert_eq!(r.status, ProjectConfigStatus::Ok);
        assert_eq!(r.file.unwrap(), file);
    }

    #[test]
    fn require_ok_excludes_refuses_a_missing_project_json() {
        let d = Scratch::new("require-excludes-missing");
        let err = require_ok_excludes(&d.path());
        assert!(err.is_err());
    }

    #[test]
    fn require_ok_excludes_refuses_a_corrupt_project_json() {
        let d = Scratch::new("require-excludes-corrupt");
        write_raw(&d, "not json at all");
        let err = require_ok_excludes(&d.path());
        assert!(err.is_err());
    }

    #[test]
    fn require_ok_excludes_refuses_an_unavailable_directory() {
        let err = require_ok_excludes("/definitely/not/a/real/mount/point");
        assert!(err.is_err());
    }

    #[test]
    fn require_ok_excludes_uses_the_file_even_when_the_passed_object_would_have_had_none() {
        let d = Scratch::new("require-excludes-ok");
        let file = ProjectConfigFile {
            pull_excludes: vec![".git/".into(), "node_modules/".into()],
            push_excludes: vec!["node_modules/".into()],
            ..Default::default()
        };
        write_blocking(&d.path(), file).unwrap();
        let (pull, push) = require_ok_excludes(&d.path()).unwrap();
        assert_eq!(pull, vec![".git/".to_string(), "node_modules/".to_string()]);
        assert_eq!(push, vec!["node_modules/".to_string()]);
    }

    #[test]
    fn seed_does_not_overwrite_an_existing_file() {
        let d = Scratch::new("no-overwrite");
        write_raw(&d, r#"{"name":"kept"}"#);
        let outcome = seed_one(&d.path(), ProjectConfigFile { name: "new".into(), ..Default::default() });
        assert_eq!(outcome, SeedOutcome::AlreadyOwned);
        let r = read_blocking(&d.path());
        assert_eq!(r.file.unwrap().name, "kept");
    }

    #[test]
    fn seed_writes_when_absent_and_non_empty() {
        let d = Scratch::new("write-missing");
        let outcome = seed_one(&d.path(), ProjectConfigFile { name: "seeded".into(), ..Default::default() });
        assert_eq!(outcome, SeedOutcome::Written);
        let r = read_blocking(&d.path());
        assert_eq!(r.file.unwrap().name, "seeded");
    }

    #[test]
    fn write_refuses_a_directory_that_is_not_there() {
        assert!(write_blocking("/definitely/not/a/real/mount/point", ProjectConfigFile::default()).is_err());
    }

    /// Read-modify-write refuses to overwrite a corrupt file, mirroring project_notes.rs's contract
    /// exactly — the bytes on disk must survive untouched (both sides of a git conflict are still there).
    #[test]
    fn write_refuses_a_corrupt_file_instead_of_overwriting_it() {
        let d = Scratch::new("write-refuse-corrupt");
        let body = "<<<<<<< HEAD\n{\"name\":\"mine\"}\n=======\n";
        write_raw(&d, body);
        assert!(write_blocking(&d.path(), ProjectConfigFile { name: "new".into(), ..Default::default() }).is_err());
        assert_eq!(fs::read_to_string(config_path(&d.path())).unwrap(), body, "a refused write must leave the corrupt bytes untouched");
    }

    /// CLAUDE.md multi-entity guard: writing one project's config must leave every other project's
    /// `project.json` byte-identical — checked across >=2 projects (each project = its own local_path here).
    #[test]
    fn writing_one_project_leaves_another_projects_config_byte_identical() {
        let a = Scratch::new("multi-a");
        let b = Scratch::new("multi-b");
        write_blocking(&a.path(), ProjectConfigFile { name: "a-original".into(), ..Default::default() }).unwrap();
        write_blocking(&b.path(), ProjectConfigFile { name: "b-original".into(), ..Default::default() }).unwrap();
        let b_before = fs::read_to_string(config_path(&b.path())).unwrap();

        write_blocking(&a.path(), ProjectConfigFile { name: "a-changed".into(), ..Default::default() }).unwrap();

        let a_after = read_blocking(&a.path()).file.unwrap();
        let b_after = fs::read_to_string(config_path(&b.path())).unwrap();
        assert_eq!(a_after.name, "a-changed");
        assert_eq!(b_after, b_before, "project B's file must stay byte-identical after project A's write");
    }

    #[test]
    fn traversal_in_local_path_is_refused_on_both_paths() {
        assert_eq!(read_blocking("/tmp/../etc").status, ProjectConfigStatus::Unavailable);
        assert!(write_blocking("/tmp/../etc", ProjectConfigFile::default()).is_err());
    }

    /// True read-modify-write - a key this struct does not name, sitting at the top level of an
    /// existing file (a hand-added key, or a newer build's field), survives a dialog save untouched.
    #[test]
    fn write_preserves_an_unknown_top_level_key() {
        let d = Scratch::new("unknown-top-level-key");
        write_raw(&d, r#"{"name":"kept-name","future_field":"do-not-drop-me"}"#);
        write_blocking(&d.path(), ProjectConfigFile { name: "edited".into(), ..Default::default() }).unwrap();
        let raw = fs::read_to_string(config_path(&d.path())).unwrap();
        assert!(raw.contains("future_field"), "an unknown top-level key must survive a write");
        assert!(raw.contains("do-not-drop-me"));
        assert_eq!(read_blocking(&d.path()).file.unwrap().name, "edited");
    }

    /// The same preservation applies one level down, inside `commands`.
    #[test]
    fn write_preserves_an_unknown_commands_key() {
        let d = Scratch::new("unknown-commands-key");
        // `deploy` is now a named field on `ProjectConfigCommands` (deploy plan) - use a genuinely unknown key here instead, so this test still exercises the `extra` flatten path it was written for.
        write_raw(&d, r#"{"commands":{"dev":"npm run dev","lint":"npm run lint"}}"#);
        write_blocking(
            &d.path(),
            ProjectConfigFile { commands: ProjectConfigCommands { dev: "npm run dev2".into(), ..Default::default() }, ..Default::default() },
        )
        .unwrap();
        let raw = fs::read_to_string(config_path(&d.path())).unwrap();
        assert!(raw.contains("\"lint\""), "an unknown commands.x key must survive a write");
        assert!(raw.contains("npm run lint"));
        let reread = read_blocking(&d.path()).file.unwrap();
        assert_eq!(reread.commands.dev, "npm run dev2", "the dialog's own edit still applies");
    }

    /// deploy plan: `commands.deploy` round-trips through the same read-modify-write path as `dev`/`build`.
    #[test]
    fn write_then_read_round_trips_commands_deploy() {
        let d = Scratch::new("deploy-roundtrip");
        let file = ProjectConfigFile {
            commands: ProjectConfigCommands { deploy: "npm run deploy".into(), ..Default::default() },
            ..Default::default()
        };
        write_blocking(&d.path(), file).unwrap();
        let r = read_blocking(&d.path());
        assert_eq!(r.file.unwrap().commands.deploy, "npm run deploy");
    }

    /// `write_project_configs_if_missing` must never write an all-empty file, and must report
    /// exactly which outcome each project got.
    #[test]
    fn seed_outcomes_cover_written_already_owned_nothing_to_seed_and_unavailable() {
        let has_values = Scratch::new("seed-has-values");
        let already_owned = Scratch::new("seed-already-owned");
        write_raw(&already_owned, r#"{"name":"existing"}"#);
        let empty = Scratch::new("seed-empty");

        assert_eq!(
            seed_one(&has_values.path(), ProjectConfigFile { name: "Legacy".into(), ..Default::default() }),
            SeedOutcome::Written
        );
        assert_eq!(read_blocking(&has_values.path()).file.unwrap().name, "Legacy");

        assert_eq!(
            seed_one(&already_owned.path(), ProjectConfigFile { name: "would-be-ignored".into(), ..Default::default() }),
            SeedOutcome::AlreadyOwned
        );
        assert_eq!(read_blocking(&already_owned.path()).file.unwrap().name, "existing", "an existing file must never be overwritten by a seed");

        assert_eq!(seed_one(&empty.path(), ProjectConfigFile::default()), SeedOutcome::NothingToSeed);
        assert_eq!(read_blocking(&empty.path()).status, ProjectConfigStatus::Missing, "refusing to seed an all-empty file must leave it Missing, not create an empty one");

        assert_eq!(
            seed_one("/definitely/not/a/real/mount/point", ProjectConfigFile { name: "x".into(), ..Default::default() }),
            SeedOutcome::Unavailable
        );
    }
}
