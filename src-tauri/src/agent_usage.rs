// @docs docs/arch/usage-claudecode.md
// @docs docs/arch/usage-antigravity.md
// Updated: 2026-06-25 (v1.3.3 logging + SSH-script resilience: timeout + error surfacing + log levels)

use crate::logger;
use serde::Serialize;
use std::io::{Read, Write};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Hard ceiling for any `ssh host sh` call. A hung `claude -p` probe (network/API stall)
/// must never wedge the UI in a permanent "loading" state — Layer 4 of the SSH-script
/// resilience design (see docs/arch/usage-claudecode.md §3c).
const REMOTE_SCRIPT_TIMEOUT_SECS: u64 = 30;

/// Sends `script` to `ssh host sh` via stdin and returns the combined output.
fn run_remote_script(host: &str, script: &str) -> Result<Output, String> {
    run_remote_script_timeout(host, script, REMOTE_SCRIPT_TIMEOUT_SECS)
}

/// Like [`run_remote_script`] but kills the remote process if it overruns `timeout_secs`,
/// returning an explicit timeout error instead of blocking forever.
fn run_remote_script_timeout(host: &str, script: &str, timeout_secs: u64) -> Result<Output, String> {
    let mut child = Command::new("ssh")
        .args([host, "sh"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn SSH: {}", e))?;

    // Drain stdout/stderr on dedicated threads BEFORE writing stdin, so a large script
    // can't deadlock against a full output pipe that ssh isn't draining yet.
    let mut out_pipe = child.stdout.take().ok_or("SSH stdout pipe missing")?;
    let mut err_pipe = child.stderr.take().ok_or("SSH stderr pipe missing")?;
    let out_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = out_pipe.read_to_end(&mut buf);
        buf
    });
    let err_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = err_pipe.read_to_end(&mut buf);
        buf
    });

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(script.as_bytes())
            .map_err(|e| format!("Failed to write to SSH stdin: {}", e))?;
        // stdin dropped here → closes the pipe so the remote shell sees EOF
    }

    // Poll for completion with a hard timeout; kill on overrun.
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {
                if start.elapsed() >= Duration::from_secs(timeout_secs) {
                    let _ = child.kill();
                    let _ = child.wait();
                    // Killing the local SSH client leaves the remote `claude -p` running as
                    // an orphan — wastes quota and can create unintended sessions. Fire a
                    // best-effort pkill on the remote to clean up. Fire-and-forget: the
                    // cleanup result does not affect this error path.
                    let host_cleanup = host.to_string();
                    std::thread::spawn(move || {
                        let _ = Command::new("ssh")
                            .args([host_cleanup.as_str(), "pkill", "-f", "claude -p"])
                            .output();
                    });
                    return Err(format!(
                        "remote script timed out after {}s (local killed, remote cleanup fired) host={}",
                        timeout_secs, host
                    ));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(format!("Failed to poll remote script: {}", e)),
        }
    };

    let stdout = out_handle.join().unwrap_or_default();
    let stderr = err_handle.join().unwrap_or_default();
    Ok(Output { status, stdout, stderr })
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

/// Truncate a string for safe log preview (no newlines, bounded length).
fn preview(s: &str, max: usize) -> String {
    let s = s.trim();
    let s = if s.len() > max {
        // Cut at a char boundary at/below `max` so multi-byte UTF-8 (e.g.
        // Vietnamese session names in the cached JSON) never panics.
        let mut end = max;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[..end]
    } else {
        s
    };
    s.replace('\n', "↵").replace('\r', "")
}

#[tauri::command]
pub async fn provision_agent_usage(agent_name: String, host: String) -> Result<bool, String> {
    logger::info("PROVISION", &format!("agent={} host={}", agent_name, host));

    if agent_name != "claudecode" {
        logger::debug("PROVISION", &format!("skip agent={} (not claudecode)", agent_name));
        return if agent_name == "antigravity" { Ok(true) } else { Err("Unknown agent".into()) };
    }

    const SCRIPT: &str = include_str!("../../scripts/provision-claudecode.sh");
    let output = run_remote_script(&host, SCRIPT)?;
    let ok = output.status.success();
    logger::info("PROVISION", &format!(
        "exit={} ok={}",
        output.status.code().unwrap_or(-1),
        ok
    ));
    if !ok {
        let err = String::from_utf8_lossy(&output.stderr);
        let err_preview = preview(&err, 200);
        logger::error("PROVISION", &format!("stderr={}", err_preview));
        return Err(format!("Provision failed: {}", err));
    }
    Ok(true)
}

#[tauri::command]
pub async fn force_sync_agent_usage(agent_name: String, host: String) -> Result<String, String> {
    logger::info("FORCE_SYNC", &format!("─── start agent={} host={}", agent_name, host));

    if agent_name != "claudecode" {
        logger::debug("FORCE_SYNC", "SKIP: not supported for this agent");
        return Err("Force sync not supported for this agent".into());
    }

    const SHELL_PART: &str = include_str!("../../scripts/force-sync-claudecode.sh");
    const PYTHON_PARSER: &str = include_str!("../../scripts/force-sync-parse.py");

    // Combine: run shell part (exports CLAUDE_SYNC_OUT), then run Python parser inline.
    // exit code is NOT checked — `claude -p /usage` may exit non-zero when rate-limited
    // but still writes to the cache file; stderr contains expected rate-limit messages.
    let script = format!(
        "{}\ncat << 'PYEOF' > /tmp/.claude_sync_parse.py\n{}PYEOF\npython3 /tmp/.claude_sync_parse.py\nrm -f /tmp/.claude_sync_parse.py\n",
        SHELL_PART, PYTHON_PARSER
    );

    logger::debug("FORCE_SYNC", "launching SSH script (shell + python parser)...");
    let output = run_remote_script(&host, &script)?;

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr);

    logger::debug("FORCE_SYNC", &format!(
        "ssh_result: exit={} stdout_bytes={} stderr_bytes={}",
        exit_code, stdout.len(), stderr.len()
    ));

    // Relay all shell stderr lines — they carry [SHELL:force-sync] diagnostic entries.
    log_shell_stderr("FORCE_SYNC", &stderr);

    // stdout is the JSON diagnostic from force-sync-parse.py. Empty stdout means the
    // parser never even ran (the remote shell died early — e.g. a bashism on a dash
    // remote, claude unavailable, or an SSH failure). This is a HARD failure: surface
    // it to the UI so it shows an error and the JS retry logic kicks in, instead of the
    // old behaviour of returning a silent `parsed:false` that masked the dash/pipefail
    // regression for many versions (see docs/research/claude-usage-dash-pipefail-regression.md).
    if stdout.is_empty() {
        logger::error("FORCE_SYNC", &format!(
            "empty stdout (exit={} stderr_bytes={}) — parser produced nothing; \
             remote shell likely died early (bashism on dash? claude missing? ssh fail?)",
            exit_code, stderr.len()
        ));
        logger::error("FORCE_SYNC", "─── done → FAILED (empty stdout)");
        return Err(format!(
            "force-sync produced no output (exit={}). The remote script may have died early — \
             check the [FORCE_SYNC] shell lines in usage.log.",
            exit_code
        ));
    }

    logger::debug("FORCE_SYNC", &format!("diagnostic_raw: {}", preview(&stdout, 500)));

    // Parse and log each field of the diagnostic JSON individually.
    match serde_json::from_str::<serde_json::Value>(&stdout) {
        Ok(diag) => {
            let parsed     = diag.get("parsed").and_then(|v| v.as_bool()).unwrap_or(false);
            let written    = diag.get("written").and_then(|v| v.as_bool()).unwrap_or(false);
            let pct        = diag.get("pct").and_then(|v| v.as_i64());
            let resets_at  = diag.get("resets_at").and_then(|v| v.as_i64()).unwrap_or(0);
            let raw_len    = diag.get("raw_len").and_then(|v| v.as_u64()).unwrap_or(0);
            let year_fix   = diag.get("year_fix_applied").and_then(|v| v.as_bool()).unwrap_or(false);
            let overdue    = diag.get("resets_at_overdue_s").and_then(|v| v.as_i64());
            let parse_err  = diag.get("parse_error").and_then(|v| v.as_str()).unwrap_or("");
            let raw_prev   = diag.get("raw_preview").and_then(|v| v.as_str()).unwrap_or("");

            logger::info("FORCE_SYNC", &format!(
                "diagnostic: parsed={} written={} pct={:?} resets_at={} raw_len={} year_fix={} overdue_s={:?}",
                parsed, written, pct, resets_at, raw_len, year_fix, overdue
            ));

            if !parse_err.is_empty() {
                logger::error("FORCE_SYNC", &format!("diagnostic: parse_error={}", parse_err));
            }
            if !raw_prev.is_empty() {
                logger::debug("FORCE_SYNC", &format!("diagnostic: raw_preview={}", preview(raw_prev, 300)));
            }

            if year_fix {
                let from = diag.get("year_fix_from").and_then(|v| v.as_i64()).unwrap_or(0);
                let to   = diag.get("year_fix_to").and_then(|v| v.as_i64()).unwrap_or(0);
                logger::info("FORCE_SYNC", &format!(
                    "diagnostic: YEAR_FIX applied resets_at {} → {} (was >1h in past, pushed to next year)",
                    from, to
                ));
            }

            if !parsed {
                logger::error("FORCE_SYNC", "parser did not parse — cache NOT updated");
            } else if !written {
                logger::error("FORCE_SYNC", "parsed ok but cache write FAILED");
                if let Some(we) = diag.get("write_error").and_then(|v| v.as_str()) {
                    logger::error("FORCE_SYNC", &format!("diagnostic: write_error={}", we));
                }
            } else {
                let now = now_secs();
                let until_reset = resets_at - now;
                logger::info("FORCE_SYNC", &format!(
                    "─── done → SUCCESS: cache updated pct={:?} resets_at={} until_reset_s={}",
                    pct, resets_at, until_reset
                ));
            }
        }
        Err(e) => {
            logger::error("FORCE_SYNC", &format!(
                "stdout is not valid JSON err={} raw_preview={}",
                e, preview(&stdout, 200)
            ));
        }
    }

    logger::debug("FORCE_SYNC", "─── done → returning diagnostic to JS");
    Ok(stdout)
}

#[tauri::command]
pub async fn get_agent_usage(
    agent_name: String,
    host: String,
) -> Result<Option<AgentUsageResponse>, String> {
    // Both inner fns are fully synchronous (wait_with_output, thread::sleep).
    // Running them directly on the async executor starves it and freezes the UI.
    // spawn_blocking offloads to the Tauri blocking thread-pool.
    tauri::async_runtime::spawn_blocking(move || {
        if agent_name == "claudecode" {
            return get_claudecode_usage(&host);
        }
        if agent_name == "antigravity" {
            return get_antigravity_usage(&host);
        }
        Err("Unknown agent".into())
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

/// Emit each non-empty shell stderr line at debug level.
fn log_shell_stderr(tag: &str, stderr: &str) {
    let lines: Vec<&str> = stderr.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        logger::debug(tag, "shell_stderr: (empty)");
        return;
    }
    logger::debug(tag, &format!("shell_stderr: {} lines follow", lines.len()));
    for line in lines {
        logger::debug(tag, &format!("  shell| {}", line));
    }
}

fn get_claudecode_usage(host: &str) -> Result<Option<AgentUsageResponse>, String> {
    logger::debug("GET_USAGE", &format!("─── start host={}", host));

    const SCRIPT: &str = include_str!("../../scripts/get-claudecode-usage.sh");
    let output = run_remote_script(host, SCRIPT)?;

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    logger::debug("GET_USAGE", &format!(
        "ssh_result: exit={} stdout_bytes={} stderr_bytes={}",
        exit_code, stdout.len(), stderr.len()
    ));

    log_shell_stderr("GET_USAGE", &stderr);

    if !output.status.success() {
        logger::error("GET_USAGE", &format!("FAIL: shell exit={} → returning None", exit_code));
        return Ok(None);
    }

    if stdout.trim().is_empty() {
        logger::info("GET_USAGE", "no cache file on remote → None");
        return Ok(None);
    }

    // STALE_RESET signal
    if stdout.trim() == "|||STALE_RESET|||" {
        logger::info("GET_USAGE", "STALE_RESET: resets_at is in the past → None (JS will forceSync)");
        return Ok(None);
    }

    logger::debug("GET_USAGE", &format!("stdout_preview: {}", preview(&stdout, 300)));

    // ── Parse delimiter chain ─────────────────────────────────────────────
    // Expected: <json>|||MTIME|||<ts>|||SUBTYPE|||<st>|||TIER|||<tier>|||AUTHINFO|||<json>

    let parts: Vec<&str> = stdout.split("|||MTIME|||").collect();
    logger::debug("GET_USAGE", &format!("parse MTIME split: {} parts (expected 2)", parts.len()));
    if parts.len() != 2 {
        logger::error("GET_USAGE", "MTIME delimiter missing in stdout → None");
        return Ok(None);
    }

    let content_raw = parts[0].trim();
    let after_mtime = parts[1];

    let mtime_split: Vec<&str> = after_mtime.split("|||SUBTYPE|||").collect();
    let mtime_sec = mtime_split[0].trim().parse::<i64>().unwrap_or(0);
    logger::debug("GET_USAGE", &format!(
        "parse mtime_sec={} (raw='{}') subtype_parts={}",
        mtime_sec,
        mtime_split[0].trim(),
        mtime_split.len()
    ));

    let (sub_type, tier, auth_json) = if mtime_split.len() > 1 {
        let sub_split: Vec<&str> = mtime_split[1].split("|||TIER|||").collect();
        let st = sub_split[0].trim();
        logger::debug("GET_USAGE", &format!(
            "parse subtype='{}' tier_parts={}",
            st, sub_split.len()
        ));
        let (t, auth) = if sub_split.len() > 1 {
            let tier_split: Vec<&str> = sub_split[1].split("|||AUTHINFO|||").collect();
            let tier_val = tier_split[0].trim();
            let auth_val = if tier_split.len() > 1 { tier_split[1].trim() } else { "{}" };
            logger::debug("GET_USAGE", &format!(
                "parse tier='{}' authinfo_len={} authinfo_parts={}",
                tier_val, auth_val.len(), tier_split.len()
            ));
            (tier_val, auth_val)
        } else {
            logger::debug("GET_USAGE", "parse TIER delimiter missing → tier=Unknown auth={}");
            ("Unknown", "{}")
        };
        (st, t, auth)
    } else {
        logger::debug("GET_USAGE", "parse SUBTYPE delimiter missing → subtype/tier/auth all Unknown");
        ("Unknown", "Unknown", "{}")
    };

    // ── JSON parse of cache content ───────────────────────────────────────
    let content_len = content_raw.len();
    let mut v: serde_json::Value = match serde_json::from_str(content_raw) {
        Ok(val) => {
            logger::debug("GET_USAGE", &format!("json_parse: ok content_len={}", content_len));
            val
        }
        Err(e) => {
            logger::error("GET_USAGE", &format!(
                "json_parse FAILED content_len={} err={} → using empty object",
                content_len, e
            ));
            serde_json::Value::Object(Default::default())
        }
    };

    // ── Inject metadata ───────────────────────────────────────────────────
    if let Some(obj) = v.as_object_mut() {
        if sub_type != "Unknown" {
            obj.insert("subscriptionType".to_string(), serde_json::json!(sub_type));
        }
        if tier != "Unknown" {
            obj.insert("rateLimitTier".to_string(), serde_json::json!(tier));
        }
        match serde_json::from_str::<serde_json::Value>(auth_json) {
            Ok(auth) => {
                let email = auth.get("email").and_then(|v| v.as_str()).unwrap_or("");
                let org   = auth.get("orgName").and_then(|v| v.as_str()).unwrap_or("");
                logger::debug("GET_USAGE", &format!(
                    "auth_inject: email='{}' org='{}'",
                    if email.is_empty() { "(none)" } else { email },
                    if org.is_empty() { "(none)" } else { org }
                ));
                if !email.is_empty() { obj.insert("email".to_string(), serde_json::json!(email)); }
                if !org.is_empty()   { obj.insert("orgName".to_string(), serde_json::json!(org)); }
            }
            Err(e) => {
                logger::error("GET_USAGE", &format!(
                    "auth_inject FAILED to parse authinfo json err={} authinfo_preview={}",
                    e, preview(auth_json, 100)
                ));
            }
        }
    }

    // ── Rate limits summary ───────────────────────────────────────────────
    if let Some(obj) = v.as_object() {
        let now = now_secs();
        let five_h = obj.get("rate_limits")
            .and_then(|r| r.get("five_hour"))
            .map(|fh| {
                let pct      = fh.get("used_percentage").and_then(|v| v.as_i64()).unwrap_or(-1);
                let resets   = fh.get("resets_at").and_then(|v| v.as_i64()).unwrap_or(0);
                let overdue  = now - resets;
                let state    = if resets == 0 { "no_reset" } else if overdue > 0 { "PAST" } else { "future" };
                format!("pct={} resets_at={} overdue_s={} state={}", pct, resets, overdue, state)
            })
            .unwrap_or_else(|| "MISSING".to_string());
        let seven_d = obj.get("rate_limits")
            .and_then(|r| r.get("seven_day"))
            .map(|sd| {
                let pct    = sd.get("used_percentage").and_then(|v| v.as_i64()).unwrap_or(-1);
                let resets = sd.get("resets_at").and_then(|v| v.as_i64()).unwrap_or(0);
                format!("pct={} resets_at={}", pct, resets)
            })
            .unwrap_or_else(|| "absent".to_string());
        logger::debug("GET_USAGE", &format!(
            "rate_limits: five_hour=[{}]  seven_day=[{}]  server_now={}",
            five_h, seven_d, now
        ));
    }

    let content = serde_json::to_string(&v).unwrap_or_default();
    logger::debug("GET_USAGE", &format!(
        "─── done → Ok(Some) mtime={} content_bytes={}",
        mtime_sec, content.len()
    ));
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
