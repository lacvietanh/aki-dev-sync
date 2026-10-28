// @docs docs/ref/statusline-unified-spec.md
// Generates the ONE unified statusline script (Claude Code + AGY CLI share the same file, which
// self-identifies from $0), then pushes it - and, for Claude Code, a settings.json patch - to the
// local and/or remote hosts the user ticked.
//
// SSOT: this module holds NO defaults. Every value below arrives from the Statusline Customizer
// (src/components/modals/ClaudeSettingModal.vue, defaultLocalConfig()) and every line of shell
// comes from the template (statusline-unified.sh, beside this file). All this file does is patch the one
// into the other. A default table here would be a second source of truth, which is exactly the
// drift Phase 2 removed - do not reintroduce one.

use crate::remote_shell::run_remote_script_bounded;
use serde::{Deserialize, Serialize};

/// The script itself, compiled in verbatim. It is a runnable, checked-in reference as well as the template: the region between the two markers below is the only part Apply rewrites.
const TEMPLATE: &str = include_str!("statusline-unified.sh");
const MARK_BEGIN: &str = "# >>> AKI-GENERATED-CONFIG >>>";
const MARK_END: &str = "# <<< AKI-GENERATED-CONFIG <<<";

#[derive(Serialize, Deserialize, Clone)]
pub struct StatuslineField {
    pub key: String,
    pub enabled: bool,
    /// Only honored for the keys in COLOR_KEYS; every other field's color is computed from its value (the UI shows "Dynamic color" and offers no picker).
    pub color: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct StatuslineThresholds {
    pub green: u8,
    pub yellow: u8,
    pub orange: u8,
    pub red: u8,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct StatuslineTrunc {
    pub account: u8,
    pub user: u8,
    pub host: u8,
    pub cwd: u8,
    pub branch: u8,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct StatuslineZebra {
    pub a: u8,
    pub b: u8,
}

/// No `#[serde(default)]` on purpose (against tauri.B3): the Vue `loadCfg()` always sends the full shape, so a missing field is malformed and Apply must fail.
#[derive(Serialize, Deserialize, Clone)]
pub struct StatuslineConfig {
    pub fields: Vec<StatuslineField>,
    pub thresholds: StatuslineThresholds,
    pub trunc: StatuslineTrunc,
    pub zebra: StatuslineZebra,
    pub separate: bool,
}

#[derive(Serialize, Clone)]
pub struct HostApplyResult {
    pub host: String,
    pub ok: bool,
    pub message: String,
}

/// Every EN_ flag the template declares, in template order. A key here that the UI never sends still gets a flag (0), so a stale saved config can never leave a gate undefined.
const EN_KEYS: &[&str] = &[
    "cli_tag",
    "account",
    "identity_user",
    "identity_host",
    "cwd",
    "model",
    "effort",
    "context",
    "cache",
    "cache_pct",
    "cache_tokens",
    "rate_limits_5h",
    "rate_reset_5h",
    "rate_limits_7d",
    "rate_reset_7d",
    "session",
    "git_branch",
    "ram",
];

/// User-picked color keys; must name the same six as COLOR_EDITABLE in ClaudeSettingModal.vue and the template's COLOR_* block.
const COLOR_KEYS: &[&str] = &[
    "identity_user",
    "identity_host",
    "cwd",
    "model",
    "git_branch",
    "account",
];

/// (child, parent), mirroring DEPENDS in ClaudeSettingModal.vue; resolved here so the generated flags are final and no shell gate re-checks a parent.
const DEPENDS: &[(&str, &str)] = &[
    ("effort", "model"),
    ("rate_reset_5h", "rate_limits_5h"),
    ("rate_reset_7d", "rate_limits_7d"),
    ("cache_pct", "cache"),
    ("cache_tokens", "cache"),
];

/// Which printed block a field belongs to. `None` = printed outside the block loop (the tag cluster) or unknown to this version of the template.
fn block_of(key: &str) -> Option<&'static str> {
    Some(match key {
        "identity_user" | "identity_host" => "identity",
        "cwd" => "cwd",
        "model" | "effort" => "model",
        "context" => "context",
        "cache" | "cache_pct" | "cache_tokens" => "cache",
        "rate_limits_5h" | "rate_reset_5h" | "rate_limits_7d" | "rate_reset_7d" => "quota",
        "session" => "session",
        "git_branch" => "git_branch",
        "ram" => "ram",
        _ => return None,
    })
}

/// Copy of STATUSLINE_COLORS in src/utils/statuslineColors.js, the source of truth: change it there first. The template's BOLD_* block mirrors its tiers.
fn ansi_for(name: &str) -> &'static str {
    match name {
        "white" => r"\033[97m",
        "cyan" => r"\033[36m",
        "green" => r"\033[01;32m",
        "blue" => r"\033[01;34m",
        "grey" => r"\033[90m",
        "red" => r"\033[31m",
        "yellow" => r"\033[01;33m",
        "magenta" => r"\033[35m",
        _ => r"\033[97m",
    }
}

// The returned reference borrows from `config`, not from `key` - tying both inputs to one lifetime (as the compiler's suggestion does) would force callers to keep the key alive as long as the config.
fn field<'a>(config: &'a StatuslineConfig, key: &str) -> Option<&'a StatuslineField> {
    config.fields.iter().find(|f| f.key == key)
}

/// Enabled AND reachable - the same rule as fieldActive() in the Vue component. A field the config does not mention at all counts as off.
fn is_active(config: &StatuslineConfig, key: &str) -> bool {
    if !field(config, key).map(|f| f.enabled).unwrap_or(false) {
        return false;
    }
    match DEPENDS.iter().find(|(child, _)| *child == key) {
        Some((_, parent)) => is_active(config, parent),
        None => true,
    }
}

/// Ascending, so the ladder can never be inverted by a config that has red below green.
fn sanitized_thresholds(t: &StatuslineThresholds) -> (u8, u8, u8, u8) {
    let mut v = [
        t.green.min(100),
        t.yellow.min(100),
        t.orange.min(100),
        t.red.min(100),
    ];
    v.sort_unstable();
    (v[0], v[1], v[2], v[3])
}

/// Into the neutral greyscale ramp the UI's picker is restricted to (16, or 232..=255). A clamp, not a fallback to some "default" shade - this module owns no defaults.
fn sanitized_shade(n: u8) -> u8 {
    if n == 16 {
        16
    } else {
        n.clamp(232, 255)
    }
}

/// Print order as the user dragged it; a block sits at its first member's position and stays listed even when every member is off (EN_ flags decide output).
fn block_order(config: &StatuslineConfig) -> Vec<&'static str> {
    let mut order: Vec<&'static str> = Vec::new();
    for f in &config.fields {
        if let Some(block) = block_of(&f.key) {
            if !order.contains(&block) {
                order.push(block);
            }
        }
    }
    order
}

/// The generated region: nothing but variable assignments the template's body reads.
fn config_block(config: &StatuslineConfig) -> String {
    let (green, yellow, orange, red) = sanitized_thresholds(&config.thresholds);
    let mut s = String::new();

    s.push_str(
        "# Enable flags, already dependency-resolved (see DEPENDS in ClaudeSettingModal.vue).\n",
    );
    for key in EN_KEYS {
        s.push_str(&format!(
            "EN_{}={}\n",
            key,
            if is_active(config, key) { 1 } else { 0 }
        ));
    }

    s.push_str("\n# User-chosen colors. Every other color is a fixed label color or comes from the ladder.\n");
    for key in COLOR_KEYS {
        let color = field(config, key).map(|f| f.color.as_str()).unwrap_or("");
        s.push_str(&format!("COLOR_{}='{}'\n", key, ansi_for(color)));
    }

    s.push_str(&format!(
        "\n# Dynamic-color ladder, ascending. Below THRESH_GREEN is the calm aqua tier.\n\
         THRESH_GREEN={green}\n\
         THRESH_YELLOW={yellow}\n\
         THRESH_ORANGE={orange}\n\
         THRESH_RED={red}\n"
    ));

    s.push_str(&format!(
        "\n# Truncate widths. The template clamps these itself (floor 3, per-field ceiling).\n\
         TRUNC_ACCOUNT={account}   # applied AFTER stripping everything from '@' onwards\n\
         TRUNC_USER={user}\n\
         TRUNC_HOST={host}\n\
         TRUNC_CWD={cwd}\n\
         TRUNC_BRANCH={branch}\n",
        account = config.trunc.account,
        user = config.trunc.user,
        host = config.trunc.host,
        cwd = config.trunc.cwd,
        branch = config.trunc.branch,
    ));

    s.push_str(&format!(
        "\n# Zebra background shades, from the neutral ramp only.\n\
         BG_ZEBRA_A={a}\n\
         BG_ZEBRA_B={b}\n\
         \n# One space either side of every zebra block. The tag cluster is never padded.\n\
         SEPARATE_BLOCKS={sep}\n",
        a = sanitized_shade(config.zebra.a),
        b = sanitized_shade(config.zebra.b),
        sep = if config.separate { 1 } else { 0 },
    ));

    s.push_str(&format!(
        "\n# Print order, by block, taken from the row order in the Statusline Customizer.\n\
         BLOCK_ORDER=\"{}\"\n",
        block_order(config).join(" ")
    ));

    s
}

/// Splices the generated region into the template. Everything outside the two markers - the whole body of the script - is shipped byte-for-byte.
pub fn generate_statusline_script(config: &StatuslineConfig) -> Result<String, String> {
    let begin = TEMPLATE
        .find(MARK_BEGIN)
        .ok_or("statusline template is missing its opening AKI-GENERATED-CONFIG marker")?;
    let head_end = TEMPLATE[begin..]
        .find('\n')
        .map(|i| begin + i + 1)
        .ok_or("statusline template's opening marker has no line break after it")?;
    let end = TEMPLATE
        .find(MARK_END)
        .ok_or("statusline template is missing its closing AKI-GENERATED-CONFIG marker")?;
    if end < head_end {
        return Err("statusline template markers are in the wrong order".to_string());
    }
    Ok(format!(
        "{}{}{}",
        &TEMPLATE[..head_end],
        config_block(config),
        &TEMPLATE[end..]
    ))
}

/// Where the one generated body is written and which settings key is patched there. Installer order (settings, backup, atomic write): docs/feat/statusline-customizer.md § Rollout to hosts.
struct Target {
    /// Short human label, used to say WHICH target failed in the per-host message.
    label: &'static str,
    /// Values the frontend may send for this target.
    aliases: &'static [&'static str],
    installer: fn(&str) -> String,
}

const TARGETS: &[Target] = &[
    Target {
        label: "Claude Code",
        aliases: &["cc", "claude"],
        installer: |body| {
            format!(
                "set -e\n\
                 command -v jq >/dev/null 2>&1 || {{ echo 'jq not found on PATH - install jq, then Apply again (nothing was changed)' >&2; exit 1; }}\n\
                 mkdir -p \"$HOME/.claude\"\n\
                 FILE=\"$HOME/.claude/statusline-command.sh\"\n\
                 SETTINGS=\"$HOME/.claude/settings.json\"\n\
                 [ -f \"$SETTINGS\" ] || echo '{{}}' > \"$SETTINGS\"\n\
                 _sdir=$(dirname \"$SETTINGS\")\n\
                 tmp=$(mktemp \"$_sdir/.settings.aki-tmp.XXXXXX\")\n\
                 if jq '.statusLine.type = \"command\" | .statusLine.command = \"~/.claude/statusline-command.sh\"' \"$SETTINGS\" > \"$tmp\"; then\n\
                   mv \"$tmp\" \"$SETTINGS\" || {{ rm -f \"$tmp\"; exit 1; }}\n\
                 else\n\
                   rm -f \"$tmp\"; exit 1\n\
                 fi\n\
                 if [ -f \"$FILE\" ]; then cp \"$FILE\" \"$FILE.aki-bak-$(date +%s)\"; fi\n\
                 tmpf=$(mktemp \"$HOME/.claude/.statusline.aki-tmp.XXXXXX\")\n\
                 cat > \"$tmpf\" <<'AKI_STATUSLINE_CLAUDE_EOF'\n{body}\nAKI_STATUSLINE_CLAUDE_EOF\n\
                 chmod +x \"$tmpf\"\n\
                 mv \"$tmpf\" \"$FILE\"\n"
            )
        },
    },
    Target {
        label: "AGY",
        aliases: &["ag", "cli"],
        installer: |body| {
            format!(
                "set -e\n\
                 command -v jq >/dev/null 2>&1 || {{ echo 'jq not found on PATH - install jq, then Apply again (nothing was changed)' >&2; exit 1; }}\n\
                 mkdir -p \"$HOME/.gemini/antigravity-cli\"\n\
                 FILE_AGY=\"$HOME/.gemini/antigravity-cli/statusline.sh\"\n\
                 SETTINGS_AGY=\"$HOME/.gemini/antigravity-cli/settings.json\"\n\
                 [ -f \"$SETTINGS_AGY\" ] || echo '{{}}' > \"$SETTINGS_AGY\"\n\
                 _sdir=$(dirname \"$SETTINGS_AGY\")\n\
                 tmp=$(mktemp \"$_sdir/.settings.aki-tmp.XXXXXX\")\n\
                 if jq --arg cmd \"$FILE_AGY\" '.statusLine.type = \"command\" | .statusLine.command = $cmd | .statusLine.enabled = true' \"$SETTINGS_AGY\" > \"$tmp\"; then\n\
                   mv \"$tmp\" \"$SETTINGS_AGY\" || {{ rm -f \"$tmp\"; exit 1; }}\n\
                 else\n\
                   rm -f \"$tmp\"; exit 1\n\
                 fi\n\
                 if [ -f \"$FILE_AGY\" ]; then cp \"$FILE_AGY\" \"$FILE_AGY.aki-bak-$(date +%s)\"; fi\n\
                 tmpf=$(mktemp \"$HOME/.gemini/antigravity-cli/.statusline.aki-tmp.XXXXXX\")\n\
                 cat > \"$tmpf\" <<'AKI_STATUSLINE_AGY_EOF'\n{body}\nAKI_STATUSLINE_AGY_EOF\n\
                 chmod +x \"$tmpf\"\n\
                 mv \"$tmpf\" \"$FILE_AGY\"\n"
            )
        },
    },
];

/// One independent script per selected target; an empty selection is an error, never an implicit AGY (docs/feat/statusline-customizer.md § Rollout to hosts).
fn build_installer_scripts(
    config: &StatuslineConfig,
    selected_targets: &[String],
) -> Result<Vec<(&'static str, String)>, String> {
    let chosen: Vec<&Target> = TARGETS
        .iter()
        .filter(|t| {
            selected_targets
                .iter()
                .any(|s| t.aliases.contains(&s.as_str()))
        })
        .collect();
    if chosen.is_empty() {
        return Err("No statusline target selected (tick agy and/or claude).".to_string());
    }
    let body = generate_statusline_script(config)?;
    Ok(chosen
        .into_iter()
        .map(|t| (t.label, (t.installer)(&body)))
        .collect())
}

/// Per CLI: `present` = the CLI exists on the host, `configured` = script installed AND settings point at it, the only state that renders a line.
#[derive(Serialize, Clone)]
pub struct StatuslineHostStatus {
    pub host: String,
    pub cc_present: bool,
    pub cc_configured: bool,
    /// Installed and wired, but its `aki-rlcache` block is older than the one this build ships, so the quota reading runs on the old cache format until it is rewritten.
    pub cc_outdated: bool,
    pub ag_present: bool,
    pub ag_configured: bool,
    /// Installed and wired by this app, but its `aki-agquota` routing block is older than this build's (or absent), so AGY may read the wrong quota pool until it is rewritten.
    pub ag_outdated: bool,
}

impl StatuslineHostStatus {
    fn unreachable(host: String) -> Self {
        Self {
            host,
            cc_present: false,
            cc_configured: false,
            cc_outdated: false,
            ag_present: false,
            ag_configured: false,
            ag_outdated: false,
        }
    }

    fn from_probe(host: String, stdout: &str) -> Self {
        let cc_configured = stdout.contains("CC_SL=1");
        let cc_outdated = cc_configured
            && marker_version(stdout, RLCACHE_MARKER)
                .zip(marker_version(TEMPLATE, RLCACHE_MARKER))
                .is_some_and(|(installed, shipped)| installed < shipped);
        let ag_configured = stdout.contains("AG_SL=1");
        // The rlcache marker proves this app wrote the script; a routing block from before the marker existed counts as version 0.
        let ag_outdated = ag_configured
            && marker_version(stdout, PROBED_AG_RLCACHE).is_some()
            && marker_version(TEMPLATE, AGQUOTA_MARKER).is_some_and(|shipped| {
                marker_version(stdout, PROBED_AG_AGQUOTA).unwrap_or(0) < shipped
            });
        Self {
            host,
            cc_present: stdout.contains("CC_PRESENT=1"),
            cc_configured,
            cc_outdated,
            ag_present: stdout.contains("AG_PRESENT=1"),
            ag_configured,
            ag_outdated,
        }
    }
}

const RLCACHE_MARKER: &str = "# aki-rlcache v";
const AGQUOTA_MARKER: &str = "# aki-agquota v";
/// The AGY script's markers as PROBE prints them (`# aki-` → `AG_`), so they never read as the Claude Code script's.
const PROBED_AG_RLCACHE: &str = "AG_rlcache v";
const PROBED_AG_AGQUOTA: &str = "AG_agquota v";

/// Highest `<marker>N` in `text`. A script with no rlcache marker was not written by this app and is never called outdated.
fn marker_version(text: &str, marker: &str) -> Option<u32> {
    text.match_indices(marker)
        .filter_map(|(i, _)| {
            let digits: String = text[i + marker.len()..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            digits.parse().ok()
        })
        .max()
}

/// Both halves of an install are checked: the script file, and the settings key naming it. A host with the file but an empty `statusLine.command` renders nothing, so it is not "configured".
const PROBE: &str = r#"
if command -v "$CLAUDE_BIN" >/dev/null 2>&1 || [ -d "$HOME/.claude" ]; then echo "CC_PRESENT=1"; else echo "CC_PRESENT=0"; fi
if [ -f "$HOME/.claude/statusline-command.sh" ] && grep -q "statusline-command.sh" "$HOME/.claude/settings.json" 2>/dev/null; then echo "CC_SL=1"; else echo "CC_SL=0"; fi
grep -o '# aki-rlcache v[0-9][0-9]*' "$HOME/.claude/statusline-command.sh" 2>/dev/null
if [ -d "$HOME/.gemini/antigravity-cli" ]; then echo "AG_PRESENT=1"; else echo "AG_PRESENT=0"; fi
if [ -f "$HOME/.gemini/antigravity-cli/statusline.sh" ] && grep -q "statusline.sh" "$HOME/.gemini/antigravity-cli/settings.json" 2>/dev/null; then echo "AG_SL=1"; else echo "AG_SL=0"; fi
grep -o '# aki-[a-z]* v[0-9][0-9]*' "$HOME/.gemini/antigravity-cli/statusline.sh" 2>/dev/null | sed 's/^# aki-/AG_/'
"#;

#[tauri::command]
pub async fn check_statusline_status(hosts: Vec<String>) -> Vec<StatuslineHostStatus> {
    tauri::async_runtime::spawn_blocking(move || {
        // One thread per host; `run_remote_script_bounded`'s per-host lock serializes a host only against itself.
        let handles: Vec<(String, std::thread::JoinHandle<StatuslineHostStatus>)> = hosts
            .into_iter()
            .map(|host| {
                let host_for_thread = host.clone();
                let handle = std::thread::spawn(move || {
                    // A host `ssh` reads as an option runs a command on this Mac; with no per-host error channel here, a refused host shows as unreachable.
                    if let Err(e) = crate::system::validate_remote_host(&host_for_thread) {
                        crate::logger::error("STATUSLINE", &format!("rejected host: {}", e));
                        return StatuslineHostStatus::unreachable(host_for_thread);
                    }
                    match run_remote_script_bounded(&host_for_thread, PROBE) {
                        Ok(out) => StatuslineHostStatus::from_probe(
                            host_for_thread,
                            &String::from_utf8_lossy(&out.stdout),
                        ),
                        Err(_) => StatuslineHostStatus::unreachable(host_for_thread),
                    }
                });
                (host, handle)
            })
            .collect();
        handles
            .into_iter()
            .map(|(host, h)| {
                h.join()
                    .unwrap_or_else(|_| StatuslineHostStatus::unreachable(host))
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
pub async fn apply_statusline_config(
    config: StatuslineConfig,
    target_hosts: Vec<String>,
    selected_targets: Option<Vec<String>>,
) -> Result<Vec<HostApplyResult>, String> {
    let targets = selected_targets.unwrap_or_default();
    // Validated before any script or thread: a host `ssh` reads as an option would run locally. Unlike the probe, Apply reports the refusal outright.
    for host in &target_hosts {
        crate::system::validate_remote_host(host)?;
    }
    let installers = std::sync::Arc::new(build_installer_scripts(&config, &targets)?);

    tauri::async_runtime::spawn_blocking(move || {
        // One thread per host, as in `check_statusline_status`.
        let handles: Vec<(String, std::thread::JoinHandle<HostApplyResult>)> = target_hosts
            .into_iter()
            .map(|host| {
                let installers = installers.clone();
                let host_for_thread = host.clone();
                let handle = std::thread::spawn(move || {
                    // One verdict per target: a failure is named and the others still run; the host's `ok` is their AND.
                    let mut applied: Vec<&str> = Vec::new();
                    let mut failed: Vec<String> = Vec::new();
                    for (label, script) in installers.iter() {
                        match run_remote_script_bounded(&host_for_thread, script) {
                            Ok(output) if output.status.success() => applied.push(label),
                            Ok(output) => {
                                let stderr =
                                    String::from_utf8_lossy(&output.stderr).trim().to_string();
                                let why = if stderr.is_empty() {
                                    format!("exit {}", output.status.code().unwrap_or(-1))
                                } else {
                                    stderr
                                };
                                failed.push(format!("{}: {}", label, why));
                            }
                            Err(e) => failed.push(format!("{}: {}", label, e)),
                        }
                    }
                    let result = HostApplyResult {
                        host: host_for_thread.clone(),
                        ok: failed.is_empty(),
                        message: apply_message(&applied, &failed),
                    };
                    crate::logger::info(
                        "STATUSLINE",
                        &format!(
                            "apply host={} ok={} msg={}",
                            result.host,
                            result.ok,
                            preview(&result.message, 200)
                        ),
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

/// A partial apply must read as partial: naming only the failure would hide that the other CLI was rewritten.
fn apply_message(applied: &[&str], failed: &[String]) -> String {
    if failed.is_empty() {
        return format!("Applied: {}", applied.join(", "));
    }
    if applied.is_empty() {
        return failed.join(" | ");
    }
    format!(
        "Applied: {} | Failed - {}",
        applied.join(", "),
        failed.join(" | ")
    )
}

fn preview(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.len() > max {
        let mut end = max;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        s[..end].replace('\n', " ")
    } else {
        s.replace('\n', " ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A transcript of the Vue component's defaults (the real SSOT); `generated_defaults_match_template` proves it is still accurate.
    fn test_config() -> StatuslineConfig {
        let f = |key: &str, enabled: bool, color: &str| StatuslineField {
            key: key.to_string(),
            enabled,
            color: color.to_string(),
        };
        StatuslineConfig {
            fields: vec![
                f("cli_tag", true, ""),
                f("account", true, "grey"),
                f("identity_user", true, "white"),
                f("identity_host", true, "white"),
                f("cwd", true, "magenta"),
                f("model", true, "cyan"),
                f("effort", true, "grey"),
                f("context", true, "white"),
                f("cache", true, ""),
                f("cache_pct", true, "grey"),
                f("cache_tokens", false, "grey"),
                f("rate_limits_5h", true, "white"),
                f("rate_reset_5h", true, "grey"),
                f("rate_limits_7d", true, "white"),
                f("rate_reset_7d", true, "grey"),
                f("session", true, "grey"),
                f("git_branch", true, "magenta"),
                f("ram", true, "grey"),
            ],
            thresholds: StatuslineThresholds {
                green: 20,
                yellow: 51,
                orange: 75,
                red: 90,
            },
            trunc: StatuslineTrunc {
                account: 4,
                user: 5,
                host: 6,
                cwd: 12,
                branch: 10,
            },
            zebra: StatuslineZebra { a: 16, b: 235 },
            separate: true,
        }
    }

    fn gen(config: &StatuslineConfig) -> String {
        generate_statusline_script(config).expect("generate")
    }

    /// A negative percentage (the old AGY `-2400%` bug), matched by shape since hostnames and model ids contain dashes.
    fn has_negative_percent(s: &str) -> bool {
        let c: Vec<char> = s.chars().collect();
        (0..c.len()).any(|i| {
            if c[i] != '-' {
                return false;
            }
            let mut j = i + 1;
            while j < c.len() && c[j].is_ascii_digit() {
                j += 1;
            }
            j > i + 1 && j < c.len() && c[j] == '%'
        })
    }

    /// The line without ANSI escapes: a color change between `Sonnet5` and `med` would fail a text assertion on the raw line.
    fn plain(line: &str) -> String {
        let mut out = String::with_capacity(line.len());
        let mut chars = line.chars();
        while let Some(c) = chars.next() {
            if c == '\u{1b}' {
                for e in chars.by_ref() {
                    if e.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    /// Anti-drift: the UI defaults must regenerate the checked-in template byte for byte.
    #[test]
    fn generated_defaults_match_template() {
        assert_eq!(gen(&test_config()), TEMPLATE);
    }

    #[test]
    fn generated_script_is_valid_shell() {
        let mut cfg = test_config();
        // Not the default config: an all-off config is the shape most likely to emit a dangling gate or an empty BLOCK_ORDER, so that is what gets syntax-checked.
        for f in cfg.fields.iter_mut() {
            f.enabled = false;
        }
        for script in [gen(&test_config()), gen(&cfg)] {
            let mut child = std::process::Command::new("bash")
                .arg("-n")
                .stdin(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .expect("spawn bash");
            use std::io::Write;
            child
                .stdin
                .as_mut()
                .unwrap()
                .write_all(script.as_bytes())
                .unwrap();
            let out = child.wait_with_output().unwrap();
            assert!(
                out.status.success(),
                "bash -n failed:\n{}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }

    #[test]
    fn every_gate_the_template_reads_is_generated() {
        let script = gen(&test_config());
        for key in EN_KEYS {
            assert!(
                script.contains(&format!("EN_{}=", key)),
                "EN_{} declared in EN_KEYS but never emitted",
                key
            );
        }
        // And the reverse: a gate the body reads but the generator never writes would be an always-empty variable, i.e. a silently invisible block.
        for line in TEMPLATE.lines() {
            for token in line.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
                // A bare `EN_` with nothing after it is prose in the template's comments, not a variable reference - only a real name is a claim about a generated flag.
                if let Some(key) = token.strip_prefix("EN_").filter(|k| !k.is_empty()) {
                    assert!(
                        EN_KEYS.contains(&key),
                        "template reads EN_{} but the generator never emits it",
                        key
                    );
                }
            }
        }
        // Same contract for the color variables - a COLOR_ the body reads but nobody writes is a dead picker in the UI (the exact bug COLOR_cwd shipped with).
        for line in TEMPLATE.lines() {
            for token in line.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
                if let Some(key) = token.strip_prefix("COLOR_").filter(|k| !k.is_empty()) {
                    assert!(
                        COLOR_KEYS.contains(&key),
                        "template reads COLOR_{} but the generator never emits it",
                        key
                    );
                }
            }
        }
    }

    #[test]
    fn a_disabled_parent_switches_its_children_off() {
        let mut cfg = test_config();
        for f in cfg.fields.iter_mut() {
            if f.key == "cache" {
                f.enabled = false;
            }
        }
        let script = gen(&cfg);
        // The children keep their own stored state (non-destructive gate) but must arrive as 0.
        assert!(script.contains("EN_cache=0"));
        assert!(
            script.contains("EN_cache_pct=0"),
            "cache_pct survived its parent being off"
        );
        assert!(
            cfg.fields.iter().any(|f| f.key == "cache_pct" && f.enabled),
            "the config itself must not be mutated - the gate is non-destructive"
        );
    }

    #[test]
    fn block_order_follows_the_field_order() {
        let mut cfg = test_config();
        // Drag RAM to the front.
        let ram = cfg.fields.iter().position(|f| f.key == "ram").unwrap();
        let ram = cfg.fields.remove(ram);
        cfg.fields.insert(0, ram);
        let script = gen(&cfg);
        assert!(
            script.contains(
                r#"BLOCK_ORDER="ram identity cwd model context cache quota session git_branch""#
            ),
            "BLOCK_ORDER did not follow the row order"
        );
    }

    #[test]
    fn out_of_range_values_are_clamped_not_defaulted() {
        let mut cfg = test_config();
        cfg.zebra = StatuslineZebra { a: 100, b: 255 };
        cfg.thresholds = StatuslineThresholds {
            green: 90,
            yellow: 20,
            orange: 75,
            red: 51,
        };
        let script = gen(&cfg);
        assert!(
            script.contains("BG_ZEBRA_A=232"),
            "a shade outside the neutral ramp was not clamped"
        );
        assert!(script.contains("BG_ZEBRA_B=255"));
        // Sorted ascending, so the ladder can never come out inverted.
        assert!(script.contains("THRESH_GREEN=20") && script.contains("THRESH_RED=90"));
    }

    #[test]
    fn renders_a_line() {
        let payload = r#"{"cwd":"/tmp/demo","model":{"display_name":"Sonnet 5"},"cost":{"total_cost_usd":8.4,"total_duration_ms":720000,"total_lines_added":122,"total_lines_removed":52},"context_window":{"total_input_tokens":120000,"total_output_tokens":14400,"context_window_size":1000000,"current_usage":{"cache_read_input_tokens":12400,"cache_creation_input_tokens":800,"input_tokens":32000}},"effort":{"level":"medium"},"rate_limits":{"five_hour":{"used_percentage":42,"resets_at":0},"seven_day":{"used_percentage":92,"resets_at":0}},"workspace":{"git_branch":"master"}}"#;
        let (line, _) = run_script("sl.sh", &gen(&test_config()), payload, &[]);
        println!("RENDERED>>>{}<<<", line);
        let p = plain(&line);
        assert!(p.contains("Sonnet5med"), "model+effort not glued: {}", p);
        assert!(
            p.contains("42%") && p.contains("92%"),
            "quota missing: {}",
            p
        );
        assert!(p.contains("master"), "branch missing: {}", p);
    }

    #[test]
    fn agy_renders_a_line() {
        // $0 decides the CLI, so the AGY case is the same script under a ~/.gemini/ path.
        let payload = r#"{"cwd":"/tmp/demo","account":{"email":"user-a@example.com"},"model":"gemini-2.5-flash","quota":{"gemini-5h":{"remaining_fraction":0.25},"gemini-weekly":{"remaining_fraction":0.5}}}"#;
        let (line, _) = run_script(
            ".gemini/antigravity-cli/statusline.sh",
            &gen(&test_config()),
            payload,
            &[],
        );
        println!("AGY RENDERED>>>{}<<<", line);
        let p = plain(&line);
        assert!(
            p.starts_with("AG"),
            "AGY path did not identify as AG: {}",
            p
        );
        assert!(p.contains("user"), "account not rendered: {}", p);
        // A remaining fraction of 0.25 is 75% used - never -2400%.
        assert!(
            !has_negative_percent(&p),
            "negative percentage in agy line: {}",
            p
        );
        assert!(p.contains("75%"), "5h used% wrong (want 75%): {}", p);
        assert!(p.contains("50%"), "7d used% wrong (want 50%): {}", p);
    }

    #[test]
    fn agy_renders_argon_quota() {
        let payload = r#"{"cwd":"/tmp/demo","account":{"email":"user-a@example.com"},"model":"Gemini 4 Argon","quota":{"gemini-weekly":{"remaining_fraction":0.8},"gemini-5h":{"remaining_fraction":0.9},"gemini-argon-wtu-weekly":{"remaining_fraction":0.4}}}"#;
        let (line, _) = run_script(
            ".gemini/antigravity-cli/agy_argon.sh",
            &gen(&test_config()),
            payload,
            &[],
        );
        println!("AGY ARGON RENDERED>>>{}<<<", line);
        let p = plain(&line);
        assert!(
            p.starts_with("AG"),
            "AGY path did not identify as AG: {}",
            p
        );
        assert!(p.contains("4Argon"), "model not compacted to 4Argon: {}", p);
        assert!(p.contains("60%"), "Argon weekly quota wrong (want 60%): {}", p);
        assert!(!p.contains("20%"), "stray gemini-weekly quota rendered: {}", p);
        assert!(!p.contains("5h:"), "5h quota should be omitted for Argon: {}", p);
        assert!(p.contains("7d:"), "7d quota missing for Argon: {}", p);
    }

    /// Whatever shape a CLI reports the model in, dropping the vendor word must leave one token with no stray punctuation at either edge.
    #[test]
    fn the_vendor_word_leaves_no_stray_punctuation_behind() {
        let cases = [
            ("gemini-2.5-flash", "2.5-flash"),
            ("claude-sonnet-4-5", "sonnet-4-5"),
            ("Claude Opus 4.8 (medium)", "Opus4.8"),
            ("claude_opus_4_8", "opus_4_8"),
            ("gpt-5.1-codex", "gpt-5.1-codex"),
        ];
        for (raw, want) in cases {
            let payload = format!(r#"{{"cwd":"/tmp/demo","model":"{}"}}"#, raw);
            let (line, _) = run_script("model.sh", &gen(&test_config()), &payload, &[]);
            let p = plain(&line);
            assert!(
                p.contains(want),
                "{:?} should render as {:?}: {}",
                raw,
                want,
                p
            );
            assert!(
                !p.contains(&format!("-{}", want)) && !p.contains(&format!("{}-", want)),
                "{:?} rendered with a leftover separator: {}",
                raw,
                p
            );
        }
    }

    #[test]
    fn agy_never_touches_the_claude_rate_limit_cache() {
        let payload = r#"{"cwd":"/tmp/demo","model":"gemini-2.5-flash","quota":{"gemini-5h":{"remaining_fraction":0.25}}}"#;
        let cache =
            r#"{"account":"","rate_limits":{"five_hour":{"used_percentage":42,"resets_at":0}}}"#;
        // `ram` is off: it prints the real machine's memory use, which failed this broad 42% check on any host sitting at 42% RAM.
        let mut cfg = test_config();
        for field in &mut cfg.fields {
            if field.key == "ram" {
                field.enabled = false;
            }
        }
        let (line, home) = run_script(
            ".gemini/antigravity-cli/agy_rlcache.sh",
            &gen(&cfg),
            payload,
            &[(".claude/rate-limits/me@example.com.json", cache)],
        );
        assert!(
            !plain(&line).contains("42%"),
            "AGY read Claude Code's rate-limit cache: {}",
            plain(&line)
        );
        let after =
            std::fs::read_to_string(home.join(".claude/rate-limits/me@example.com.json")).unwrap();
        assert_eq!(after, cache, "AGY rewrote Claude Code's rate-limit cache");
    }

    // ---- helpers -------------------------------------------------------------------------

    /// Runs a script against a payload in a private $HOME. `name` is the invocation path (decides CC vs AG); `files` are HOME-relative fixtures.
    fn run_script(
        name: &str,
        script: &str,
        payload: &str,
        files: &[(&str, &str)],
    ) -> (String, std::path::PathBuf) {
        run_script_env(name, script, payload, files, &[])
    }

    /// `run_script` with extra env; a `claude` stub shadows the real CLI so no test reaches the real login.
    fn run_script_env(
        name: &str,
        script: &str,
        payload: &str,
        files: &[(&str, &str)],
        env: &[(&str, String)],
    ) -> (String, std::path::PathBuf) {
        run_script_aged(name, script, payload, files, env, &[])
    }

    /// `run_script_env` with fixtures back-dated before the run: `aged` is (HOME-relative path, seconds old). A fixture named `bin/claude` replaces the always-failing stub.
    fn run_script_aged(
        name: &str,
        script: &str,
        payload: &str,
        files: &[(&str, &str)],
        env: &[(&str, String)],
        aged: &[(&str, u64)],
    ) -> (String, std::path::PathBuf) {
        let home =
            std::env::temp_dir().join(format!("aki-statusline-test/{}", name.replace('/', "_")));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        for (rel, contents) in files {
            let p = home.join(rel);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&p, contents).unwrap();
        }
        for (rel, age_secs) in aged {
            std::fs::OpenOptions::new()
                .write(true)
                .open(home.join(rel))
                .unwrap()
                .set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(*age_secs))
                .unwrap();
        }
        let path = home.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, script).unwrap();
        let bin = home.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        if !bin.join("claude").exists() {
            std::fs::write(bin.join("claude"), "#!/bin/sh\nexit 1\n").unwrap();
        }
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(bin.join("claude"), std::fs::Permissions::from_mode(0o755))
                .unwrap();
        }
        let path_env = format!(
            "{}:{}",
            bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let out = std::process::Command::new("bash")
            .arg(&path)
            .env("HOME", &home)
            .env("PATH", path_env)
            // If run from inside a Claude Code session, CLAUDE_CONFIG_DIR is inherited and overrides this fake HOME, leaking the real account into the script's output.
            .env_remove("CLAUDE_CONFIG_DIR")
            .envs(env.iter().map(|(k, v)| (*k, v.as_str())))
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut c| {
                use std::io::Write;
                c.stdin.as_mut().unwrap().write_all(payload.as_bytes())?;
                c.wait_with_output()
            })
            .unwrap();
        (String::from_utf8_lossy(&out.stdout).to_string(), home)
    }

    // ---- behavioural tests, per docs/plan/done/1.18.0-statusline-apply-correctness.md §P2-4 ------

    #[test]
    fn cc_account_reads_the_fresh_auth_cache() {
        // Payload deliberately carries no email - Claude Code never sends one.
        let payload = r#"{"cwd":"/tmp/demo","model":{"display_name":"Sonnet 5"}}"#;
        let (line, _) = run_script(
            "cc_account.sh",
            &gen(&test_config()),
            payload,
            &[(
                ".claude/auth-cache-default.json",
                r#"{"email":"disk-user@example.com"}"#,
            )],
        );
        println!("CC ACCOUNT>>>{}<<<", line);
        // Truncated to TRUNC_ACCOUNT after the domain is dropped: "disk-user@..." -> "disk".
        let p = plain(&line);
        assert!(
            p.contains("disk"),
            "account from the fresh auth cache did not render: {}",
            p
        );
        assert!(
            !p.contains("example.com"),
            "the domain was printed instead of stripped: {}",
            p
        );
    }

    #[test]
    fn cc_rate_limits_survive_a_payload_that_omits_them() {
        let payload = r#"{"cwd":"/tmp/demo","model":{"display_name":"Sonnet 5"}}"#;
        let cache = r#"{"account":"","rate_limits":{"five_hour":{"used_percentage":42,"resets_at":0},"seven_day":{"used_percentage":92,"resets_at":0}}}"#;
        let (line, _) = run_script(
            "cc_rlcache.sh",
            &gen(&test_config()),
            payload,
            &[
                (
                    ".claude/auth-cache-default.json",
                    r#"{"email":"me@example.com"}"#,
                ),
                (".claude/rate-limits/me@example.com.json", cache),
            ],
        );
        println!("CC RLCACHE>>>{}<<<", line);
        let p = plain(&line);
        assert!(p.contains("42%"), "5h not restored from cache: {}", p);
        assert!(p.contains("92%"), "7d not restored from cache: {}", p);
    }

    #[test]
    fn cc_rate_limits_merge_instead_of_overwrite() {
        // Payload has only five_hour; seven_day must survive from the cache, and the rewritten cache must still hold both.
        let payload = r#"{"cwd":"/tmp/demo","model":{"display_name":"Sonnet 5"},"rate_limits":{"five_hour":{"used_percentage":10,"resets_at":0}}}"#;
        let cache = r#"{"account":"","rate_limits":{"five_hour":{"used_percentage":42,"resets_at":0},"seven_day":{"used_percentage":92,"resets_at":0}}}"#;
        let (line, home) = run_script(
            "cc_rlmerge.sh",
            &gen(&test_config()),
            payload,
            &[
                (
                    ".claude/auth-cache-default.json",
                    r#"{"email":"me@example.com"}"#,
                ),
                (".claude/rate-limits/me@example.com.json", cache),
            ],
        );
        println!("CC RLMERGE>>>{}<<<", line);
        let p = plain(&line);
        assert!(p.contains("10%"), "fresh 5h value not used: {}", p);
        assert!(p.contains("92%"), "cached 7d lost on merge: {}", p);
        let written =
            std::fs::read_to_string(home.join(".claude/rate-limits/me@example.com.json")).unwrap();
        assert!(
            written.contains("seven_day"),
            "rewritten cache dropped seven_day: {}",
            written
        );
    }

    /// Renders `payload_pct` over a cache holding 100% for `cached_reset`, the payload's window ending at `payload_reset`; returns the line and the rewritten cache.
    fn render_stale_session_over_full_cache(name: &str, payload_pct: u32, cached_reset: u64, payload_reset: u64) -> (String, String) {
        let payload = format!(
            r#"{{"cwd":"/tmp/demo","rate_limits":{{"five_hour":{{"used_percentage":{},"resets_at":{}}}}}}}"#,
            payload_pct, payload_reset
        );
        let cache = format!(
            r#"{{"account":"me@example.com","rate_limits":{{"five_hour":{{"used_percentage":100,"resets_at":{}}}}}}}"#,
            cached_reset
        );
        let (line, home) = run_script(
            name,
            &gen(&test_config()),
            &payload,
            &[
                (".claude/auth-cache-default.json", r#"{"email":"me@example.com"}"#),
                (".claude/rate-limits/me@example.com.json", cache.as_str()),
            ],
        );
        let written = std::fs::read_to_string(home.join(".claude/rate-limits/me@example.com.json")).unwrap();
        (plain(&line), written)
    }

    #[test]
    fn cc_stale_session_cannot_lower_usage_inside_one_window() {
        let reset = now_epoch() + 3600;
        let (p, written) = render_stale_session_over_full_cache("cc_rl_monotone.sh", 83, reset, reset);
        assert!(p.contains("100%"), "stale 83% replaced the 100% reading: {}", p);
        assert!(written.contains(r#""used_percentage":100"#), "cache lost the 100%: {}", written);
    }

    #[test]
    fn cc_new_window_replaces_the_previous_windows_usage() {
        let now = now_epoch();
        let (p, written) = render_stale_session_over_full_cache("cc_rl_new_window.sh", 3, now + 600, now + 18000);
        assert!(p.contains("3%"), "new window still shows the old usage: {}", p);
        assert!(written.contains(r#""used_percentage":3"#), "cache kept the old window's 100%: {}", written);
    }

    fn now_epoch() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    #[test]
    fn cc_drops_a_cached_quota_whose_reset_has_passed() {
        // The 1.10.0-1.17.0 bug: an entry the account no longer has (here: seven_day, already past its reset) stayed in the cache forever and rendered as a phantom "7d 45%".
        let now = now_epoch();
        let payload = format!(
            r#"{{"cwd":"/tmp/demo","rate_limits":{{"five_hour":{{"used_percentage":50,"resets_at":{}}}}}}}"#,
            now + 7200
        );
        let cache = format!(
            r#"{{"account":"","rate_limits":{{"five_hour":{{"used_percentage":42,"resets_at":{}}},"seven_day":{{"used_percentage":45,"resets_at":{}}}}}}}"#,
            now + 7200,
            now - 3600
        );
        let (line, home) = run_script(
            "cc_rlexpired.sh",
            &gen(&test_config()),
            &payload,
            &[
                (
                    ".claude/auth-cache-default.json",
                    r#"{"email":"me@example.com"}"#,
                ),
                (".claude/rate-limits/me@example.com.json", cache.as_str()),
            ],
        );
        println!("CC RLEXPIRED>>>{}<<<", line);
        let p = plain(&line);
        assert!(p.contains("50%"), "live 5h lost: {}", p);
        assert!(!p.contains("45%"), "expired 7d still rendered: {}", p);
        let written =
            std::fs::read_to_string(home.join(".claude/rate-limits/me@example.com.json")).unwrap();
        assert!(
            !written.contains("seven_day"),
            "expired entry kept in cache: {}",
            written
        );
    }

    fn quota_file(pct: u32, org: &str) -> String {
        format!(
            r#"{{"account":"x","org_id":"{}","rate_limits":{{"seven_day":{{"used_percentage":{},"resets_at":{}}}}}}}"#,
            org,
            pct,
            now_epoch() + 7200
        )
    }

    #[test]
    fn cc_ignores_a_cache_written_by_another_account() {
        let mine = quota_file(30, "");
        let theirs = quota_file(45, "");
        let (line, home) = run_script(
            "cc_rlforeign.sh",
            &gen(&test_config()),
            r#"{"cwd":"/tmp/demo","model":{"display_name":"Sonnet 5"}}"#,
            &[
                (
                    ".claude/auth-cache-default.json",
                    r#"{"email":"me@example.com"}"#,
                ),
                (".claude/rate-limits/me@example.com.json", mine.as_str()),
                (
                    ".claude/rate-limits/other@example.com.json",
                    theirs.as_str(),
                ),
            ],
        );
        println!("CC RLFOREIGN>>>{}<<<", line);
        let p = plain(&line);
        assert!(p.contains("30%"), "this account's own quota lost: {}", p);
        assert!(
            !p.contains("45%"),
            "another account's cached quota leaked into this line: {}",
            p
        );
        let after =
            std::fs::read_to_string(home.join(".claude/rate-limits/other@example.com.json"))
                .unwrap();
        assert_eq!(
            after, theirs,
            "this session rewrote another account's quota file"
        );
    }

    #[test]
    fn cc_config_dir_slot_picks_its_own_account() {
        // Two logins share one config dir: CLAUDE_CONFIG_DIR set (hashed slot) vs unset (default slot).
        let home = std::env::temp_dir().join("aki-statusline-test/cc_slot.sh");
        let config_dir = home.join(".claude").display().to_string();
        let slot = String::from_utf8(
            std::process::Command::new("sh")
                .arg("-c")
                .arg("printf '%s' \"$1\" | shasum -a 256 | cut -c1-8")
                .arg("sh")
                .arg(&config_dir)
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap();
        let slot_auth = format!(".claude/auth-cache-{}.json", slot.trim());
        let mine = quota_file(30, "");
        let theirs = quota_file(45, "");
        let (line, _) = run_script_env(
            "cc_slot.sh",
            &gen(&test_config()),
            r#"{"cwd":"/tmp/demo","model":{"display_name":"Sonnet 5"}}"#,
            &[
                (
                    ".claude/auth-cache-default.json",
                    r#"{"email":"me@example.com"}"#,
                ),
                (slot_auth.as_str(), r#"{"email":"other@example.com"}"#),
                (".claude/rate-limits/me@example.com.json", mine.as_str()),
                (
                    ".claude/rate-limits/other@example.com.json",
                    theirs.as_str(),
                ),
            ],
            &[("CLAUDE_CONFIG_DIR", config_dir.clone())],
        );
        println!("CC SLOT>>>{}<<<", line);
        let p = plain(&line);
        assert!(
            p.contains("45%"),
            "the slot's own account quota not shown: {}",
            p
        );
        assert!(!p.contains("30%"), "the default slot's quota leaked: {}", p);
    }

    #[test]
    fn cc_drops_a_cache_from_a_recreated_account() {
        // Same address, new org: the old plan's numbers must not carry over.
        let old = quota_file(45, "org-old");
        let (line, _) = run_script(
            "cc_rlorg.sh",
            &gen(&test_config()),
            r#"{"cwd":"/tmp/demo","model":{"display_name":"Sonnet 5"}}"#,
            &[
                (
                    ".claude/auth-cache-default.json",
                    r#"{"email":"me@example.com","orgId":"org-new"}"#,
                ),
                (".claude/rate-limits/me@example.com.json", old.as_str()),
            ],
        );
        println!("CC RLORG>>>{}<<<", line);
        let p = plain(&line);
        assert!(
            !p.contains("45%"),
            "a recreated account inherited the old quota: {}",
            p
        );
        assert!(p.contains("Sonnet5"), "rest of the line lost: {}", p);
    }

    #[test]
    fn cc_survives_a_corrupt_rate_limits_cache() {
        let payload = r#"{"cwd":"/tmp/demo","model":{"display_name":"Sonnet 5"},"context_window":{"total_input_tokens":100,"total_output_tokens":20,"context_window_size":1000}}"#;
        let (line, _) = run_script(
            "cc_rlcorrupt.sh",
            &gen(&test_config()),
            payload,
            &[
                (
                    ".claude/auth-cache-default.json",
                    r#"{"email":"me@example.com"}"#,
                ),
                (
                    ".claude/rate-limits/me@example.com.json",
                    "{not json at all",
                ),
            ],
        );
        println!("CC RLCORRUPT>>>{}<<<", line);
        assert!(
            !line.trim().is_empty(),
            "corrupt cache blanked the whole statusline"
        );
        assert!(
            plain(&line).contains("Sonnet5"),
            "corrupt cache lost the rest of the line: {}",
            plain(&line)
        );
    }

    #[test]
    fn agy_account_falls_back_to_google_accounts_object() {
        let payload = r#"{"cwd":"/tmp/demo","model":"gemini-2.5-flash"}"#;
        let (line, _) = run_script(
            ".gemini/antigravity-cli/agy_account.sh",
            &gen(&test_config()),
            payload,
            &[(
                ".gemini/google_accounts.json",
                r#"{"active":"agy-user@example.com","old":["someone@example.com"]}"#,
            )],
        );
        println!("AGY ACCOUNT>>>{}<<<", line);
        assert!(
            plain(&line).contains("agy-"),
            "agy account fallback did not render: {}",
            plain(&line)
        );
    }

    #[test]
    fn agy_reset_eta_includes_minutes() {
        // 5400s = 1h30m - the old `printf '%dh%dm' "$h"` printed 1h0m.
        let payload = r#"{"cwd":"/tmp/demo","model":"gemini-2.5-flash","quota":{"gemini-5h":{"remaining_fraction":0.4,"reset_in_seconds":5400}}}"#;
        let (line, _) = run_script(
            ".gemini/antigravity-cli/agy_eta.sh",
            &gen(&test_config()),
            payload,
            &[],
        );
        println!("AGY ETA>>>{}<<<", line);
        assert!(
            plain(&line).contains("1h30m"),
            "reset ETA lost its minutes: {}",
            plain(&line)
        );
    }

    #[test]
    fn a_disabled_reset_hides_only_the_eta() {
        let mut cfg = test_config();
        for f in cfg.fields.iter_mut() {
            if f.key == "rate_reset_5h" {
                f.enabled = false;
            }
        }
        let payload = r#"{"cwd":"/tmp/demo","model":"gemini-2.5-flash","quota":{"gemini-5h":{"remaining_fraction":0.4,"reset_in_seconds":5400}}}"#;
        let (line, _) = run_script(
            ".gemini/antigravity-cli/agy_noeta.sh",
            &gen(&cfg),
            payload,
            &[],
        );
        let p = plain(&line);
        assert!(
            p.contains("60%"),
            "the 5h reading itself disappeared: {}",
            p
        );
        assert!(
            !p.contains("1h30m"),
            "reset ETA rendered while switched off: {}",
            p
        );
    }

    // From here on the config is built from the JSON the customizer really posts, which alone catches a serde shape mismatch.

    /// Verbatim `JSON.stringify(defaultLocalConfig())` from ClaudeSettingModal.vue, `version` and all. If the UI's defaults change, this string changes with them - that is the point.
    const VUE_DEFAULT_JSON: &str = r#"{
      "fields": [
        {"key":"cli_tag","enabled":true,"color":""},
        {"key":"account","enabled":true,"color":"grey"},
        {"key":"identity_user","enabled":true,"color":"white"},
        {"key":"identity_host","enabled":true,"color":"white"},
        {"key":"cwd","enabled":true,"color":"magenta"},
        {"key":"model","enabled":true,"color":"cyan"},
        {"key":"effort","enabled":true,"color":"grey"},
        {"key":"context","enabled":true,"color":"white"},
        {"key":"cache","enabled":true,"color":""},
        {"key":"cache_pct","enabled":true,"color":"grey"},
        {"key":"cache_tokens","enabled":false,"color":"grey"},
        {"key":"rate_limits_5h","enabled":true,"color":"white"},
        {"key":"rate_reset_5h","enabled":true,"color":"grey"},
        {"key":"rate_limits_7d","enabled":true,"color":"white"},
        {"key":"rate_reset_7d","enabled":true,"color":"grey"},
        {"key":"session","enabled":true,"color":"grey"},
        {"key":"git_branch","enabled":true,"color":"magenta"},
        {"key":"ram","enabled":true,"color":"grey"}
      ],
      "thresholds": {"green":20,"yellow":51,"orange":75,"red":90},
      "trunc": {"account":4,"user":5,"host":6,"cwd":12,"branch":10},
      "zebra": {"a":16,"b":235},
      "separate": true,
      "version": 3
    }"#;

    fn from_json(json: &str) -> StatuslineConfig {
        serde_json::from_str(json).expect("the UI payload must deserialize")
    }

    /// End-to-end anti-drift: the UI's real JSON, unmodified, must rebuild the checked-in script byte for byte.
    #[test]
    fn the_ui_payload_reproduces_the_template() {
        assert_eq!(gen(&from_json(VUE_DEFAULT_JSON)), TEMPLATE);
    }

    #[test]
    fn a_field_the_backend_does_not_know_is_ignored_not_fatal() {
        // `version` is UI-only bookkeeping, and a future UI may add more. Unknown keys must not break Apply for everyone on an older build.
        let with_extra = VUE_DEFAULT_JSON.replace(
            r#""version": 3"#,
            r#""version": 4, "somethingTheUiAddedLater": {"x": 1}"#,
        );
        assert_eq!(gen(&from_json(&with_extra)), TEMPLATE);
    }

    #[test]
    fn a_missing_section_is_rejected_rather_than_silently_defaulted() {
        // The deliberate absence of #[serde(default)] - see the comment on StatuslineConfig. A half-written payload must fail the Apply, not produce a script from values nobody chose.
        let without_trunc = VUE_DEFAULT_JSON.replace(
            r#""trunc": {"account":4,"user":5,"host":6,"cwd":12,"branch":10},"#,
            "",
        );
        assert!(
            serde_json::from_str::<StatuslineConfig>(&without_trunc).is_err(),
            "a payload with no trunc section was accepted"
        );
    }

    /// Every UI toggle flipped alone: its marker appears exactly while it is on, and flipping it changes the line (how a dead CWD color picker shipped).
    #[test]
    fn every_toggle_flips_its_own_output_and_nothing_else() {
        // `+ 45` / `+ 1_800` fix a flake: each run truncates against its own clock, so round values flip `1h30m` to `1h29m` mid-sweep. Keep them off the boundary.
        let now = now_epoch();
        let payload = format!(
            r#"{{"cwd":"/tmp/Aki-Dev-Sync","model":{{"display_name":"Opus 4.8"}},"effort":{{"level":"medium"}},
            "cost":{{"total_cost_usd":8.4,"total_duration_ms":720000,"total_lines_added":12,"total_lines_removed":5}},
            "context_window":{{"total_input_tokens":120000,"total_output_tokens":14400,"context_window_size":1000000,
              "current_usage":{{"cache_read_input_tokens":44400,"cache_creation_input_tokens":800,"input_tokens":32000}}}},
            "rate_limits":{{"five_hour":{{"used_percentage":19,"resets_at":{}}},
                           "seven_day":{{"used_percentage":56,"resets_at":{}}}}},
            "workspace":{{"git_branch":"master"}}}}"#,
            now + 5_400 + 45,
            now + 90_000 + 1_800
        );
        let payload = payload.as_str();
        let files = [(
            ".claude/auth-cache-default.json",
            r#"{"email":"ntu-gen@example.com"}"#,
        )];
        // The host half of the identity block is whatever this machine is called, cut to TRUNC_HOST.
        let host = String::from_utf8(
            std::process::Command::new("hostname")
                .arg("-s")
                .output()
                .expect("hostname")
                .stdout,
        )
        .expect("hostname is utf-8");
        let host: String = host.trim().chars().take(6).collect();
        assert!(!host.is_empty(), "this machine reports no hostname");

        let cfg = from_json(VUE_DEFAULT_JSON);
        let (base, _) = run_script("toggles_base.sh", &gen(&cfg), payload, &files);
        let base = plain(&base);

        // (field key, text it owns, on by default). Off-by-default rows must ADD the marker when on, so a dead switch cannot hide behind a default.
        let cases: &[(&str, &str, bool)] = &[
            ("cli_tag", "CC", true),
            ("account", "ntu-", true),
            ("identity_user", "@", true),
            ("identity_host", host.as_str(), true),
            ("cwd", "Aki-D", true),
            ("model", "Opus", true),
            ("effort", "med", true),
            ("context", "ctx", true),
            ("cache", "\u{21ac}", true),
            ("cache_pct", "\u{21ac}", true),
            ("cache_tokens", "44k", false),
            ("rate_limits_5h", "5h:", true),
            ("rate_reset_5h", "1h30m", true),
            ("rate_limits_7d", "7d:", true),
            ("rate_reset_7d", "1d1h", true),
            ("session", "ss", true),
            ("git_branch", "master", true),
            ("ram", "\u{2685}", true),
        ];
        // A gate added to the template without a row here would otherwise ship untested.
        for key in EN_KEYS {
            assert!(
                cases.iter().any(|(k, _, _)| k == key),
                "{} has no row in the toggle sweep",
                key
            );
        }

        for (key, marker, on_by_default) in cases {
            let mut flipped = from_json(VUE_DEFAULT_JSON);
            for f in flipped.fields.iter_mut() {
                if &f.key == key {
                    f.enabled = !*on_by_default;
                }
            }
            let (line, _) =
                run_script(&format!("flip_{}.sh", key), &gen(&flipped), payload, &files);
            let line = plain(&line);
            let (with, without) = if *on_by_default {
                (&base, &line)
            } else {
                (&line, &base)
            };
            assert!(
                with.contains(marker),
                "{} is on but {:?} is missing: {}",
                key,
                marker,
                with
            );
            assert!(
                !without.contains(marker),
                "{} is off but {:?} is still rendered: {}",
                key,
                marker,
                without
            );
            assert_ne!(line, base, "flipping {} changed nothing", key);
        }
    }

    /// The same sweep for the settings that are not on/off. Each one is checked against the escape codes, since that is where a width, a shade or a color actually lands.
    #[test]
    fn the_numeric_and_color_settings_reach_the_rendered_line() {
        let payload = r#"{"cwd":"/tmp/Aki-Dev-Sync","model":{"display_name":"Opus 4.8"},"workspace":{"git_branch":"master"}}"#;

        let mut cfg = from_json(VUE_DEFAULT_JSON);
        cfg.trunc.cwd = 4;
        cfg.trunc.branch = 3;
        let (line, _) = run_script("trunc.sh", &gen(&cfg), payload, &[]);
        let p = plain(&line);
        assert!(
            p.contains("Aki-") && !p.contains("Aki-Dev"),
            "cwd not cut to 4: {}",
            p
        );
        assert!(
            p.contains("mas") && !p.contains("master"),
            "branch not cut to 3: {}",
            p
        );

        let mut cfg = from_json(VUE_DEFAULT_JSON);
        for f in cfg.fields.iter_mut() {
            if f.key == "git_branch" {
                f.color = "green".to_string();
            }
        }
        let (line, _) = run_script("color.sh", &gen(&cfg), payload, &[]);
        assert!(
            line.contains("\u{1b}[01;32mmaster"),
            "the branch color picker did not reach the line: {:?}",
            line
        );

        let mut cfg = from_json(VUE_DEFAULT_JSON);
        cfg.zebra = StatuslineZebra { a: 233, b: 240 };
        cfg.separate = false;
        let (line, _) = run_script("zebra.sh", &gen(&cfg), payload, &[]);
        assert!(
            line.contains("\u{1b}[48;5;233m") && line.contains("\u{1b}[48;5;240m"),
            "custom zebra shades not applied: {:?}",
            line
        );
        assert!(
            !line.contains("\u{1b}[48;5;233m "),
            "separate is off but the block is still padded: {:?}",
            line
        );

        // A tighter ladder repaints 19% from the calm tier (xterm 86) to red; the tier codes live in STATUSLINE_TIERS (src/utils/statuslineColors.js).
        let quota = r#"{"cwd":"/tmp/demo","rate_limits":{"five_hour":{"used_percentage":19,"resets_at":0}}}"#;
        let (calm, _) = run_script(
            "ladder_calm.sh",
            &gen(&from_json(VUE_DEFAULT_JSON)),
            quota,
            &[],
        );
        assert!(
            calm.contains("\u{1b}[01;38;5;86m19%"),
            "19% should be the calm tier: {:?}",
            calm
        );
        let mut cfg = from_json(VUE_DEFAULT_JSON);
        cfg.thresholds = StatuslineThresholds {
            green: 5,
            yellow: 10,
            orange: 15,
            red: 18,
        };
        let (hot, _) = run_script("ladder_hot.sh", &gen(&cfg), quota, &[]);
        assert!(
            hot.contains("\u{1b}[01;31m19%"),
            "19% should be red now: {:?}",
            hot
        );
    }

    /// Dragging a row in the UI is just a reorder of `fields`; the printed order must follow it.
    #[test]
    fn dragging_a_row_reorders_the_rendered_line() {
        let payload = r#"{"cwd":"/tmp/demo","context_window":{"total_input_tokens":1000,"total_output_tokens":0,"context_window_size":200000}}"#;
        let mut cfg = from_json(VUE_DEFAULT_JSON);
        let ram = cfg.fields.iter().position(|f| f.key == "ram").unwrap();
        let ram = cfg.fields.remove(ram);
        cfg.fields.insert(1, ram);
        let (line, _) = run_script("drag.sh", &gen(&cfg), payload, &[]);
        let p = plain(&line);
        assert!(
            p.find('\u{2685}').unwrap() < p.find("ctx").unwrap(),
            "RAM dragged to the front but still prints after context: {}",
            p
        );
    }

    /// The installer scripts for a set of ticked targets.
    fn scripts(aliases: &[&str]) -> Vec<(&'static str, String)> {
        let sel: Vec<String> = aliases.iter().map(|a| a.to_string()).collect();
        build_installer_scripts(&test_config(), &sel).expect("build installer scripts")
    }

    /// Every ticked target's script as one blob - for assertions about what an Apply touches overall, as opposed to how it is split.
    fn joined(aliases: &[&str]) -> String {
        scripts(aliases)
            .into_iter()
            .map(|(_, s)| s)
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn no_target_selected_is_an_error_not_a_silent_agy_write() {
        let err = build_installer_scripts(&test_config(), &[]).unwrap_err();
        assert!(
            err.to_lowercase().contains("target"),
            "unexpected error text: {}",
            err
        );
        let cc = joined(&["cc"]);
        assert!(cc.contains("AKI_STATUSLINE_CLAUDE_EOF"));
        assert!(
            !cc.contains("AKI_STATUSLINE_AGY_EOF"),
            "cc-only apply wrote the AGY file too"
        );
        let both = joined(&["cc", "ag"]);
        assert!(
            both.contains("AKI_STATUSLINE_CLAUDE_EOF") && both.contains("AKI_STATUSLINE_AGY_EOF")
        );
    }

    /// §3.19(a): two independently runnable scripts, so a Claude Code failure cannot stop the AGY half.
    #[test]
    fn each_target_is_its_own_independent_script() {
        let both = scripts(&["cc", "ag"]);
        assert_eq!(
            both.len(),
            2,
            "the two targets were not split into two scripts"
        );
        for (label, s) in &both {
            assert!(
                s.starts_with("set -e\n"),
                "{} script does not fail fast on its own",
                label
            );
        }
        // Compared without the shared body, which names both CLIs legitimately; the installer commands must not cross over.
        let body = gen(&test_config());
        let steps = |label: &str| -> String {
            both.iter()
                .find(|(l, _)| *l == label)
                .unwrap()
                .1
                .replace(body.as_str(), "")
        };
        assert!(
            !steps("Claude Code").contains("antigravity-cli"),
            "the CC installer touches AGY's files"
        );
        assert!(
            !steps("AGY").contains(".claude/"),
            "the AGY installer touches Claude Code's files"
        );
    }

    /// §3.19(b): settings first, so a host without `jq` ends unchanged rather than holding an unreferenced script.
    #[test]
    fn settings_are_patched_before_the_script_is_written() {
        for alias in ["cc", "ag"] {
            let s = joined(&[alias]);
            let patch = s.find("jq ").expect("no jq patch");
            let write = s.find("cat > ").expect("no script write");
            assert!(
                patch < write,
                "{}: script is written before the settings patch",
                alias
            );
            assert!(
                s.contains("command -v jq"),
                "{}: missing jq precondition - a missing jq must fail before anything is touched",
                alias
            );
        }
    }

    /// §3.19(c). Every Apply keeps its own backup. The old `[ ! -f "$FILE.aki-bak" ]` guard backed up once ever, so a statusline hand-tuned after the first Apply was destroyed with no copy.
    #[test]
    fn every_apply_backs_up_to_a_timestamped_file() {
        for alias in ["cc", "ag"] {
            let s = joined(&[alias]);
            assert!(
                s.contains(".aki-bak-$(date +%s)"),
                "{}: backup is not timestamped, so a second Apply overwrites the only copy",
                alias
            );
            assert!(
                !s.contains("! -f \"$FILE.aki-bak\"") && !s.contains("! -f \"$FILE_AGY.aki-bak\""),
                "{}: the back-up-once-ever guard is back",
                alias
            );
            let backup = s.find(".aki-bak-").expect("no backup");
            let write = s.find("cat > ").expect("no script write");
            assert!(
                backup < write,
                "{}: backup taken after the file was overwritten",
                alias
            );
        }
    }

    /// Installers run through `ssh host sh` (dash on most remotes); only the outer script is checked, the quoted heredoc body has its own `bash -n` test.
    #[test]
    fn installer_scripts_are_posix_sh_safe() {
        for (label, script) in scripts(&["cc", "ag"]) {
            let mut child = std::process::Command::new("sh")
                .arg("-n")
                .stdin(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .expect("spawn sh");
            use std::io::Write;
            child
                .stdin
                .as_mut()
                .unwrap()
                .write_all(script.as_bytes())
                .unwrap();
            let out = child.wait_with_output().unwrap();
            assert!(
                out.status.success(),
                "{}: sh -n failed:\n{}",
                label,
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }

    /// A partial apply must read as partial: naming only the failure would hide that the other CLI really was rewritten, which is what the user needs to know before retrying.
    #[test]
    fn apply_message_names_what_landed_and_what_did_not() {
        assert_eq!(
            apply_message(&["Claude Code", "AGY"], &[]),
            "Applied: Claude Code, AGY"
        );
        let partial = apply_message(&["AGY"], &["Claude Code: jq not found".to_string()]);
        assert!(
            partial.contains("AGY") && partial.contains("Claude Code: jq not found"),
            "{}",
            partial
        );
        assert_eq!(apply_message(&[], &["AGY: boom".to_string()]), "AGY: boom");
    }

    /// A CLI runs nothing until its settings point at the script; skipping this for AGY left `statusLine.command` empty.
    #[test]
    fn each_target_registers_its_script_in_the_cli_settings() {
        for (alias, settings, script) in [
            (
                "cc",
                ".claude/settings.json",
                ".claude/statusline-command.sh",
            ),
            (
                "ag",
                ".gemini/antigravity-cli/settings.json",
                ".gemini/antigravity-cli/statusline.sh",
            ),
        ] {
            let sh = joined(&[alias]);
            assert!(
                sh.contains(settings),
                "{} never touches {}",
                alias,
                settings
            );
            assert!(
                sh.contains(r#".statusLine.type = "command""#),
                "{} does not set statusLine.type",
                alias
            );
            assert!(
                sh.contains(".statusLine.command = "),
                "{} does not set statusLine.command",
                alias
            );
            assert!(
                sh.contains(script),
                "{} does not name its script path",
                alias
            );
        }
    }

    /// The indicator reads this probe: per CLI, and a script nothing points at is NOT configured.
    #[test]
    fn the_probe_reports_each_cli_separately_and_needs_both_halves() {
        let cc_sh = ".claude/statusline-command.sh";
        let cc_json = ".claude/settings.json";
        let ag_sh = ".gemini/antigravity-cli/statusline.sh";
        let ag_json = ".gemini/antigravity-cli/settings.json";
        let wired = r#"{"statusLine":{"type":"command","command":"statusline.sh"}}"#;
        let empty = r#"{"statusLine":{"type":"","command":""}}"#;

        type Case<'a> = (&'a str, &'a [(&'a str, &'a str)], &'a [&'a str]);
        let cases: &[Case<'_>] = &[
            ("nothing", &[], &["CC_SL=0", "AG_PRESENT=0", "AG_SL=0"]),
            // Script written, settings never patched - the bug this test exists for.
            (
                "ag_half",
                &[(ag_sh, "#!/bin/bash"), (ag_json, empty)],
                &["AG_PRESENT=1", "AG_SL=0", "CC_SL=0"],
            ),
            (
                "ag_full",
                &[(ag_sh, "#!/bin/bash"), (ag_json, wired)],
                &["AG_PRESENT=1", "AG_SL=1", "CC_SL=0"],
            ),
            (
                "cc_full",
                &[
                    (cc_sh, "#!/bin/bash"),
                    (
                        cc_json,
                        r#"{"statusLine":{"command":"statusline-command.sh"}}"#,
                    ),
                ],
                &["CC_PRESENT=1", "CC_SL=1", "AG_SL=0"],
            ),
        ];
        for (name, files, want) in cases {
            let (out, _) = run_script(&format!("probe_{}.sh", name), PROBE, "", files);
            for token in *want {
                assert!(
                    out.contains(token),
                    "{}: probe did not report {}: {}",
                    name,
                    token,
                    out
                );
            }
        }
    }

    /// An upgrade that changes the cache format (CC) or the quota routing (AGY) must reach hosts already wired up, or their reading silently stays on the old behavior.
    #[test]
    fn the_probe_flags_an_older_hook_as_outdated_per_cli() {
        let cc_sh = ".claude/statusline-command.sh";
        let cc_json = ".claude/settings.json";
        let wired = r#"{"statusLine":{"command":"statusline-command.sh"}}"#;
        let shipped = marker_version(TEMPLATE, RLCACHE_MARKER).expect("the template carries an aki-rlcache marker");
        let older = format!("#!/bin/bash\n# aki-rlcache v3\n# aki-rlcache v{} - old\n", shipped - 1);
        let current = format!("#!/bin/bash\n# aki-rlcache v{} - new\n", shipped);

        let cases: &[(&str, &str, bool)] = &[
            ("older", &older, true),
            ("current", &current, false),
            ("not_ours", "#!/bin/bash\necho hand-written\n", false),
        ];
        for (name, script, want) in cases {
            let (out, _) = run_script(
                &format!("probe_rl_{}.sh", name),
                PROBE,
                "",
                &[(cc_sh, script), (cc_json, wired)],
            );
            let status = StatuslineHostStatus::from_probe("local".into(), &out);
            assert!(status.cc_configured, "{}: not configured: {}", name, out);
            assert_eq!(status.cc_outdated, *want, "{}: probe said {}", name, out);
        }

        let ag_sh = ".gemini/antigravity-cli/statusline.sh";
        let ag_json = ".gemini/antigravity-cli/settings.json";
        let ag_wired = r#"{"statusLine":{"type":"command","command":"statusline.sh"}}"#;
        let routing = marker_version(TEMPLATE, AGQUOTA_MARKER).expect("the template carries an aki-agquota marker");
        let ours = format!("#!/bin/bash\n# aki-rlcache v{}\n", shipped);
        let ag_cases: &[(&str, String, bool)] = &[
            ("ag_before_marker", ours.clone(), true),
            ("ag_older", format!("{}# aki-agquota v{}\n", ours, routing - 1), true),
            ("ag_current", format!("{}# aki-agquota v{}\n", ours, routing), false),
            ("ag_not_ours", "#!/bin/bash\necho hand-written\n".to_string(), false),
        ];
        for (name, script, want) in ag_cases {
            let (out, _) = run_script(
                &format!("probe_ag_{}.sh", name),
                PROBE,
                "",
                &[(ag_sh, script.as_str()), (ag_json, ag_wired)],
            );
            let status = StatuslineHostStatus::from_probe("local".into(), &out);
            assert!(status.ag_configured, "{}: not configured: {}", name, out);
            assert!(!status.cc_outdated, "{}: AGY markers read as Claude Code's: {}", name, out);
            assert_eq!(status.ag_outdated, *want, "{}: probe said {}", name, out);
        }
    }

    #[test]
    fn both_targets_receive_the_same_body() {
        // One physical script, installed at two paths - if these ever differ, the "$0 decides the CLI" contract is broken and each CLI is back to having its own dialect.
        let both = joined(&["cc", "ag"]);
        let body = gen(&test_config());
        assert_eq!(
            both.matches(body.as_str()).count(),
            2,
            "the two targets got different bodies"
        );
    }

    // ---- P0-2: no shared predictable /tmp dump -----------------------------------------------

    /// An older installed script may still write this path during tests, so assert on this test's own payload, not on file existence.
    fn dump_has_sentinel(dump: &std::path::Path) -> bool {
        std::fs::read_to_string(dump).is_ok_and(|c| c.contains("sentinel"))
    }

    /// Both P0-2 tests own the same fixed legacy path, so they must not interleave under the parallel test runner.
    static LEGACY_DUMP_PATH: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// §P0-2. Running either execution path with a payload that contains a sentinel email/path
    /// must not create /tmp/statusline_stdin_dump.json.
    #[test]
    fn no_tmp_dump_on_any_execution_path() {
        let _guard = LEGACY_DUMP_PATH.lock().unwrap_or_else(|e| e.into_inner());
        let dump = std::path::Path::new("/tmp/statusline_stdin_dump.json");
        run_script(
            "no_dump_cc.sh",
            &gen(&test_config()),
            r#"{"cwd":"/sentinel/path","model":{"display_name":"Sonnet 5"}}"#,
            &[(
                ".claude/auth-cache-default.json",
                r#"{"email":"sentinel@example.com"}"#,
            )],
        );
        assert!(
            !dump_has_sentinel(dump),
            "the CC path wrote its payload to /tmp/statusline_stdin_dump.json"
        );
        run_script(
            ".gemini/antigravity-cli/no_dump_ag.sh",
            &gen(&test_config()),
            r#"{"cwd":"/sentinel/path","model":"gemini-2.5-flash"}"#,
            &[],
        );
        assert!(
            !dump_has_sentinel(dump),
            "the AGY path wrote its payload to /tmp/statusline_stdin_dump.json"
        );
    }

    /// §P0-2. A pre-planted symlink at the legacy /tmp path must not be followed: the script
    /// must not write through it to its target.
    #[test]
    fn legacy_tmp_symlink_is_not_followed() {
        let _guard = LEGACY_DUMP_PATH.lock().unwrap_or_else(|e| e.into_inner());
        let dump = std::path::Path::new("/tmp/statusline_stdin_dump.json");
        let target = std::env::temp_dir().join("aki-symlinktest-sentinel");
        let _ = std::fs::remove_file(dump);
        let _ = std::fs::remove_file(&target);
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, dump).ok();
        run_script(
            "sym_test.sh",
            &gen(&test_config()),
            r#"{"cwd":"/tmp/demo","model":{"display_name":"Sonnet 5"}}"#,
            &[],
        );
        assert!(
            !target.exists(),
            "the script followed the legacy /tmp symlink and wrote through it"
        );
        let _ = std::fs::remove_file(dump);
    }

    // ---- P0-3: transactional installer -------------------------------------------------------

    /// Runs an installer script in a private temp home; returns (exit_success, home_path).
    fn run_installer(
        label: &str,
        script: &str,
        files: &[(&str, &str)],
    ) -> (bool, std::path::PathBuf) {
        let home =
            std::env::temp_dir().join(format!("aki-installer-test/{}", label.replace('/', "_")));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        for (rel, contents) in files {
            let p = home.join(rel);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&p, contents).unwrap();
        }
        let script_path = home.join("installer.sh");
        std::fs::write(&script_path, script).unwrap();
        let status = std::process::Command::new("sh")
            .arg(&script_path)
            .env("HOME", &home)
            .env_remove("CLAUDE_CONFIG_DIR")
            .output()
            .unwrap()
            .status;
        (status.success(), home)
    }

    fn jq_present() -> bool {
        std::process::Command::new("sh")
            .args(["-c", "command -v jq >/dev/null 2>&1"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// §P0-3 static. The settings mktemp must use a directory derived from the settings
    /// file's own path — not the bare system /tmp — so the final mv is an atomic same-fs rename.
    #[test]
    fn settings_tmp_is_same_directory_as_settings_file() {
        for alias in ["cc", "ag"] {
            let s = joined(&[alias]);
            assert!(
                !s.contains("tmp=$(mktemp)\n"),
                "{}: bare tmp=$(mktemp) without a directory argument still present (would land in /tmp)",
                alias
            );
            assert!(
                s.contains("dirname"),
                "{}: settings temp file location does not derive from dirname of settings path",
                alias
            );
        }
    }

    /// §P0-3 behavioral. Malformed settings JSON: installer must exit non-zero and leave
    /// both the settings file and any pre-existing script file unchanged.
    #[test]
    fn installer_rejects_malformed_settings_and_leaves_state_intact() {
        for (alias, settings_rel, script_rel) in [
            (
                "cc",
                ".claude/settings.json",
                ".claude/statusline-command.sh",
            ),
            (
                "ag",
                ".gemini/antigravity-cli/settings.json",
                ".gemini/antigravity-cli/statusline.sh",
            ),
        ] {
            let original_script = "#!/bin/sh\necho original";
            let (ok, home) = run_installer(
                &format!("malformed_{}", alias),
                &joined(&[alias]),
                &[
                    (settings_rel, "{not valid json"),
                    (script_rel, original_script),
                ],
            );
            assert!(
                !ok,
                "{}: installer should have failed on malformed settings JSON",
                alias
            );
            let settings_after = std::fs::read_to_string(home.join(settings_rel)).unwrap();
            assert_eq!(
                settings_after, "{not valid json",
                "{}: malformed settings was overwritten on failure",
                alias
            );
            let script_after = std::fs::read_to_string(home.join(script_rel)).unwrap();
            assert!(
                script_after.contains("original"),
                "{}: script was overwritten despite settings patch failure",
                alias
            );
        }
    }

    /// §P0-3 behavioral. After a jq failure, no orphaned .settings.aki-tmp.* files must
    /// remain in the settings directory.
    #[test]
    fn installer_cleans_up_settings_tmp_on_failure() {
        if !jq_present() {
            return;
        }
        for (alias, settings_rel, settings_dir) in [
            ("cc", ".claude/settings.json", ".claude"),
            (
                "ag",
                ".gemini/antigravity-cli/settings.json",
                ".gemini/antigravity-cli",
            ),
        ] {
            let (_, home) = run_installer(
                &format!("cleanup_{}", alias),
                &joined(&[alias]),
                &[(settings_rel, "{not valid json")],
            );
            let dir = home.join(settings_dir);
            if dir.exists() {
                let orphans: Vec<_> = std::fs::read_dir(&dir)
                    .unwrap()
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        e.file_name()
                            .to_string_lossy()
                            .contains(".settings.aki-tmp.")
                    })
                    .collect();
                assert!(
                    orphans.is_empty(),
                    "{}: orphaned settings temp file left after failure: {:?}",
                    alias,
                    orphans.iter().map(|e| e.path()).collect::<Vec<_>>()
                );
            }
        }
    }

    /// §P0-3 behavioral. A successful install must patch settings AND write the script for
    /// both targets — writing only one half is the bug this transaction exists to prevent.
    #[test]
    fn installer_succeeds_and_writes_both_halves() {
        if !jq_present() {
            return;
        }
        for (alias, settings_rel, script_rel) in [
            (
                "cc",
                ".claude/settings.json",
                ".claude/statusline-command.sh",
            ),
            (
                "ag",
                ".gemini/antigravity-cli/settings.json",
                ".gemini/antigravity-cli/statusline.sh",
            ),
        ] {
            let (ok, home) = run_installer(&format!("success_{}", alias), &joined(&[alias]), &[]);
            assert!(ok, "{}: installer failed on a clean home", alias);
            let settings = std::fs::read_to_string(home.join(settings_rel)).unwrap_or_default();
            assert!(
                settings.contains("statusLine"),
                "{}: settings.json does not contain statusLine after install: {}",
                alias,
                settings
            );
            assert!(
                home.join(script_rel).exists(),
                "{}: script file not written after install",
                alias
            );
        }
    }

    // ---- auth-cache TTL, AGY plan-tier record, marker tripwire ---------------------------------

    const AUTH_CACHE_REL: &str = ".claude/auth-cache-default.json";
    /// A `claude` that answers `auth status` with an account other than the one any test seeds in the auth cache.
    const CLAUDE_AUTH_STUB: &str = "#!/bin/sh\nprintf '%s' '{\"email\":\"stub-user@example.com\"}'\n";

    fn render_with_auth_cache_aged(name: &str, age_secs: u64) -> (String, std::path::PathBuf) {
        let payload = r#"{"cwd":"/tmp/demo","model":{"display_name":"Sonnet 5"}}"#;
        run_script_aged(
            name,
            &gen(&test_config()),
            payload,
            &[
                (AUTH_CACHE_REL, r#"{"email":"disk-user@example.com"}"#),
                ("bin/claude", CLAUDE_AUTH_STUB),
            ],
            &[],
            &[(AUTH_CACHE_REL, age_secs)],
        )
    }

    #[test]
    fn cc_auth_cache_inside_its_ttl_wins_over_the_live_cli() {
        let (line, home) = render_with_auth_cache_aged("cc_auth_fresh.sh", 0);
        let p = plain(&line);
        assert!(p.contains("disk"), "fresh auth cache not used: {}", p);
        assert!(!p.contains("stub"), "live CLI consulted despite a fresh auth cache: {}", p);
        let cached = std::fs::read_to_string(home.join(AUTH_CACHE_REL)).unwrap();
        assert!(cached.contains("disk-user"), "fresh auth cache was rewritten: {}", cached);
    }

    #[test]
    fn cc_auth_cache_past_its_ttl_is_replaced_by_the_live_cli() {
        let (line, home) = render_with_auth_cache_aged("cc_auth_expired.sh", 3600);
        let p = plain(&line);
        assert!(p.contains("stub"), "expired auth cache still used: {}", p);
        assert!(!p.contains("disk"), "expired auth cache leaked into the line: {}", p);
        let cached = std::fs::read_to_string(home.join(AUTH_CACHE_REL)).unwrap();
        assert!(cached.contains("stub-user"), "live auth status was not cached: {}", cached);
    }

    fn plan_tier_dir(home: &std::path::Path) -> std::path::PathBuf {
        home.join(".gemini/antigravity-cli/aki-plan-tier")
    }

    #[test]
    fn agy_records_its_plan_tier_under_the_sanitized_account() {
        let payload = r#"{"cwd":"/tmp/demo","model":"gemini-2.5-flash","email":"ag/user@example.com","plan_tier":"Google AI Pro"}"#;
        let (_, home) = run_script(
            ".gemini/antigravity-cli/tier_write.sh",
            &gen(&test_config()),
            payload,
            &[],
        );
        let record = std::fs::read_to_string(plan_tier_dir(&home).join("ag_user@example.com"))
            .expect("plan-tier record not written");
        assert_eq!(record.lines().next(), Some("Google AI Pro"));
    }

    #[test]
    fn agy_without_a_plan_tier_writes_no_record() {
        for (name, payload) in [
            (
                ".gemini/antigravity-cli/tier_empty.sh",
                r#"{"cwd":"/tmp/demo","model":"gemini-2.5-flash","email":"ag-user@example.com","plan_tier":""}"#,
            ),
            (
                ".gemini/antigravity-cli/tier_absent.sh",
                r#"{"cwd":"/tmp/demo","model":"gemini-2.5-flash","email":"ag-user@example.com"}"#,
            ),
        ] {
            let (_, home) = run_script(name, &gen(&test_config()), payload, &[]);
            assert!(!plan_tier_dir(&home).exists(), "{}: a plan-tier record was written without a tier", name);
        }
    }

    #[test]
    fn cc_never_writes_a_plan_tier_record() {
        let payload = r#"{"cwd":"/tmp/demo","model":{"display_name":"Sonnet 5"},"email":"cc-user@example.com","plan_tier":"Google AI Pro"}"#;
        let (_, home) = run_script("cc_tier.sh", &gen(&test_config()), payload, &[]);
        assert!(!plan_tier_dir(&home).exists(), "the CC path wrote an AGY plan-tier record");
    }

    /// FNV-1a 64: dependency-free and fixed forever, unlike `DefaultHasher`.
    fn fnv1a64(bytes: &[u8]) -> u64 {
        bytes.iter().fold(0xcbf29ce484222325_u64, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x100000001b3))
    }

    /// The template lines after the one starting with `marker`, up to (not including) the next line starting with `end`.
    fn marker_block(marker: &str, end: &str) -> &'static str {
        let mut offset = 0;
        let mut body_start = None;
        for line in TEMPLATE.split_inclusive('\n') {
            if let Some(start) = body_start {
                if line.starts_with(end) {
                    return &TEMPLATE[start..offset];
                }
            } else if line.starts_with(marker) {
                body_start = Some(offset + line.len());
            }
            offset += line.len();
        }
        panic!("no block from {:?} to {:?} in the template", marker, end)
    }

    /// Installed hosts are compared by marker version alone, so a block edit that keeps its version never reaches a host that already has it.
    #[test]
    fn a_marker_block_edit_bumps_its_version() {
        // (marker, line that ends the block, pinned version, pinned FNV-1a of the block body after the marker line)
        let pins: [(&str, &str, u32, u64); 2] = [
            (RLCACHE_MARKER, AGQUOTA_MARKER, 8, 0xa2a75017354850df),
            (AGQUOTA_MARKER, r#"if [ -n "$JSON_CWD""#, 2, 0x4afa8d9b17150a34),
        ];
        for (marker, end, pinned_version, pinned_hash) in pins {
            let shipped = marker_version(TEMPLATE, marker).expect("the template carries the marker");
            let hash = fnv1a64(marker_block(marker, end).as_bytes());
            let verdict = match (shipped == pinned_version, hash == pinned_hash) {
                (true, true) => continue,
                (true, false) => "block text changed - bump the marker AND update the pinned hash",
                (false, true) => "version alone changed - update the pinned version",
                (false, false) => "marker bumped with the text - update the pinned version and hash",
            };
            panic!("{}{}: {} (shipped v{}, hash now {:#018x})", marker, shipped, verdict, shipped, hash);
        }
    }
}
