//! Per-project tasks & notes, stored **inside the project's own working directory**.
//!
//! THE DECISION THIS FILE IMPLEMENTS (docs/plan/done/1.22.0-notes-json-ssot.md §1): the local repo is the
//! single source of truth for a project. Tasks and notes used to live in the app's central
//! `projects.json`, which is state on one Mac — it does not travel with the repo, is invisible to
//! anyone who opens the project, and is not recoverable from the project's own history. They now
//! live at `<local_path>/.akidevsync/notes.json`, a file meant to be committed.
//!
//! Everything about that file — its path, its shape, its atomic write, its lock — is owned here and
//! nowhere else. `sync.rs` spells the `.akidevsync/` literal once more, in the rsync protect filter that keeps a mirroring transfer from deleting it.
//!
//! WHY THE READ RETURNS A TAGGED STATUS AND NOT A DEFAULTED STRUCT. `global_note.rs`, the closest
//! precedent, does `unwrap_or_default()` on a corrupt file. That is defensible there: the file is
//! in app data, nothing else writes it, and a broken one is genuinely empty. It would be a data-loss
//! bug HERE. This file lives in a git repo the user also edits by hand and pulls over, on a path
//! that may be an unmounted external volume — so "could not read it" is an ordinary, recoverable
//! state, and collapsing it into "it is empty" means the UI shows an empty note and then saves that
//! emptiness over the user's real one. A missing file and an unreadable file are different facts,
//! and only the first one is writable.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Written into every file so anyone who finds `.akidevsync/notes.json` in a checkout can find out
/// what wrote it. A bare URL, no prose: the identification job is done by the URL resolving.
const ABOUT_URL: &str = "https://github.com/lacvietanh/aki-dev-sync";

/// Reserved for a future shape change. Written and read; **nothing branches on it** — a file with a
/// higher `schema` is still read normally. Version dispatch appears when a second shape does.
const SCHEMA_VERSION: u32 = 1;

/// The on-disk shape.
///
/// `#[serde(default)]` on EVERY field, no exceptions (project rule: a missing default silently
/// drops the field on the next write). This is what lets a file written by a future version, or one
/// hand-trimmed to `{"notes":"x"}`, still load without a migration step.
///
/// `tasks` is opaque to Rust (`serde_json::Value`), exactly as in `global_note.rs`: the task shape
/// and its migrations have one owner, `src/utils/tasks.js`. Rust never inspects an element.
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
    /// Milliseconds since epoch, stamped by Rust at write time. Not a UI value — it is the
    /// staleness fence that lets a write report that it replaced someone else's newer edit.
    #[serde(default)]
    pub updated_at: u64,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectNotesStatus {
    /// File read and parsed. Writable.
    Ok,
    /// `local_path` is a readable directory, but the file is not there. The normal state for a
    /// project that has never taken a note, and for a fresh clone. **Writable** — the write creates
    /// the directory and the file.
    Missing,
    /// `local_path` is not a directory right now (unmounted volume, deleted folder, permission
    /// denied), or the file exists but cannot be read. **Not writable.**
    Unavailable,
    /// The file exists and was read but is not valid JSON — a git merge-conflict marker is the
    /// realistic case. **Not writable**, and deliberately not defaulted to empty.
    Corrupt,
}

#[derive(Serialize, Clone, Debug)]
pub struct ProjectNotesRead {
    pub status: ProjectNotesStatus,
    /// `Some` only when `status == Ok`. The UI must not be able to read content off a status it is
    /// not allowed to write back.
    pub file: Option<ProjectNotesFile>,
    /// The real io/serde message for `Unavailable`/`Corrupt`, surfaced in the modal's tooltip. A
    /// parse error naming a line is what turns "corrupt" into "you have conflict markers at line 4".
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
    /// The file as it now stands on disk — the JS side re-seeds its store from this rather than
    /// from what it hoped it wrote.
    pub file: ProjectNotesFile,
    /// The write replaced content newer than what the caller had read. Last-write-wins is the
    /// chosen policy, but never SILENTLY: this is what raises the Toast telling the user to check
    /// git. See the module's concurrency note on `write_project_notes`.
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

/// Same boundary check every other consumer of a stored `local_path` goes through
/// (`projects.rs::validate_path_segment`): the value reaches this module from `projects.json`, which
/// a paired companion can write into, so it is not trusted here either.
fn check_path(local_path: &str) -> Result<(), String> {
    crate::projects::validate_path_segment("local_path", local_path)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The synchronous read. Never returns `Err` for a missing/corrupt/unreachable file — those are
/// *statuses*, and turning them into command failures is what would make every caller re-derive the
/// distinction from an error string.
fn read_blocking(local_path: &str) -> ProjectNotesRead {
    if let Err(e) = check_path(local_path) {
        return ProjectNotesRead::unavailable(e);
    }
    let dir = Path::new(local_path);
    // Checked BEFORE the file: "the volume is not mounted" and "this project has no notes yet" look
    // identical at the file level (both are a NotFound), and only the second one may be written to.
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
        // NOT `unwrap_or_default()`. See the module doc comment: defaulting here means showing an
        // empty note over a git-conflicted file and then saving that emptiness on top of it.
        Err(e) => ProjectNotesRead::corrupt(e),
    }
}

/// Reads one project's notes file.
///
/// `async fn` + `spawn_blocking`, and this is NOT the "plain fast local file I/O" exemption in the
/// Tauri stack rule: `local_path` is exactly the path class `projects.rs::load_projects_blocking`
/// documents as able to stall in the kernel for tens of seconds on an unhealthy network mount.
/// `system.rs::read_project_changelog` — which also reads a file inside `local_path` — is
/// `spawn_blocking` for the same reason.
#[tauri::command]
pub async fn read_project_notes(local_path: String) -> Result<ProjectNotesRead, String> {
    tauri::async_runtime::spawn_blocking(move || read_blocking(&local_path))
        .await
        .map_err(|e| format!("read_project_notes task join error: {}", e))
}

/// Batch read for app boot: one IPC round-trip for the whole project list instead of N.
///
/// One target failing never fails the batch — each project gets its own status, because a single
/// unmounted volume must not blank every other project's task badges.
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

/// Read-modify-write of one project's notes file.
///
/// **`None` for a field means "leave what is on disk alone", never "clear it"** — the multi-entity
/// regression guard (CLAUDE.md) applied to the file's own fields, and the same contract as
/// `write_global_note`. This is what lets a `git pull` that changed `notes` survive a task-only
/// write issued from the app a second later.
///
/// The whole read-modify-write is serialized by `WRITE_LOCK`. `write_atomic` guarantees a *whole*
/// file, not the *right* one: two writers interleaving between the read and the rename is a lost
/// update, and that is reachable in ordinary use — the host window and a paired companion both drive
/// this command. One global mutex rather than a per-path map: writes are small, rare and user-paced,
/// so a keyed map would be speculative structure for contention that does not exist.
///
/// `base_updated_at` is the `updated_at` the caller last read. If the disk is newer, the write still
/// lands (last-write-wins) but reports `clobbered: true`, and the JS side raises a Toast pointing at
/// git. Refusing the write instead would strand the user's typing with no way to save it; silently
/// overwriting would hide that someone else's edit is now only in git history.
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

/// The write's synchronous core. Split out from the command so the unit tests below drive the real
/// thing directly instead of standing up a tokio runtime — the shipped path and the tested path are
/// then the same code, and the test module does not depend on which tokio features happen to be
/// enabled in this crate.
fn write_blocking(
    local_path: &str,
    notes: Option<String>,
    tasks: Option<Vec<serde_json::Value>>,
    base_updated_at: Option<u64>,
) -> Result<ProjectNotesWrite, String> {
    check_path(local_path)?;
    // Refuse rather than create: `create_dir_all` on an unmounted volume's mount point would
    // materialise a directory in the EMPTY mount stub and write the user's notes somewhere they
    // will disappear the moment the real volume comes back.
    if !Path::new(local_path).is_dir() {
        return Err(format!("'{}' is not a readable directory right now", local_path));
    }

    let path = notes_path(local_path);
    let current = read_blocking(local_path);
    let mut file = match current.status {
        ProjectNotesStatus::Ok => current.file.unwrap_or_default(),
        ProjectNotesStatus::Missing => ProjectNotesFile::default(),
        // Writing over a file we could not parse would destroy whatever is in it — which, for the
        // realistic cause (conflict markers), is BOTH sides of the user's edit.
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
        // No base means the caller never read the file (first write of a session). Nothing was
        // observed, so nothing can have been clobbered from the caller's point of view.
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

    // Pretty-printed: this file is meant to be diffed in git, and a one-line JSON blob makes every
    // edit a whole-file diff.
    let json = serde_json::to_string_pretty(&file)
        .map_err(|e| format!("Failed to serialize notes: {}", e))?;
    // Atomic (temp + rename): this is the user's own writing and, until they commit it, its only
    // copy — a write torn by a crash or a full disk destroys it outright.
    crate::system::write_atomic(&path, &json)?;

    Ok(ProjectNotesWrite { file, clobbered })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A scratch directory that removes itself. No `tempfile` dependency is in this crate and one
    /// test module is not the reason to add one.
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

    /// Alias for the real write core (`write_blocking`) — the command adds only the lock and the
    /// `spawn_blocking` hop, neither of which changes the file semantics under test.
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
        // The distinction the whole module exists for: an unmounted volume must never look like
        // "this project simply has no notes yet", because that state is writable.
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
        // The trap this asserts against: `unwrap_or_default()` would hand back an empty file here,
        // the UI would show an empty note, and the next save would write that emptiness over the
        // user's conflicted text.
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
        // A notes-only edit must leave `tasks` exactly as it was — `None` means "leave it alone",
        // never "clear it".
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
        // The bytes must be untouched: for the realistic cause (conflict markers) an overwrite
        // destroys BOTH sides of the user's edit.
        assert_eq!(fs::read_to_string(notes_path(&d.path())).unwrap(), body);
    }

    #[test]
    fn write_refuses_a_directory_that_is_not_there() {
        // Creating it would materialise the notes inside an empty mount stub, where they vanish the
        // moment the real volume comes back.
        assert!(write_sync("/definitely/not/a/real/mount/point", Some("x".into()), None, None).is_err());
    }

    #[test]
    fn traversal_in_local_path_is_refused_on_both_paths() {
        assert_eq!(read_blocking("/tmp/../etc").status, ProjectNotesStatus::Unavailable);
        assert!(write_sync("/tmp/../etc", Some("x".into()), None, None).is_err());
    }
}
