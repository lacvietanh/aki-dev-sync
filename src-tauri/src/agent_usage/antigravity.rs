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

    const PROBE: &str = include_str!("../../../scripts/get-antigravity-usage.sh");

    /// Runs the real probe as a local poll does (`sh`, script on stdin) in a private $HOME; `ps` is stubbed to its header, so no process is seen. `frames` = (file, body, age in s).
    #[cfg(unix)]
    fn run_probe(name: &str, frames: &[(&str, &str, u64)]) -> (bool, String) {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;
        let home = std::env::temp_dir().join(format!("aki-ag-probe-test/{}", name));
        let _ = std::fs::remove_dir_all(&home);
        let dir = home.join(".gemini/antigravity-cli/rate-limits");
        let bin = home.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        if !frames.is_empty() {
            std::fs::create_dir_all(&dir).unwrap();
        }
        for (file, body, age_secs) in frames {
            let path = dir.join(file);
            std::fs::write(&path, body).unwrap();
            std::fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(
                    std::time::SystemTime::now() - std::time::Duration::from_secs(*age_secs),
                )
                .unwrap();
        }
        std::fs::write(bin.join("ps"), "#!/bin/sh\necho 'USER PID COMMAND'\n").unwrap();
        std::fs::set_permissions(bin.join("ps"), std::fs::Permissions::from_mode(0o755)).unwrap();
        let path_env = format!(
            "{}:{}",
            bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let out = std::process::Command::new("sh")
            .env("HOME", &home)
            .env("PATH", path_env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut c| {
                c.stdin.take().unwrap().write_all(PROBE.as_bytes())?;
                c.wait_with_output()
            })
            .unwrap();
        let _ = std::fs::remove_dir_all(&home);
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).to_string(),
        )
    }

    #[cfg(unix)]
    fn frame(email: &str, bucket: &str) -> String {
        format!(
            "{{\"email\":\"{}\"}}\n{{\"command\":{{\"name\":\"usage\",\"data\":{{\"groups\":[{{\"name\":\"Gemini Models\",\"buckets\":[{{\"id\":\"{}\",\"window\":\"5h\",\"remaining_fraction\":0.5}}]}}]}}}}}}\n",
            email, bucket
        )
    }

    #[cfg(unix)]
    #[test]
    fn probe_reads_each_fresh_quota_frame_as_its_own_account_and_skips_stale_or_malformed_ones() {
        if std::process::Command::new("curl")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("skipping: curl not on PATH");
            return;
        }
        let six_hours = 6 * 3600;
        let (ok, stdout) = run_probe(
            "frames",
            &[
                (
                    "kim@example.com.frame",
                    &frame("kim@example.com", "gemini-5h"),
                    60,
                ),
                (
                    "lva@example.com.frame",
                    &frame("lva@example.com", "3p-5h"),
                    six_hours - 600,
                ),
                (
                    "old@example.com.frame",
                    &frame("old@example.com", "gemini-5h"),
                    six_hours + 600,
                ),
                ("bad@example.com.frame", "not a status line\n{}\n", 60),
            ],
        );
        assert!(
            ok,
            "fresh frames must answer the poll with no process running: {}",
            stdout
        );
        assert_eq!(stdout.matches("|||AGPROC|||").count(), 2, "{}", stdout);
        let payload = parse_antigravity_frames(&stdout, 1_750_000_000)
            .unwrap()
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&payload.content).unwrap();
        assert_eq!(v["email"], "kim@example.com");
        let emails: Vec<&str> = v["allAccounts"]
            .as_array()
            .expect("both accounts listed")
            .iter()
            .filter_map(|a| a["email"].as_str())
            .collect();
        assert_eq!(emails, ["kim@example.com", "lva@example.com"]);
    }

    #[cfg(unix)]
    #[test]
    fn probe_without_a_process_or_a_quota_frame_still_reports_not_running() {
        if std::process::Command::new("curl")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("skipping: curl not on PATH");
            return;
        }
        let (ok, stdout) = run_probe("no-frames", &[]);
        assert!(
            !ok,
            "no process and no frame must stay an error: {}",
            stdout
        );
        assert!(!stdout.contains("|||AGPROC|||"), "{}", stdout);
    }

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
