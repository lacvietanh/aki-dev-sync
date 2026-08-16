use tauri::Manager;

/// The global note file's on-disk shape. `#[serde(default)]` on both fields (the project's
/// serde-default rule) means an OLD file that only ever had `{"content": "..."}` still
/// deserializes cleanly, with `tasks` backfilled to an empty Vec — no migration step needed and no
/// silent field drop.
///
/// `tasks` is opaque to Rust on purpose (`serde_json::Value`, not a typed struct): the task shape
/// and its migrations have exactly one owner, src/utils/tasks.js, exactly as a project's `tasks`
/// field is opaque inside projects.json. Rust never inspects or validates the task objects.
#[derive(serde::Serialize, serde::Deserialize, Default)]
pub struct GlobalNoteFile {
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub tasks: Vec<serde_json::Value>,
}

fn note_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("globalnote.json"))
}

#[tauri::command]
pub async fn read_global_note(app: tauri::AppHandle) -> Result<GlobalNoteFile, String> {
    let path = note_path(&app)?;

    if !path.exists() {
        return Ok(GlobalNoteFile::default());
    }

    let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    // Unparseable (corrupt/truncated) file falls back to default, matching the previous behaviour's unwrap_or_default() rather than surfacing a hard error to the user.
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

/// Read-modify-write: `None` for either field means "leave what is on disk alone", never "clear
/// it". This is the multi-entity regression guard (CLAUDE.md) applied to the note file itself —
/// a content-only save (typing in the textarea) or an older companion that only ever sends
/// `content` must never wipe the task list as a side effect, and a task-only edit must never
/// touch the note text.
///
/// The whole read-modify-write is serialized by [`WRITE_LOCK`]: two writers interleaving between
/// the read and the write is a lost update, and `write_atomic` cannot help with it - it guarantees
/// a whole file, not the right one. This is reachable in ordinary use, because the host window and
/// a paired companion both call this command against the same file: the phone saving a task and
/// the Mac saving note text at the same moment each read the pre-change file, and whichever
/// renames last silently discards the other's edit. An async mutex (not `std::sync::Mutex`) because
/// the guard is held across `.await`-able async command body.
#[tauri::command]
pub async fn write_global_note(
    app: tauri::AppHandle,
    content: Option<String>,
    tasks: Option<Vec<serde_json::Value>>,
) -> Result<(), String> {
    static WRITE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = WRITE_LOCK.lock().await;

    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;

    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let path = dir.join("globalnote.json");

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
