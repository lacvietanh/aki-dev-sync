use crate::agent_usage::antigravity_payload::parse_antigravity_frames;
use crate::agent_usage::probe_log::{log_shell_stderr, preview};
use crate::agent_usage::probe_result::{host_answered, now_secs, AgentUsageResult};
use crate::logger;
use crate::remote_shell::{run_remote_shell, Shell, REMOTE_SCRIPT_TIMEOUT_SECS};
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

/// True the first time it's called for a given host in this app process, false after.
fn ag_tool_missing_once(host: &str) -> bool {
    static SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let seen = SEEN.get_or_init(|| Mutex::new(HashSet::new()));
    seen.lock().unwrap().insert(host.to_string())
}

pub(crate) fn get_antigravity_usage(host: &str) -> Result<AgentUsageResult, String> {
    logger::debug("USAGE:antigravity", &format!("start host={}", host));

    const SCRIPT: &str = include_str!("../../../scripts/get-antigravity-usage.sh");

    let output = match run_remote_shell(host, Shell::Plain, "", SCRIPT, REMOTE_SCRIPT_TIMEOUT_SECS) {
        Ok(o) => o,
        Err(e) => {
            logger::debug("USAGE:antigravity", &format!("soft-miss (spawn/timeout): {}", e));
            return Ok(AgentUsageResult::unreachable(e));
        }
    };

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    logger::debug("USAGE:antigravity", &format!(
        "exit={} stdout_b={} stderr_b={}",
        exit_code, stdout.len(), stderr.len()
    ));

    log_shell_stderr("USAGE:antigravity", &stderr);

    if !output.status.success() {
        if exit_code == 3 && ag_tool_missing_once(host) {
            logger::error("USAGE:antigravity", &format!(
                "required tool (curl) missing on host={} exit={} stderr={}",
                host, exit_code, preview(&stderr, 200)
            ));
        } else if exit_code == 127 && ag_tool_missing_once(host) {
            logger::error("USAGE:antigravity", &format!(
                "shell executable miss host={} exit={} stderr={}",
                host, exit_code, preview(&stderr, 200)
            ));
        } else {
            logger::debug("USAGE:antigravity", &format!("soft-miss: {}", stderr.trim()));
        }

        if !host_answered(host, exit_code) {
            return Ok(AgentUsageResult::unreachable(format!("ssh could not reach {} (exit 255)", host)));
        }
        return Ok(AgentUsageResult::miss(format!("probe exited {}", exit_code)));
    }

    if stdout.trim().is_empty() {
        logger::debug("USAGE:antigravity", "done: null empty stdout");
        return Ok(AgentUsageResult::miss("no live AG session"));
    }

    let now = now_secs();
    match parse_antigravity_frames(&stdout, now) {
        Ok(Some(resp)) => {
            logger::debug("USAGE:antigravity", &format!("done: ok b={}", resp.content.len()));
            Ok(AgentUsageResult::hit(resp))
        }
        Ok(None) => {
            logger::debug("USAGE:antigravity", "done: no usable frames");
            Ok(AgentUsageResult::miss("no live AG session"))
        }
        Err(e) => {
            logger::error("USAGE:antigravity", &format!("frame_parse err={}", e));
            Ok(AgentUsageResult::miss("malformed probe output"))
        }
    }
}
