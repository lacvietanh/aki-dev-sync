//! Claude Code CLI state cleanup - sizes and deletes the CLI's leftovers under `~/.claude`.
//!
//! Design, invariants and the group table: `docs/plan/done/claudecode-cleanup.md`.
//! Path-by-path impact reference: `docs/ref/claudecode-cleanup-paths.md`.
//!
//! Security model: frontend never sends a path, only catalogue keys resolved via CATALOG. Anything absent from CATALOG is undeletable, keeping ~/.claude/skills/ and Aki rules structurally unreachable.

use serde::Serialize;
use std::fs;
use std::path::Path;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    File,
    Dir,
    /// ~/.claude/projects/ emptied around project memory/ subdirs (authored memory is preserved and only deleted via Kind::MemoryDirs; see docs/plan/done/claudecode-cleanup.md §2).
    ProjectsDir,
    /// Every ~/.claude/projects/<slug>/memory/ as one entry, separated from ProjectsDir so memory deletion requires deliberate selection.
    MemoryDirs,
}

struct CatalogEntry {
    group: &'static str,
    key: &'static str,
    /// Always relative to home directory (never absolute, no '..'; asserted by catalog_paths_stay_under_home).
    rel: &'static str,
    kind: Kind,
    label: &'static str,
}

const GROUP_ACCOUNT: &str = "account";
const GROUP_DATA: &str = "data";
const GROUP_MEMORY: &str = "memory";
const GROUP_CACHE: &str = "cache";

/// Every deletable path, and the only source of truth for what this feature may touch.
const CATALOG: &[CatalogEntry] = &[
    // -- Account & auth: deleting these signs the CLI out. ---------------------------------------
    CatalogEntry { group: GROUP_ACCOUNT, key: "claude-json",         rel: ".claude.json",                     kind: Kind::File, label: "OAuth token & account" },
    CatalogEntry { group: GROUP_ACCOUNT, key: "credentials",         rel: ".claude/.credentials.json",        kind: Kind::File, label: "Legacy credentials" },
    CatalogEntry { group: GROUP_ACCOUNT, key: "auth-cache",          rel: ".claude/auth-cache.json",          kind: Kind::File, label: "Auth / plan cache" },
    CatalogEntry { group: GROUP_ACCOUNT, key: "stats-cache",         rel: ".claude/stats-cache.json",         kind: Kind::File, label: "Usage stats cache" },
    CatalogEntry { group: GROUP_ACCOUNT, key: "rate-limits-cache",   rel: ".claude/rate-limits-cache.json",   kind: Kind::File, label: "Rate limit cache" },
    CatalogEntry { group: GROUP_ACCOUNT, key: "daemon-auth-status",  rel: ".claude/daemon-auth-status.json",  kind: Kind::File, label: "Daemon auth status" },
    CatalogEntry { group: GROUP_ACCOUNT, key: "daemon-auth-cooldown",rel: ".claude/daemon-auth-cooldown",     kind: Kind::File, label: "Daemon auth cooldown" },
    // -- Session data: history, transcripts, undo snapshots. -------------------------------------
    CatalogEntry { group: GROUP_DATA,    key: "projects",            rel: ".claude/projects",                 kind: Kind::ProjectsDir, label: "Chat transcripts (keeps memory/)" },
    CatalogEntry { group: GROUP_DATA,    key: "history",             rel: ".claude/history.jsonl",            kind: Kind::File, label: "Prompt history" },
    CatalogEntry { group: GROUP_DATA,    key: "file-history",        rel: ".claude/file-history",             kind: Kind::Dir,  label: "File undo snapshots" },
    CatalogEntry { group: GROUP_DATA,    key: "sessions",            rel: ".claude/sessions",                 kind: Kind::Dir,  label: "Session records" },
    CatalogEntry { group: GROUP_DATA,    key: "paste-cache",         rel: ".claude/paste-cache",              kind: Kind::Dir,  label: "Pasted text cache" },
    CatalogEntry { group: GROUP_DATA,    key: "plans",               rel: ".claude/plans",                    kind: Kind::Dir,  label: "Saved plan files" },
    CatalogEntry { group: GROUP_DATA,    key: "session-env",         rel: ".claude/session-env",              kind: Kind::Dir,  label: "Per-session env" },
    CatalogEntry { group: GROUP_DATA,    key: "tasks",               rel: ".claude/tasks",                    kind: Kind::Dir,  label: "Background task output" },
    CatalogEntry { group: GROUP_DATA,    key: "shell-snapshots",     rel: ".claude/shell-snapshots",          kind: Kind::Dir,  label: "Shell snapshots" },
    CatalogEntry { group: GROUP_DATA,    key: "downloads",           rel: ".claude/downloads",                kind: Kind::Dir,  label: "Downloaded files" },
    // -- Agent memory: deletable, but alone in its own group so "select all of Data" cannot reach it.
    CatalogEntry { group: GROUP_MEMORY,  key: "agent-memory",        rel: ".claude/projects",                 kind: Kind::MemoryDirs, label: "Agent memory (all projects)" },
    // -- Caches: safe, regenerated on demand. ----------------------------------------------------
    CatalogEntry { group: GROUP_CACHE,   key: "cache",               rel: ".claude/cache",                    kind: Kind::Dir,  label: "App cache" },
    CatalogEntry { group: GROUP_CACHE,   key: "backups",             rel: ".claude/backups",                  kind: Kind::Dir,  label: "Auto backups" },
    CatalogEntry { group: GROUP_CACHE,   key: "plugins",             rel: ".claude/plugins",                  kind: Kind::Dir,  label: "Plugin binaries" },
    CatalogEntry { group: GROUP_CACHE,   key: "telemetry",           rel: ".claude/telemetry",                kind: Kind::Dir,  label: "Telemetry logs" },
    CatalogEntry { group: GROUP_CACHE,   key: "debug",               rel: ".claude/debug",                    kind: Kind::Dir,  label: "Debug logs" },
    CatalogEntry { group: GROUP_CACHE,   key: "daemon-log",          rel: ".claude/daemon.log",               kind: Kind::File, label: "Daemon log" },
    CatalogEntry { group: GROUP_CACHE,   key: "os-cache",            rel: "Library/Caches/claude-code",       kind: Kind::Dir,  label: "macOS cache" },
];

/// Shown in modal's read-only "Kept" group for visibility. Deletion is gated strictly by CATALOG membership alone.
const KEPT: &[(&str, &str)] = &[
    (".claude/settings.json",         "Settings & permissions"),
    (".claude/settings.local.json",   "Local settings"),
    (".claude/config.json",           "CLI config"),
    (".claude/CLAUDE.md",             "Global rules"),
    (".claude/CLAUDE.local.md",       "Machine-local rules"),
    (".claude/skills",                "Skills"),
    (".claude/hooks",                 "Hooks"),
    (".claude/statusline-command.sh", "Statusline script"),
];

/// Top-level names under ~/.claude that Unlisted aggregate must not double-count (reported by CATALOG or KEPT).
fn is_accounted_for(name: &str) -> bool {
    CATALOG
        .iter()
        .any(|e| e.rel.strip_prefix(".claude/") == Some(name))
        || KEPT.iter().any(|(rel, _)| rel.strip_prefix(".claude/") == Some(name))
        // Aki rule-corpus backups (`CLAUDE.md.akidevrule-backup-*`, `settings.json.aki-bak-*`) are rolling filenames, so they cannot be listed literally the way KEPT's entries are.
        || name.contains(".aki")
}

// ---------------------------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------------------------

#[derive(Serialize)]
pub struct CleanupEntry {
    key: String,
    label: String,
    path: String,
    /// Always bytes. Unit base and formatting are handled by frontend in src/utils/bytes.js.
    bytes: u64,
    exists: bool,
}

#[derive(Serialize)]
pub struct CleanupGroup {
    id: String,
    label: String,
    deletable: bool,
    bytes: u64,
    entries: Vec<CleanupEntry>,
}

#[derive(Serialize)]
pub struct CleanupReport {
    #[serde(rename = "freedBytes")]
    freed_bytes: u64,
    removed: Vec<String>,
    errors: Vec<String>,
}

// ---------------------------------------------------------------------------------------------
// Sizing
// ---------------------------------------------------------------------------------------------

/// Apparent size of path without following symlinks (prevents traversing outside ~/.claude or inflating size).
fn path_size(path: &Path) -> u64 {
    let Ok(meta) = fs::symlink_metadata(path) else { return 0 };
    if meta.is_symlink() {
        return meta.len();
    }
    if meta.is_file() {
        return meta.len();
    }
    if !meta.is_dir() {
        return 0;
    }
    let Ok(entries) = fs::read_dir(path) else { return 0 };
    entries.flatten().map(|e| path_size(&e.path())).sum()
}

/// `projects/` minus every `<slug>/memory/` - what the Data group actually reclaims.
fn projects_size_excluding_memory(root: &Path) -> u64 {
    let Ok(slugs) = fs::read_dir(root) else { return 0 };
    slugs
        .flatten()
        .map(|slug| {
            let p = slug.path();
            if !p.is_dir() {
                return path_size(&p);
            }
            let Ok(inner) = fs::read_dir(&p) else { return 0 };
            inner
                .flatten()
                .filter(|e| e.file_name() != "memory")
                .map(|e| path_size(&e.path()))
                .sum()
        })
        .sum()
}

/// Discovers projects/<slug>/memory/ paths and count (count ensures empty memory dirs display as "0 B" rather than "-").
fn memory_dirs(root: &Path) -> (u64, usize) {
    fs::read_dir(root)
        .map(|slugs| {
            slugs
                .flatten()
                .map(|s| s.path().join("memory"))
                .filter(|m| fs::symlink_metadata(m).is_ok())
                .fold((0u64, 0usize), |(b, n), m| (b + path_size(&m), n + 1))
        })
        .unwrap_or((0, 0))
}

fn entry_size(home: &Path, e: &CatalogEntry) -> u64 {
    let path = home.join(e.rel);
    match e.kind {
        Kind::ProjectsDir => projects_size_excluding_memory(&path),
        Kind::MemoryDirs => memory_dirs(&path).0,
        _ => path_size(&path),
    }
}

fn entry_exists(home: &Path, e: &CatalogEntry) -> bool {
    match e.kind {
        Kind::MemoryDirs => memory_dirs(&home.join(e.rel)).1 > 0,
        _ => fs::symlink_metadata(home.join(e.rel)).is_ok(),
    }
}

/// Row display path (MemoryDirs formats as wildcard ~/rel/*/memory since it represents multiple directories).
fn entry_display_path(e: &CatalogEntry) -> String {
    match e.kind {
        Kind::MemoryDirs => format!("~/{}/*/memory", e.rel),
        _ => format!("~/{}", e.rel),
    }
}

// ---------------------------------------------------------------------------------------------
// Scan
// ---------------------------------------------------------------------------------------------

fn build_group(home: &Path, id: &str, label: &str) -> CleanupGroup {
    let entries: Vec<CleanupEntry> = CATALOG
        .iter()
        .filter(|e| e.group == id)
        .map(|e| CleanupEntry {
            key: e.key.to_string(),
            label: e.label.to_string(),
            path: entry_display_path(e),
            bytes: entry_size(home, e),
            exists: entry_exists(home, e),
        })
        .collect();
    CleanupGroup {
        id: id.to_string(),
        label: label.to_string(),
        deletable: true,
        bytes: entries.iter().map(|e| e.bytes).sum(),
        entries,
    }
}

/// Read-only group: protected paths plus Unlisted aggregate for non-catalog items (deny-by-default undeletable space).
fn build_kept_group(home: &Path) -> CleanupGroup {
    let mut entries: Vec<CleanupEntry> = KEPT
        .iter()
        .map(|(rel, label)| {
            let path = home.join(rel);
            CleanupEntry {
                key: String::new(),
                label: label.to_string(),
                path: format!("~/{}", rel),
                bytes: path_size(&path),
                exists: fs::symlink_metadata(&path).is_ok(),
            }
        })
        .collect();

    let (unlisted_bytes, unlisted_count) = fs::read_dir(home.join(".claude"))
        .map(|it| {
            it.flatten()
                .filter(|e| !is_accounted_for(&e.file_name().to_string_lossy()))
                .fold((0u64, 0usize), |(b, n), e| (b + path_size(&e.path()), n + 1))
        })
        .unwrap_or((0, 0));
    entries.push(CleanupEntry {
        key: String::new(),
        label: format!("Unlisted ({} items)", unlisted_count),
        path: "~/.claude/…".to_string(),
        bytes: unlisted_bytes,
        exists: unlisted_count > 0,
    });

    CleanupGroup {
        id: "kept".to_string(),
        label: "Kept".to_string(),
        deletable: false,
        bytes: entries.iter().map(|e| e.bytes).sum(),
        entries,
    }
}

fn scan(home: &Path) -> Vec<CleanupGroup> {
    vec![
        build_group(home, GROUP_ACCOUNT, "Account"),
        build_group(home, GROUP_DATA, "Data"),
        build_group(home, GROUP_MEMORY, "Agent memory"),
        build_group(home, GROUP_CACHE, "Cache"),
        build_kept_group(home),
    ]
}

/// Sizes all catalogue entries via spawn_blocking to avoid blocking IPC dispatch thread during large tree walks (RULE-stack-tauri A1).
#[tauri::command]
pub async fn scan_claude_cleanup() -> Result<Vec<CleanupGroup>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let home = dirs::home_dir().ok_or("Cannot resolve home dir")?;
        Ok(scan(&home))
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

// ---------------------------------------------------------------------------------------------
// Delete
// ---------------------------------------------------------------------------------------------

fn resolve(key: &str) -> Option<&'static CatalogEntry> {
    CATALOG.iter().find(|e| e.key == key)
}

/// Empties projects/<slug>/ except memory/; removes slug dir only if empty (slugs with surviving memory are kept).
fn remove_projects_except_memory(root: &Path) -> Result<(), String> {
    let slugs = match fs::read_dir(root) {
        Ok(it) => it,
        Err(_) => return Ok(()), // nothing there is not a failure
    };
    let mut first_err: Option<String> = None;
    for slug in slugs.flatten() {
        let p = slug.path();
        if !p.is_dir() {
            if let Err(e) = fs::remove_file(&p) {
                first_err.get_or_insert(e.to_string());
            }
            continue;
        }
        let mut kept_any = false;
        if let Ok(inner) = fs::read_dir(&p) {
            for item in inner.flatten() {
                if item.file_name() == "memory" {
                    kept_any = true;
                    continue;
                }
                let ip = item.path();
                let res = if ip.is_dir() && !ip.is_symlink() {
                    fs::remove_dir_all(&ip)
                } else {
                    fs::remove_file(&ip)
                };
                if let Err(e) = res {
                    kept_any = true;
                    first_err.get_or_insert(e.to_string());
                }
            }
        }
        if !kept_any {
            let _ = fs::remove_dir(&p);
        }
    }
    match first_err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// Removes projects/<slug>/memory/ and cleans up parent slug dirs if left completely empty.
fn remove_memory_dirs(root: &Path) -> Result<(), String> {
    let slugs = match fs::read_dir(root) {
        Ok(it) => it,
        Err(_) => return Ok(()),
    };
    let mut first_err: Option<String> = None;
    for slug in slugs.flatten() {
        let p = slug.path();
        if !p.is_dir() {
            continue;
        }
        let memory = p.join("memory");
        if fs::symlink_metadata(&memory).is_ok() {
            if let Err(e) = fs::remove_dir_all(&memory) {
                first_err.get_or_insert(e.to_string());
            }
        }
        // Fails harmlessly when transcripts are still there - non-empty is not an error here.
        let _ = fs::remove_dir(&p);
    }
    match first_err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

fn remove_entry(home: &Path, e: &CatalogEntry) -> Result<(), String> {
    let path = home.join(e.rel);
    if fs::symlink_metadata(&path).is_err() {
        return Ok(()); // already gone
    }
    match e.kind {
        Kind::ProjectsDir => remove_projects_except_memory(&path),
        Kind::MemoryDirs => remove_memory_dirs(&path),
        Kind::Dir => fs::remove_dir_all(&path).map_err(|err| err.to_string()),
        Kind::File => fs::remove_file(&path).map_err(|err| err.to_string()),
    }
}

/// Deletes given catalogue keys (keys only, never paths; unknown keys refused per docs/plan/done/claudecode-cleanup.md §2).
/// Per-entry failures are collected into report instead of aborting overall cleanup run.
#[tauri::command]
pub async fn run_claude_cleanup(keys: Vec<String>) -> Result<CleanupReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let home = dirs::home_dir().ok_or("Cannot resolve home dir")?;
        let mut report = CleanupReport { freed_bytes: 0, removed: vec![], errors: vec![] };

        for key in &keys {
            let Some(entry) = resolve(key) else {
                report.errors.push(format!("unknown cleanup key \"{}\"", key));
                crate::logger::error("claude_cleanup", &format!("refused unknown key \"{}\"", key));
                continue;
            };
            // Measured before deleting: afterwards there is nothing left to measure.
            let before = entry_size(&home, entry);
            match remove_entry(&home, entry) {
                Ok(()) => {
                    report.freed_bytes += before;
                    report.removed.push(entry.key.to_string());
                }
                Err(e) => report.errors.push(format!("{}: {}", entry.rel, e)),
            }
        }

        crate::logger::info(
            "claude_cleanup",
            &format!(
                "removed {} entries, {} bytes, {} errors",
                report.removed.len(),
                report.freed_bytes,
                report.errors.len()
            ),
        );
        Ok(report)
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

// ---------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Asserts catalogue relative paths never escape home dir (no absolute paths or '..').
    #[test]
    fn catalog_paths_stay_under_home() {
        for e in CATALOG {
            assert!(!e.rel.starts_with('/'), "{} is absolute", e.rel);
            assert!(!e.rel.contains(".."), "{} escapes home", e.rel);
            assert!(!e.key.is_empty(), "{} has no key", e.rel);
        }
    }

    #[test]
    fn catalog_keys_are_unique() {
        for (i, a) in CATALOG.iter().enumerate() {
            for b in &CATALOG[i + 1..] {
                assert_ne!(a.key, b.key, "duplicate key {}", a.key);
            }
        }
    }

    /// Verifies critical paths (Aki rules, settings, skills) are strictly undeletable; agent memory is tested separately in memory_is_deletable_but_only_through_its_own_group.
    #[test]
    fn protected_paths_are_not_deletable() {
        const NEVER: &[&str] = &[
            ".claude/skills",
            ".claude/hooks",
            ".claude/settings.json",
            ".claude/settings.local.json",
            ".claude/CLAUDE.md",
            ".claude/CLAUDE.local.md",
            ".claude/statusline-command.sh",
            ".claude",
            ".aki",
        ];
        for e in CATALOG {
            assert!(!NEVER.contains(&e.rel), "{} must never be deletable", e.rel);
        }
    }

    /// Verifies memory deletion is isolated to its own group so selecting all Data entries never touches memory.
    #[test]
    fn memory_is_deletable_but_only_through_its_own_group() {
        let memory = resolve("agent-memory").expect("memory must be deletable");
        assert_eq!(memory.group, GROUP_MEMORY);
        assert!(
            !CATALOG.iter().any(|e| e.group == GROUP_DATA && e.kind == Kind::MemoryDirs),
            "no Data entry may delete memory"
        );
        assert_eq!(
            CATALOG.iter().filter(|e| e.group == GROUP_MEMORY).count(),
            1,
            "the memory group must hold exactly one entry, so its checkbox means one thing"
        );
    }

    #[test]
    fn unknown_key_resolves_to_nothing() {
        assert!(resolve("skills").is_none());
        assert!(resolve("../../etc/passwd").is_none());
        assert!(resolve("").is_none());
        assert!(resolve("projects").is_some());
    }

    /// End-to-end temp tree test: transcript cleanup removes chat history while preserving authored memory and slug dir.
    #[test]
    fn projects_cleanup_preserves_memory() {
        let root = std::env::temp_dir().join(format!(
            "aki-cleanup-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let slug = root.join("-Volumes-DEV-demo");
        fs::create_dir_all(slug.join("memory")).unwrap();
        fs::write(slug.join("memory/MEMORY.md"), "keep me").unwrap();
        fs::write(slug.join("transcript.jsonl"), "drop me").unwrap();
        fs::create_dir_all(slug.join("subdir")).unwrap();
        fs::write(slug.join("subdir/x.json"), "drop me too").unwrap();

        // Size must exclude memory/ or the modal would promise space it never frees.
        assert_eq!(projects_size_excluding_memory(&root), "drop me".len() as u64 + "drop me too".len() as u64);

        remove_projects_except_memory(&root).unwrap();

        assert!(slug.join("memory/MEMORY.md").exists(), "memory was destroyed");
        assert!(!slug.join("transcript.jsonl").exists());
        assert!(!slug.join("subdir").exists());
        assert!(slug.exists(), "slug dir with surviving memory must be kept");

        // …and ticking memory as well finishes the job: memory goes, and the now-empty slug folder goes with it rather than being left behind as a hollow directory.
        let (bytes, count) = memory_dirs(&root);
        assert_eq!(count, 1);
        assert_eq!(bytes, "keep me".len() as u64);
        remove_memory_dirs(&root).unwrap();
        assert!(!slug.exists(), "an emptied slug dir must not be left behind");

        fs::remove_dir_all(&root).ok();
    }

    /// Verifies memory deletion leaves transcripts intact (bidirectional independence).
    #[test]
    fn memory_delete_leaves_transcripts_alone() {
        let root = std::env::temp_dir().join(format!(
            "aki-cleanup-mem-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let slug = root.join("-Volumes-DEV-demo");
        fs::create_dir_all(slug.join("memory")).unwrap();
        fs::write(slug.join("memory/MEMORY.md"), "gone").unwrap();
        fs::write(slug.join("transcript.jsonl"), "stays").unwrap();

        remove_memory_dirs(&root).unwrap();

        assert!(!slug.join("memory").exists());
        assert!(slug.join("transcript.jsonl").exists(), "transcripts must survive");

        fs::remove_dir_all(&root).ok();
    }
}

