// @docs docs/arch/usage-claudecode.md
// @docs docs/arch/usage-antigravity.md

pub mod antigravity;
pub mod antigravity_logout;
pub mod antigravity_payload;
pub mod claudecode;
pub mod probe_log;
pub mod probe_result;

pub use antigravity_logout::*;
pub use probe_result::AgentUsageResult;

#[tauri::command]
pub async fn provision_agent_usage(agent_name: String, host: String) -> Result<bool, String> {
    crate::system::validate_remote_host(&host)?;
    tauri::async_runtime::spawn_blocking(move || {
        claudecode::provision_agent_usage_sync(&agent_name, &host)
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

#[tauri::command]
pub async fn get_agent_usage(
    agent_name: String,
    host: String,
) -> Result<AgentUsageResult, String> {
    crate::system::validate_remote_host(&host)?;
    tauri::async_runtime::spawn_blocking(move || {
        if agent_name == "claudecode" {
            return claudecode::get_claudecode_usage(&host);
        }
        if agent_name == "antigravity" {
            return antigravity::get_antigravity_usage(&host);
        }
        Err("Unknown agent".into())
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}
