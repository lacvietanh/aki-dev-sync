use crate::agent_usage::antigravity_payload::parse_antigravity_frames;
use crate::agent_usage::probe_log::{log_shell_stderr, preview};
use crate::agent_usage::probe_result::{host_answered, now_secs, AgentUsageResult};
use crate::logger;
use crate::remote_shell::{run_remote_shell, Shell, REMOTE_SCRIPT_TIMEOUT_SECS};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Re-report a missing tool after this interval so hosts are re-probed during long-lived sessions.
const AG_TOOL_MISSING_SUPPRESSION_SECS: i64 = 300;

fn should_report_missing_tool(
    reported_at: &mut HashMap<String, i64>,
    host: &str,
    now: i64,
) -> bool {
    match reported_at.get(host) {
        Some(last) if now.saturating_sub(*last) < AG_TOOL_MISSING_SUPPRESSION_SECS => false,
        _ => {
            reported_at.insert(host.to_string(), now);
            true
        }
    }
}

fn ag_tool_missing_now(host: &str, now: i64) -> bool {
    static REPORTED_AT: OnceLock<Mutex<HashMap<String, i64>>> = OnceLock::new();
    let reported_at = REPORTED_AT.get_or_init(|| Mutex::new(HashMap::new()));
    should_report_missing_tool(&mut reported_at.lock().unwrap(), host, now)
}

fn ag_tool_missing_once(host: &str) -> bool {
    ag_tool_missing_now(host, now_secs())
}

pub(crate) fn get_antigravity_usage(host: &str) -> Result<AgentUsageResult, String> {
    logger::debug("USAGE:antigravity", &format!("start host={}", host));

    const SCRIPT: &str = include_str!("../../../scripts/get-antigravity-usage.sh");

    let output = match run_remote_shell(host, Shell::Plain, "", SCRIPT, REMOTE_SCRIPT_TIMEOUT_SECS)
    {
        Ok(o) => o,
        Err(e) => {
            logger::debug(
                "USAGE:antigravity",
                &format!("soft-miss (spawn/timeout): {}", e),
            );
            return Ok(AgentUsageResult::unreachable(e));
        }
    };

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    logger::debug(
        "USAGE:antigravity",
        &format!(
            "exit={} stdout_b={} stderr_b={}",
            exit_code,
            stdout.len(),
            stderr.len()
        ),
    );

    log_shell_stderr("USAGE:antigravity", &stderr);

    if !output.status.success() {
        if exit_code == 3 && ag_tool_missing_once(host) {
            logger::error(
                "USAGE:antigravity",
                &format!(
                    "required tool (curl) missing on host={} exit={} stderr={}",
                    host,
                    exit_code,
                    preview(&stderr, 200)
                ),
            );
        } else if exit_code == 127 && ag_tool_missing_once(host) {
            logger::error(
                "USAGE:antigravity",
                &format!(
                    "shell executable miss host={} exit={} stderr={}",
                    host,
                    exit_code,
                    preview(&stderr, 200)
                ),
            );
        } else {
            logger::debug(
                "USAGE:antigravity",
                &format!("soft-miss: {}", stderr.trim()),
            );
        }

        if !host_answered(host, exit_code) {
            return Ok(AgentUsageResult::unreachable(format!(
                "ssh could not reach {} (exit 255)",
                host
            )));
        }
        return Ok(AgentUsageResult::miss(format!(
            "probe exited {}",
            exit_code
        )));
    }

    if stdout.trim().is_empty() {
        logger::debug("USAGE:antigravity", "done: null empty stdout");
        return Ok(AgentUsageResult::miss("no live AG session"));
    }

    let now = now_secs();
    match parse_antigravity_frames(&stdout, now) {
        Ok(Some(resp)) => {
            logger::debug(
                "USAGE:antigravity",
                &format!("done: ok b={}", resp.content.len()),
            );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_tool_reports_once_within_suppression_ttl() {
        let mut reported_at = HashMap::new();

        assert!(should_report_missing_tool(&mut reported_at, "remote", 100));
        assert!(!should_report_missing_tool(
            &mut reported_at,
            "remote",
            100 + AG_TOOL_MISSING_SUPPRESSION_SECS - 1,
        ));
    }

    #[test]
    fn missing_tool_reports_again_after_suppression_ttl() {
        let mut reported_at = HashMap::new();

        assert!(should_report_missing_tool(&mut reported_at, "remote", 100));
        assert!(should_report_missing_tool(
            &mut reported_at,
            "remote",
            100 + AG_TOOL_MISSING_SUPPRESSION_SECS,
        ));
    }
}
