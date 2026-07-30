// Merges the recommended Antigravity CLI (agy) permission allowlist into
// ~/.gemini/antigravity-cli/settings.json's `permissions.allow`, so a new machine or a new agy
// account does not have to click through its permission prompt for every common dev command
// (git, cat, curl, ...). Deliberately NOT folded into statusline.rs's `apply_statusline_config` -
// that command is ticked from the Statusline Customizer, and a user applying a statusline should
// never have their agy permissions silently widened as a side effect of an unrelated action.
//
// Shares the AGY target's jq-merge/backup/atomic-write shape from statusline.rs (see that file's
// `Target` doc comment for why each step is ordered the way it is) and reuses its multi-host
// fan-out (`HostApplyResult`) so the same UI list can render either apply's result.

use crate::remote_shell::run_remote_script_bounded;
use crate::statusline::HostApplyResult;

/// The recommended allowlist, exported from a real settings.json and checked in as the SSOT for
/// what this app seeds. Editing this file changes what a future Apply writes; it never touches a
/// machine's live settings.json until Apply actually runs.
const ALLOWLIST_JSON: &str = include_str!("../../share/gemini_allowlist_unified.json");

/// Only `permissions.allow` is touched - the union of what's already there and the seed list, not
/// an overwrite. `settings.json` also carries `agentMode`, `model`, `statusLine` and other keys
/// this app must not disturb.
fn build_installer_script() -> String {
    format!(
        "set -e\n\
         command -v jq >/dev/null 2>&1 || {{ echo 'jq not found on PATH - install jq, then Apply again (nothing was changed)' >&2; exit 1; }}\n\
         mkdir -p \"$HOME/.gemini/antigravity-cli\"\n\
         SETTINGS=\"$HOME/.gemini/antigravity-cli/settings.json\"\n\
         [ -f \"$SETTINGS\" ] || echo '{{}}' > \"$SETTINGS\"\n\
         cp \"$SETTINGS\" \"$SETTINGS.aki-bak-$(date +%s)\"\n\
         SEED=$(mktemp)\n\
         cat > \"$SEED\" <<'AKI_GEMINI_ALLOWLIST_EOF'\n{allowlist}\nAKI_GEMINI_ALLOWLIST_EOF\n\
         jq -e '.permissions.allow | length > 0' \"$SEED\" >/dev/null 2>&1 || {{ echo 'seed allowlist is empty or invalid (heredoc capture failed) - aborting, nothing was changed' >&2; rm -f \"$SEED\"; exit 1; }}\n\
         tmp=$(mktemp)\n\
         jq -s '.[0] as $cur | .[1].permissions.allow as $seed | $cur | .permissions.allow = (((.permissions.allow // []) + $seed) | unique)' \"$SETTINGS\" \"$SEED\" > \"$tmp\" && [ -s \"$tmp\" ] && mv \"$tmp\" \"$SETTINGS\"\n\
         rm -f \"$SEED\" \"$tmp\"\n",
        allowlist = ALLOWLIST_JSON,
    )
}

#[tauri::command]
pub async fn apply_gemini_allowlist(target_hosts: Vec<String>) -> Result<Vec<HostApplyResult>, String> {
    // Validated before any thread is spawned - same boundary check as apply_statusline_config,
    // since this list arrives from the frontend (and, over the relay, from a companion).
    for host in &target_hosts {
        crate::system::validate_remote_host(host)?;
    }
    let script = std::sync::Arc::new(build_installer_script());

    tauri::async_runtime::spawn_blocking(move || {
        let handles: Vec<(String, std::thread::JoinHandle<HostApplyResult>)> = target_hosts
            .into_iter()
            .map(|host| {
                let script = script.clone();
                let host_for_thread = host.clone();
                let handle = std::thread::spawn(move || {
                    let result = match run_remote_script_bounded(&host_for_thread, &script) {
                        Ok(output) if output.status.success() => HostApplyResult {
                            host: host_for_thread.clone(),
                            ok: true,
                            message: "Allowlist merged into settings.json".to_string(),
                        },
                        Ok(output) => {
                            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                            let why = if stderr.is_empty() {
                                format!("exit {}", output.status.code().unwrap_or(-1))
                            } else {
                                stderr
                            };
                            HostApplyResult { host: host_for_thread.clone(), ok: false, message: why }
                        }
                        Err(e) => HostApplyResult { host: host_for_thread.clone(), ok: false, message: e },
                    };
                    crate::logger::info(
                        "GEMINI_ALLOWLIST",
                        &format!("apply host={} ok={} msg={}", result.host, result.ok, result.message),
                    );
                    result
                });
                (host, handle)
            })
            .collect();
        handles
            .into_iter()
            .map(|(host, h)| {
                h.join().unwrap_or(HostApplyResult {
                    host,
                    ok: false,
                    message: "apply thread panicked".to_string(),
                })
            })
            .collect()
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))
}

