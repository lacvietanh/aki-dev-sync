// @docs docs/arch/usage-claudecode.md
// @docs docs/arch/usage-antigravity.md
// Updated: 2026-06-25 (v1.3.3 logging + SSH-script resilience: timeout + error surfacing + log levels)

use crate::logger;
use serde::Serialize;
use std::collections::HashSet;
use std::io::{Read, Write};
use std::process::{Command, Output, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Hard ceiling for any `ssh host sh` call. A hung `claude -p` probe (network/API stall)
/// must never wedge the UI in a permanent "loading" state — Layer 4 of the SSH-script
/// resilience design (see docs/arch/usage-claudecode.md §3c).
const REMOTE_SCRIPT_TIMEOUT_SECS: u64 = 30;

/// Sends `script` to `ssh host sh` via stdin and returns the combined output.
pub(crate) fn run_remote_script(host: &str, script: &str) -> Result<Output, String> {
    run_remote_script_timeout(host, script, REMOTE_SCRIPT_TIMEOUT_SECS)
}

fn is_local_host(host: &str) -> bool {
    host == "local" || host == "localhost"
}

/// Prepended to every script sent through [`run_remote_script_timeout`], local or remote.
/// Resolves a `claude` binary path into `$CLAUDE_BIN` via static, deterministic file checks
/// BEFORE falling back to PATH/login-shell lookup.
///
/// WHY: force-sync/provision were seen failing with `exit=127 command not found: claude`
/// inside `zsh -lc`/`bash -lc`, seconds after this app's own cold start, then succeeding
/// again minutes later with the identical command — a PATH race against the user's shell
/// rc/profile (nvm, path_helper, etc.) not having finished sourcing yet at that exact
/// moment. A `[ -x "$path" ]` file-existence test has no dependency on rc-sourcing timing,
/// so trying known install locations first structurally removes the race instead of
/// patching each call site that happens to invoke `claude` today.
///
/// NOTE: mac-only path list for now — this app currently ships for macOS only (see
/// CLAUDE.md). If a Linux/Windows build ships later, extend the list below.
const CLAUDE_BIN_RESOLVER_PREAMBLE: &str = r#"
_resolve_claude_bin() {
    for _c in "$HOME/.local/bin/claude" "$HOME/.claude/local/claude" \
              /opt/homebrew/bin/claude /usr/local/bin/claude; do
        [ -x "$_c" ] && { printf '%s' "$_c"; return; }
    done
    command -v claude 2>/dev/null && return
    if command -v zsh >/dev/null 2>&1; then
        zsh -lc 'command -v claude' 2>/dev/null && return
    fi
    bash -lc 'command -v claude' 2>/dev/null
}
CLAUDE_BIN=$(_resolve_claude_bin)
[ -z "$CLAUDE_BIN" ] && CLAUDE_BIN=claude
export CLAUDE_BIN
"#;

/// Like [`run_remote_script`] but kills the remote process if it overruns `timeout_secs`,
/// returning an explicit timeout error instead of blocking forever.
///
/// `host == "local"`/`"localhost"` runs `script` through a local `sh` instead of SSH — this
/// is how Claude Code usage is monitored when it runs on the same machine as this app, no
/// remote involved. The scripts under scripts/*.sh are pure POSIX sh against `$HOME`, so they
/// work identically local or remote.
fn run_remote_script_timeout(host: &str, script: &str, timeout_secs: u64) -> Result<Output, String> {
    let local = is_local_host(host);
    let mut child = if local {
        Command::new("sh")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn local sh: {}", e))?
    } else {
        Command::new("ssh")
            .args([host, "sh"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn SSH: {}", e))?
    };

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
            .write_all(CLAUDE_BIN_RESOLVER_PREAMBLE.as_bytes())
            .map_err(|e| format!("Failed to write to SSH stdin: {}", e))?;
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
                    // best-effort pkill to clean it up (over SSH for a remote host, or directly
                    // for the local machine). Fire-and-forget: the cleanup result does not
                    // affect this error path.
                    let host_cleanup = host.to_string();
                    std::thread::spawn(move || {
                        let _ = if local {
                            Command::new("pkill").args(["-f", "claude -p"]).output()
                        } else {
                            Command::new("ssh")
                                .args([host_cleanup.as_str(), "pkill", "-f", "claude -p"])
                                .output()
                        };
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
    s.replace('\n', "\u{21b5}").replace('\r', "")
}

/// Returns "CC" for claudecode, "AG" for antigravity, or the agent name as-is.
#[inline]
fn ab(agent: &str) -> &str {
    match agent {
        "claudecode"  => "CC",
        "antigravity" => "AG",
        other         => other,
    }
}

#[tauri::command]
pub async fn provision_agent_usage(agent_name: String, host: String) -> Result<bool, String> {
    logger::info("PROVISION", &format!("{} host={}", ab(&agent_name), host));

    if agent_name != "claudecode" {
        logger::debug("PROVISION", &format!("skip {}", ab(&agent_name)));
        return if agent_name == "antigravity" { Ok(true) } else { Err("Unknown agent".into()) };
    }

    const SCRIPT: &str = include_str!("../../scripts/provision-claudecode.sh");
    let output = run_remote_script(&host, SCRIPT)?;
    let ok = output.status.success();
    logger::info("PROVISION", &format!("exit={} ok={}", output.status.code().unwrap_or(-1), ok));
    let err = String::from_utf8_lossy(&output.stderr);
    if !ok {
        let err_preview = preview(&err, 200);
        logger::error("PROVISION", &format!("stderr={}", err_preview));
        return Err(format!("Provision failed: {}", err));
    }
    // The script now always exits 0 (auth caching is best-effort), but a non-empty stderr still
    // carries the [SHELL:provision] empty-auth diagnostic — a real signal correlated with Bug B
    // (empty /usage). Log it at ERROR so it lands in usage.log even in production (no --debug).
    if !err.trim().is_empty() {
        logger::error("PROVISION", &format!("stderr (non-fatal)={}", preview(&err, 200)));
    }
    Ok(true)
}

#[tauri::command]
pub async fn force_sync_agent_usage(agent_name: String, host: String) -> Result<String, String> {
    logger::info("FORCE_SYNC", &format!("{} host={}", ab(&agent_name), host));

    if agent_name != "claudecode" {
        logger::debug("FORCE_SYNC", &format!("skip {}", ab(&agent_name)));
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

    logger::debug("FORCE_SYNC", "launching");
    let output = run_remote_script(&host, &script)?;

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr);

    logger::debug("FORCE_SYNC", &format!(
        "exit={} stdout_b={} stderr_b={}",
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
            "empty stdout exit={} stderr_b={}",
            exit_code, stderr.len()
        ));
        logger::error("FORCE_SYNC", "status=FAILED empty stdout");
        log_shell_stderr_error("FORCE_SYNC", &stderr);
        return Err(format!(
            "force-sync produced no output (exit={}). The remote script may have died early — \
             check the [FORCE_SYNC] shell lines in usage.log.",
            exit_code
        ));
    }

    logger::debug("FORCE_SYNC", &format!("diag_raw: {}", preview(&stdout, 500)));

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
                "diag parsed={} written={} pct={:?} resets_at={} raw_len={} year_fix={} overdue_s={:?}",
                parsed, written, pct, resets_at, raw_len, year_fix, overdue
            ));

            if !parse_err.is_empty() {
                logger::error("FORCE_SYNC", &format!("parse_error={}", parse_err));
            }
            if !raw_prev.is_empty() {
                logger::debug("FORCE_SYNC", &format!("raw_preview={}", preview(raw_prev, 300)));
            }

            if year_fix {
                let from = diag.get("year_fix_from").and_then(|v| v.as_i64()).unwrap_or(0);
                let to   = diag.get("year_fix_to").and_then(|v| v.as_i64()).unwrap_or(0);
                logger::info("FORCE_SYNC", &format!("year_fix from={} to={}", from, to));
            }

            if !parsed {
                logger::error("FORCE_SYNC", "parse failed: cache not updated");
                // raw_preview is often empty here (that IS the bug — `/usage` returned nothing),
                // so the shell stderr is the only clue to WHY. Surface both at error level.
                if !raw_prev.is_empty() {
                    logger::error("FORCE_SYNC", &format!("raw_preview={}", preview(raw_prev, 300)));
                }
                log_shell_stderr_error("FORCE_SYNC", &stderr);
            } else if !written {
                logger::error("FORCE_SYNC", "write failed");
                if let Some(we) = diag.get("write_error").and_then(|v| v.as_str()) {
                    logger::error("FORCE_SYNC", &format!("write_error={}", we));
                }
            } else {
                let now = now_secs();
                let until_reset = resets_at - now;
                logger::info("FORCE_SYNC", &format!(
                    "done: ok pct={:?} resets_at={} until_s={}",
                    pct, resets_at, until_reset
                ));
            }
        }
        Err(e) => {
            logger::error("FORCE_SYNC", &format!("json_parse err={} raw={}", e, preview(&stdout, 200)));
            log_shell_stderr_error("FORCE_SYNC", &stderr);
        }
    }

    logger::debug("FORCE_SYNC", "done: returning diag");
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
        logger::debug(tag, "stderr: (empty)");
        return;
    }
    logger::debug(tag, &format!("stderr: {} lines", lines.len()));
    for line in lines {
        logger::debug(tag, &format!("  | {}", line));
    }
}

/// Emit shell stderr at ERROR level — used only on FORCE_SYNC failure paths so the diagnostic
/// (e.g. `run_usage: EMPTY stdout — claude stderr=…`, the one clue to WHY `/usage` was empty)
/// lands in usage.log even in production, where info/debug are suppressed. Do not call on the
/// happy path — that would spam the file on every successful sync.
fn log_shell_stderr_error(tag: &str, stderr: &str) {
    let lines: Vec<&str> = stderr.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        logger::error(tag, "shell stderr on failure: (empty — the remote wrote nothing to stderr)");
        return;
    }
    logger::error(tag, &format!("shell stderr on failure: {} lines", lines.len()));
    for line in lines {
        logger::error(tag, &format!("  | {}", line));
    }
}

/// True the first time it's called for a given host in this app process, false after —
/// used to force one bypass of the auth-cache TTL right after app launch (see
/// `AKI_FORCE_AUTH_REFRESH` in get-claudecode-usage.sh). A CC account switch is rare and
/// happens outside the app, so there's no reliable in-app event to hook; "app was just
/// opened" is the one moment a stale cached email is most likely to be noticed and easiest
/// to guarantee correct, without adding any extra polling.
fn cc_auth_force_needed(host: &str) -> bool {
    static SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let seen = SEEN.get_or_init(|| Mutex::new(HashSet::new()));
    seen.lock().unwrap().insert(host.to_string())
}

fn get_claudecode_usage(host: &str) -> Result<Option<AgentUsageResponse>, String> {
    logger::debug("GET_USAGE", &format!("start host={}", host));

    const SCRIPT: &str = include_str!("../../scripts/get-claudecode-usage.sh");
    let force_auth = cc_auth_force_needed(host);
    let script_owned;
    let script: &str = if force_auth {
        logger::info("GET_USAGE", "first check this session — forcing auth refresh (bypass cache TTL)");
        script_owned = format!("AKI_FORCE_AUTH_REFRESH=1\n{}", SCRIPT);
        &script_owned
    } else {
        SCRIPT
    };
    let output = run_remote_script(host, script)?;

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    logger::debug("GET_USAGE", &format!(
        "exit={} stdout_b={} stderr_b={}",
        exit_code, stdout.len(), stderr.len()
    ));

    log_shell_stderr("GET_USAGE", &stderr);

    if !output.status.success() {
        logger::error("GET_USAGE", &format!("shell exit={}", exit_code));
        return Ok(None);
    }

    if stdout.trim().is_empty() {
        logger::info("GET_USAGE", "null: no cache");
        return Ok(None);
    }

    // STALE_RESET signal
    if stdout.trim() == "|||STALE_RESET|||" {
        logger::info("GET_USAGE", "null: STALE_RESET");
        return Ok(None);
    }

    logger::debug("GET_USAGE", &format!("stdout: {}", preview(&stdout, 300)));

    // ── Parse delimiter chain ─────────────────────────────────────────────
    // Expected: <json>|||MTIME|||<ts>|||SUBTYPE|||<st>|||TIER|||<tier>|||AUTHINFO|||<json>

    let parts: Vec<&str> = stdout.split("|||MTIME|||").collect();
    logger::debug("GET_USAGE", &format!("mtime_parts={}", parts.len()));
    if parts.len() != 2 {
        logger::error("GET_USAGE", "no MTIME delimiter");
        return Ok(None);
    }

    let content_raw = parts[0].trim();
    let after_mtime = parts[1];

    let mtime_split: Vec<&str> = after_mtime.split("|||SUBTYPE|||").collect();
    let mtime_sec = mtime_split[0].trim().parse::<i64>().unwrap_or(0);
    logger::debug("GET_USAGE", &format!("mtime={} subtype_parts={}", mtime_sec, mtime_split.len()));

    let (sub_type, tier, auth_json) = if mtime_split.len() > 1 {
        let sub_split: Vec<&str> = mtime_split[1].split("|||TIER|||").collect();
        let st = sub_split[0].trim();
        logger::debug("GET_USAGE", &format!("subtype='{}' tier_parts={}", st, sub_split.len()));
        let (t, auth) = if sub_split.len() > 1 {
            let tier_split: Vec<&str> = sub_split[1].split("|||AUTHINFO|||").collect();
            let tier_val = tier_split[0].trim();
            let auth_val = if tier_split.len() > 1 { tier_split[1].trim() } else { "{}" };
            logger::debug("GET_USAGE", &format!("tier='{}' authinfo_b={}", tier_val, auth_val.len()));
            (tier_val, auth_val)
        } else {
            logger::debug("GET_USAGE", "no TIER delimiter");
            ("Unknown", "{}")
        };
        (st, t, auth)
    } else {
        logger::debug("GET_USAGE", "no SUBTYPE delimiter");
        ("Unknown", "Unknown", "{}")
    };

    // ── JSON parse of cache content ───────────────────────────────────────
    let content_len = content_raw.len();
    let mut v: serde_json::Value = match serde_json::from_str(content_raw) {
        Ok(val) => {
            logger::debug("GET_USAGE", &format!("json_ok b={}", content_len));
            val
        }
        Err(e) => {
            logger::error("GET_USAGE", &format!("json_parse err={} b={}", e, content_len));
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
                logger::debug("GET_USAGE", &format!("auth email='{}' org='{}'", email, org));
                if !email.is_empty() { obj.insert("email".to_string(), serde_json::json!(email)); }
                if !org.is_empty()   { obj.insert("orgName".to_string(), serde_json::json!(org)); }
            }
            Err(e) => {
                logger::error("GET_USAGE", &format!("auth_parse err={} preview={}", e, preview(auth_json, 100)));
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
        logger::debug("GET_USAGE", &format!("rl 5h=[{}] 7d=[{}]", five_h, seven_d));
    }

    let content = serde_json::to_string(&v).unwrap_or_default();
    logger::debug("GET_USAGE", &format!("done mtime={} b={}", mtime_sec, content.len()));
    Ok(Some(AgentUsageResponse {
        content,
        fetched_at: now_secs().to_string(),
        file_modified_at: mtime_sec.to_string(),
    }))
}

fn get_antigravity_usage(host: &str) -> Result<Option<AgentUsageResponse>, String> {
    logger::debug("USAGE:antigravity", &format!("start host={}", host));

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

    let exit_code = output.status.code().unwrap_or(-1);
    logger::debug("USAGE:antigravity", &format!(
        "exit={} stdout_b={} stderr_b={}",
        exit_code, output.stdout.len(), output.stderr.len()
    ));

    if !output.status.success() {
        // Every non-zero exit here is a *transient monitor* condition, never a user-facing
        // fault: the IDE isn't running, is mid-restart, hasn't opened its Connect port yet,
        // was just signed out, or a single localhost RPC probe timed out. To the UI they all
        // mean the same thing — "no live reading this poll" — and the frontend already handles
        // that (composable null path shows the last cached account). Surfacing any of them as
        // an IPC Err only produced a flickering error banner every poll: that WAS the usage
        // instability. So swallow all AG script failures to Ok(None); just log the reason.
        let stderr = String::from_utf8_lossy(&output.stderr);
        logger::debug("USAGE:antigravity", &format!("soft-miss: {}", stderr.trim()));
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        logger::debug("USAGE:antigravity", "done: null empty stdout");
        return Ok(None);
    }

    let now = now_secs().to_string();
    logger::debug("USAGE:antigravity", &format!("done: ok b={}", stdout.len()));
    Ok(Some(AgentUsageResponse {
        content: stdout.to_string(),
        fetched_at: now.clone(),
        file_modified_at: now,
    }))
}

/// Must match the actual /Applications/*.app bundle name — used for `osascript quit app`,
/// `pkill`, the Application Support folder name, and the "<name> Safe Storage" Keychain item.
const ANTIGRAVITY_APP_NAME: &str = "Antigravity IDE";

/// Electron userData files that hold only the logged-in web session (cookies, chromium
/// local/session storage, network identity state) — deleting these is equivalent to a
/// browser "sign out", while leaving User/ (settings, keybindings, snippets, extensions,
/// workspaceStorage) and globalStorage/ (extension state incl. rules/permissions) untouched.
const ANTIGRAVITY_ACCOUNT_ONLY_PATHS: &[&str] = &[
    "Cookies",
    "Cookies-journal",
    "Local Storage",
    "Session Storage",
    "Network Persistent State",
    "DIPS",
    "DIPS-wal",
    "TransportSecurity",
    "Trust Tokens",
    "Trust Tokens-journal",
];

fn antigravity_support_dir() -> Result<std::path::PathBuf, String> {
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| "Could not resolve home directory".to_string())?;
    let home = std::path::PathBuf::from(home);

    #[cfg(target_os = "macos")]
    {
        Ok(home.join("Library/Application Support").join(ANTIGRAVITY_APP_NAME))
    }
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA").map_err(|_| "APPDATA not set".to_string())?;
        Ok(std::path::PathBuf::from(appdata).join(ANTIGRAVITY_APP_NAME))
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        Ok(home.join(".config").join(ANTIGRAVITY_APP_NAME))
    }
}

/// globalState keys that hold the live Antigravity OAuth session. Deleting these forces a
/// real re-login while leaving every other globalState row (settings, extension state) intact.
const ANTIGRAVITY_AUTH_KEYS: &[&str] = &[
    "antigravityUnifiedStateSync.oauthToken",
    "antigravityUnifiedStateSync.userStatus",
];

/// Delete the OAuth session rows from `User/globalStorage/state.vscdb` (and `.backup`) via the
/// system `sqlite3`. Best-effort: any failure (no sqlite3, file absent) is a silent no-op so a
/// partial logout still wipes cookies + Keychain. Must be called only after the IDE is quit.
fn remove_antigravity_auth_rows(base: &std::path::Path) {
    let where_in = ANTIGRAVITY_AUTH_KEYS
        .iter()
        .map(|k| format!("'{}'", k))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!("DELETE FROM ItemTable WHERE key IN ({});", where_in);

    for db_name in ["state.vscdb", "state.vscdb.backup"] {
        let db = base.join("User/globalStorage").join(db_name);
        if !db.is_file() {
            continue;
        }
        let out = Command::new("sqlite3")
            .arg(&db)
            .arg(&sql)
            .output();
        match out {
            Ok(o) if o.status.success() => {
                logger::info("LOGOUT:antigravity", &format!("cleared {}", db_name));
            }
            Ok(o) => {
                let err = String::from_utf8_lossy(&o.stderr);
                logger::error("LOGOUT:antigravity", &format!("sqlite3 failed on {}: {}", db_name, err.trim()));
            }
            Err(e) => {
                logger::error("LOGOUT:antigravity", &format!("could not run sqlite3 on {}: {}", db_name, e));
            }
        }
    }
}

#[tauri::command]
pub async fn logout_antigravity() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        // Quit the app first so Chromium isn't holding these files open while we delete them.
        #[cfg(target_os = "macos")]
        {
            let _ = Command::new("osascript")
                .args(["-e", &format!(r#"quit app "{}""#, ANTIGRAVITY_APP_NAME)])
                .output();
            std::thread::sleep(Duration::from_millis(800));
            let _ = Command::new("pkill").args(["-f", ANTIGRAVITY_APP_NAME]).output();
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = Command::new("pkill").args(["-f", ANTIGRAVITY_APP_NAME]).output();
            std::thread::sleep(Duration::from_millis(800));
        }

        let base = antigravity_support_dir()?;
        for name in ANTIGRAVITY_ACCOUNT_ONLY_PATHS {
            let path = base.join(name);
            if path.is_dir() {
                let _ = std::fs::remove_dir_all(&path);
            } else if path.is_file() {
                let _ = std::fs::remove_file(&path);
            }
        }

        // THE actual credential. Antigravity keeps its live OAuth session in VS Code's
        // globalState SQLite store (User/globalStorage/state.vscdb) under the keys
        // `antigravityUnifiedStateSync.oauthToken` / `.userStatus`. These are NOT Electron
        // safeStorage ciphertext (they carry no v10/v11 prefix), so wiping cookies and the
        // Keychain "Safe Storage" key above does NOT invalidate them — the IDE re-reads the
        // token verbatim on next launch and silently signs back in. That was the "logout does
        // nothing" bug. We must delete these two rows from state.vscdb (and its .backup, which
        // Antigravity restores from if the primary is missing). The app is already quit above,
        // so the SQLite file is unlocked. macOS ships /usr/bin/sqlite3; deleting only these two
        // keys leaves all other globalState (settings, extension state, rules) untouched.
        remove_antigravity_auth_rows(&base);

        // The actual OAuth session survives a plain file wipe: Electron's `safeStorage`
        // encrypts it and stores only the ciphertext in app files (state.vscdb etc.), while
        // the AES key itself lives in exactly one macOS Keychain item named
        // "<AppName> Safe Storage". Deleting that single, precisely-named item — not a
        // keychain scan/dump — makes the stored ciphertext permanently undecryptable, which
        // is what actually forces re-login, without touching User/ or globalStorage/ (so
        // extensions, settings, rules, and permissions all survive untouched).
        #[cfg(target_os = "macos")]
        {
            let service = format!("{} Safe Storage", ANTIGRAVITY_APP_NAME);
            let _ = Command::new("security")
                .args(["delete-generic-password", "-s", &service])
                .output();
        }

        Ok(())
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}
