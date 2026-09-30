//! Per-(project, host) sync state: `~/.aki/devsync/state/<project_id>/<host>/{baseline.json,last_sync.json}`.
//!
//! THE DECISION THIS FILE IMPLEMENTS (docs/plan/settings-and-state-layout.md): sync state belongs to a
//! (project, host) PAIR, not to a project alone - a baseline written against one host must never be read
//! back against another (docs/research/akidevsync-project-config-scope-2.md § F1/Evidence). This is the
//! sole owner of that directory: every function here is scoped to exactly one (id, host), per the
//! CLAUDE.md multi-entity guard - writing one pair's state must leave every other pair's files untouched.
//!
//! `baseline.json` also carries the `remote_path` it was recorded against (not just the file map): a
//! target whose `remote_path` changed since the baseline was written is treated as having NO baseline by
//! the caller (sync.rs), since the ancestor is someone else's tree.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// One (project, host) pair's last common state.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Baseline {
    #[serde(default)]
    pub remote_path: String,
    #[serde(default)]
    pub files: HashMap<String, u64>,
}

/// One (project, host) pair's most recent sync outcome - the state moved out of `SyncProject.last_sync_*`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LastSync {
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub time: u64,
    #[serde(default)]
    pub status: String,
}

/// Boot-hydrate shape: which host this was, alongside the outcome, so JS can render the newest one per project.
#[derive(Serialize, Clone, Debug)]
pub struct LastSyncWithHost {
    pub host: String,
    pub action: String,
    pub time: u64,
    pub status: String,
}

#[cfg(test)]
static TEST_STATE_ROOT: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

/// Fails loudly (coding.C1) instead of silently degrading to `temp_dir()`: a fallback there would write
/// sync state to a directory that is cleared on reboot, with no error surfaced anywhere (N2).
fn state_root() -> Result<PathBuf, String> {
    #[cfg(test)]
    {
        if let Some(dir) = TEST_STATE_ROOT.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            return Ok(dir);
        }
    }
    Ok(crate::app_paths::app_data_dir()?.join("state"))
}

/// The ONE place `state/<id>/<host>/` is spelled. `host` is validated by callers before it ever reaches
/// here (`validate_remote_host`), same guard as every other host-as-path-component use. `project_id` is
/// validated here (N1) since every read/write path funnels through this one function.
fn pair_dir(project_id: &str, host: &str) -> Result<PathBuf, String> {
    crate::projects::validate_path_segment("project_id", project_id)?;
    Ok(state_root()?.join(project_id).join(host))
}

fn baseline_path(project_id: &str, host: &str) -> Result<PathBuf, String> {
    Ok(pair_dir(project_id, host)?.join("baseline.json"))
}

fn last_sync_path(project_id: &str, host: &str) -> Result<PathBuf, String> {
    Ok(pair_dir(project_id, host)?.join("last_sync.json"))
}

pub fn read_baseline(project_id: &str, host: &str) -> Option<Baseline> {
    let content = std::fs::read_to_string(baseline_path(project_id, host).ok()?).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn write_baseline(project_id: &str, host: &str, baseline: &Baseline) -> Result<(), String> {
    let path = baseline_path(project_id, host)?;
    let json =
        serde_json::to_string(baseline).map_err(|e| format!("baseline serialize: {}", e))?;
    crate::system::write_atomic(&path, &json)
}

pub fn read_last_sync(project_id: &str, host: &str) -> Option<LastSync> {
    let content = std::fs::read_to_string(last_sync_path(project_id, host).ok()?).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn write_last_sync_blocking(
    project_id: &str,
    host: &str,
    entry: &LastSync,
) -> Result<(), String> {
    let path = last_sync_path(project_id, host)?;
    let json = serde_json::to_string(entry).map_err(|e| format!("last_sync serialize: {}", e))?;
    crate::system::write_atomic(&path, &json)
}

/// Local, fast, app-data-only file I/O (no network mount involved - `local_path` is never touched here),
/// so this stays a plain sync command per stack-tauri A1's "not this bug class" carve-out.
#[tauri::command]
pub fn write_last_sync(
    project_id: String,
    host: String,
    action: String,
    time: u64,
    status: String,
) -> Result<(), String> {
    crate::system::validate_remote_host(&host)?;
    write_last_sync_blocking(&project_id, &host, &LastSync { action, time, status })
}

/// Single (project, host) read - the delete-preview auto-approval path (`useSync.js`) needs the sync that
/// actually ran against the host it is about to sync with NOW, never `read_last_sync_all`'s
/// newest-across-every-host figure (correct for the table's last-action cell, wrong here: a more recent
/// sync to a DIFFERENT host must never suppress a delete confirmation for this one - N-e,
/// docs/plan/settings-and-state-layout.md).
#[tauri::command]
pub async fn read_last_sync_for_host(project_id: String, host: String) -> Result<Option<LastSync>, String> {
    crate::system::validate_remote_host(&host)?;
    tauri::async_runtime::spawn_blocking(move || read_last_sync(&project_id, &host))
        .await
        .map_err(|e| format!("read_last_sync_for_host task join error: {}", e))
}

/// Boot-hydrate: for each id, the newest `last_sync.json` across every host it has state for
/// ("which host did I last sync with" = the newest `state/<id>/*/last_sync.json`, per the layout plan).
#[tauri::command]
pub async fn read_last_sync_all(
    ids: Vec<String>,
) -> Result<HashMap<String, LastSyncWithHost>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut out = HashMap::new();
        for id in ids {
            let dir = match state_root() {
                Ok(root) => root.join(&id),
                Err(_) => continue,
            };
            let entries = match std::fs::read_dir(&dir) {
                Ok(e) => e,
                Err(_) => continue,
            };
            let mut newest: Option<LastSyncWithHost> = None;
            for entry in entries.flatten() {
                let host = entry.file_name().to_string_lossy().to_string();
                if let Some(ls) = read_last_sync(&id, &host) {
                    let better = newest.as_ref().map(|n| ls.time > n.time).unwrap_or(true);
                    if better {
                        newest = Some(LastSyncWithHost {
                            host,
                            action: ls.action,
                            time: ls.time,
                            status: ls.status,
                        });
                    }
                }
            }
            if let Some(n) = newest {
                out.insert(id, n);
            }
        }
        out
    })
    .await
    .map_err(|e| format!("read_last_sync_all task join error: {}", e))
}

/// B5 (docs/plan/settings-and-state-layout.md § B): removes only `state/<id>/` - every host's baseline
/// and last_sync for exactly ONE project. Wired from `removeProject` in remoteActions.js. Scoped to a
/// single project id (1.9.3 multi-entity guard): every OTHER project's `state/<other_id>/` must stay
/// byte-identical, tested below across >=2 projects x >=2 hosts. Local, fast, app-data-only file I/O
/// (stack-tauri A1's "not this bug class" carve-out), so no `spawn_blocking` is required.
#[tauri::command]
pub fn delete_project_state(project_id: String) -> Result<(), String> {
    crate::projects::validate_path_segment("project_id", &project_id)?;
    let dir = state_root()?.join(&project_id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("delete_project_state: {}", e))?;
    }
    Ok(())
}

/// One-shot, idempotent migration (docs/plan/settings-and-state-layout.md § Migration, amended § Amendments
/// for the 1.32.1 remote_path fix):
/// - `remote_host` + `hooks` -> `targets.<remote_host>` (only when that entry is absent).
/// - a stale `targets.<host>.remote_path` left by a pre-1.32.1 run of this same migration is lifted back
///   onto the project's own top-level `remote_path` (only when that field is still empty), then cleared
///   from every target so it can never resurface - `remote_path` is a project fact now, never a per-host
///   one (see `Target`'s doc comment). Prefers the active `remote_host`'s own recorded value, else the
///   first non-empty one found across any target, since only one path per project has ever existed in
///   practice.
/// - the legacy flat `baselines/<id>.json` -> `state/<id>/<host>/baseline.json`, recording the current `remote_path`.
///   An ancestor already present at that per-host location wins over the legacy file (e.g. a sync already
///   ran there this launch) - the legacy file is only ever consulted when no state baseline exists yet.
/// - `last_sync_action/time/status` -> `state/<id>/<host>/last_sync.json`, then cleared on the project
///   (skip_serializing_if keeps them from coming back on the next save, per the 1.13.0 sync_git lesson).
///
/// Host used for the baseline/last_sync moves is `last_sync_host` if present, else `remote_host` - the best
/// guess for which remote the recorded state actually belongs to (this plan is what makes that guess exact
/// going forward). Runs entirely against app-data (never `local_path`), so no network-mount stall risk
/// (stack-tauri A1).
pub fn migrate_settings_and_state(projects: &mut [crate::projects::SyncProject]) -> bool {
    let mut changed = false;
    for p in projects.iter_mut() {
        if crate::system::validate_remote_host(&p.remote_host).is_err() {
            continue;
        }
        if !p.targets.contains_key(&p.remote_host) {
            p.targets.insert(
                p.remote_host.clone(),
                crate::projects::Target {
                    remote_path: String::new(),
                    hooks: Some(p.hooks.clone()),
                },
            );
            changed = true;
        }

        if p.remote_path.trim().is_empty() {
            let lifted = p
                .targets
                .get(&p.remote_host)
                .map(|t| t.remote_path.clone())
                .filter(|v| !v.trim().is_empty())
                .or_else(|| {
                    p.targets
                        .values()
                        .map(|t| t.remote_path.clone())
                        .find(|v| !v.trim().is_empty())
                });
            if let Some(path) = lifted {
                p.remote_path = path;
                changed = true;
            }
        }
        for target in p.targets.values_mut() {
            if !target.remote_path.is_empty() {
                target.remote_path.clear();
                changed = true;
            }
        }

        let host = p.last_sync_host.clone().unwrap_or_else(|| p.remote_host.clone());
        if crate::system::validate_remote_host(&host).is_err() {
            continue;
        }

        if let Some(files) = crate::sync::legacy_flat_baseline(&p.id) {
            let already_had_state_baseline = read_baseline(&p.id, &host).is_some();
            let write_succeeded = !already_had_state_baseline
                && write_baseline(
                    &p.id,
                    &host,
                    &Baseline {
                        remote_path: p.remote_path.clone(),
                        files,
                    },
                )
                .is_ok();
            if write_succeeded {
                changed = true;
            }
            // The legacy file is deleted only once its content is safely preserved elsewhere: either a
            // fresher per-host baseline already existed and wins, or this write just landed it there. A
            // failed write with no existing per-host baseline must leave the legacy file in place - deleting
            // it then would discard the only copy of the baseline with nothing written to replace it.
            if already_had_state_baseline || write_succeeded {
                crate::sync::remove_legacy_flat_baseline(&p.id);
            }
        }

        if p.last_sync_action.is_some() || p.last_sync_time.is_some() || p.last_sync_status.is_some() {
            let entry = LastSync {
                action: p.last_sync_action.clone().unwrap_or_default(),
                time: p.last_sync_time.unwrap_or(0),
                status: p.last_sync_status.clone().unwrap_or_default(),
            };
            if write_last_sync_blocking(&p.id, &host, &entry).is_ok() {
                p.last_sync_action = None;
                p.last_sync_time = None;
                p.last_sync_status = None;
                p.last_sync_host = None;
                changed = true;
            }
        }
    }
    changed
}

/// Boot-time entrypoint (docs/plan/settings-and-state-layout.md § Migration): runs in `setup()` beside
/// `app_paths::migrate_legacy_app_data`, before the logger opens, so every path involved
/// (`app_paths::app_data_dir`, `sync::baseline_dir`'s fallback) is resolved deterministically instead of
/// depending on `sync::APP_DATA_DIR` being primed later by whichever sync command happens to run first.
/// Loads `projects.json` directly rather than taking an already-loaded list over IPC - there is no
/// frontend yet at this point in startup. Returns a one-line summary for the caller to log, same shape as
/// `app_paths::migrate_legacy_app_data`.
pub fn migrate_settings_and_state_on_boot(app: &tauri::AppHandle) -> String {
    // Load_projects_raw only, never load_projects_blocking - the latter stats/reads project icons under each project's local_path, which can stall on an unmounted/slow mount with no window open yet.
    let mut projects = match crate::projects::load_projects_raw(app) {
        Ok(p) => p,
        Err(e) => return format!("settings/state migration skipped: could not load projects.json: {}", e),
    };
    if projects.is_empty() {
        return "settings/state migration: no projects".to_string();
    }
    if !migrate_settings_and_state(&mut projects) {
        return "settings/state migration: nothing to migrate".to_string();
    }
    match crate::projects::save_projects(app.clone(), projects) {
        Ok(()) => {
            "settings/state migration: moved remote_host/hooks into per-host targets, lifted any stale per-host remote_path back to the project level, and moved legacy baselines/last_sync into per-host state".to_string()
        }
        Err(e) => format!("settings/state migration: reshaped in memory but save failed: {}", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::{SyncHooks, SyncProject, Target};
    use std::collections::BTreeMap;
    use std::sync::{Mutex, MutexGuard};

    // Serializes every test that overrides TEST_STATE_ROOT (a process-global), since cargo test runs
    // tests in parallel threads within one process - without this lock two tests would race on the
    // same global and read each other's scratch dir.
    static TEST_SERIAL: Mutex<()> = Mutex::new(());

    /// Self-cleaning scratch app-data dir so tests never touch the real `~/.aki/devsync`.
    struct Scratch {
        dir: PathBuf,
        _guard: MutexGuard<'static, ()>,
    }
    impl Scratch {
        fn new(tag: &str) -> Self {
            let guard = TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            let dir = std::env::temp_dir().join(format!(
                "aki-sync-state-test-{}-{}",
                tag,
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            *TEST_STATE_ROOT.lock().unwrap_or_else(|e| e.into_inner()) = Some(dir.clone());
            // Also redirect sync.rs's pre-1.32.0 baseline lookups (legacy_flat_baseline / its app-data and pre-1.7.1 fallbacks) into this same scratch dir, never the real home.
            crate::sync::set_test_dirs(
                Some(dir.join("app-data")),
                Some(dir.join("legacy-baselines")),
            );
            Scratch { dir, _guard: guard }
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            *TEST_STATE_ROOT.lock().unwrap_or_else(|e| e.into_inner()) = None;
            crate::sync::set_test_dirs(None, None);
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// Seeds a pre-1.32.0 per-project baseline file at the exact path `legacy_flat_baseline` reads,
    /// inside the current `Scratch`'s injected dir (S5) - never the real home.
    fn write_legacy_baseline(project_id: &str, files: &[(&str, u64)]) {
        let path = crate::sync::baseline_path_for_test(project_id);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let map: HashMap<String, u64> =
            files.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        std::fs::write(&path, serde_json::to_string(&map).unwrap()).unwrap();
    }

    fn make_project(id: &str, remote_host: &str, remote_path: &str) -> SyncProject {
        SyncProject {
            id: id.to_string(),
            name: id.to_string(),
            local_path: format!("/local/{}", id),
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
            targets: BTreeMap::new(),
            deploy: None,
            tasks: None,
            notes: None,
        }
    }

    #[test]
    fn write_and_read_baseline_round_trips() {
        let _s = Scratch::new("roundtrip");
        let mut files = HashMap::new();
        files.insert("a.txt".to_string(), 100u64);
        write_baseline("proj-a", "hostA", &Baseline { remote_path: "~/app".into(), files: files.clone() }).unwrap();
        let read = read_baseline("proj-a", "hostA").unwrap();
        assert_eq!(read.remote_path, "~/app");
        assert_eq!(read.files.get("a.txt"), Some(&100u64));
    }

    #[test]
    fn missing_baseline_reads_none() {
        let _s = Scratch::new("missing");
        assert!(read_baseline("no-such-project", "no-such-host").is_none());
    }

    /// `#[serde(default)]` on every `LastSync` field - a `last_sync.json` missing a key (a future
    /// build that dropped one, or a hand-trimmed file) must still read as Some with the default filled in,
    /// not silently collapse to None the way a hard deserialize failure would (`read_last_sync` swallows
    /// any parse error into None, so a missing-field-only file would otherwise be indistinguishable from
    /// no file at all).
    #[test]
    fn last_sync_json_missing_a_key_still_reads() {
        let _s = Scratch::new("last-sync-missing-key");
        let path = last_sync_path("proj-a", "hostA").unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"action":"PUSH","time":123}"#).unwrap();

        let read = read_last_sync("proj-a", "hostA").expect("a file missing only `status` must still parse");
        assert_eq!(read.action, "PUSH");
        assert_eq!(read.time, 123);
        assert_eq!(read.status, "", "the missing key must default rather than fail the whole read");
    }

    /// CLAUDE.md multi-entity guard: writing one (id, host) pair must leave every other pair's files
    /// byte-identical - checked across >=2 projects x >=2 hosts.
    #[test]
    fn writing_one_pair_leaves_every_other_pair_byte_identical() {
        let _s = Scratch::new("isolation");
        let pairs = [
            ("proj-a", "hostA"),
            ("proj-a", "hostB"),
            ("proj-b", "hostA"),
            ("proj-b", "hostB"),
        ];
        for (id, host) in pairs {
            let mut files = HashMap::new();
            files.insert(format!("{}-{}.txt", id, host), 1u64);
            write_baseline(id, host, &Baseline { remote_path: format!("~/{}", host), files }).unwrap();
            write_last_sync_blocking(
                id,
                host,
                &LastSync { action: "PUSH".into(), time: 1, status: "success".into() },
            )
            .unwrap();
        }
        let before: HashMap<_, _> = pairs
            .iter()
            .map(|(id, host)| {
                (
                    (*id, *host),
                    (
                        std::fs::read_to_string(baseline_path(id, host).unwrap()).unwrap(),
                        std::fs::read_to_string(last_sync_path(id, host).unwrap()).unwrap(),
                    ),
                )
            })
            .collect();

        // Mutate exactly one pair.
        let mut files = HashMap::new();
        files.insert("changed.txt".to_string(), 999u64);
        write_baseline("proj-a", "hostA", &Baseline { remote_path: "~/hostA".into(), files }).unwrap();
        write_last_sync_blocking(
            "proj-a",
            "hostA",
            &LastSync { action: "PULL".into(), time: 999, status: "success".into() },
        )
        .unwrap();

        for (id, host) in pairs {
            let (b, l) = (
                std::fs::read_to_string(baseline_path(id, host).unwrap()).unwrap(),
                std::fs::read_to_string(last_sync_path(id, host).unwrap()).unwrap(),
            );
            if (id, host) == ("proj-a", "hostA") {
                assert_ne!(b, before[&(id, host)].0, "the targeted pair should have changed");
            } else {
                assert_eq!(b, before[&(id, host)].0, "other pair's baseline must stay byte-identical");
                assert_eq!(l, before[&(id, host)].1, "other pair's last_sync must stay byte-identical");
            }
        }
    }

    /// Deleting one project's state must remove only `state/<id>/`, leaving every other project's
    /// (and every other host's) files byte-identical - the same >=2 projects x >=2 hosts guard as the
    /// write-isolation test above, applied to deletion.
    #[test]
    fn deleting_one_project_state_leaves_every_other_project_and_host_byte_identical() {
        let _s = Scratch::new("delete-project-state");
        let pairs = [
            ("proj-a", "hostA"),
            ("proj-a", "hostB"),
            ("proj-b", "hostA"),
            ("proj-b", "hostB"),
        ];
        for (id, host) in pairs {
            let mut files = HashMap::new();
            files.insert(format!("{}-{}.txt", id, host), 1u64);
            write_baseline(id, host, &Baseline { remote_path: format!("~/{}", host), files }).unwrap();
            write_last_sync_blocking(
                id,
                host,
                &LastSync { action: "PUSH".into(), time: 1, status: "success".into() },
            )
            .unwrap();
        }
        let untouched: HashMap<_, _> = pairs
            .iter()
            .filter(|(id, _)| *id == "proj-b")
            .map(|(id, host)| {
                (
                    (*id, *host),
                    (
                        std::fs::read_to_string(baseline_path(id, host).unwrap()).unwrap(),
                        std::fs::read_to_string(last_sync_path(id, host).unwrap()).unwrap(),
                    ),
                )
            })
            .collect();

        delete_project_state("proj-a".to_string()).unwrap();

        assert!(!pair_dir("proj-a", "hostA").unwrap().exists());
        assert!(!pair_dir("proj-a", "hostB").unwrap().exists());
        for (id, host) in [("proj-b", "hostA"), ("proj-b", "hostB")] {
            let (b, l) = (
                std::fs::read_to_string(baseline_path(id, host).unwrap()).unwrap(),
                std::fs::read_to_string(last_sync_path(id, host).unwrap()).unwrap(),
            );
            assert_eq!(b, untouched[&(id, host)].0, "other project's baseline must stay byte-identical");
            assert_eq!(l, untouched[&(id, host)].1, "other project's last_sync must stay byte-identical");
        }
    }

    /// B5/N1: a project id with directory traversal must never escape `state/` - deletion is refused, not
    /// silently resolved against some other path.
    #[test]
    fn delete_project_state_rejects_traversal_in_project_id() {
        let _s = Scratch::new("delete-project-state-traversal");
        assert!(delete_project_state("../escaped".to_string()).is_err());
    }

    /// The same traversal guard applies to every read/write path, not only deletion - `pair_dir` is
    /// the one funnel every one of them shares.
    #[test]
    fn write_last_sync_rejects_traversal_in_project_id() {
        let _s = Scratch::new("write-last-sync-traversal");
        let entry = LastSync { action: "PUSH".into(), time: 1, status: "success".into() };
        assert!(write_last_sync_blocking("../escaped", "hostA", &entry).is_err());
    }

    /// 1.32.1 fix: remote_path is a project fact, not per-host - migration creates the target entry (for
    /// hooks) but never copies remote_path into it, and never touches the project's own top-level value.
    #[test]
    fn migration_creates_a_target_entry_without_copying_remote_path_into_it() {
        let _s = Scratch::new("migrate-targets");
        let mut projects = vec![make_project("proj-a", "hostA", "~/app")];
        let changed = migrate_settings_and_state(&mut projects);
        assert!(changed);
        let target = projects[0].targets.get("hostA").unwrap();
        assert_eq!(target.remote_path, "", "remote_path is never copied into a target");
        assert_eq!(projects[0].remote_path, "~/app", "the project's own top-level path is untouched");
    }

    /// The core 1.32.1 fix: a `targets.<host>.remote_path` left by a pre-1.32.1 run of this same migration
    /// is lifted back onto the project's own top-level field (the only place it belongs now), then cleared
    /// from the target so a later host switch can never resurrect it.
    #[test]
    fn migration_lifts_a_stale_per_host_remote_path_back_onto_the_project() {
        let _s = Scratch::new("migrate-lift-remote-path");
        let mut p = make_project("proj-a", "hostA", "");
        p.targets.insert(
            "hostA".to_string(),
            Target { remote_path: "~/from-1.32.0".to_string(), hooks: None },
        );
        let mut projects = vec![p];
        let changed = migrate_settings_and_state(&mut projects);
        assert!(changed);
        assert_eq!(projects[0].remote_path, "~/from-1.32.0");
        assert_eq!(
            projects[0].targets.get("hostA").unwrap().remote_path,
            "",
            "cleared so it can never resurface on a later host switch"
        );
    }

    #[test]
    fn migration_keeps_an_existing_target_entrys_hooks_untouched() {
        let _s = Scratch::new("migrate-no-overwrite");
        let mut projects = vec![make_project("proj-a", "hostA", "~/app")];
        projects[0].targets.insert(
            "hostA".to_string(),
            Target {
                remote_path: "~/stale-kept-path".to_string(),
                hooks: Some(SyncHooks { pre_pull_cmd: Some("echo kept".to_string()), ..Default::default() }),
            },
        );
        migrate_settings_and_state(&mut projects);
        let target = projects[0].targets.get("hostA").unwrap();
        assert_eq!(
            target.hooks.as_ref().unwrap().pre_pull_cmd.as_deref(),
            Some("echo kept"),
            "an existing target's hooks are never overwritten"
        );
        assert_eq!(target.remote_path, "", "stale remote_path is cleared even on an existing target");
    }

    #[test]
    fn migration_moves_last_sync_fields_into_state_and_clears_them() {
        let _s = Scratch::new("migrate-last-sync");
        let mut p = make_project("proj-a", "hostA", "~/app");
        p.last_sync_action = Some("PUSH".to_string());
        p.last_sync_time = Some(12345);
        p.last_sync_status = Some("success".to_string());
        p.last_sync_host = Some("hostA".to_string());
        let mut projects = vec![p];

        migrate_settings_and_state(&mut projects);

        assert!(projects[0].last_sync_action.is_none());
        assert!(projects[0].last_sync_time.is_none());
        assert!(projects[0].last_sync_status.is_none());
        assert!(projects[0].last_sync_host.is_none());
        let ls = read_last_sync("proj-a", "hostA").unwrap();
        assert_eq!(ls.action, "PUSH");
        assert_eq!(ls.time, 12345);
        assert_eq!(ls.status, "success");
    }

    /// Idempotency proven by bytes, not just the returned bool - a second run must leave every
    /// written file byte-identical to the first run's output, across >=2 projects x >=2 hosts.
    #[test]
    fn migration_is_idempotent_and_second_run_leaves_files_byte_identical() {
        let _s = Scratch::new("migrate-idempotent-bytes");
        write_legacy_baseline("proj-a", &[("a.txt", 1)]);
        write_legacy_baseline("proj-b", &[("b.txt", 2)]);

        let mut pa = make_project("proj-a", "hostA", "~/app-a");
        pa.last_sync_action = Some("PUSH".to_string());
        pa.last_sync_time = Some(1);
        pa.last_sync_status = Some("success".to_string());
        let pb = make_project("proj-b", "hostB", "~/app-b");
        let mut projects = vec![pa, pb];

        let first = migrate_settings_and_state(&mut projects);
        assert!(first);

        let snapshot = |projects: &[SyncProject]| {
            (
                serde_json::to_string(projects).unwrap(),
                std::fs::read_to_string(baseline_path("proj-a", "hostA").unwrap()).unwrap(),
                std::fs::read_to_string(baseline_path("proj-b", "hostB").unwrap()).unwrap(),
                std::fs::read_to_string(last_sync_path("proj-a", "hostA").unwrap()).unwrap(),
            )
        };
        let before = snapshot(&projects);

        let second = migrate_settings_and_state(&mut projects);
        assert!(!second, "a second run must be a no-op once targets and state already exist");
        assert_eq!(snapshot(&projects), before, "a no-op second run must leave every file byte-identical");
    }

    /// docs/plan/settings-and-state-layout.md § Migration: "an existing state baseline is NEVER
    /// overwritten" - a fresher per-host baseline (e.g. written by a sync that already ran this launch)
    /// must survive a legacy flat baseline still sitting on disk.
    #[test]
    fn migration_never_overwrites_an_existing_state_baseline() {
        let _s = Scratch::new("migrate-no-overwrite-state-baseline");
        let fresh = Baseline {
            remote_path: "~/app".to_string(),
            files: HashMap::from([("fresh.txt".to_string(), 999u64)]),
        };
        write_baseline("proj-a", "hostA", &fresh).unwrap();
        write_legacy_baseline("proj-a", &[("stale.txt", 1)]);

        let mut projects = vec![make_project("proj-a", "hostA", "~/app")];
        migrate_settings_and_state(&mut projects);

        let after = read_baseline("proj-a", "hostA").unwrap();
        assert_eq!(after.files.get("fresh.txt"), Some(&999u64));
        assert!(
            !after.files.contains_key("stale.txt"),
            "the legacy baseline must not have overwritten the existing state baseline"
        );
    }

    /// R1 sibling: a failed `write_baseline` must never let the legacy flat baseline
    /// be deleted when no per-host baseline already exists - deleting it then would discard the only copy
    /// with nothing written in its place. Forces the write to fail by pre-occupying the exact
    /// `baseline.json` path with a directory (same technique as `system.rs`'s
    /// `write_atomic_leaves_the_old_file_intact_when_the_write_cannot_land`), so `write_atomic`'s final
    /// rename fails.
    #[test]
    fn migration_keeps_the_legacy_baseline_when_the_state_write_fails() {
        let _s = Scratch::new("migrate-keep-legacy-on-failed-write");
        write_legacy_baseline("proj-a", &[("a.txt", 1)]);

        let blocked_path = baseline_path("proj-a", "hostA").unwrap();
        std::fs::create_dir_all(&blocked_path).unwrap();

        // Pre-populate the target (with remote_path already cleared, the steady state going forward) so
        // neither the `targets` creation branch nor the stale-remote_path clear-up branch flips `changed` -
        // isolates this assertion to the baseline-write outcome only.
        let mut p = make_project("proj-a", "hostA", "~/app");
        p.targets.insert("hostA".to_string(), Target { remote_path: String::new(), hooks: None });
        let mut projects = vec![p];
        let changed = migrate_settings_and_state(&mut projects);

        assert!(!changed, "a failed state write must not report a change");
        assert!(read_baseline("proj-a", "hostA").is_none(), "the blocked path holds no readable baseline");
        assert!(
            crate::sync::legacy_flat_baseline("proj-a").is_some(),
            "the legacy file must survive a failed write instead of being deleted with its content nowhere else"
        );
    }

    /// The migration test suite's multi-entity coverage, exercised on the one function that rewrites
    /// state for every project at once (not just the pure per-pair writers already covered above).
    #[test]
    fn migration_moves_legacy_flat_baselines_across_two_projects_and_two_hosts() {
        let _s = Scratch::new("migrate-legacy-baseline-2x2");
        write_legacy_baseline("proj-a", &[("a.txt", 1)]);
        write_legacy_baseline("proj-b", &[("b.txt", 2)]);

        let mut projects = vec![
            make_project("proj-a", "hostA", "~/app-a"),
            make_project("proj-b", "hostB", "~/app-b"),
        ];
        let changed = migrate_settings_and_state(&mut projects);
        assert!(changed);

        let ba = read_baseline("proj-a", "hostA").unwrap();
        assert_eq!(ba.remote_path, "~/app-a");
        assert_eq!(ba.files.get("a.txt"), Some(&1u64));

        let bb = read_baseline("proj-b", "hostB").unwrap();
        assert_eq!(bb.remote_path, "~/app-b");
        assert_eq!(bb.files.get("b.txt"), Some(&2u64));

        assert!(crate::sync::legacy_flat_baseline("proj-a").is_none(), "legacy file must be removed after moving");
        assert!(crate::sync::legacy_flat_baseline("proj-b").is_none(), "legacy file must be removed after moving");
    }

    /// Each project already has real sync state on a SECOND host (e.g. it synced there before the
    /// active host was switched) - migration only ever touches the ACTIVE host's pair, so that second-host
    /// state must stay completely untouched, both on the first migration run and on a second, idempotent one.
    #[test]
    fn migration_leaves_pre_existing_second_host_state_byte_identical_across_two_runs() {
        let _s = Scratch::new("migrate-2x2-second-host-untouched");
        write_legacy_baseline("proj-a", &[("a.txt", 1)]);
        write_legacy_baseline("proj-b", &[("b.txt", 2)]);

        // Pre-existing state on a host other than each project's active remote_host.
        write_baseline(
            "proj-a",
            "hostB",
            &Baseline { remote_path: "~/old-a".into(), files: HashMap::from([("old-a.txt".to_string(), 5u64)]) },
        )
        .unwrap();
        write_last_sync_blocking("proj-a", "hostB", &LastSync { action: "PULL".into(), time: 5, status: "success".into() }).unwrap();
        write_baseline(
            "proj-b",
            "hostA",
            &Baseline { remote_path: "~/old-b".into(), files: HashMap::from([("old-b.txt".to_string(), 6u64)]) },
        )
        .unwrap();
        write_last_sync_blocking("proj-b", "hostA", &LastSync { action: "PUSH".into(), time: 6, status: "success".into() }).unwrap();

        let untouched_snapshot = || {
            (
                std::fs::read_to_string(baseline_path("proj-a", "hostB").unwrap()).unwrap(),
                std::fs::read_to_string(last_sync_path("proj-a", "hostB").unwrap()).unwrap(),
                std::fs::read_to_string(baseline_path("proj-b", "hostA").unwrap()).unwrap(),
                std::fs::read_to_string(last_sync_path("proj-b", "hostA").unwrap()).unwrap(),
            )
        };
        let before = untouched_snapshot();

        let mut projects = vec![
            make_project("proj-a", "hostA", "~/app-a"),
            make_project("proj-b", "hostB", "~/app-b"),
        ];
        assert!(migrate_settings_and_state(&mut projects), "first run must move the active-host legacy baselines");
        assert_eq!(untouched_snapshot(), before, "pre-existing second-host state must survive the first run byte-identical");

        assert!(!migrate_settings_and_state(&mut projects), "a second run must be a no-op");
        assert_eq!(untouched_snapshot(), before, "pre-existing second-host state must survive the second run byte-identical too");

        assert_eq!(read_baseline("proj-a", "hostA").unwrap().files.get("a.txt"), Some(&1u64));
        assert_eq!(read_baseline("proj-b", "hostB").unwrap().files.get("b.txt"), Some(&2u64));
    }

    /// Load -> save must never bring `last_sync_*` back once migrated (1.13.0 sync_git lesson).
    #[test]
    fn migrated_project_round_trips_through_json_without_last_sync_fields() {
        let _s = Scratch::new("load-save");
        let mut p = make_project("proj-a", "hostA", "~/app");
        p.last_sync_action = Some("PUSH".to_string());
        p.last_sync_time = Some(1);
        p.last_sync_status = Some("success".to_string());
        let mut projects = vec![p];
        migrate_settings_and_state(&mut projects);

        let json = serde_json::to_string(&projects[0]).unwrap();
        assert!(!json.contains("last_sync_action"));
        assert!(!json.contains("last_sync_time"));
        assert!(!json.contains("last_sync_status"));
        assert!(!json.contains("last_sync_host"));

        let reloaded: SyncProject = serde_json::from_str(&json).unwrap();
        assert!(reloaded.last_sync_action.is_none());
    }

    /// Migration rehearsal seam (docs/plan/1.32.0-mac-handoff.md § 2, release.B5's "rehearse from the
    /// PREVIOUS state, never from empty"): runs the real `migrate_settings_and_state` against a COPY of a
    /// real `~/.aki/devsync` tree, never the original. `#[ignore]` so an ordinary `cargo test` never
    /// touches it; a missing/empty `AKI_REHEARSAL_DEVSYNC_COPY` PANICS rather than passing vacuously
    /// - a bare `cargo test -- --ignored` sweep that forgets the env var must
    /// fail loudly, not report a false "1 passed". Redirects every path this migration reads/writes via
    /// the same test-only statics the unit tests above use (`TEST_STATE_ROOT`, `sync::set_test_dirs`), so
    /// nothing here can resolve against the real home directory.
    ///
    /// `AKI_REHEARSAL_LEGACY_BASELINES_COPY` is optional: `sync::legacy_flat_baseline` falls back to the
    /// pre-1.7.1 `~/.aki/devsync-baselines` directory when a project's new-location baseline is absent
    /// (`sync.rs::read_baseline`'s `.or_else`), so a full rehearsal needs that directory copied too when it
    /// exists on the real machine - `scripts/rehearse-settings-migration.sh` sets this whenever it finds one.
    ///
    /// Invoked by `scripts/rehearse-settings-migration.sh`, which owns the copy step; this test owns the
    /// migration + postcondition assertions and prints its own counts to stdout (`--nocapture`).
    #[test]
    #[ignore]
    fn rehearse_migration_against_a_real_devsync_copy() {
        let root = match std::env::var("AKI_REHEARSAL_DEVSYNC_COPY") {
            Ok(v) if !v.trim().is_empty() => PathBuf::from(v),
            _ => panic!(
                "AKI_REHEARSAL_DEVSYNC_COPY must be set to a devsync COPY directory - never run this test bare; use scripts/rehearse-settings-migration.sh"
            ),
        };
        assert!(root.is_dir(), "AKI_REHEARSAL_DEVSYNC_COPY does not exist or is not a directory: {}", root.display());
        let legacy_baselines_dir = match std::env::var("AKI_REHEARSAL_LEGACY_BASELINES_COPY") {
            Ok(v) if !v.trim().is_empty() => PathBuf::from(v),
            _ => root.join("no-such-legacy-baselines-dir"),
        };

        *TEST_STATE_ROOT.lock().unwrap_or_else(|e| e.into_inner()) = Some(root.join("state"));
        crate::sync::set_test_dirs(Some(root.clone()), Some(legacy_baselines_dir));

        let projects_json_path = root.join("projects.json");
        let backup_raw = std::fs::read_to_string(&projects_json_path)
            .unwrap_or_else(|e| panic!("cannot read {}: {}", projects_json_path.display(), e));
        let mut projects: Vec<SyncProject> = serde_json::from_str(&backup_raw)
            .unwrap_or_else(|e| panic!("{} did not parse as Vec<SyncProject>: {}", projects_json_path.display(), e));

        let backup_total = projects.len();
        let mut per_host: HashMap<String, u32> = HashMap::new();
        let mut empty_host = 0u32;
        let mut had_legacy_baseline: Vec<(String, String)> = Vec::new(); // (id, host used for the move)
        let mut predicted_seed = 0u32;
        for p in &projects {
            if p.remote_host.trim().is_empty() {
                empty_host += 1;
            } else {
                *per_host.entry(p.remote_host.clone()).or_insert(0) += 1;
            }
            if crate::sync::legacy_flat_baseline(&p.id).is_some() {
                let host = p.last_sync_host.clone().unwrap_or_else(|| p.remote_host.clone());
                had_legacy_baseline.push((p.id.clone(), host));
            }
            // Mirrors project_config.rs::is_all_empty against the exact six legacy fields the JS seeding
            // step reads (`write_project_configs_if_missing`) - a *prediction*, since seeding itself only
            // runs from JS and this Rust rehearsal cannot exercise it.
            let local_dir = PathBuf::from(&p.local_path);
            let already_owned = local_dir.join(".akidevsync").join("project.json").exists();
            if local_dir.is_dir() && !already_owned {
                let seed_file = crate::project_config::ProjectConfigFile {
                    name: p.name.clone(),
                    production_url: p.production_url.clone().unwrap_or_default(),
                    pull_excludes: p.pull_excludes.clone(),
                    push_excludes: p.push_excludes.clone(),
                    commands: crate::project_config::ProjectConfigCommands {
                        dev: p.dev_cmd_override.clone().unwrap_or_default(),
                        build: p.build_cmd_override.clone().unwrap_or_default(),
                        deploy: String::new(),
                        extra: Default::default(),
                    },
                    extra: Default::default(),
                };
                if !crate::project_config::is_all_empty(&seed_file) {
                    predicted_seed += 1;
                }
            }
        }
        let legacy_baseline_files = had_legacy_baseline.len();

        println!("--- rehearsal: pre-migration counts (source of truth: {}) ---", projects_json_path.display());
        println!("projects total: {}", backup_total);
        for (host, n) in &per_host {
            println!("  remote_host={}: {}", host, n);
        }
        println!("  remote_host=<empty>: {}", empty_host);
        println!("legacy flat baseline files: {}", legacy_baseline_files);
        println!(
            "predicted project.json seed count (reachable folder, no existing project.json, ≥1 non-empty legacy field): {}",
            predicted_seed
        );

        let changed_first = migrate_settings_and_state(&mut projects);
        println!("first run changed: {}", changed_first);

        // Persist and RE-READ from disk, same as the real boot path's load -> migrate -> save -> (next
        // launch) load again - checking `projects.len()` against itself in memory proves nothing about
        // what actually landed on disk.
        let projects_json_first =
            serde_json::to_string_pretty(&projects).expect("serialize migrated projects");
        std::fs::write(&projects_json_path, &projects_json_first).expect("write migrated projects.json");
        let reread_raw = std::fs::read_to_string(&projects_json_path).expect("re-read projects.json");
        let reread: Vec<SyncProject> =
            serde_json::from_str(&reread_raw).expect("re-read projects.json must still parse");
        assert_eq!(
            reread.len(),
            backup_total,
            "projects.json re-read from disk after migration must still list every project the backup had ({})",
            backup_total
        );

        let mut with_state_dir = 0u32;
        for (id, host) in &had_legacy_baseline {
            assert!(
                !host.trim().is_empty(),
                "project {} had a legacy baseline but resolved to an empty host - migration cannot place it",
                id
            );
            let b = read_baseline(id, host);
            assert!(b.is_some(), "project {} host {} should have a per-host baseline.json after migration", id, host);
            assert!(pair_dir(id, host).unwrap().is_dir());
            with_state_dir += 1;
        }
        println!("post-migration: {} state/<id>/<host>/ dirs created from a legacy baseline (matches count above)", with_state_dir);
        assert_eq!(with_state_dir as usize, legacy_baseline_files);

        let snapshot_state_files = |root: &PathBuf| -> Vec<(PathBuf, String)> {
            let state_dir = root.join("state");
            let mut out = Vec::new();
            if let Ok(walker) = std::fs::read_dir(&state_dir) {
                for id_entry in walker.flatten() {
                    if let Ok(host_walker) = std::fs::read_dir(id_entry.path()) {
                        for host_entry in host_walker.flatten() {
                            if let Ok(file_walker) = std::fs::read_dir(host_entry.path()) {
                                for f in file_walker.flatten() {
                                    let content = std::fs::read_to_string(f.path()).unwrap_or_default();
                                    out.push((f.path(), content));
                                }
                            }
                        }
                    }
                }
            }
            out
        };
        let after_first = snapshot_state_files(&root);

        // Second run reloads from disk too (the same file § "Persist and RE-READ" above just wrote), not
        // the in-memory Vec left over from the first run - proves the round trip through disk is
        // idempotent, not just the in-memory struct.
        let mut projects_second: Vec<SyncProject> =
            serde_json::from_str(&reread_raw).expect("second-run parse of the re-read projects.json");
        let changed_second = migrate_settings_and_state(&mut projects_second);
        assert!(!changed_second, "a second run against already-migrated data must be a no-op (release.B5 rehearsal must prove idempotency, not just assume it)");
        let projects_json_second = serde_json::to_string_pretty(&projects_second).expect("serialize second-run projects");
        std::fs::write(&projects_json_path, &projects_json_second).expect("write second-run projects.json");
        let after_second = snapshot_state_files(&root);
        assert_eq!(after_first, after_second, "second run must leave every state file byte-identical to the first run's output");
        assert_eq!(
            projects_json_second, projects_json_first,
            "second run must leave projects.json byte-identical to the first run's output"
        );
        println!("second run changed: {} (expected false) - {} state files byte-compared identical", changed_second, after_second.len());

        println!("--- rehearsal PASSED: {} projects (re-read from disk matches backup), {} per-host counts, {} legacy baselines migrated into {} state dirs, predicted {} project.json seeds, idempotent on re-run ---", backup_total, per_host.len(), legacy_baseline_files, with_state_dir, predicted_seed);
    }
}
