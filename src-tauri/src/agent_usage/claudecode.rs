use crate::agent_usage::probe_log::{ab, log_shell_stderr, preview};
use crate::agent_usage::probe_result::{host_answered, now_secs, AgentUsageResponse, AgentUsageResult};
use crate::logger;
use crate::remote_shell::{run_remote_script, run_remote_shell, Shell, REMOTE_SCRIPT_TIMEOUT_SECS};
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

const CLAUDE_CALL_TIMEOUT_SECS: u64 = 45;

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

if command -v timeout >/dev/null 2>&1; then
    AKI_CLAUDE_TMO="timeout -k 5 __CLAUDE_CALL_TIMEOUT__ "
elif command -v gtimeout >/dev/null 2>&1; then
    AKI_CLAUDE_TMO="gtimeout -k 5 __CLAUDE_CALL_TIMEOUT__ "
elif command -v perl >/dev/null 2>&1; then
    AKI_CLAUDE_TMO="perl -e 'alarm shift; exec @ARGV or exit 127' __CLAUDE_CALL_TIMEOUT__ "
else
    AKI_CLAUDE_TMO=""
    printf '[SHELL:preamble] WARNING no timeout/gtimeout/perl on this host - claude calls run unbounded\n' >&2
fi
export AKI_CLAUDE_TMO
"#;

fn cc_auth_force_needed(host: &str) -> bool {
    static SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let seen = SEEN.get_or_init(|| Mutex::new(HashSet::new()));
    seen.lock().unwrap().insert(host.to_string())
}

pub(crate) fn provision_agent_usage_sync(agent_name: &str, host: &str) -> Result<bool, String> {
    logger::info("PROVISION", &format!("{} host={}", ab(agent_name), host));

    if agent_name != "claudecode" {
        logger::debug("PROVISION", &format!("skip {}", ab(agent_name)));
        return if agent_name == "antigravity" { Ok(true) } else { Err("Unknown agent".into()) };
    }

    const SCRIPT: &str = include_str!("../../../scripts/provision-claudecode.sh");
    let output = run_remote_script(host, SCRIPT)?;
    let ok = output.status.success();
    logger::info("PROVISION", &format!("exit={} ok={}", output.status.code().unwrap_or(-1), ok));
    let err = String::from_utf8_lossy(&output.stderr);
    if !ok {
        let err_preview = preview(&err, 200);
        logger::error("PROVISION", &format!("stderr={}", err_preview));
        return Err(format!("Provision failed: {}", err));
    }
    if !err.trim().is_empty() {
        logger::error("PROVISION", &format!("stderr (non-fatal)={}", preview(&err, 200)));
    }
    Ok(true)
}

pub(crate) fn get_claudecode_usage(host: &str) -> Result<AgentUsageResult, String> {
    logger::debug("GET_USAGE", &format!("start host={}", host));

    const SCRIPT: &str = include_str!("../../../scripts/get-claudecode-usage.sh");
    let force_auth = cc_auth_force_needed(host);
    let script_owned;
    let script: &str = if force_auth {
        logger::info("GET_USAGE", "first check this session - forcing auth refresh (bypass cache TTL)");
        script_owned = format!("AKI_FORCE_AUTH_REFRESH=1\n{}", SCRIPT);
        &script_owned
    } else {
        SCRIPT
    };

    let preamble = CLAUDE_BIN_RESOLVER_PREAMBLE
        .replace("__CLAUDE_CALL_TIMEOUT__", &CLAUDE_CALL_TIMEOUT_SECS.to_string());

    let output = match run_remote_shell(host, Shell::Plain, &preamble, script, REMOTE_SCRIPT_TIMEOUT_SECS) {
        Ok(o) => o,
        Err(e) => {
            logger::debug("GET_USAGE", &format!("soft-miss (spawn/timeout): {}", e));
            return Ok(AgentUsageResult::unreachable(e));
        }
    };

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    logger::debug("GET_USAGE", &format!(
        "exit={} stdout_b={} stderr_b={}",
        exit_code, stdout.len(), stderr.len()
    ));

    log_shell_stderr("GET_USAGE", &stderr);

    if !output.status.success() {
        if !host_answered(host, exit_code) {
            logger::error("GET_USAGE", &format!("host unreachable (ssh exit={})", exit_code));
            return Ok(AgentUsageResult::unreachable(format!("ssh could not reach {} (exit 255)", host)));
        }
        logger::error("GET_USAGE", &format!("shell exit={}", exit_code));
        return Ok(AgentUsageResult::miss(format!("probe exited {}", exit_code)));
    }

    if stdout.trim().is_empty() {
        logger::info("GET_USAGE", "null: no cache");
        return Ok(AgentUsageResult::miss("no cache"));
    }

    if stdout.trim() == "|||STALE_RESET|||" {
        logger::info("GET_USAGE", "null: STALE_RESET");
        return Ok(AgentUsageResult::miss("STALE_RESET"));
    }

    logger::debug("GET_USAGE", &format!("stdout: {}", preview(&stdout, 300)));

    let parts: Vec<&str> = stdout.split("|||MTIME|||").collect();
    logger::debug("GET_USAGE", &format!("mtime_parts={}", parts.len()));
    if parts.len() != 2 {
        logger::error("GET_USAGE", "no MTIME delimiter");
        return Ok(AgentUsageResult::miss("malformed probe output (no MTIME delimiter)"));
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

    let content_len = content_raw.len();
    let mut v: serde_json::Value = match serde_json::from_str(content_raw) {
        Ok(val) => {
            logger::debug("GET_USAGE", &format!("json_ok b={}", content_len));
            val
        }
        Err(e) => {
            logger::error("GET_USAGE", &format!("json_parse err={} b={}", e, content_len));
            return Ok(AgentUsageResult::miss("malformed probe output (cache JSON did not parse)"));
        }
    };

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

    if let Some(obj) = v.as_object() {
        let now = now_secs();
        let summary = obj.get("rate_limits")
            .and_then(|r| r.as_object())
            .map(|rl| {
                if rl.is_empty() { return "EMPTY".to_string(); }
                let mut keys: Vec<&String> = rl.keys().collect();
                keys.sort();
                keys.iter()
                    .map(|k| {
                        let b = &rl[*k];
                        let pct    = b.get("used_percentage").and_then(|v| v.as_i64()).unwrap_or(-1);
                        let resets = b.get("resets_at").and_then(|v| v.as_i64()).unwrap_or(0);
                        let state  = if resets == 0 { "no_reset" }
                                     else if now - resets > 0 { "PAST" } else { "future" };
                        format!("{}=[pct={} resets_at={} overdue_s={} state={}]",
                                k, pct, resets, now - resets, state)
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_else(|| "MISSING".to_string());
        logger::debug("GET_USAGE", &format!("rl {}", summary));
    }

    let content = serde_json::to_string(&v).unwrap_or_default();
    logger::debug("GET_USAGE", &format!("done mtime={} b={}", mtime_sec, content.len()));
    Ok(AgentUsageResult::hit(AgentUsageResponse {
        content,
        fetched_at: now_secs().to_string(),
        file_modified_at: mtime_sec.to_string(),
    }))
}
