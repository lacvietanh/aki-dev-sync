/// On-disk shape: `#[serde(default)]` backfills empty tasks for legacy single-content files without migrations.
/// Tasks is opaque (`serde_json::Value`), owned solely by src/utils/tasks.js and never validated by Rust.
#[derive(serde::Serialize, serde::Deserialize, Default)]
pub struct GlobalNoteFile {
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub tasks: Vec<serde_json::Value>,
}

fn note_path() -> Result<std::path::PathBuf, String> {
    Ok(crate::app_paths::app_data_dir()?.join("globalnote.json"))
}

#[tauri::command]
pub async fn read_global_note(_app: tauri::AppHandle) -> Result<GlobalNoteFile, String> {
    let path = note_path()?;

    if !path.exists() {
        return Ok(GlobalNoteFile::default());
    }

    let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    // Unparseable (corrupt/truncated) file falls back to default, matching the previous behaviour's unwrap_or_default() rather than surfacing a hard error to the user.
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

/// Read-modify-write: `None` preserves on-disk data (CLAUDE.md multi-entity guard) so field-specific saves never wipe other fields.
/// Serialized by `WRITE_LOCK` (async mutex) to prevent concurrent host/companion lost updates across `.await` points.
#[tauri::command]
pub async fn write_global_note(
    _app: tauri::AppHandle,
    content: Option<String>,
    tasks: Option<Vec<serde_json::Value>>,
) -> Result<(), String> {
    static WRITE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = WRITE_LOCK.lock().await;

    let path = note_path()?;

    let mut current = if path.exists() {
        let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str::<GlobalNoteFile>(&raw).unwrap_or_default()
    } else {
        GlobalNoteFile::default()
    };

    if let Some(c) = content {
        current.content = c;
    }
    if let Some(t) = tasks {
        current.tasks = t;
    }

    let json = serde_json::to_string(&current).map_err(|e| e.to_string())?;
    // Atomic (temp + rename): the note is the user's own writing and this is its only copy, so a write torn by a crash or a full disk destroys it outright.
    crate::system::write_atomic(&path, &json)
}
