use crate::remote_shell::is_local_host;
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize)]
pub struct AgentUsageResponse {
    pub content: String,
    pub fetched_at: String,
    pub file_modified_at: String,
}

/// One answer to `get_agent_usage`, and the reason there is no reading when there isn't one.
#[derive(Serialize)]
pub struct AgentUsageResult {
    /// True when the host itself ran the probe and exited on its own, whatever it exited with.
    pub host_answered: bool,
    /// Why there is no reading. `None` when `data` is present.
    pub miss_reason: Option<String>,
    pub data: Option<AgentUsageResponse>,
}

impl AgentUsageResult {
    pub(crate) fn hit(data: AgentUsageResponse) -> Self {
        Self {
            host_answered: true,
            miss_reason: None,
            data: Some(data),
        }
    }

    /// The host answered; there is simply no reading this poll.
    pub(crate) fn miss(reason: impl Into<String>) -> Self {
        Self {
            host_answered: true,
            miss_reason: Some(reason.into()),
            data: None,
        }
    }

    /// The call never reached the host (connection refused, DNS, auth, network unreachable).
    pub(crate) fn unreachable(reason: impl Into<String>) -> Self {
        Self {
            host_answered: false,
            miss_reason: Some(reason.into()),
            data: None,
        }
    }
}

/// Whether the HOST itself answered, given the exit status of one probe.
pub(crate) fn host_answered(host: &str, exit_code: i32) -> bool {
    is_local_host(host) || exit_code != 255
}

pub(crate) fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_255_is_the_only_unreachable_signal() {
        assert!(!host_answered("hostB", 255), "ssh 255 = never reached the host");
        assert!(host_answered("hostB", 0));
        assert!(host_answered("hostB", 1), "remote script's own failure - the host answered");
        assert!(host_answered("hostB", 127), "node missing on the remote - the host answered");
        assert!(host_answered("hostB", -1), "no code (signalled) - not a connection failure");
    }

    #[test]
    fn a_local_probe_always_answers() {
        assert!(host_answered("local", 255));
        assert!(host_answered("localhost", 255));
    }

    #[test]
    fn result_constructors_carry_reachability() {
        let miss = AgentUsageResult::miss("no cache");
        assert!(miss.host_answered && miss.data.is_none());
        let down = AgentUsageResult::unreachable("ssh could not reach hostB");
        assert!(!down.host_answered && down.data.is_none());
        let hit = AgentUsageResult::hit(AgentUsageResponse {
            content: "{}".into(),
            fetched_at: "0".into(),
            file_modified_at: "0".into(),
        });
        assert!(hit.host_answered && hit.miss_reason.is_none() && hit.data.is_some());
    }
}
