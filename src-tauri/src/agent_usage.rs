// @docs docs/arch/usage-claudecode.md
// @docs docs/arch/usage-antigravity.md
// Updated: 2026-06-24 (v1.2.9 - added auto-probe session support)

use serde::Serialize;
use std::io::Write;
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

/// Sends `script` to `ssh host sh` via stdin and returns the combined output.
fn run_remote_script(host: &str, script: &str) -> Result<Output, String> {
    let mut child = Command::new("ssh")
        .args([host, "sh"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn SSH: {}", e))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(script.as_bytes())
            .map_err(|e| format!("Failed to write to SSH stdin: {}", e))?;
    }

    child
        .wait_with_output()
        .map_err(|e| format!("Failed to execute remote script: {}", e))
}

#[derive(Serialize)]
pub struct AgentUsageResponse {
    pub content: String,
    pub fetched_at: String,
    pub file_modified_at: String,
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[tauri::command]
pub async fn provision_agent_usage(agent_name: String, host: String) -> Result<bool, String> {
    if agent_name != "claudecode" {
        return if agent_name == "antigravity" { Ok(true) } else { Err("Unknown agent".into()) };
    }

    const SCRIPT: &str = include_str!("../../scripts/provision-claudecode.sh");
    let output = run_remote_script(&host, SCRIPT)?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Provision failed: {}", err));
    }
    Ok(true)
}

#[tauri::command]
pub async fn force_sync_agent_usage(agent_name: String, host: String) -> Result<String, String> {
    if agent_name != "claudecode" {
        return Err("Force sync not supported for this agent".into());
    }

    const SHELL_PART: &str = include_str!("../../scripts/force-sync-claudecode.sh");
    const PYTHON_PARSER: &str = include_str!("../../scripts/force-sync-parse.py");

    // Combine: run shell part to capture usage output, then run Python parser inline
    let script = format!(
        "{}\ncat << 'PYEOF' > /tmp/.claude_sync_parse.py\n{}PYEOF\npython3 /tmp/.claude_sync_parse.py\nrm -f /tmp/.claude_sync_parse.py\n",
        SHELL_PART, PYTHON_PARSER
    );

    let output = run_remote_script(&host, &script)?;

    // Intentionally not checking exit code: `claude -p /usage` exits non-zero when rate-limited,
    // but still writes to the cache file. Log stderr for diagnostics but still return Ok.
    if !output.stderr.is_empty() {
        let err = String::from_utf8_lossy(&output.stderr);
        if !err.trim().is_empty() && !err.contains("You've hit") {
            eprintln!("[force_sync] remote stderr: {}", err.trim());
        }
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        Ok("{\"parsed\":false,\"raw_preview\":\"\"}".into())
    } else {
        Ok(stdout)
    }
}

#[tauri::command]
pub async fn get_agent_usage(
    agent_name: String,
    host: String,
) -> Result<Option<AgentUsageResponse>, String> {
    if agent_name == "claudecode" {
        return get_claudecode_usage(&host);
    }

    if agent_name == "antigravity" {
        return get_antigravity_usage(&host);
    }

    Err("Unknown agent".into())
}

fn get_claudecode_usage(host: &str) -> Result<Option<AgentUsageResponse>, String> {
    const SCRIPT: &str = include_str!("../../scripts/get-claudecode-usage.sh");
    let output = run_remote_script(host, SCRIPT)?;
    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        return Ok(None);
    }

    let parts: Vec<&str> = stdout.split("|||MTIME|||").collect();
    // STALE_RESET signal: cache resets_at has passed — return None so UI shows "no data"
    if stdout.trim() == "|||STALE_RESET|||" {
        return Ok(None);
    }
    if parts.len() != 2 {
        return Ok(None);
    }

    let mtime_split: Vec<&str> = parts[1].split("|||SUBTYPE|||").collect();
    let mtime_sec = mtime_split[0].trim().parse::<i64>().unwrap_or(0);

    let content = parts[0].trim().to_string();

    let (sub_type, tier, auth_json) = if mtime_split.len() > 1 {
        let sub_split: Vec<&str> = mtime_split[1].split("|||TIER|||").collect();
        let st = sub_split[0].trim();
        let (t, auth) = if sub_split.len() > 1 {
            let tier_split: Vec<&str> = sub_split[1].split("|||AUTHINFO|||").collect();
            let tier_val = tier_split[0].trim();
            let auth_val = if tier_split.len() > 1 { tier_split[1].trim() } else { "{}" };
            (tier_val, auth_val)
        } else {
            ("Unknown", "{}")
        };
        (st, t, auth)
    } else {
        ("Unknown", "Unknown", "{}")
    };

    // Append subscription metadata + auth info (email, orgName) into the JSON object.
    let content = {
        let mut v: serde_json::Value = serde_json::from_str(&content)
            .unwrap_or(serde_json::Value::Object(Default::default()));
        if let Some(obj) = v.as_object_mut() {
            if sub_type != "Unknown" {
                obj.insert("subscriptionType".to_string(), serde_json::json!(sub_type));
            }
            if tier != "Unknown" {
                obj.insert("rateLimitTier".to_string(), serde_json::json!(tier));
            }
            if let Ok(auth) = serde_json::from_str::<serde_json::Value>(auth_json) {
                if let Some(email) = auth.get("email").and_then(|v| v.as_str()) {
                    if !email.is_empty() {
                        obj.insert("email".to_string(), serde_json::json!(email));
                    }
                }
                if let Some(org) = auth.get("orgName").and_then(|v| v.as_str()) {
                    if !org.is_empty() {
                        obj.insert("orgName".to_string(), serde_json::json!(org));
                    }
                }
            }
        }
        serde_json::to_string(&v).unwrap_or_default()
    };

    Ok(Some(AgentUsageResponse {
        content,
        fetched_at: now_secs().to_string(),
        file_modified_at: mtime_sec.to_string(),
    }))
}

fn get_antigravity_usage(host: &str) -> Result<Option<AgentUsageResponse>, String> {
    let script = include_str!("../../scripts/get-antigravity-usage.js");

    let mut command = if host == "local" || host == "localhost" {
        let mut c = Command::new("zsh");
        c.args(["-lc", "node"]);
        c.stdin(Stdio::piped())
         .stdout(Stdio::piped())
         .stderr(Stdio::piped());
        c
    } else {
        let mut c = Command::new("ssh");
        c.args([host, "node"]);
        c.stdin(Stdio::piped())
         .stdout(Stdio::piped())
         .stderr(Stdio::piped());
        c
    };

    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to spawn node: {}", e))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(script.as_bytes())
            .map_err(|e| format!("Failed to write script to node stdin: {}", e))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed to run node script: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("command not found") || stderr.contains("is not running") {
            return Ok(None);
        }
        return Err(format!("Antigravity usage error: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        return Ok(None);
    }

    let now = now_secs().to_string();
    Ok(Some(AgentUsageResponse {
        content: stdout.to_string(),
        fetched_at: now.clone(),
        file_modified_at: now,
    }))
}


