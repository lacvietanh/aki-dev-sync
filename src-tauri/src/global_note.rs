use tauri::Manager;

#[tauri::command]
pub async fn read_global_note(app: tauri::AppHandle) -> Result<String, String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("globalnote.json");

    if !path.exists() {
        return Ok(String::new());
    }

    let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
    Ok(v["content"].as_str().unwrap_or("").to_string())
}

#[tauri::command]
pub async fn write_global_note(app: tauri::AppHandle, content: String) -> Result<(), String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;

    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let json = serde_json::json!({ "content": content });
    std::fs::write(dir.join("globalnote.json"), json.to_string())
        .map_err(|e| e.to_string())
}
