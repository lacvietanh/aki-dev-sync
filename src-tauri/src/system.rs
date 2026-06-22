use base64::{engine::general_purpose, Engine as _};
use std::process::Command;

use crate::sync::expand_remote_tilde;

/// Opens `target` with an optional macOS application. Falls back to plain `open` on non-macOS.
fn open_with(app_name: Option<&str>, target: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let mut cmd = Command::new("open");
        if let Some(app) = app_name {
            cmd.args(["-a", app]);
        }
        cmd.arg(target);
        cmd.spawn().map_err(|e| format!("Failed to open '{}': {}", target, e))?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        Command::new("open")
            .arg(target)
            .spawn()
            .map_err(|e| format!("Failed to open '{}': {}", target, e))?;
    }
    Ok(())
}

/// Validates that an SSH host contains only characters safe for shell interpolation.
fn validate_ssh_host(host: &str) -> Result<(), String> {
    if host.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_') {
        Ok(())
    } else {
        Err(format!("SSH host '{}' contains unsafe characters", host))
    }
}

/// Escapes a value for use inside an AppleScript double-quoted string.
fn applescript_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[tauri::command]
pub fn open_local_dir(path: String) -> Result<(), String> {
    if !std::path::Path::new(&path).exists() {
        return Err(format!("Directory does not exist: {}", path));
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .args(["-R", &path])
            .spawn()
            .map_err(|e| format!("Failed to open Finder: {}", e))?;
        return Ok(());
    }
    #[cfg(not(target_os = "macos"))]
    open_with(None, &path)
}

#[tauri::command]
pub fn open_in_terminal(path: String) -> Result<(), String> {
    if !std::path::Path::new(&path).exists() {
        return Err(format!("Directory does not exist: {}", path));
    }
    open_with(Some("Terminal"), &path)
}

#[tauri::command]
pub fn open_in_vscode(path: String) -> Result<(), String> {
    if !std::path::Path::new(&path).exists() {
        return Err(format!("Directory does not exist: {}", path));
    }
    open_with(Some("Visual Studio Code"), &path)
}

#[tauri::command]
pub fn open_antigravity_app() -> Result<(), String> {
    open_with(Some("Antigravity"), "")
}

#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    open_with(None, &url)
}

#[tauri::command]
pub fn open_remote_terminal(host: String, path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        validate_ssh_host(&host)?;
        let expanded = expand_remote_tilde(&path);
        let safe_path = applescript_escape(&expanded);
        let script = format!(
            "tell application \"Terminal\" to do script \"ssh {} -t 'cd \\\"{}\\\" ; exec bash'\"",
            host, safe_path
        );
        Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .spawn()
            .map_err(|e| format!("Failed to open remote terminal: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
pub fn get_project_icon_base64(local_path: String) -> Result<Option<String>, String> {
    let path = std::path::Path::new(&local_path);
    if !path.join("nuxt.config.ts").exists() && !path.join("nuxt.config.js").exists() {
        return Ok(None);
    }

    let candidates = [
        "public/favicon/favicon.ico",
        "public/favicon.ico",
        "public/favicon/icon.png",
        "public/icon.png",
        "public/favicon/apple-touch-icon.png",
        "public/favicon/icon-192.png",
        "public/favicon/icon-512-maskable.png",
    ];

    let mut best: Option<(std::path::PathBuf, u64, &str)> = None;
    for icon in &candidates {
        let icon_path = path.join(icon);
        if let Ok(meta) = std::fs::metadata(&icon_path) {
            let size = meta.len();
            if best.as_ref().map_or(true, |(_, best_size, _)| size < *best_size) {
                best = Some((icon_path, size, icon));
            }
        }
    }

    if let Some((icon_path, size, icon_name)) = best {
        if size > 150_000 {
            return Ok(None);
        }
        if let Ok(bytes) = std::fs::read(&icon_path) {
            let mime = if icon_name.ends_with(".png") { "image/png" } else { "image/x-icon" };
            return Ok(Some(format!("data:{};base64,{}", mime, general_purpose::STANDARD.encode(&bytes))));
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_host_accepts_hostname() {
        assert!(validate_ssh_host("myserver").is_ok());
    }

    #[test]
    fn ssh_host_accepts_dotted_hostname() {
        assert!(validate_ssh_host("prod.example.com").is_ok());
    }

    #[test]
    fn ssh_host_accepts_ip() {
        assert!(validate_ssh_host("192.168.1.100").is_ok());
    }

    #[test]
    fn ssh_host_rejects_semicolon() {
        assert!(validate_ssh_host("host; rm -rf /").is_err());
    }

    #[test]
    fn ssh_host_rejects_backtick() {
        assert!(validate_ssh_host("host`cmd`").is_err());
    }

    #[test]
    fn ssh_host_rejects_space() {
        assert!(validate_ssh_host("my host").is_err());
    }

    #[test]
    fn applescript_escape_backslash() {
        assert_eq!(applescript_escape("a\\b"), "a\\\\b");
    }

    #[test]
    fn applescript_escape_double_quote() {
        assert_eq!(applescript_escape("say \"hi\""), "say \\\"hi\\\"");
    }

    #[test]
    fn applescript_escape_clean_string() {
        assert_eq!(applescript_escape("/Users/aki/app"), "/Users/aki/app");
    }

    #[test]
    fn applescript_escape_tilde_path() {
        assert_eq!(applescript_escape("$HOME/app"), "$HOME/app");
    }
}
