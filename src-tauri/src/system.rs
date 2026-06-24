use base64::{engine::general_purpose, Engine as _};
use std::process::Command;

use crate::sync::expand_remote_tilde;

/// Creates a Command and injects Homebrew/local paths on macOS to ensure consistent behavior
/// between dev (terminal PATH) and build (macOS GUI PATH).
pub fn create_command(cmd: &str) -> Command {
    let mut c = Command::new(cmd);
    #[cfg(target_os = "macos")]
    {
        let current_path = std::env::var("PATH").unwrap_or_default();
        let new_path = if current_path.is_empty() {
            "/opt/homebrew/bin:/usr/local/bin".to_string()
        } else {
            format!("/opt/homebrew/bin:/usr/local/bin:{}", current_path)
        };
        c.env("PATH", new_path);
    }
    c
}

#[derive(serde::Serialize)]
pub struct IdeAvailability {
    pub vscode: bool,
    pub vscode_insiders: bool,
    pub antigravity: bool,
}

fn validate_remote_host(host: &str) -> Result<(), String> {
    if host
        .chars()
        .all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_' || c == '@')
    {
        Ok(())
    } else {
        Err(format!("Remote host '{}' contains unsafe characters", host))
    }
}

fn applescript_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Passes args directly to macOS `open`. JS is responsible for building the arg list.
#[tauri::command]
pub fn macos_open(args: Vec<String>) -> Result<(), String> {
    Command::new("open")
        .args(&args)
        .spawn()
        .map_err(|e| format!("Failed to open: {}", e))?;
    Ok(())
}

/// Subprocess-based remote openers that cannot be expressed as a plain `open` call:
/// - `terminal`: SSH via AppleScript (macOS-only)
/// - `antigravity`: `antigravity-ide --remote` CLI
#[tauri::command]
pub fn open_remote_subprocess(ide_name: String, host: String, path: String) -> Result<(), String> {
    validate_remote_host(&host)?;
    match ide_name.as_str() {
        "terminal" => {
            #[cfg(target_os = "macos")]
            {
                let expanded = expand_remote_tilde(&path);
                let safe_path = applescript_escape(&expanded);
                let script = format!(
                    "tell application \"Terminal\" to do script \"ssh {} -t 'mkdir -p \\\"{}\\\" && cd \\\"{}\\\" ; exec bash'\"",
                    host, safe_path, safe_path
                );
                Command::new("osascript")
                    .arg("-e")
                    .arg(&script)
                    .spawn()
                    .map_err(|e| format!("Failed to open remote terminal: {}", e))?;
            }
            Ok(())
        }
        "antigravity" => {
            let expanded = expand_remote_tilde(&path);
            // Use login shell so antigravity-ide is found via user PATH (JetBrains Toolbox,
            // custom profile setup, etc.) — not available in macOS GUI app's stripped PATH.
            // Prefer $SHELL (zsh on macOS Catalina+) so ~/.zshrc is sourced; fall back to bash.
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
            let safe_path = expanded.replace('\'', "'\\''");
            let shell_cmd = format!(
                "antigravity-ide --remote 'ssh-remote+{}' '{}'",
                host, safe_path
            );
            // -ilc: interactive (-i) sources ~/.zshrc (not just ~/.zprofile); login (-l) sources
            // ~/.zprofile. Both needed because antigravity-ide PATH is typically set in ~/.zshrc,
            // which a non-interactive login shell (-lc) never reads — causing silent failure when
            // the app launches from Finder vs. from a terminal that already inherited full PATH.
            Command::new(&shell)
                .args(["-ilc", &shell_cmd])
                .spawn()
                .map_err(|e| format!("Failed to open Antigravity remotely: {}", e))?;
            Ok(())
        }
        _ => Err(format!("Unknown subprocess target: {}", ide_name)),
    }
}

#[tauri::command]
pub fn get_project_icon_base64(local_path: String) -> Result<Option<String>, String> {
    let path = std::path::Path::new(&local_path);
    let is_nuxt = path.join("nuxt.config.ts").exists() || path.join("nuxt.config.js").exists();
    let is_tauri = path.join("src-tauri/tauri.conf.json").exists();

    if !is_nuxt && !is_tauri {
        return Ok(None);
    }

    let candidates = if is_tauri {
        vec![
            "src-tauri/icons/128x128.png",
            "src-tauri/icons/128x128@2x.png",
            "src-tauri/icons/64x64.png",
            "src-tauri/icons/32x32.png",
            "src-tauri/icons/icon.png",
            "src-tauri/icons/icon.ico",
        ]
    } else {
        vec![
            "public/favicon/favicon.ico",
            "public/favicon.ico",
            "public/favicon/icon.png",
            "public/icon.png",
            "public/favicon/apple-touch-icon.png",
            "public/favicon/icon-192.png",
            "public/favicon/icon-512-maskable.png",
        ]
    };

    let mut best: Option<(std::path::PathBuf, u64, &str)> = None;
    for icon in &candidates {
        let icon_path = path.join(icon);
        if let Ok(meta) = std::fs::metadata(&icon_path) {
            let size = meta.len();
            if best
                .as_ref()
                .map_or(true, |(_, best_size, _)| size < *best_size)
            {
                best = Some((icon_path, size, icon));
            }
        }
    }

    if let Some((icon_path, size, icon_name)) = best {
        if size > 150_000 {
            return Ok(None);
        }
        if let Ok(bytes) = std::fs::read(&icon_path) {
            let mime = if icon_name.ends_with(".png") {
                "image/png"
            } else {
                "image/x-icon"
            };
            return Ok(Some(format!(
                "data:{};base64,{}",
                mime,
                general_purpose::STANDARD.encode(&bytes)
            )));
        }
    }

    Ok(None)
}

#[tauri::command]
pub fn check_ide_availability() -> IdeAvailability {
    #[cfg(target_os = "macos")]
    {
        return IdeAvailability {
            vscode: std::path::Path::new("/Applications/Visual Studio Code.app").exists(),
            vscode_insiders: std::path::Path::new(
                "/Applications/Visual Studio Code - Insiders.app",
            )
            .exists(),
            antigravity: std::path::Path::new("/Applications/Antigravity IDE.app").exists()
                || std::path::Path::new("/Applications/Antigravity.app").exists(),
        };
    }
    #[cfg(not(target_os = "macos"))]
    IdeAvailability {
        vscode: false,
        vscode_insiders: false,
        antigravity: false,
    }
}

#[tauri::command]
pub fn resolve_remote_path(host: String, path: String) -> Result<String, String> {
    if !path.starts_with("~/") && path != "~" && !path.contains("$HOME") {
        return Ok(path);
    }
    
    let expanded = expand_remote_tilde(&path);
    
    let mut command = create_command("ssh");
    // Pass the command as a single argument so SSH passes it intact to the remote shell.
    // Otherwise SSH concatenates multiple args with spaces and `bash -c` gets split.
    let script = format!("bash -c \"echo {}\"", expanded);
    command.args([&host, &script]);
    
    let output = command.output()
        .map_err(|e| format!("Failed to resolve remote path: {}", e))?;
        
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(format!("SSH error resolving path: {}", String::from_utf8_lossy(&output.stderr)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_host_accepts_hostname() {
        assert!(validate_remote_host("myserver").is_ok());
    }

    #[test]
    fn remote_host_accepts_user_at_host() {
        assert!(validate_remote_host("user@myserver").is_ok());
    }

    #[test]
    fn remote_host_accepts_dotted_hostname() {
        assert!(validate_remote_host("prod.example.com").is_ok());
    }

    #[test]
    fn remote_host_accepts_ip() {
        assert!(validate_remote_host("192.168.1.100").is_ok());
    }

    #[test]
    fn remote_host_rejects_semicolon() {
        assert!(validate_remote_host("host; rm -rf /").is_err());
    }

    #[test]
    fn remote_host_rejects_backtick() {
        assert!(validate_remote_host("host`cmd`").is_err());
    }

    #[test]
    fn remote_host_rejects_space() {
        assert!(validate_remote_host("my host").is_err());
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
