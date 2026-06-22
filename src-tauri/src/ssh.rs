use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

use crate::projects::get_app_data_dir;

#[derive(Serialize)]
pub struct SshHistoryStatus {
    pub can_undo: bool,
    pub can_redo: bool,
}

fn ssh_config_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Cannot find home directory")?;
    Ok(home.join(".ssh").join("config"))
}

/// Swap SSH config between `from` (source to restore) and `to` (destination for current).
/// Pattern: current → to, from → current, delete from.
fn swap_ssh_state(from: &Path, to: &Path) -> Result<String, String> {
    if !from.exists() {
        return Err("No state available".to_string());
    }
    let config = ssh_config_path()?;
    // unwrap_or_default is intentional: a missing SSH config is equivalent to an empty one
    let current = if config.exists() { fs::read_to_string(&config).unwrap_or_default() } else { String::new() };
    let restore = fs::read_to_string(from).map_err(|e| format!("Failed to read state: {}", e))?;
    fs::write(to, &current).map_err(|e| format!("Failed to write state: {}", e))?;
    fs::write(&config, &restore).map_err(|e| format!("Failed to restore SSH config: {}", e))?;
    let _ = fs::remove_file(from);
    Ok(restore)
}

#[tauri::command]
pub fn get_ssh_hosts() -> Result<Vec<String>, String> {
    let path = ssh_config_path()?;
    let mut hosts = Vec::new();
    if let Ok(content) = fs::read_to_string(&path) {
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("Host ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() > 1 {
                    let host = parts[1];
                    if !host.contains('*') && !host.contains('?') {
                        hosts.push(host.to_string());
                    }
                }
            }
        }
    }
    Ok(hosts)
}

#[tauri::command]
pub fn read_ssh_config() -> Result<String, String> {
    let path = ssh_config_path()?;
    if path.exists() {
        fs::read_to_string(&path).map_err(|e| format!("Failed to read SSH config: {}", e))
    } else {
        Ok(String::new())
    }
}

#[tauri::command]
pub fn get_ssh_history_status(app: AppHandle) -> Result<SshHistoryStatus, String> {
    let app_dir = get_app_data_dir(&app)?;
    Ok(SshHistoryStatus {
        can_undo: app_dir.join("ssh_undo_state.txt").exists(),
        can_redo: app_dir.join("ssh_redo_state.txt").exists(),
    })
}

#[tauri::command]
pub fn save_ssh_config(app: AppHandle, content: String) -> Result<(), String> {
    let config = ssh_config_path()?;
    let ssh_dir = config.parent().expect("~/.ssh has no parent");
    if !ssh_dir.exists() {
        fs::create_dir_all(ssh_dir).map_err(|e| format!("Failed to create .ssh dir: {}", e))?;
    }

    let current = if config.exists() { fs::read_to_string(&config).unwrap_or_default() } else { String::new() };

    let app_dir = get_app_data_dir(&app)?;
    let undo_path = app_dir.join("ssh_undo_state.txt");
    let redo_path = app_dir.join("ssh_redo_state.txt");

    fs::write(&undo_path, current).map_err(|e| format!("Failed to write undo state: {}", e))?;
    let _ = fs::remove_file(&redo_path);
    fs::write(&config, content).map_err(|e| format!("Failed to write SSH config: {}", e))
}

#[tauri::command]
pub fn undo_ssh_config(app: AppHandle) -> Result<String, String> {
    let app_dir = get_app_data_dir(&app)?;
    swap_ssh_state(&app_dir.join("ssh_undo_state.txt"), &app_dir.join("ssh_redo_state.txt"))
}

#[tauri::command]
pub fn redo_ssh_config(app: AppHandle) -> Result<String, String> {
    let app_dir = get_app_data_dir(&app)?;
    swap_ssh_state(&app_dir.join("ssh_redo_state.txt"), &app_dir.join("ssh_undo_state.txt"))
}
