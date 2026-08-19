// Single funnel for "where is the app data dir": `$HOME/.aki/devsync`, ecosystem-standardized
// away from Tauri's per-OS default (`~/Library/Application Support/aki.devsync/` on macOS).
// Every module that needs the current app data dir calls `app_data_dir()` here instead of
// resolving it itself. `sync.rs`'s pre-1.7.1 `legacy_baseline_dir()` (`~/.aki/devsync-baselines`)
// is a separate, older, read-only fallback path and is untouched by this module.

use std::fs;
use std::path::{Path, PathBuf};

pub fn app_data_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Cannot resolve home directory")?;
    let dir = home.join(".aki").join("devsync");
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create app data dir: {}", e))?;
    Ok(dir)
}

const LEGACY_ARTIFACTS: &[&str] = &[
    "projects.json",
    "usage.log",
    "globalnote.json",
    "ssh_undo_state.txt",
    "ssh_redo_state.txt",
    "baselines",
    "companion-devices.json",
    "companion-server.json",
];

/// Per-file tally for one legacy artifact: a directory accumulates one count per file under it, a plain file lands on exactly one.
#[derive(Default)]
struct Tally {
    moved: u32,
    skipped: u32,
    failed: u32,
}

/// Per file, not per directory: one unreadable child must not leave a half-copied directory that a later launch reads as finished. docs/plan/done/appdata-dir-to-aki-devsync.md
fn migrate_tree(src: &Path, dest: &Path, tally: &mut Tally) {
    if src.is_dir() {
        if fs::create_dir_all(dest).is_err() {
            tally.failed += 1;
            return;
        }
        let entries = match fs::read_dir(src) {
            Ok(e) => e,
            Err(_) => {
                tally.failed += 1;
                return;
            }
        };
        for entry in entries.flatten() {
            migrate_tree(&entry.path(), &dest.join(entry.file_name()), tally);
        }
        let now_empty = fs::read_dir(src).map(|mut it| it.next().is_none()).unwrap_or(false);
        if now_empty {
            let _ = fs::remove_dir(src);
        }
    } else if dest.exists() {
        tally.skipped += 1;
    } else {
        match fs::copy(src, dest) {
            Ok(_) => match fs::remove_file(src) {
                Ok(()) => tally.moved += 1,
                Err(_) => tally.failed += 1,
            },
            Err(_) => tally.failed += 1,
        }
    }
}

/// A mixed or failed artifact reports its counts under `failed`, so a partial directory never reads like a clean one in the log.
fn record(name: &str, tally: Tally, moved: &mut Vec<String>, skipped: &mut Vec<String>, failed: &mut Vec<String>) {
    match tally {
        Tally { moved: m, skipped: 0, failed: 0 } if m > 0 => moved.push(name.to_string()),
        Tally { moved: 0, skipped: s, failed: 0 } if s > 0 => skipped.push(name.to_string()),
        Tally { moved: 0, skipped: 0, failed: 0 } => {}
        Tally { moved: m, skipped: s, failed: f } => {
            failed.push(format!("{} (partial: moved={} skipped={} failed={})", name, m, s, f))
        }
    }
}

/// Runs before anything touches app data, `logger::init` included - hence the returned summary instead of a log call. docs/plan/done/appdata-dir-to-aki-devsync.md
pub fn migrate_legacy_app_data(app: &tauri::AppHandle) -> String {
    use tauri::Manager;

    let legacy_dir = match app.path().app_data_dir() {
        Ok(d) => d,
        Err(e) => return format!("app data migration skipped: legacy dir unresolved: {}", e),
    };
    if !legacy_dir.exists() {
        return "app data migration: no legacy dir present".to_string();
    }
    let new_dir = match app_data_dir() {
        Ok(d) => d,
        Err(e) => return format!("app data migration skipped: {}", e),
    };

    let mut moved = Vec::new();
    let mut skipped = Vec::new();
    let mut failed = Vec::new();

    for name in LEGACY_ARTIFACTS {
        let src = legacy_dir.join(name);
        if !src.exists() {
            continue;
        }
        let dest = new_dir.join(name);
        let mut tally = Tally::default();
        migrate_tree(&src, &dest, &mut tally);
        record(name, tally, &mut moved, &mut skipped, &mut failed);
    }

    let legacy_now_empty = fs::read_dir(&legacy_dir).map(|mut it| it.next().is_none()).unwrap_or(false);
    if legacy_now_empty {
        let _ = fs::remove_dir_all(&legacy_dir);
    }

    format!(
        "app data migration: moved={:?} skipped={:?} failed={:?}",
        moved, skipped, failed
    )
}
