use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use tauri::{AppHandle, Emitter, Manager, Window};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SyncHooks {
    pub pre_pull_cmd: Option<String>,
    pub post_pull_cmd: Option<String>,
    pub pre_push_cmd: Option<String>,
    pub post_push_cmd: Option<String>,
    pub run_hooks_on_remote: bool,
}

#[derive(Serialize, Clone)]
struct LogPayload {
    project_id: String,
    line: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SyncProject {
    pub id: String,
    pub name: String,
    pub local_path: String,
    pub remote_host: String,
    pub remote_path: String,
    pub production_url: Option<String>,
    pub pull_excludes: Vec<String>,
    pub push_excludes: Vec<String>,
    pub hooks: SyncHooks,
    pub last_sync_action: Option<String>,
    pub last_sync_time: Option<u64>,
}

#[tauri::command]
fn get_ssh_hosts() -> Result<Vec<String>, String> {
    let home = dirs::home_dir().ok_or("Cannot find home directory")?;
    let ssh_config_path = home.join(".ssh").join("config");
    let mut hosts = Vec::new();

    if let Ok(content) = fs::read_to_string(&ssh_config_path) {
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("Host ") {
                let host_parts: Vec<&str> = line.split_whitespace().collect();
                if host_parts.len() > 1 {
                    let host = host_parts[1];
                    if !host.contains('*') && !host.contains('?') {
                        hosts.push(host.to_string());
                    }
                }
            }
        }
    }

    Ok(hosts)
}

#[derive(Serialize)]
pub struct GitInfo {
    status: String,
    remote_url: String,
    log: String,
}

#[tauri::command]
fn get_git_info(local_path: String) -> Result<GitInfo, String> {
    let path = std::path::Path::new(&local_path);
    if !path.join(".git").exists() {
        return Ok(GitInfo {
            status: "No Git".to_string(),
            remote_url: "".to_string(),
            log: "Not a git repository.".to_string(),
        });
    }

    let mut status = "Clean".to_string();
    let mut log = String::new();
    let mut remote_url = String::new();

    // 1. Get porcelain status
    if let Ok(output) = Command::new("git").current_dir(path).args(["status", "--porcelain"]).output() {
        if !output.status.success() {
            status = "Git Error".to_string();
        } else {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.trim().is_empty() {
                status = "Dirty".to_string();
            }
        }
    } else {
        status = "Git Error".to_string();
    }

    // 2. Check if ahead (only if Clean so far)
    if status == "Clean" {
        if let Ok(out) = Command::new("git").current_dir(path).args(["status", "-sb"]).output() {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                if stdout.contains("[ahead ") {
                    status = "Ahead".to_string();
                }
            }
        }
    }

    // 3. Get Remote URL
    if let Ok(out) = Command::new("git").current_dir(path).args(["remote", "get-url", "origin"]).output() {
        if out.status.success() {
            remote_url = String::from_utf8_lossy(&out.stdout).trim().to_string();
        }
    }

    // 4. Get detailed log
    if let Ok(output) = Command::new("git").current_dir(path).args(["status"]).output() {
        if output.status.success() {
            log.push_str(&String::from_utf8_lossy(&output.stdout));
        } else {
            log.push_str(&String::from_utf8_lossy(&output.stderr));
        }
    }
    
    log.push_str("\n\n--- Recent Commits ---\n");
    if let Ok(log_out) = Command::new("git").current_dir(path).args(["log", "-n", "10", "--oneline"]).output() {
        if log_out.status.success() {
            log.push_str(&String::from_utf8_lossy(&log_out.stdout));
        }
    }

    Ok(GitInfo {
        status,
        remote_url,
        log,
    })
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("Failed to open URL: {}", e))?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("Failed to open URL: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
fn get_project_files(local_path: String) -> Result<Vec<String>, String> {
    let path = std::path::Path::new(&local_path);
    if !path.exists() {
        return Ok(vec![]);
    }

    if path.join(".git").exists() {
        let output = Command::new("git")
            .current_dir(path)
            .args(["status", "--porcelain"])
            .output()
            .map_err(|e| format!("Failed to execute git status: {}", e))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let files: Vec<String> = stdout
                .lines()
                .filter(|s| !s.trim().is_empty())
                .map(|s| {
                    if s.len() > 3 {
                        let file_path = &s[3..];
                        // If git quotes the path (e.g. spaces), remove the quotes
                        if file_path.starts_with('"') && file_path.ends_with('"') {
                            file_path[1..file_path.len() - 1].to_string()
                        } else {
                            file_path.to_string()
                        }
                    } else {
                        "".to_string()
                    }
                })
                .filter(|s| !s.is_empty())
                .collect();
            return Ok(files);
        }
    }

    Ok(vec![])
}

fn get_projects_path(app: &AppHandle) -> Result<PathBuf, String> {
    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Cannot get app data dir: {}", e))?;
    if !app_dir.exists() {
        fs::create_dir_all(&app_dir)
            .map_err(|e| format!("Failed to create app data dir: {}", e))?;
    }
    Ok(app_dir.join("projects.json"))
}

#[tauri::command]
fn load_projects(app: AppHandle) -> Result<Vec<SyncProject>, String> {
    let path = get_projects_path(&app)?;
    if path.exists() {
        let content =
            fs::read_to_string(&path).map_err(|e| format!("Failed to read projects: {}", e))?;
        if let Ok(projects) = serde_json::from_str::<Vec<SyncProject>>(&content) {
            return Ok(projects);
        }
    }

    Ok(vec![])
}

#[tauri::command]
fn save_projects(app: AppHandle, projects: Vec<SyncProject>) -> Result<(), String> {
    let path = get_projects_path(&app)?;
    let content = serde_json::to_string_pretty(&projects)
        .map_err(|e| format!("Failed to serialize projects: {}", e))?;
    fs::write(&path, content).map_err(|e| format!("Failed to write projects: {}", e))?;
    Ok(())
}

fn stream_process_output<R: std::io::Read + Send + 'static>(
    reader: R,
    window: Window,
    project_id: String,
    prefix: &str,
) -> thread::JoinHandle<()> {
    let prefix_owned = prefix.to_string();
    thread::spawn(move || {
        let buf_reader = BufReader::new(reader);
        for line in buf_reader.lines().flatten() {
            let _ = window.emit(
                "sync-log",
                LogPayload {
                    project_id: project_id.clone(),
                    line: format!("{}{}", prefix_owned, line),
                },
            );
        }
    })
}

fn execute_hook(
    window: &Window,
    project_id: &str,
    host: &str,
    cmd: &str,
    run_remote: bool,
    prefix: &str,
) -> Result<(), String> {
    let _ = window.emit(
        "sync-log",
        LogPayload {
            project_id: project_id.to_string(),
            line: format!("\n>>> {}Executing hook: {}\n", prefix, cmd),
        },
    );

    let mut command = if run_remote {
        let mut c = Command::new("ssh");
        c.args([host, cmd]);
        c
    } else {
        let mut c = Command::new("sh");
        c.args(["-c", cmd]);
        c
    };

    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start hook: {}", e))?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let stderr_thread = stream_process_output(stderr, window.clone(), project_id.to_string(), "[ERR] ");
    let stdout_thread = stream_process_output(stdout, window.clone(), project_id.to_string(), "");

    let _ = stderr_thread.join();
    let _ = stdout_thread.join();
    let status = child
        .wait()
        .map_err(|e| format!("Error waiting for hook: {}", e))?;
    if !status.success() {
        return Err(format!(
            "Hook finished with exit code: {}",
            status.code().unwrap_or(-1)
        ));
    }
    Ok(())
}

#[tauri::command]
async fn run_sync(
    window: Window,
    project: SyncProject,
    direction: String, // "pull" or "push"
    dry_run: bool,
    specific_paths: Vec<String>,
    sync_git: bool,
) -> Result<(), String> {
    let is_push = direction == "push";

    // Validate inputs
    if project.local_path.contains("..") {
        return Err("Invalid local path format: directory traversal not allowed".to_string());
    }

    let prefix = if dry_run { "[DRY RUN] " } else { "" };

    // Pre-sync hook
    if !dry_run {
        let pre_cmd = if is_push {
            &project.hooks.pre_push_cmd
        } else {
            &project.hooks.pre_pull_cmd
        };
        if let Some(cmd) = pre_cmd {
            if !cmd.trim().is_empty() {
                execute_hook(
                    &window,
                    &project.id,
                    &project.remote_host,
                    cmd,
                    project.hooks.run_hooks_on_remote,
                    prefix,
                )?;
            }
        }
    } else {
        let _ = window.emit(
            "sync-log",
            LogPayload {
                project_id: project.id.clone(),
                line: format!("\n>>> {}Skipping pre-sync hook\n", prefix),
            },
        );
    }

    // Prepare paths — always add trailing slash so rsync syncs directory
    // *contents*, not the directory itself (avoids creating nested subdirs).
    let local = {
        let p = project.local_path.trim_end_matches('/');
        format!("{}/", p)
    };
    let remote = project.remote_path.trim_end_matches('/');
    let remote_full = format!("{}:{}/", project.remote_host, remote);

    let (src, dest) = if is_push {
        (&local, &remote_full)
    } else {
        (&remote_full, &local)
    };

    // Construct rsync command
    let mut args = vec!["-avzu".to_string()];
    if dry_run {
        args.push("--dry-run".to_string());
    }

    if is_push {
        let remote_dir = if project.remote_path.starts_with("~/") {
            project.remote_path.replacen("~/", "$HOME/", 1)
        } else if project.remote_path == "~" {
            "$HOME".to_string()
        } else {
            project.remote_path.clone()
        };
        args.push(format!("--rsync-path=mkdir -p \"{}\" && rsync", remote_dir));
    } else {
        let _ = std::fs::create_dir_all(&project.local_path);
    }

    if !specific_paths.is_empty() && is_push {
        // Advanced PUSH: use -R (Relative) to push specific files preserving tree
        args.push("-R".to_string());
        for p in &specific_paths {
            args.push(p.clone());
        }
        args.push(dest.clone());
    } else {
        // Standard Sync
        let excludes = if is_push {
            &project.push_excludes
        } else {
            &project.pull_excludes
        };
        let should_sync_git = sync_git && is_push;
        
        for e in excludes {
            if !e.trim().is_empty() {
                if should_sync_git && e.trim() == ".git/" {
                    continue; // Skip excluding .git/ if we explicitly want to sync it (only on push)
                }
                args.push(format!("--exclude={}", e));
            }
        }
        
        // Explicitly exclude .git/ if we do NOT want to sync it (or if it's a pull)
        if !should_sync_git {
            if !excludes.iter().any(|x| x.trim() == ".git/") {
                args.push("--exclude=.git/".to_string());
            }
        }

        if !is_push {
            args.push("--delete".to_string());
        }
        args.push(src.clone());
        args.push(dest.clone());
    }

    let cmd_str = format!("rsync {}", args.join(" "));
    let _ = window.emit(
        "sync-log",
        LogPayload {
            project_id: project.id.clone(),
            line: format!(">>> {}Executing command: {}\n", prefix, cmd_str),
        },
    );

    let mut command = Command::new("rsync");
    if !specific_paths.is_empty() && is_push {
        command.current_dir(&project.local_path);
    }

    let mut child = command
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to execute rsync: {}", e))?;

    let stdout = child.stdout.take().ok_or("Cannot take stdout")?;
    let stderr = child.stderr.take().ok_or("Cannot take stderr")?;

    let stderr_thread = stream_process_output(stderr, window.clone(), project.id.clone(), "[ERR] ");
    let stdout_thread = stream_process_output(stdout, window.clone(), project.id.clone(), "");

    let _ = stderr_thread.join();
    let _ = stdout_thread.join();
    let status = child
        .wait()
        .map_err(|e| format!("Error waiting for rsync to finish: {}", e))?;
    if !status.success() {
        return Err(format!(
            "rsync finished with exit code: {}",
            status.code().unwrap_or(-1)
        ));
    }

    // Post-sync hook
    if !dry_run {
        let post_cmd = if is_push {
            &project.hooks.post_push_cmd
        } else {
            &project.hooks.post_pull_cmd
        };
        if let Some(cmd) = post_cmd {
            if !cmd.trim().is_empty() {
                execute_hook(
                    &window,
                    &project.id,
                    &project.remote_host,
                    cmd,
                    project.hooks.run_hooks_on_remote,
                    prefix,
                )?;
            }
        }
    } else {
        let _ = window.emit(
            "sync-log",
            LogPayload {
                project_id: project.id.clone(),
                line: format!("\n>>> {}Skipping post-sync hook\n", prefix),
            },
        );
    }

    let _ = window.emit(
        "sync-log",
        LogPayload {
            project_id: project.id.clone(),
            line: format!("\n>>> SYNC COMPLETED SUCCESSFULLY{}! <<<\n", prefix),
        },
    );
    Ok(())
}

#[tauri::command]
fn read_ssh_config() -> Result<String, String> {
    let home = dirs::home_dir().ok_or("Cannot find home directory")?;
    let ssh_config_path = home.join(".ssh").join("config");
    if ssh_config_path.exists() {
        fs::read_to_string(&ssh_config_path).map_err(|e| format!("Failed to read SSH config: {}", e))
    } else {
        Ok("".to_string())
    }
}

#[derive(serde::Serialize)]
pub struct SshHistoryStatus {
    can_undo: bool,
    can_redo: bool,
}

#[tauri::command]
fn get_ssh_history_status(app: AppHandle) -> Result<SshHistoryStatus, String> {
    let app_dir = get_projects_path(&app)?.parent().unwrap().to_path_buf();
    let undo_path = app_dir.join("ssh_undo_state.txt");
    let redo_path = app_dir.join("ssh_redo_state.txt");
    Ok(SshHistoryStatus {
        can_undo: undo_path.exists(),
        can_redo: redo_path.exists(),
    })
}

#[tauri::command]
fn save_ssh_config(app: AppHandle, content: String) -> Result<(), String> {
    let home = dirs::home_dir().ok_or("Cannot find home directory")?;
    let ssh_dir = home.join(".ssh");
    if !ssh_dir.exists() {
        fs::create_dir_all(&ssh_dir).map_err(|e| format!("Failed to create .ssh dir: {}", e))?;
    }
    let ssh_config_path = ssh_dir.join("config");
    
    // Read current config to save as undo state
    let current_content = if ssh_config_path.exists() {
        fs::read_to_string(&ssh_config_path).unwrap_or_else(|_| "".to_string())
    } else {
        "".to_string()
    };
    
    let app_dir = get_projects_path(&app)?.parent().unwrap().to_path_buf();
    let undo_path = app_dir.join("ssh_undo_state.txt");
    let redo_path = app_dir.join("ssh_redo_state.txt");
    
    // Write undo state
    fs::write(&undo_path, current_content).map_err(|e| format!("Failed to write undo state: {}", e))?;
    // Delete redo state since we are creating a new branch of history
    let _ = fs::remove_file(&redo_path);
    
    // Write new config
    fs::write(&ssh_config_path, content).map_err(|e| format!("Failed to write SSH config: {}", e))
}

#[tauri::command]
fn undo_ssh_config(app: AppHandle) -> Result<String, String> {
    let app_dir = get_projects_path(&app)?.parent().unwrap().to_path_buf();
    let undo_path = app_dir.join("ssh_undo_state.txt");
    let redo_path = app_dir.join("ssh_redo_state.txt");
    
    if !undo_path.exists() {
        return Err("No undo state available".to_string());
    }
    
    let home = dirs::home_dir().ok_or("Cannot find home directory")?;
    let ssh_config_path = home.join(".ssh").join("config");
    
    // Read current config to save as redo state
    let current_content = if ssh_config_path.exists() {
        fs::read_to_string(&ssh_config_path).unwrap_or_else(|_| "".to_string())
    } else {
        "".to_string()
    };
    
    // Write redo state
    fs::write(&redo_path, current_content).map_err(|e| format!("Failed to write redo state: {}", e))?;
    
    // Read undo state
    let undo_content = fs::read_to_string(&undo_path).map_err(|e| format!("Failed to read undo state: {}", e))?;
    
    // Restore undo state to config
    fs::write(&ssh_config_path, &undo_content).map_err(|e| format!("Failed to restore SSH config: {}", e))?;
    
    // Delete undo state
    let _ = fs::remove_file(&undo_path);
    
    Ok(undo_content)
}

#[derive(serde::Serialize)]
pub struct AgentUsageResponse {
    pub content: String,
    pub fetched_at: String,
    pub file_modified_at: String,
}

#[tauri::command]
async fn provision_agent_usage(agent_name: String, host: String) -> Result<bool, String> {
    if agent_name == "claudecode" {
        let cmd = r#"
            FILE="$HOME/.claude/statusline-command.sh"
            if [ ! -f "$FILE" ]; then exit 0; fi
            if ! grep -q "rate-limits-cache" "$FILE"; then
                cat << 'EOF' > /tmp/patch.sh
rl_input=$(echo "$input" | jq -c '.rate_limits // empty')
if [ -z "$rl_input" ] && [ -f "$HOME/.claude/rate-limits-cache.json" ]; then
    input=$(echo "$input" | jq --argjson old "$(cat "$HOME/.claude/rate-limits-cache.json")" '
        if ($old.rate_limits != null) then
            .rate_limits = ($old.rate_limits | map_values(.used_percentage = 100))
        else . end
    ')
fi
printf '%s' "$input" > "$HOME/.claude/rate-limits-cache.json"
EOF
                sed -i.bak -e '/input=$(cat)/r /tmp/patch.sh' "$FILE"
                rm -f /tmp/patch.sh
            fi
        "#;
        
        let mut c = Command::new("ssh");
        c.args([&host, "sh"]);
        c.stdin(Stdio::piped());
        c.stdout(Stdio::piped());
        c.stderr(Stdio::piped());
        
        let mut child = c.spawn().map_err(|e| format!("Failed to spawn SSH for provision: {}", e))?;
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let _ = stdin.write_all(cmd.as_bytes());
        }
        
        let output = child.wait_with_output().map_err(|e| format!("Failed to SSH for provision: {}", e))?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Provision failed: {}", err));
        }
        return Ok(true);
    } else if agent_name == "antigravity" {
        // Antigravity is local. Skeleton logic.
        return Ok(true);
    }
    
    Err("Unknown agent".into())
}

#[tauri::command]
async fn force_sync_agent_usage(agent_name: String, host: String) -> Result<bool, String> {
    if agent_name == "claudecode" {
        let cmd = r#"
            export CLAUDE_SYNC_OUT=$(if command -v zsh >/dev/null 2>&1; then zsh -lc "claude --model haiku -p /usage 2>/dev/null"; else bash -lc "claude --model haiku -p /usage 2>/dev/null"; fi)
            
            cat << 'EOF' > /tmp/.claude_sync_parse.py
import sys, re, json, datetime, os

out = os.environ.get("CLAUDE_SYNC_OUT", "")
match = re.search(r'(\d+)%\s*used\s*.\s*resets\s*([a-zA-Z]+\s+\d+),\s*(\d+):(\d+)([ap]m)', out, re.IGNORECASE)
if not match:
    sys.exit(0)

pct = int(match.group(1))
year = datetime.datetime.now().year
date_str = f"{match.group(2)} {year} {match.group(3)}:{match.group(4)}{match.group(5)}"
try:
    dt = datetime.datetime.strptime(date_str, "%b %d %Y %I:%M%p")
    resets_at = int(dt.timestamp())
except Exception:
    sys.exit(0)

cache_file = os.path.expanduser("~/.claude/rate-limits-cache.json")
data = {}
if os.path.exists(cache_file):
    try:
        with open(cache_file, "r") as f:
            data = json.load(f)
    except Exception:
        pass

if "rate_limits" not in data or data["rate_limits"] is None:
    data["rate_limits"] = {}

data["rate_limits"]["five_hour"] = {
    "used_percentage": pct,
    "resets_at": resets_at
}

try:
    with open(cache_file, "w") as f:
        json.dump(data, f)
except Exception:
    pass
EOF
            python3 /tmp/.claude_sync_parse.py
            rm -f /tmp/.claude_sync_parse.py
        "#;
        
        let mut c = Command::new("ssh");
        c.args([&host, "sh"]);
        c.stdin(Stdio::piped());
        c.stdout(Stdio::piped());
        c.stderr(Stdio::piped());
        
        let mut child = c.spawn().map_err(|e| format!("Failed to spawn SSH for force sync: {}", e))?;
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let _ = stdin.write_all(cmd.as_bytes());
        }
        
        let _output = child.wait_with_output().map_err(|e| format!("Failed to SSH for force sync: {}", e))?;
        
        // We do not check _output.status.success() here.
        // If Claude hits a rate limit (e.g. "You've hit your session limit"), it exits with a non-zero code.
        // However, the API was still called and the rate-limits-cache.json was successfully updated.
        // We just return Ok(true) to let the frontend refresh the UI.
        
        return Ok(true);
    }
    
    Err("Force sync not supported for this agent".into())
}

#[tauri::command]
async fn get_agent_usage(agent_name: String, host: String) -> Result<Option<AgentUsageResponse>, String> {
    if agent_name == "claudecode" {
        let cmd = r#"
            FILE="$HOME/.claude/rate-limits-cache.json"
            CREDS="$HOME/.claude/.credentials.json"
            if [ -f "$FILE" ]; then
                MTIME=$(stat -c %Y "$FILE" 2>/dev/null || stat -f %m "$FILE" 2>/dev/null)
                
                SUB_TYPE="Unknown"
                TIER="Unknown"
                if [ -f "$CREDS" ]; then
                    FOUND=$(grep -o '"subscriptionType"\s*:\s*"[^"]*"' "$CREDS" | head -n 1 | awk -F'"' '{print $4}')
                    if [ ! -z "$FOUND" ]; then
                        SUB_TYPE="$FOUND"
                    fi
                    FOUND_TIER=$(grep -o '"rateLimitTier"\s*:\s*"[^"]*"' "$CREDS" | head -n 1 | awk -F'"' '{print $4}')
                    if [ ! -z "$FOUND_TIER" ]; then
                        TIER="$FOUND_TIER"
                    fi
                fi

                cat "$FILE"
                echo "|||MTIME|||$MTIME"
                echo "|||SUBTYPE|||$SUB_TYPE"
                echo "|||TIER|||$TIER"
            fi
        "#;
        
        let mut c = Command::new("ssh");
        c.args([&host, "sh"]);
        c.stdin(Stdio::piped());
        c.stdout(Stdio::piped());
        c.stderr(Stdio::piped());
        
        let mut child = c.spawn().map_err(|e| format!("Failed to spawn SSH for get usage: {}", e))?;
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let _ = stdin.write_all(cmd.as_bytes());
        }
        
        let output = child.wait_with_output().map_err(|e| format!("Failed to SSH for get usage: {}", e))?;
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.trim().is_empty() {
                return Ok(None);
            }
            
            let parts: Vec<&str> = stdout.split("|||MTIME|||").collect();
            if parts.len() == 2 {
                let mtime_split: Vec<&str> = parts[1].split("|||SUBTYPE|||").collect();
                let mtime_sec = mtime_split[0].trim().parse::<i64>().unwrap_or(0);
                
                let mut content = parts[0].trim().to_string();
                
                let (sub_type, tier) = if mtime_split.len() > 1 {
                    let sub_split: Vec<&str> = mtime_split[1].split("|||TIER|||").collect();
                    let st = sub_split[0].trim();
                    let t = if sub_split.len() > 1 { sub_split[1].trim() } else { "Unknown" };
                    (st, t)
                } else {
                    ("Unknown", "Unknown")
                };
                
                if content.ends_with('}') {
                    content.pop();
                    if sub_type != "Unknown" {
                        content.push_str(&format!(r#", "subscriptionType": "{}""#, sub_type));
                    }
                    if tier != "Unknown" {
                        content.push_str(&format!(r#", "rateLimitTier": "{}""#, tier));
                    }
                    content.push('}');
                }
                
                use std::time::{SystemTime, UNIX_EPOCH};
                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
                
                return Ok(Some(AgentUsageResponse {
                    content,
                    fetched_at: now.to_string(),
                    file_modified_at: mtime_sec.to_string(),
                }));
            }
        }
        return Ok(None);
    } else if agent_name == "antigravity" {
        let cmd = "npx --yes antigravity-usage --json";
        
        let mut command = if host == "local" || host == "localhost" {
            let mut c = Command::new("zsh");
            c.args(["-lc", cmd]);
            c
        } else {
            let mut c = Command::new("ssh");
            c.args([&host, cmd]);
            c
        };
        
        let output = command.output().map_err(|e| format!("Failed to run antigravity-usage: {}", e))?;
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.trim().is_empty() {
                return Ok(None);
            }
            
            use std::time::{SystemTime, UNIX_EPOCH};
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
            
            return Ok(Some(AgentUsageResponse {
                content: stdout.to_string(),
                fetched_at: now.to_string(),
                file_modified_at: now.to_string(),
            }));
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Don't fail the whole UI if the tool isn't installed, just return None or a soft error.
            if stderr.contains("command not found") {
                return Ok(None);
            }
            return Err(format!("Antigravity Usage Error: {}", stderr));
        }
    }
    
    Err("Unknown agent".into())
}

#[tauri::command]
fn redo_ssh_config(app: AppHandle) -> Result<String, String> {
    let app_dir = get_projects_path(&app)?.parent().unwrap().to_path_buf();
    let undo_path = app_dir.join("ssh_undo_state.txt");
    let redo_path = app_dir.join("ssh_redo_state.txt");
    
    if !redo_path.exists() {
        return Err("No redo state available".to_string());
    }
    
    let home = dirs::home_dir().ok_or("Cannot find home directory")?;
    let ssh_config_path = home.join(".ssh").join("config");
    
    // Read current config to save as undo state
    let current_content = if ssh_config_path.exists() {
        fs::read_to_string(&ssh_config_path).unwrap_or_else(|_| "".to_string())
    } else {
        "".to_string()
    };
    
    // Write undo state
    fs::write(&undo_path, current_content).map_err(|e| format!("Failed to write undo state: {}", e))?;
    
    // Read redo state
    let redo_content = fs::read_to_string(&redo_path).map_err(|e| format!("Failed to read redo state: {}", e))?;
    
    // Restore redo state to config
    fs::write(&ssh_config_path, &redo_content).map_err(|e| format!("Failed to restore SSH config: {}", e))?;
    
    // Delete redo state
    let _ = fs::remove_file(&redo_path);
    
    Ok(redo_content)
}

#[tauri::command]
fn get_project_icon_base64(local_path: String) -> Result<Option<String>, String> {
    let path = std::path::Path::new(&local_path);
    if path.join("nuxt.config.ts").exists() || path.join("nuxt.config.js").exists() {
        let possible_icons = [
            "public/favicon/favicon.ico",
            "public/favicon.ico",
            "public/favicon/icon.png",
            "public/icon.png",
            "public/favicon/apple-touch-icon.png",
            "public/favicon/icon-192.png",
            "public/favicon/icon-512-maskable.png",
        ];
        
        let mut best_icon: Option<(std::path::PathBuf, u64, &str)> = None;
        
        for icon in &possible_icons {
            let icon_path = path.join(icon);
            if let Ok(metadata) = std::fs::metadata(&icon_path) {
                let size = metadata.len();
                if let Some((_, best_size, _)) = best_icon {
                    if size < best_size {
                        best_icon = Some((icon_path, size, icon));
                    }
                } else {
                    best_icon = Some((icon_path, size, icon));
                }
            }
        }
        
        if let Some((icon_path, size, icon_name)) = best_icon {
            if size > 150_000 {
                // If even the smallest icon is too large (>150KB), ignore to prevent UI lag.
                return Ok(None);
            }
            if let Ok(bytes) = std::fs::read(&icon_path) {
                use base64::{Engine as _, engine::general_purpose};
                let b64 = general_purpose::STANDARD.encode(&bytes);
                let mime = if icon_name.ends_with(".png") { "image/png" } else { "image/x-icon" };
                return Ok(Some(format!("data:{};base64,{}", mime, b64)));
            }
        }
    }
    Ok(None)
}

#[tauri::command]
fn open_local_dir(path: String) -> Result<(), String> {
    let os_path = std::path::Path::new(&path);
    if !os_path.exists() {
        return Err("Directory does not exist".to_string());
    }
    
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg("-R")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to open finder: {}", e))?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to open directory: {}", e))?;
    }
    
    Ok(())
}

#[tauri::command]
fn open_in_terminal(path: String) -> Result<(), String> {
    let os_path = std::path::Path::new(&path);
    if !os_path.exists() {
        return Err("Directory does not exist".to_string());
    }
    
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .args(["-a", "Terminal", &path])
            .spawn()
            .map_err(|e| format!("Failed to open Terminal: {}", e))?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to open Terminal: {}", e))?;
    }
    
    Ok(())
}

#[tauri::command]
fn open_in_vscode(path: String) -> Result<(), String> {
    let os_path = std::path::Path::new(&path);
    if !os_path.exists() {
        return Err("Directory does not exist".to_string());
    }
    
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .args(["-a", "Visual Studio Code", &path])
            .spawn()
            .map_err(|e| format!("Failed to open VS Code: {}", e))?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        Command::new("code")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to open VS Code: {}", e))?;
    }
    
    Ok(())
}

#[tauri::command]
fn open_antigravity_app() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .args(["-a", "Antigravity"])
            .spawn()
            .map_err(|e| format!("Failed to open Antigravity: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
fn open_remote_terminal(host: String, path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let safe_path = if path.starts_with("~/") {
            path.replacen("~/", "$HOME/", 1)
        } else if path == "~" {
            "$HOME".to_string()
        } else {
            path
        };
        
        let script = format!(
            "tell application \"Terminal\" to do script \"ssh {} -t 'cd \\\"{}\\\"; exec bash'\"",
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_ssh_hosts,
            load_projects,
            save_projects,
            run_sync,
            get_git_info,
            get_project_files,
            read_ssh_config,
            save_ssh_config,
            undo_ssh_config,
            redo_ssh_config,
            get_ssh_history_status,
            provision_agent_usage,
            get_agent_usage,
            force_sync_agent_usage,
            get_project_icon_base64,
            open_local_dir,
            open_in_terminal,
            open_in_vscode,
            open_antigravity_app,
            open_remote_terminal,
            open_url
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
