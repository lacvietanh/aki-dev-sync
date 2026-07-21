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

/// Runs `shell_cmd` in Terminal.app via AppleScript, avoiding the double-window bug where a
/// cold-started Terminal spawns its own default (home-dir) window at launch *and* `do script`
/// spawns a second one for the command. When Terminal has to be launched from scratch, we reuse
/// its freshly-created default window (`in window 1`) instead of letting `do script` open another;
/// when Terminal is already running, behavior is unchanged (`do script` opens a new window as before).
///
/// The wait for that default window is a poll (up to ~2s, checking every 100ms), not a fixed
/// `delay` - a flat delay races a slow shell startup (heavy .zshrc: nvm, conda, etc.): if the
/// window isn't up yet when we check, we'd fall through to `do script` opening a *second* window,
/// and the slow default window would still appear on its own moments later (the exact "one window
/// at $HOME + one at the right target" bug this helper exists to prevent).
#[cfg(target_os = "macos")]
fn open_terminal_with_command(shell_cmd: &str) -> Result<(), String> {
    let safe_cmd = applescript_escape(shell_cmd);
    let script = format!(
        "tell application \"Terminal\"\n\
         \tset wasOff to not running\n\
         \tif wasOff then\n\
         \t\tlaunch\n\
         \t\trepeat 20 times\n\
         \t\t\tif (count of windows) > 0 then exit repeat\n\
         \t\t\tdelay 0.1\n\
         \t\tend repeat\n\
         \tend if\n\
         \tif wasOff and (count of windows) > 0 then\n\
         \t\tdo script \"{cmd}\" in window 1\n\
         \telse\n\
         \t\tdo script \"{cmd}\"\n\
         \tend if\n\
         \tactivate\n\
         end tell",
        cmd = safe_cmd
    );
    Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .spawn()
        .map_err(|e| format!("Failed to open Terminal: {}", e))?;
    Ok(())
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

/// Opens a local Terminal window `cd`'d into `local_path`. Routed through
/// `open_terminal_with_command` (not a plain `open -a Terminal <path>` via `macos_open`) so it
/// gets the same cold-start double-window protection as `run_project_command`/SSH terminal.
#[tauri::command]
pub fn open_local_terminal(local_path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let shell_cmd = format!("cd \"{}\"", local_path);
        open_terminal_with_command(&shell_cmd)?;
    }
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
                // Quotes here are shell quotes inside the ssh command, not AppleScript quotes  - 
                // open_terminal_with_command applies the AppleScript escaping separately.
                let shell_cmd = format!(
                    "ssh {} -t 'mkdir -p \"{}\" && cd \"{}\" ; exec bash'",
                    host, expanded, expanded
                );
                open_terminal_with_command(&shell_cmd)?;
            }
            Ok(())
        }
        "antigravity" => {
            let expanded = expand_remote_tilde(&path);
            // Use login shell so antigravity-ide is found via user PATH (JetBrains Toolbox,
            // custom profile setup, etc.) - not available in macOS GUI app's stripped PATH.
            // Prefer $SHELL (zsh on macOS Catalina+) so ~/.zshrc is sourced; fall back to bash.
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
            let safe_path = expanded.replace('\'', "'\\''");
            let shell_cmd = format!(
                "antigravity-ide --remote 'ssh-remote+{}' '{}'",
                host, safe_path
            );
            // -ilc: interactive (-i) sources ~/.zshrc (not just ~/.zprofile); login (-l) sources
            // ~/.zprofile. Both needed because antigravity-ide PATH is typically set in ~/.zshrc,
            // which a non-interactive login shell (-lc) never reads - causing silent failure when
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

const SSH_COLOR_MARKER_BEGIN: &str = "# --- Aki SSH remote color BEGIN (managed by Aki Dev Sync - safe to remove) ---";
const SSH_COLOR_MARKER_END: &str = "# --- Aki SSH remote color END ---";

/// Wraps `ssh` so the local Terminal.app/iTerm2 background tints while a remote session is
/// active, then resets on exit - the same OSC 11/111 background-swap trick the user already
/// hand-rolled locally, packaged so it can be (re)installed from the app. Idempotent: re-running
/// strips any previously-installed block (between the markers) before writing a fresh one, so
/// repeated installs never duplicate.
const SSH_COLOR_SNIPPET: &str = r#"
ssh() {
  printf '\033]11;#1a0f0f\007'
  command ssh "$@"
  printf '\033]111\007'
}
"#;

/// Local-machine-only: the background swap needs to happen in the *local* shell that is
/// launching `ssh`, so there is nothing to push to remote hosts here (unlike the statusline
/// customizer, which does need per-host rollout).
///
/// `spawn_blocking`-wrapped per CLAUDE.md's blocking-UI rule: even "just" file I/O is a
/// synchronous syscall, and the house rule now has zero exceptions for that - every command
/// touching disk or a subprocess goes through the blocking thread-pool, no case-by-case judgment
/// calls about whether a given file happens to be small.
#[tauri::command]
pub async fn install_ssh_terminal_color() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let home = std::env::var("HOME").map_err(|e| e.to_string())?;
        let zshrc_path = std::path::Path::new(&home).join(".zshrc");
        let existing = std::fs::read_to_string(&zshrc_path).unwrap_or_default();

        let mut kept_lines: Vec<&str> = Vec::new();
        let mut skipping = false;
        for line in existing.lines() {
            if line == SSH_COLOR_MARKER_BEGIN {
                skipping = true;
                continue;
            }
            if line == SSH_COLOR_MARKER_END {
                skipping = false;
                continue;
            }
            if !skipping {
                kept_lines.push(line);
            }
        }

        let mut new_content = kept_lines.join("\n");
        if !new_content.is_empty() && !new_content.ends_with('\n') {
            new_content.push('\n');
        }
        new_content.push_str(SSH_COLOR_MARKER_BEGIN);
        new_content.push_str(SSH_COLOR_SNIPPET);
        new_content.push_str(SSH_COLOR_MARKER_END);
        new_content.push('\n');

        if zshrc_path.exists() {
            let backup_path = std::path::Path::new(&home).join(".zshrc.aki-bak");
            if !backup_path.exists() {
                std::fs::copy(&zshrc_path, &backup_path).map_err(|e| e.to_string())?;
            }
        }
        std::fs::write(&zshrc_path, new_content).map_err(|e| e.to_string())?;
        Ok(zshrc_path.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

/// Resolves the local AkiClaudeDoc checkout by trying well-known candidate paths first (same
/// conservative pattern as the CLAUDE_BIN resolver - a file-existence check has no dependency on
/// where any given machine happens to keep its dev tree), so it's never a guess. Its exact
/// location varies per machine (see CLAUDE.md), so if none of these hit, the caller falls back to
/// pointing the user at the GitHub repo to clone it.
fn find_akiclaudedoc_install_script(home: &str) -> Option<String> {
    let candidates = [
        "/Volumes/DEV/AkiClaudeDoc/install.sh".to_string(),
        format!("{}/AkiClaudeDoc/install.sh", home),
        format!("{}/dev/AkiClaudeDoc/install.sh", home),
        format!("{}/Developer/AkiClaudeDoc/install.sh", home),
        format!("{}/Documents/AkiClaudeDoc/install.sh", home),
    ];
    candidates.into_iter().find(|c| std::path::Path::new(c).exists())
}

/// Runs the local AkiClaudeDoc `install.sh` in a visible Terminal window (the script prints
/// colored progress output the user should see), or errors out pointing at the repo to clone if
/// no checkout is found on this machine.
#[tauri::command]
pub fn install_akiclaudedoc() -> Result<(), String> {
    let home = std::env::var("HOME").map_err(|e| e.to_string())?;
    match find_akiclaudedoc_install_script(&home) {
        #[cfg(target_os = "macos")]
        Some(script) => {
            let shell_cmd = format!("bash \"{}\"", script);
            open_terminal_with_command(&shell_cmd)
        }
        #[cfg(not(target_os = "macos"))]
        Some(_) => Ok(()),
        None => Err(
            "Không tìm thấy AkiClaudeDoc trên máy này. Clone repo trước: https://github.com/lacvietanh/AkiClaudeDoc"
                .to_string(),
        ),
    }
}

use std::sync::{Mutex, OnceLock};
use std::collections::HashMap;
use crate::projects::SyncProject;

pub struct IconData {
    pub bytes: Vec<u8>,
    pub mime_type: String,
}

static PROJECT_ICONS: OnceLock<Mutex<HashMap<String, IconData>>> = OnceLock::new();

pub fn get_project_icons() -> &'static Mutex<HashMap<String, IconData>> {
    PROJECT_ICONS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn load_and_cache_project_icons(projects: &[SyncProject]) {
    let mut cache = get_project_icons().lock().unwrap();
    cache.clear();

    for project in projects {
        let path = std::path::Path::new(&project.local_path);
        let is_nuxt = path.join("nuxt.config.ts").exists() || path.join("nuxt.config.js").exists();
        let is_tauri = path.join("src-tauri/tauri.conf.json").exists();
        let is_web = !is_nuxt && !is_tauri && (path.join("package.json").exists() || path.join("index.html").exists());

        let candidates = if is_tauri {
            vec![
                "src-tauri/icons/32x32.png",
                "src-tauri/icons/64x64.png",
                "src-tauri/icons/icon.png",
                "src-tauri/icons/128x128.png",
            ]
        } else if is_nuxt || is_web {
            vec![
                "public/favicon/icon-48.png",
                "public/favicon.ico",
                "public/favicon/favicon.ico",
                "public/favicon/icon-192.png",
                "public/icon.png",
                "favicon.ico",
                "icon.png",
            ]
        } else {
            vec![
                "public/favicon/icon-48.png",
                "public/favicon.ico",
                "public/favicon/favicon.ico",
                "public/icon.png",
                "favicon.ico",
                "icon.png",
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
            if size <= 250_000 {
                if let Ok(bytes) = std::fs::read(&icon_path) {
                    let mime_type = if icon_name.ends_with(".png") {
                        "image/png".to_string()
                    } else {
                        "image/x-icon".to_string()
                    };
                    cache.insert(project.id.clone(), IconData { bytes, mime_type });
                }
            }
        }
    }
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
pub async fn resolve_remote_path(host: String, path: String) -> Result<String, String> {
    if !path.starts_with("~/") && path != "~" && !path.contains("$HOME") {
        return Ok(path);
    }

    // The SSH round-trip is blocking IO. This command used to be a plain `pub fn`, so Tauri
    // ran it on the main thread and the whole UI froze for the duration of the network call.
    // Move it onto the blocking pool (CLAUDE.md "async fn + blocking subprocess" pitfall) so
    // the UI stays responsive while the resolve is in flight.
    tauri::async_runtime::spawn_blocking(move || {
        let expanded = expand_remote_tilde(&path);

        let mut command = create_command("ssh");
        // Pass the command as a single argument so SSH passes it intact to the remote shell.
        // Otherwise SSH concatenates multiple args with spaces and `bash -c` gets split.
        let script = format!("bash -c \"echo {}\"", expanded);
        command.args([&host, &script]);

        let output = command
            .output()
            .map_err(|e| format!("Failed to resolve remote path: {}", e))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err(format!("SSH error resolving path: {}", String::from_utf8_lossy(&output.stderr)))
        }
    })
    .await
    .map_err(|e| format!("resolve_remote_path task join error: {}", e))?
}

/// Resolves the local path of a project's `REPORT.html` (produced by the akihtmlreport skill),
/// pulling it from the remote first if the remote's copy is newer. Local-only projects (no
/// remote_host/remote_path) just check local. Errors only when neither side has the file.
///
/// Deliberately thin: mtime comparison reuses `git::get_file_conflict_info` (the existing
/// local/remote stat-diff primitive, used by the SELECT conflict check) and the pull reuses
/// `sync::rsync_pull_file` - no bespoke SSH stat script or rsync invocation here.
#[tauri::command]
pub async fn resolve_report_html(
    local_path: String,
    remote_host: Option<String>,
    remote_path: Option<String>,
) -> Result<String, String> {
    let host = remote_host.unwrap_or_default();
    let rpath = remote_path.unwrap_or_default();

    let local_exists = std::path::Path::new(&local_path).join("REPORT.html").exists();

    let mut remote_exists = false;
    let mut remote_mtime = 0i64;
    let mut local_mtime = 0i64;
    if !host.is_empty() && !rpath.is_empty() {
        let info = crate::git::get_file_conflict_info(
            local_path.clone(),
            host.clone(),
            rpath.clone(),
            vec!["REPORT.html".to_string()],
        )
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| "internal error: no conflict-info result".to_string())?;
        remote_exists = info.remote_exists;
        remote_mtime = info.remote_mtime;
        local_mtime = info.local_mtime;
    }

    if !local_exists && !remote_exists {
        return Err("No REPORT.html found locally or on the remote.".to_string());
    }
    if remote_exists && (!local_exists || remote_mtime > local_mtime) {
        crate::sync::rsync_pull_file(&host, &rpath, "REPORT.html", &local_path)?;
    }
    Ok(std::path::Path::new(&local_path).join("REPORT.html").to_string_lossy().to_string())
}

/// Looks for `filename` in `~/Downloads` so the update modal can offer to open an
/// already-downloaded installer instead of re-triggering a browser download.
/// `file_name()` strips any directory components from the (externally-sourced,
/// GitHub API) filename to prevent escaping the Downloads directory.
#[tauri::command]
pub fn find_in_downloads(filename: String) -> Result<Option<String>, String> {
    let safe_name = std::path::Path::new(&filename)
        .file_name()
        .ok_or_else(|| "Invalid filename".to_string())?;
    let home = std::env::var("HOME").map_err(|e| e.to_string())?;
    let path = std::path::Path::new(&home).join("Downloads").join(safe_name);
    Ok(if path.exists() { Some(path.to_string_lossy().to_string()) } else { None })
}

/// Runs on every app startup (`onMounted` in `AppHeader.vue`) plus manual "Check for Updates"  - 
/// `curl`'s blocking network wait must never sit on the command-dispatch thread (a slow or dead
/// network would freeze the whole app on launch). `spawn_blocking` per CLAUDE.md's blocking-UI
/// rule.
#[tauri::command]
pub async fn check_for_updates() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let out = create_command("curl")
            .args(&[
                "-s",
                "-H", "User-Agent: aki-dev-sync",
                "https://api.github.com/repos/lacvietanh/aki-dev-sync/releases/latest"
            ])
            .output()
            .map_err(|e| format!("Failed to check for updates: {}", e))?;

        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            Ok(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            Err(if stderr.trim().is_empty() { "Network error checking for updates".to_string() } else { stderr })
        }
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}


#[derive(serde::Serialize)]
pub struct ProjectStackInfo {
    pub is_node: bool,
    pub is_tauri: bool,
    pub is_nuxt: bool,
    pub label: String,
    pub cmd: String,
    pub dev_cmd: String,
    pub build_cmd: String,
}

/// True when `path` looks like a Nuxt project (config file or generated `.nuxt` dir present).
fn is_nuxt_project(path: &std::path::Path) -> bool {
    path.join("nuxt.config.js").exists() || path.join("nuxt.config.ts").exists() || path.join(".nuxt").exists()
}

#[tauri::command]
pub fn check_project_stack(local_path: String) -> ProjectStackInfo {
    let path = std::path::Path::new(&local_path);
    let is_node = path.join("package.json").exists();
    let is_tauri = path.join("src-tauri").exists() || path.join("src-tauri/tauri.conf.json").exists();
    let is_nuxt = is_nuxt_project(path);

    let mut pm = "npm";
    let mut run_prefix = "run ";
    if path.join("pnpm-lock.yaml").exists() {
        pm = "pnpm";
        run_prefix = "";
    } else if path.join("yarn.lock").exists() {
        pm = "yarn";
        run_prefix = "";
    } else if path.join("bun.lockb").exists() || path.join("bun.lock").exists() {
        pm = "bun";
        run_prefix = "";
    }

    let (dev_cmd, build_cmd) = if is_tauri {
        (format!("{pm} {run_prefix}tauri dev"), format!("{pm} {run_prefix}build:app"))
    } else if is_nuxt {
        (format!("{pm} {run_prefix}dev"), format!("{pm} {run_prefix}build"))
    } else if is_node {
        (format!("{pm} {run_prefix}dev"), format!("{pm} {run_prefix}build"))
    } else {
        ("".to_string(), "".to_string())
    };

    // Keep label/cmd for backward compat
    let (label, cmd) = if !dev_cmd.is_empty() {
        ("Run Dev".to_string(), dev_cmd.clone())
    } else {
        ("".to_string(), "".to_string())
    };

    ProjectStackInfo {
        is_node,
        is_tauri,
        is_nuxt,
        label,
        cmd,
        dev_cmd,
        build_cmd,
    }
}

/// Opens a Terminal window `cd`'d into `local_path` running `cmd`. Shared by `run_project_command`
/// (BUILD) and `run_project_dev` (DEV) so the terminal-launch line is not duplicated between them.
#[cfg(target_os = "macos")]
fn run_in_project_terminal(local_path: &str, cmd: &str) -> Result<(), String> {
    let shell_cmd = format!("cd \"{}\" && {}", local_path, cmd);
    open_terminal_with_command(&shell_cmd)
}

#[tauri::command]
pub fn run_project_command(local_path: String, cmd: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        run_in_project_terminal(&local_path, &cmd)?;
    }
    Ok(())
}

/// DEV button command: opens the dev command in Terminal, exactly like `run_project_command`
/// (BUILD). An earlier version also polled for the dev server's port to come up and auto-opened
/// it in a browser; removed - it never reliably worked across the range of real project configs
/// (custom dev scripts, non-standard ports, monorepo boot times) and the fixed-cost complexity
/// (port resolution, TCP poll, detached background task) wasn't worth the unreliable payoff. The
/// user opens the browser themselves once the Terminal shows the server is up.
#[tauri::command]
pub fn run_project_dev(local_path: String, cmd: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        run_in_project_terminal(&local_path, &cmd)?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (&local_path, &cmd);
    }
    Ok(())
}

#[tauri::command]
pub fn read_project_changelog(local_path: String) -> Result<String, String> {
    let path = std::path::Path::new(&local_path);
    let names = ["CHANGELOG.md", "changelog.md", "CHANGELOG.txt", "changelog.txt", "CHANGELOG", "changelog"];
    for name in names {
        let file_path = path.join(name);
        if file_path.exists() {
            let bytes = std::fs::read(file_path)
                .map_err(|e| format!("Failed to read file: {}", e))?;
            return Ok(String::from_utf8_lossy(&bytes).into_owned());
        }
    }
    Err("No changelog file found".to_string())
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
