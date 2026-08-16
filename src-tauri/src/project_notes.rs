//! Per-project tasks & notes, stored **inside the project's own working directory**.
//!
//! THE DECISION THIS FILE IMPLEMENTS (docs/plan/done/1.22.0-notes-json-ssot.md §1): the local repo is the single source of truth for a project. Tasks/notes used to live in central projects.json (local to one Mac, unversioned with repo); they now live at <local_path>/.akidevsync/notes.json meant to be committed.
//!
//! Everything about that file — path, shape, atomic write, lock — is owned here. sync.rs spells .akidevsync/ once more in the rsync protect filter to prevent mirroring deletion.
//!
//! WHY READ RETURNS TAGGED STATUS (NOT DEFAULTED STRUCT): unlike app-data files where unwrap_or_default() is safe, this file lives in git repos on potentially unmounted volumes. Collapsing unreadable files into empty causes UI to overwrite real user notes. Missing vs unreadable are distinct states, and only missing is writable.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Written into every file for provenance: bare URL identifying aki-dev-sync without prose.
const ABOUT_URL: &str = "https://github.com/lacvietanh/aki-dev-sync";

/// Reserved for future schema changes (read/written, no branching yet: files with higher schema still load normally).
const SCHEMA_VERSION: u32 = 1;

/// On-disk shape: serde(default) on every field lets forward-compatible or hand-trimmed files load without migration.
/// tasks is opaque JSON Value (schema/migrations owned by frontend src/utils/tasks.js, Rust never inspects elements).
#[derive(Serialize, Deserialize, Clone, Default, Debug)]
pub struct ProjectNotesFile {
    #[serde(default)]
    pub about: String,
    #[serde(default)]
    pub schema: u32,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub tasks: Vec<serde_json::Value>,
    /// Timestamp (epoch ms) stamped by Rust at write time: staleness fence to detect clobbered edits.
    #[serde(default)]
    pub updated_at: u64,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectNotesStatus {
    /// File read and parsed. Writable.
    Ok,
    /// local_path is readable directory but notes file is missing (fresh clone/new project). Writable (creates dir + file).
    Missing,
    /// local_path is not a directory (unmounted volume/perms) or unreadable file. Not writable.
    Unavailable,
    /// File exists but contains invalid JSON (e.g. git merge conflict markers). Not writable and never defaulted to empty.
    Corrupt,
}

#[derive(Serialize, Clone, Debug)]
pub struct ProjectNotesRead {
    pub status: ProjectNotesStatus,
    /// Some only when status == Ok: UI cannot read content from unwritable states.
    pub file: Option<ProjectNotesFile>,
    /// Error message for Unavailable/Corrupt surfaced in UI tooltip (e.g. line numbers of conflict markers).
    pub error: Option<String>,
}

impl ProjectNotesRead {
    fn ok(file: ProjectNotesFile) -> Self {
        Self { status: ProjectNotesStatus::Ok, file: Some(file), error: None }
    }
    fn missing() -> Self {
        Self { status: ProjectNotesStatus::Missing, file: None, error: None }
    }
    fn unavailable(e: impl std::fmt::Display) -> Self {
        Self { status: ProjectNotesStatus::Unavailable, file: None, error: Some(e.to_string()) }
    }
    fn corrupt(e: impl std::fmt::Display) -> Self {
        Self { status: ProjectNotesStatus::Corrupt, file: None, error: Some(e.to_string()) }
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct ProjectNotesWrite {
    /// Final on-disk file content used by JS to re-seed its store.
    pub file: ProjectNotesFile,
    /// True if write replaced newer content (last-write-wins with explicit user notification).
    pub clobbered: bool,
}

/// One entry of the boot-time batch read.
#[derive(Deserialize, Clone, Debug)]
pub struct ProjectNotesTarget {
    pub id: String,
    pub local_path: String,
}

/// The ONE place `.akidevsync/notes.json` is spelled in the Rust tree.
fn notes_path(local_path: &str) -> PathBuf {
    Path::new(local_path).join(".akidevsync").join("notes.json")
}

/// Validates local_path segment against traversal/control chars (untrusted input from projects.json / paired companion).
fn check_path(local_path: &str) -> Result<(), String> {
    crate::projects::validate_path_segment("local_path", local_path)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Synchronous read: returns structured statuses rather than Err for missing/corrupt/unreachable states.
fn read_blocking(local_path: &str) -> ProjectNotesRead {
    if let Err(e) = check_path(local_path) {
        return ProjectNotesRead::unavailable(e);
    }
    let dir = Path::new(local_path);
    // Checked BEFORE the file: "the volume is not mounted" and "this project has no notes yet" look identical at the file level (both are a NotFound), and only the second one may be written to.
    if !dir.is_dir() {
        return ProjectNotesRead::unavailable(format!(
            "'{}' is not a readable directory right now",
            local_path
        ));
    }
    let path = notes_path(local_path);
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return ProjectNotesRead::missing(),
        Err(e) => return ProjectNotesRead::unavailable(e),
    };
    match serde_json::from_str::<ProjectNotesFile>(&raw) {
        Ok(file) => ProjectNotesRead::ok(file),
        // NOT `unwrap_or_default()`. See the module doc comment: defaulting here means showing an empty note over a git-conflicted file and then saving that emptiness on top of it.
        Err(e) => ProjectNotesRead::corrupt(e),
    }
}

/// Reads one project's notes file.
/// Uses spawn_blocking because local_path can live on network/external mounts that stall in kernel metadata/read calls.
#[tauri::command]
pub async fn read_project_notes(local_path: String) -> Result<ProjectNotesRead, String> {
    tauri::async_runtime::spawn_blocking(move || read_blocking(&local_path))
        .await
        .map_err(|e| format!("read_project_notes task join error: {}", e))
}

/// Batch read for app boot: one IPC round-trip for all projects; failures on unmounted volumes are isolated per target.
#[tauri::command]
pub async fn read_project_notes_map(
    targets: Vec<ProjectNotesTarget>,
) -> Result<HashMap<String, ProjectNotesRead>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        targets.into_iter().map(|t| (t.id, read_blocking(&t.local_path))).collect()
    })
    .await
    .map_err(|e| format!("read_project_notes_map task join error: {}", e))
}

/// Read-modify-write of notes file: None preserves disk fields (CLAUDE.md multi-entity guard); serialized via WRITE_LOCK.
/// base_updated_at detects concurrent edits (last-write-wins with clobbered: true Toast alert).
#[tauri::command]
pub async fn write_project_notes(
    local_path: String,
    notes: Option<String>,
    tasks: Option<Vec<serde_json::Value>>,
    base_updated_at: Option<u64>,
) -> Result<ProjectNotesWrite, String> {
    static WRITE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = WRITE_LOCK.lock().await;

    tauri::async_runtime::spawn_blocking(move || {
        write_blocking(&local_path, notes, tasks, base_updated_at)
    })
    .await
    .map_err(|e| format!("write_project_notes task join error: {}", e))?
}

/// Synchronous core of write: allows unit tests to execute shipped logic directly without tokio runtime dependency.
fn write_blocking(
    local_path: &str,
    notes: Option<String>,
    tasks: Option<Vec<serde_json::Value>>,
    base_updated_at: Option<u64>,
) -> Result<ProjectNotesWrite, String> {
    check_path(local_path)?;
    // Refuse if not dir: prevents creating folders in empty mount stubs where notes would vanish on volume remount.
    if !Path::new(local_path).is_dir() {
        return Err(format!("'{}' is not a readable directory right now", local_path));
    }

    let path = notes_path(local_path);
    let current = read_blocking(local_path);
    let mut file = match current.status {
        ProjectNotesStatus::Ok => current.file.unwrap_or_default(),
        ProjectNotesStatus::Missing => ProjectNotesFile::default(),
        // Writing over a file we could not parse would destroy whatever is in it — which, for the realistic cause (conflict markers), is BOTH sides of the user's edit.
        ProjectNotesStatus::Corrupt => {
            return Err(format!(
                "'{}' is not valid JSON — resolve it before saving ({})",
                path.display(),
                current.error.unwrap_or_default()
            ))
        }
        ProjectNotesStatus::Unavailable => {
            return Err(current
                .error
                .unwrap_or_else(|| format!("Cannot read '{}'", path.display())))
        }
    };

    let clobbered = match base_updated_at {
        Some(base) => file.updated_at > base,
        // No base means the caller never read the file (first write of a session). Nothing was observed, so nothing can have been clobbered from the caller's point of view.
        None => false,
    };

    if let Some(n) = notes {
        file.notes = n;
    }
    if let Some(t) = tasks {
        file.tasks = t;
    }
    // Rewritten on every write, so a hand-deleted or hand-edited value self-heals.
    file.about = ABOUT_URL.to_string();
    file.schema = SCHEMA_VERSION;
    file.updated_at = now_ms();

    // Pretty-printed: this file is meant to be diffed in git, and a one-line JSON blob makes every edit a whole-file diff.
    let json = serde_json::to_string_pretty(&file)
        .map_err(|e| format!("Failed to serialize notes: {}", e))?;
    // Atomic (temp + rename): this is the user's own writing and, until they commit it, its only copy — a write torn by a crash or a full disk destroys it outright.
    crate::system::write_atomic(&path, &json)?;

    Ok(ProjectNotesWrite { file, clobbered })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Self-cleaning scratch directory helper (avoids external tempfile crate dependency).
    struct Scratch(PathBuf);
    impl Scratch {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("aki-notes-test-{}-{}", tag, std::process::id()));
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
        let p = notes_path(&dir.path());
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
    }

    /// Synchronous test wrapper for write_blocking.
    fn write_sync(
        local_path: &str,
        notes: Option<String>,
        tasks: Option<Vec<serde_json::Value>>,
        base: Option<u64>,
    ) -> Result<ProjectNotesWrite, String> {
        write_blocking(local_path, notes, tasks, base)
    }

    #[test]
    fn missing_dir_reads_unavailable_not_missing() {
        // Unmounted volume must never report Missing (which is writable), only Unavailable.
        let r = read_blocking("/definitely/not/a/real/mount/point");
        assert_eq!(r.status, ProjectNotesStatus::Unavailable);
        assert!(r.file.is_none());
        assert!(r.error.is_some());
    }

    #[test]
    fn missing_file_in_a_real_dir_reads_missing() {
        let d = Scratch::new("missing");
        let r = read_blocking(&d.path());
        assert_eq!(r.status, ProjectNotesStatus::Missing);
        assert!(r.file.is_none());
    }

    #[test]
    fn invalid_json_reads_corrupt_and_never_defaults() {
        let d = Scratch::new("corrupt");
        write_raw(&d, "<<<<<<< HEAD\n{\"notes\":\"mine\"}\n=======\n");
        let r = read_blocking(&d.path());
        assert_eq!(r.status, ProjectNotesStatus::Corrupt);
        // Asserts corrupt JSON is never defaulted to empty (which would cause destructive overwrite of conflict markers).
        assert!(r.file.is_none());
    }

    #[test]
    fn a_file_with_only_one_key_still_loads() {
        // Every field carries #[serde(default)], so a hand-trimmed file is valid, not corrupt.
        let d = Scratch::new("partial");
        write_raw(&d, r#"{"notes":"x"}"#);
        let r = read_blocking(&d.path());
        assert_eq!(r.status, ProjectNotesStatus::Ok);
        let f = r.file.unwrap();
        assert_eq!(f.notes, "x");
        assert!(f.tasks.is_empty());
    }

    #[test]
    fn write_preserves_the_field_it_was_not_given() {
        let d = Scratch::new("preserve");
        write_sync(&d.path(), Some("hello".into()), Some(vec![serde_json::json!({"id":"a"})]), None)
            .unwrap();
        // A notes-only edit must leave `tasks` exactly as it was — `None` means "leave it alone", never "clear it".
        let w = write_sync(&d.path(), Some("changed".into()), None, None).unwrap();
        assert_eq!(w.file.notes, "changed");
        assert_eq!(w.file.tasks.len(), 1);
        // …and the reverse.
        let w = write_sync(&d.path(), None, Some(vec![]), None).unwrap();
        assert_eq!(w.file.notes, "changed");
        assert!(w.file.tasks.is_empty());
    }

    #[test]
    fn write_rewrites_about_and_schema_even_if_hand_deleted() {
        let d = Scratch::new("about");
        write_raw(&d, r#"{"notes":"kept","tasks":[]}"#);
        let w = write_sync(&d.path(), Some("kept".into()), None, None).unwrap();
        assert_eq!(w.file.about, ABOUT_URL);
        assert_eq!(w.file.schema, SCHEMA_VERSION);
        assert!(w.file.updated_at > 0);
    }

    #[test]
    fn write_reports_clobbered_when_the_disk_is_newer_than_the_caller_read() {
        let d = Scratch::new("clobber");
        let first = write_sync(&d.path(), Some("v1".into()), None, None).unwrap();
        // Caller read v1, then someone else (a git pull, the other screen) wrote v2.
        write_raw(&d, &format!(r#"{{"notes":"v2","updated_at":{}}}"#, first.file.updated_at + 5000));
        let w = write_sync(&d.path(), Some("v3".into()), None, Some(first.file.updated_at)).unwrap();
        assert!(w.clobbered, "a write over newer content must say so");
        assert_eq!(w.file.notes, "v3", "last-write-wins is the chosen policy, not refusal");

        // A caller whose base IS current is not clobbering anything.
        let w2 = write_sync(&d.path(), Some("v4".into()), None, Some(w.file.updated_at)).unwrap();
        assert!(!w2.clobbered);
    }

    #[test]
    fn write_refuses_a_corrupt_file_instead_of_overwriting_it() {
        let d = Scratch::new("refuse");
        let body = "<<<<<<< HEAD\n{}\n";
        write_raw(&d, body);
        assert!(write_sync(&d.path(), Some("new".into()), None, None).is_err());
        // The bytes must be untouched: for the realistic cause (conflict markers) an overwrite destroys BOTH sides of the user's edit.
        assert_eq!(fs::read_to_string(notes_path(&d.path())).unwrap(), body);
    }

    #[test]
    fn write_refuses_a_directory_that_is_not_there() {
        // Creating it would materialise the notes inside an empty mount stub, where they vanish the moment the real volume comes back.
        assert!(write_sync("/definitely/not/a/real/mount/point", Some("x".into()), None, None).is_err());
    }

    #[test]
    fn traversal_in_local_path_is_refused_on_both_paths() {
        assert_eq!(read_blocking("/tmp/../etc").status, ProjectNotesStatus::Unavailable);
        assert!(write_sync("/tmp/../etc", Some("x".into()), None, None).is_err());
    }
}
