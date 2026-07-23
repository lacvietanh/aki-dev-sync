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

use crate::agent_usage::run_remote_script_bounded;
use serde::{Deserialize, Serialize};

/// The script itself, compiled in verbatim. It is a runnable, checked-in reference as well as the
/// template: the region between the two markers below is the only part Apply rewrites.
const TEMPLATE: &str = include_str!("statusline-unified.sh");
const MARK_BEGIN: &str = "# >>> AKI-GENERATED-CONFIG >>>";
const MARK_END: &str = "# <<< AKI-GENERATED-CONFIG <<<";

#[derive(Serialize, Deserialize, Clone)]
pub struct StatuslineField {
    pub key: String,
    pub enabled: bool,
    /// Only honored for the keys in COLOR_KEYS; every other field's color is computed from its
    /// value (the UI shows "Dynamic color" and offers no picker).
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

/// Deliberately WITHOUT `#[serde(default)]` on any field, against the usual "new serde fields on
/// persisted JSON need a default" rule (RULE-stack-tauri B3). That rule guards against a field
/// silently dropping to nothing; here the opposite is wanted. A default here would be a Rust-side
/// default, i.e. the second source of truth this whole module exists to remove - and it cannot
/// happen by accident anyway: `loadCfg()` in the Vue component always builds the complete shape,
/// and a stored config from an older `CONFIG_VERSION` is discarded rather than sent. So a missing
/// field means a genuinely malformed payload, and failing the Apply loudly beats writing a script
/// from values the user never chose.
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

/// Every EN_ flag the template declares, in template order. A key here that the UI never sends
/// still gets a flag (0), so a stale saved config can never leave a gate undefined.
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

/// The keys whose color the user picks. Mirrors COLOR_EDITABLE in ClaudeSettingModal.vue and the
/// COLOR_* block in the template - all three lists must name the same six keys, or the UI grows a
/// picker that changes nothing.
const COLOR_KEYS: &[&str] = &[
    "identity_user",
    "identity_host",
    "cwd",
    "model",
    "git_branch",
    "account",
];

/// (child, parent). Mirrors DEPENDS in ClaudeSettingModal.vue: a child is only reachable while its
/// parent is on. Resolved here so the generated flags are already final and no shell gate has to
/// re-check a parent.
const DEPENDS: &[(&str, &str)] = &[
    ("effort", "model"),
    ("rate_reset_5h", "rate_limits_5h"),
    ("rate_reset_7d", "rate_limits_7d"),
    ("cache_pct", "cache"),
    ("cache_tokens", "cache"),
];

/// Which printed block a field belongs to. `None` = printed outside the block loop (the tag
/// cluster) or unknown to this version of the template.
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

// The returned reference borrows from `config`, not from `key` - tying both inputs to one lifetime
// (as the compiler's suggestion does) would force callers to keep the key alive as long as the config.
fn field<'a>(config: &'a StatuslineConfig, key: &str) -> Option<&'a StatuslineField> {
    config.fields.iter().find(|f| f.key == key)
}

/// Enabled AND reachable - the same rule as fieldActive() in the Vue component. A field the config
/// does not mention at all counts as off.
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

/// Into the neutral greyscale ramp the UI's picker is restricted to (16, or 232..=255). A clamp,
/// not a fallback to some "default" shade - this module owns no defaults.
fn sanitized_shade(n: u8) -> u8 {
    if n == 16 {
        16
    } else {
        n.clamp(232, 255)
    }
}

/// Print order, one entry per block, in the order the user dragged the rows into. A block appears
/// at its first member's position; whether it prints at all is the EN_ flags' business, not this
/// list's, so a block with everything switched off still appears here and simply renders empty.
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

    s.push_str("# Enable flags, already dependency-resolved (see DEPENDS in ClaudeSettingModal.vue).\n");
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
        "\n# Dynamic-color ladder, ascending. Below THRESH_GREEN is the calm blue tier.\n\
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

/// Splices the generated region into the template. Everything outside the two markers - the whole
/// body of the script - is shipped byte-for-byte.
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

/// Where one generated body gets written, and what else has to be patched there. Both targets take
/// the SAME body - the script decides at run time which CLI it is speaking for.
struct Target {
    /// Values the frontend may send for this target.
    aliases: &'static [&'static str],
    installer: fn(&str) -> String,
}

const TARGETS: &[Target] = &[
    Target {
        aliases: &["cc", "claude"],
        installer: |body| {
            format!(
                "mkdir -p \"$HOME/.claude\"\n\
                 FILE=\"$HOME/.claude/statusline-command.sh\"\n\
                 if [ -f \"$FILE\" ] && [ ! -f \"$FILE.aki-bak\" ]; then cp \"$FILE\" \"$FILE.aki-bak\"; fi\n\
                 cat > \"$FILE\" <<'AKI_STATUSLINE_CLAUDE_EOF'\n{body}AKI_STATUSLINE_CLAUDE_EOF\n\
                 chmod +x \"$FILE\"\n\
                 SETTINGS=\"$HOME/.claude/settings.json\"\n\
                 [ -f \"$SETTINGS\" ] || echo '{{}}' > \"$SETTINGS\"\n\
                 tmp=$(mktemp)\n\
                 jq '.statusLine.type = \"command\" | .statusLine.command = \"~/.claude/statusline-command.sh\"' \"$SETTINGS\" > \"$tmp\" && mv \"$tmp\" \"$SETTINGS\"\n"
            )
        },
    },
    Target {
        aliases: &["ag", "cli"],
        installer: |body| {
            format!(
                "mkdir -p \"$HOME/.gemini/antigravity-cli\"\n\
                 FILE_AGY=\"$HOME/.gemini/antigravity-cli/statusline.sh\"\n\
                 if [ -f \"$FILE_AGY\" ] && [ ! -f \"$FILE_AGY.aki-bak\" ]; then cp \"$FILE_AGY\" \"$FILE_AGY.aki-bak\"; fi\n\
                 cat > \"$FILE_AGY\" <<'AKI_STATUSLINE_AGY_EOF'\n{body}AKI_STATUSLINE_AGY_EOF\n\
                 chmod +x \"$FILE_AGY\"\n\
                 SETTINGS_AGY=\"$HOME/.gemini/antigravity-cli/settings.json\"\n\
                 [ -f \"$SETTINGS_AGY\" ] || echo '{{}}' > \"$SETTINGS_AGY\"\n\
                 tmp=$(mktemp)\n\
                 jq --arg cmd \"$FILE_AGY\" '.statusLine.type = \"command\" | .statusLine.command = $cmd | .statusLine.enabled = true' \"$SETTINGS_AGY\" > \"$tmp\" && mv \"$tmp\" \"$SETTINGS_AGY\"\n"
            )
        },
    },
];

/// Writes only the targets the caller actually asked for. An empty selection is an error, never an
/// implicit "AGY anyway" - a silent fallback here is exactly how Apply ended up writing a file the
/// user never ticked.
fn build_installer_script(
    config: &StatuslineConfig,
    selected_targets: &[String],
) -> Result<String, String> {
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
    let mut combined = String::from("set -e\n");
    for target in chosen {
        combined.push_str(&(target.installer)(&body));
        combined.push('\n');
    }
    Ok(combined)
}

/// Per CLI, because a host can have one, both or neither. Reporting a single "configured" flag made
/// the indicator lie the moment AGY was the target: applying for AGY left the flag false forever.
/// `present` = that CLI exists on the host at all; `configured` = its statusline is installed AND
/// its settings point at it, which is the only state that actually renders a line.
#[derive(Serialize, Clone)]
pub struct StatuslineHostStatus {
    pub host: String,
    pub cc_present: bool,
    pub cc_configured: bool,
    pub ag_present: bool,
    pub ag_configured: bool,
}

impl StatuslineHostStatus {
    fn unreachable(host: String) -> Self {
        Self { host, cc_present: false, cc_configured: false, ag_present: false, ag_configured: false }
    }
}

/// Both halves of an install are checked: the script file, and the settings key naming it. A host
/// with the file but an empty `statusLine.command` renders nothing, so it is not "configured".
const PROBE: &str = r#"
if command -v "$CLAUDE_BIN" >/dev/null 2>&1 || [ -d "$HOME/.claude" ]; then echo "CC_PRESENT=1"; else echo "CC_PRESENT=0"; fi
if [ -f "$HOME/.claude/statusline-command.sh" ] && grep -q "statusline-command.sh" "$HOME/.claude/settings.json" 2>/dev/null; then echo "CC_SL=1"; else echo "CC_SL=0"; fi
if [ -d "$HOME/.gemini/antigravity-cli" ]; then echo "AG_PRESENT=1"; else echo "AG_PRESENT=0"; fi
if [ -f "$HOME/.gemini/antigravity-cli/statusline.sh" ] && grep -q "statusline.sh" "$HOME/.gemini/antigravity-cli/settings.json" 2>/dev/null; then echo "AG_SL=1"; else echo "AG_SL=0"; fi
"#;

#[tauri::command]
pub async fn check_statusline_status(hosts: Vec<String>) -> Vec<StatuslineHostStatus> {
    tauri::async_runtime::spawn_blocking(move || {
        // One OS thread per host - hosts are independent, so none should wait on another (the
        // per-host lock inside run_remote_script_bounded only serializes a host against ITSELF,
        // i.e. against other features touching that same host; it never blocks one host on a
        // different one). Mirrors the fan-out in apply_statusline_config below.
        let handles: Vec<(String, std::thread::JoinHandle<StatuslineHostStatus>)> = hosts
            .into_iter()
            .map(|host| {
                let host_for_thread = host.clone();
                let handle = std::thread::spawn(move || {
                    match run_remote_script_bounded(&host_for_thread, PROBE) {
                        Ok(out) => {
                            let stdout = String::from_utf8_lossy(&out.stdout);
                            StatuslineHostStatus {
                                host: host_for_thread,
                                cc_present: stdout.contains("CC_PRESENT=1"),
                                cc_configured: stdout.contains("CC_SL=1"),
                                ag_present: stdout.contains("AG_PRESENT=1"),
                                ag_configured: stdout.contains("AG_SL=1"),
                            }
                        }
                        Err(_) => StatuslineHostStatus::unreachable(host_for_thread),
                    }
                });
                (host, handle)
            })
            .collect();
        handles
            .into_iter()
            .map(|(host, h)| h.join().unwrap_or_else(|_| StatuslineHostStatus::unreachable(host)))
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
    let installer = std::sync::Arc::new(build_installer_script(&config, &targets)?);

    tauri::async_runtime::spawn_blocking(move || {
        // One OS thread per host, run concurrently - see the matching comment in
        // check_statusline_status. Applying to host A must never wait on host B; the per-host
        // lock in run_remote_script_bounded still keeps this app's OWN calls to any one host
        // (this apply, the auto-install probe, usage polling, ...) from overlapping each other.
        let handles: Vec<(String, std::thread::JoinHandle<HostApplyResult>)> = target_hosts
            .into_iter()
            .map(|host| {
                let installer = installer.clone();
                let host_for_thread = host.clone();
                let handle = std::thread::spawn(move || {
                    let outcome = run_remote_script_bounded(&host_for_thread, &installer);
                    let result = match outcome {
                        Ok(output) => {
                            let ok = output.status.success();
                            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                            HostApplyResult {
                                host: host_for_thread.clone(),
                                ok,
                                message: if ok { "Applied".to_string() } else { stderr },
                            }
                        }
                        Err(e) => HostApplyResult { host: host_for_thread.clone(), ok: false, message: e },
                    };
                    crate::logger::info(
                        "STATUSLINE",
                        &format!("apply host={} ok={} msg={}", result.host, result.ok, preview(&result.message, 200)),
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

    /// Test fixture ONLY - the product's defaults live in the Vue component. This is a transcript
    /// of them so the tests have something to generate from; `generated_defaults_match_template`
    /// below is what proves the transcript is still accurate.
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
            thresholds: StatuslineThresholds { green: 20, yellow: 51, orange: 75, red: 90 },
            trunc: StatuslineTrunc { account: 4, user: 5, host: 6, cwd: 12, branch: 10 },
            zebra: StatuslineZebra { a: 16, b: 235 },
            separate: true,
        }
    }

    fn gen(config: &StatuslineConfig) -> String {
        generate_statusline_script(config).expect("generate")
    }

    /// True if the line prints a negative percentage - the old AGY bug rendered a remaining
    /// fraction as `-2400%`. Checked by shape, not by looking for a bare '-': hostnames and model
    /// ids legitimately contain dashes.
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

    /// A rendered line with its ANSI escapes removed. Assertions are about the text the user reads;
    /// against the raw line even `"Sonnet5med"` fails, because a color change sits between the two
    /// halves. Anything asserting on the escapes themselves uses the raw line instead.
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

    /// The anti-drift test. Generating from the UI's defaults must reproduce the checked-in
    /// template byte for byte - if it does not, the file people read and the file Apply writes
    /// have diverged, which is the whole class of bug Phase 2 existed to end.
    #[test]
    fn generated_defaults_match_template() {
        assert_eq!(gen(&test_config()), TEMPLATE);
    }

    #[test]
    fn generated_script_is_valid_shell() {
        let mut cfg = test_config();
        // Not the default config: an all-off config is the shape most likely to emit a dangling
        // gate or an empty BLOCK_ORDER, so that is what gets syntax-checked.
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
            child.stdin.as_mut().unwrap().write_all(script.as_bytes()).unwrap();
            let out = child.wait_with_output().unwrap();
            assert!(out.status.success(), "bash -n failed:\n{}", String::from_utf8_lossy(&out.stderr));
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
        // And the reverse: a gate the body reads but the generator never writes would be an
        // always-empty variable, i.e. a silently invisible block.
        for line in TEMPLATE.lines() {
            for token in line.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
                // A bare `EN_` with nothing after it is prose in the template's comments, not a
                // variable reference - only a real name is a claim about a generated flag.
                if let Some(key) = token.strip_prefix("EN_").filter(|k| !k.is_empty()) {
                    assert!(
                        EN_KEYS.contains(&key),
                        "template reads EN_{} but the generator never emits it",
                        key
                    );
                }
            }
        }
        // Same contract for the color variables - a COLOR_ the body reads but nobody writes is a
        // dead picker in the UI (the exact bug COLOR_cwd shipped with).
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
        assert!(script.contains("EN_cache_pct=0"), "cache_pct survived its parent being off");
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
            script.contains(r#"BLOCK_ORDER="ram identity cwd model context cache quota session git_branch""#),
            "BLOCK_ORDER did not follow the row order"
        );
    }

    #[test]
    fn out_of_range_values_are_clamped_not_defaulted() {
        let mut cfg = test_config();
        cfg.zebra = StatuslineZebra { a: 100, b: 255 };
        cfg.thresholds = StatuslineThresholds { green: 90, yellow: 20, orange: 75, red: 51 };
        let script = gen(&cfg);
        assert!(script.contains("BG_ZEBRA_A=232"), "a shade outside the neutral ramp was not clamped");
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
        assert!(p.contains("42%") && p.contains("92%"), "quota missing: {}", p);
        assert!(p.contains("master"), "branch missing: {}", p);
    }

    #[test]
    fn agy_renders_a_line() {
        // $0 decides the CLI, so the AGY case is the same script under a ~/.gemini/ path.
        let payload = r#"{"cwd":"/tmp/demo","account":{"email":"user-a@example.com"},"model":"gemini-2.5-flash","quota":{"gemini-5h":{"remaining_fraction":0.25},"gemini-weekly":{"remaining_fraction":0.5}}}"#;
        let (line, _) = run_script(".gemini/antigravity-cli/statusline.sh", &gen(&test_config()), payload, &[]);
        println!("AGY RENDERED>>>{}<<<", line);
        let p = plain(&line);
        assert!(p.starts_with("AG"), "AGY path did not identify as AG: {}", p);
        assert!(p.contains("user"), "account not rendered: {}", p);
        // A remaining fraction of 0.25 is 75% used - never -2400%.
        assert!(!has_negative_percent(&p), "negative percentage in agy line: {}", p);
        assert!(p.contains("75%"), "5h used% wrong (want 75%): {}", p);
        assert!(p.contains("50%"), "7d used% wrong (want 50%): {}", p);
    }

    /// Dropping the vendor word out of a raw model id leaves its separators behind. Whatever a CLI
    /// reports - display name, raw id, snake_case, a trailing "(...)" note - what reaches the line
    /// must be one token with no stray punctuation at either edge.
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
            assert!(p.contains(want), "{:?} should render as {:?}: {}", raw, want, p);
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
        let cache = r#"{"account":"","rate_limits":{"five_hour":{"used_percentage":42,"resets_at":0}}}"#;
        let (line, home) = run_script(
            ".gemini/antigravity-cli/agy_rlcache.sh",
            &gen(&test_config()),
            payload,
            &[(".claude/rate-limits-cache.json", cache)],
        );
        assert!(!plain(&line).contains("42%"), "AGY read Claude Code's rate-limit cache: {}", plain(&line));
        let after = std::fs::read_to_string(home.join(".claude/rate-limits-cache.json")).unwrap();
        assert_eq!(after, cache, "AGY rewrote Claude Code's rate-limit cache");
    }

    // ---- helpers -------------------------------------------------------------------------

    /// Runs a generated script against a payload inside a private $HOME, so tests that exercise the
    /// on-disk fallbacks (~/.claude.json, the rlcache) never read or write the real home dir.
    /// `name` doubles as the path the script is invoked by, which is what decides CC vs AG.
    /// `files` are (path-relative-to-HOME, contents) written before the run.
    fn run_script(name: &str, script: &str, payload: &str, files: &[(&str, &str)]) -> (String, std::path::PathBuf) {
        let home = std::env::temp_dir().join(format!("aki-statusline-test/{}", name.replace('/', "_")));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        for (rel, contents) in files {
            let p = home.join(rel);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&p, contents).unwrap();
        }
        let path = home.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, script).unwrap();
        let out = std::process::Command::new("bash")
            .arg(&path)
            .env("HOME", &home)
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

    // ---- behavioural tests, per docs/plan/1.18.0-statusline-apply-correctness.md §P2-4 ------

    #[test]
    fn cc_account_falls_back_to_claude_json() {
        // Payload deliberately carries no email - Claude Code never sends one.
        let payload = r#"{"cwd":"/tmp/demo","model":{"display_name":"Sonnet 5"}}"#;
        let (line, _) = run_script(
            "cc_account.sh",
            &gen(&test_config()),
            payload,
            &[(".claude.json", r#"{"oauthAccount":{"emailAddress":"disk-user@example.com"}}"#)],
        );
        println!("CC ACCOUNT>>>{}<<<", line);
        // Truncated to TRUNC_ACCOUNT after the domain is dropped: "disk-user@..." -> "disk".
        let p = plain(&line);
        assert!(p.contains("disk"), "account fallback to ~/.claude.json did not render: {}", p);
        assert!(!p.contains("example.com"), "the domain was printed instead of stripped: {}", p);
    }

    #[test]
    fn cc_rate_limits_survive_a_payload_that_omits_them() {
        let payload = r#"{"cwd":"/tmp/demo","model":{"display_name":"Sonnet 5"}}"#;
        let cache = r#"{"account":"","rate_limits":{"five_hour":{"used_percentage":42,"resets_at":0},"seven_day":{"used_percentage":92,"resets_at":0}}}"#;
        let (line, _) = run_script(
            "cc_rlcache.sh",
            &gen(&test_config()),
            payload,
            &[(".claude/rate-limits-cache.json", cache)],
        );
        println!("CC RLCACHE>>>{}<<<", line);
        let p = plain(&line);
        assert!(p.contains("42%"), "5h not restored from cache: {}", p);
        assert!(p.contains("92%"), "7d not restored from cache: {}", p);
    }

    #[test]
    fn cc_rate_limits_merge_instead_of_overwrite() {
        // Payload has only five_hour; seven_day must survive from the cache, and the rewritten
        // cache must still hold both.
        let payload = r#"{"cwd":"/tmp/demo","model":{"display_name":"Sonnet 5"},"rate_limits":{"five_hour":{"used_percentage":10,"resets_at":0}}}"#;
        let cache = r#"{"account":"","rate_limits":{"five_hour":{"used_percentage":42,"resets_at":0},"seven_day":{"used_percentage":92,"resets_at":0}}}"#;
        let (line, home) = run_script(
            "cc_rlmerge.sh",
            &gen(&test_config()),
            payload,
            &[(".claude/rate-limits-cache.json", cache)],
        );
        println!("CC RLMERGE>>>{}<<<", line);
        let p = plain(&line);
        assert!(p.contains("10%"), "fresh 5h value not used: {}", p);
        assert!(p.contains("92%"), "cached 7d lost on merge: {}", p);
        let written = std::fs::read_to_string(home.join(".claude/rate-limits-cache.json")).unwrap();
        assert!(written.contains("seven_day"), "rewritten cache dropped seven_day: {}", written);
    }

    fn now_epoch() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    #[test]
    fn cc_drops_a_cached_quota_whose_reset_has_passed() {
        // The 1.10.0-1.17.0 bug: an entry the account no longer has (here: seven_day, already
        // past its reset) stayed in the cache forever and rendered as a phantom "7d 45%".
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
            &[(".claude/rate-limits-cache.json", cache.as_str())],
        );
        println!("CC RLEXPIRED>>>{}<<<", line);
        let p = plain(&line);
        assert!(p.contains("50%"), "live 5h lost: {}", p);
        assert!(!p.contains("45%"), "expired 7d still rendered: {}", p);
        let written = std::fs::read_to_string(home.join(".claude/rate-limits-cache.json")).unwrap();
        assert!(!written.contains("seven_day"), "expired entry kept in cache: {}", written);
    }

    #[test]
    fn cc_ignores_a_cache_written_by_another_account() {
        let now = now_epoch();
        let cache = format!(
            r#"{{"account":"other@example.com","rate_limits":{{"seven_day":{{"used_percentage":45,"resets_at":{}}}}}}}"#,
            now + 7200
        );
        let (line, _) = run_script(
            "cc_rlforeign.sh",
            &gen(&test_config()),
            r#"{"cwd":"/tmp/demo","model":{"display_name":"Sonnet 5"}}"#,
            &[
                (".claude.json", r#"{"oauthAccount":{"emailAddress":"me@example.com"}}"#),
                (".claude/rate-limits-cache.json", cache.as_str()),
            ],
        );
        println!("CC RLFOREIGN>>>{}<<<", line);
        let p = plain(&line);
        assert!(!p.contains("45%"), "another account's cached quota leaked into this line: {}", p);
        assert!(p.contains("Sonnet5"), "rest of the line lost: {}", p);
    }

    #[test]
    fn cc_survives_a_corrupt_rate_limits_cache() {
        let payload = r#"{"cwd":"/tmp/demo","model":{"display_name":"Sonnet 5"},"context_window":{"total_input_tokens":100,"total_output_tokens":20,"context_window_size":1000}}"#;
        let (line, _) = run_script(
            "cc_rlcorrupt.sh",
            &gen(&test_config()),
            payload,
            &[(".claude/rate-limits-cache.json", "{not json at all")],
        );
        println!("CC RLCORRUPT>>>{}<<<", line);
        assert!(!line.trim().is_empty(), "corrupt cache blanked the whole statusline");
        assert!(plain(&line).contains("Sonnet5"), "corrupt cache lost the rest of the line: {}", plain(&line));
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
        assert!(plain(&line).contains("agy-"), "agy account fallback did not render: {}", plain(&line));
    }

    #[test]
    fn agy_reset_eta_includes_minutes() {
        // 5400s = 1h30m - the old `printf '%dh%dm' "$h"` printed 1h0m.
        let payload = r#"{"cwd":"/tmp/demo","model":"gemini-2.5-flash","quota":{"gemini-5h":{"remaining_fraction":0.4,"reset_in_seconds":5400}}}"#;
        let (line, _) = run_script(".gemini/antigravity-cli/agy_eta.sh", &gen(&test_config()), payload, &[]);
        println!("AGY ETA>>>{}<<<", line);
        assert!(plain(&line).contains("1h30m"), "reset ETA lost its minutes: {}", plain(&line));
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
        let (line, _) = run_script(".gemini/antigravity-cli/agy_noeta.sh", &gen(&cfg), payload, &[]);
        let p = plain(&line);
        assert!(p.contains("60%"), "the 5h reading itself disappeared: {}", p);
        assert!(!p.contains("1h30m"), "reset ETA rendered while switched off: {}", p);
    }

    // ---- the Vue payload, deserialized exactly as the IPC call delivers it ------------------
    //
    // Everything above builds a StatuslineConfig in Rust. These build it from the JSON the
    // customizer actually posts, which is the only way to catch a shape mismatch between the two
    // sides - the tests above would keep passing even if serde could no longer read the real thing.

    /// Verbatim `JSON.stringify(defaultLocalConfig())` from ClaudeSettingModal.vue, `version` and
    /// all. If the UI's defaults change, this string changes with them - that is the point.
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

    /// The end-to-end anti-drift check: what the UI posts, unmodified, must rebuild the checked-in
    /// script byte-for-byte. Together with `generated_defaults_match_template` this pins both ends -
    /// the Rust fixture and the real JSON - to the same file.
    #[test]
    fn the_ui_payload_reproduces_the_template() {
        assert_eq!(gen(&from_json(VUE_DEFAULT_JSON)), TEMPLATE);
    }

    #[test]
    fn a_field_the_backend_does_not_know_is_ignored_not_fatal() {
        // `version` is UI-only bookkeeping, and a future UI may add more. Unknown keys must not
        // break Apply for everyone on an older build.
        let with_extra = VUE_DEFAULT_JSON.replace(
            r#""version": 3"#,
            r#""version": 4, "somethingTheUiAddedLater": {"x": 1}"#,
        );
        assert_eq!(gen(&from_json(&with_extra)), TEMPLATE);
    }

    #[test]
    fn a_missing_section_is_rejected_rather_than_silently_defaulted() {
        // The deliberate absence of #[serde(default)] - see the comment on StatuslineConfig. A
        // half-written payload must fail the Apply, not produce a script from values nobody chose.
        let without_trunc = VUE_DEFAULT_JSON
            .replace(r#""trunc": {"account":4,"user":5,"host":6,"cwd":12,"branch":10},"#, "");
        assert!(
            serde_json::from_str::<StatuslineConfig>(&without_trunc).is_err(),
            "a payload with no trunc section was accepted"
        );
    }

    /// Every toggle in the UI, flipped one at a time against one realistic payload: the marker it
    /// owns must appear exactly while the switch is on, and flipping it must change the line. A
    /// toggle that quietly does nothing is the bug class this table exists to catch (it is how the
    /// CWD color picker shipped dead), and it is exactly what a human clicking through the modal
    /// would check by eye - all 18 gates, which is the part doing it by hand never gets right.
    #[test]
    fn every_toggle_flips_its_own_output_and_nothing_else() {
        // Carries a branch, both rate limits, cache traffic and an account on disk, so no case is a
        // no-op. The two resets are stamped relative to now, which is the scale the ETA is cut from.
        let now = now_epoch();
        let payload = format!(
            r#"{{"cwd":"/tmp/Aki-Dev-Sync","model":{{"display_name":"Opus 4.8"}},"effort":{{"level":"medium"}},
            "cost":{{"total_cost_usd":8.4,"total_duration_ms":720000,"total_lines_added":12,"total_lines_removed":5}},
            "context_window":{{"total_input_tokens":120000,"total_output_tokens":14400,"context_window_size":1000000,
              "current_usage":{{"cache_read_input_tokens":44400,"cache_creation_input_tokens":800,"input_tokens":32000}}}},
            "rate_limits":{{"five_hour":{{"used_percentage":19,"resets_at":{}}},
                           "seven_day":{{"used_percentage":56,"resets_at":{}}}}},
            "workspace":{{"git_branch":"master"}}}}"#,
            now + 5_400,
            now + 90_000
        );
        let payload = payload.as_str();
        let files = [(
            ".claude.json",
            r#"{"oauthAccount":{"emailAddress":"ntu-gen@example.com"}}"#,
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

        // (field key, the text it owns, whether the UI ships it on). Off-by-default rows are driven
        // the other way round - on must ADD the marker - so a dead switch cannot hide behind a
        // default that never renders it in the first place.
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
            let (line, _) = run_script(&format!("flip_{}.sh", key), &gen(&flipped), payload, &files);
            let line = plain(&line);
            let (with, without) = if *on_by_default {
                (&base, &line)
            } else {
                (&line, &base)
            };
            assert!(with.contains(marker), "{} is on but {:?} is missing: {}", key, marker, with);
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

    /// The same sweep for the settings that are not on/off. Each one is checked against the escape
    /// codes, since that is where a width, a shade or a color actually lands.
    #[test]
    fn the_numeric_and_color_settings_reach_the_rendered_line() {
        let payload = r#"{"cwd":"/tmp/Aki-Dev-Sync","model":{"display_name":"Opus 4.8"},"workspace":{"git_branch":"master"}}"#;

        let mut cfg = from_json(VUE_DEFAULT_JSON);
        cfg.trunc.cwd = 4;
        cfg.trunc.branch = 3;
        let (line, _) = run_script("trunc.sh", &gen(&cfg), payload, &[]);
        let p = plain(&line);
        assert!(p.contains("Aki-") && !p.contains("Aki-Dev"), "cwd not cut to 4: {}", p);
        assert!(p.contains("mas") && !p.contains("master"), "branch not cut to 3: {}", p);

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

        // A tighter ladder must repaint the same reading - here 19% goes from the calm tier to red.
        let quota = r#"{"cwd":"/tmp/demo","rate_limits":{"five_hour":{"used_percentage":19,"resets_at":0}}}"#;
        let (calm, _) = run_script("ladder_calm.sh", &gen(&from_json(VUE_DEFAULT_JSON)), quota, &[]);
        assert!(calm.contains("\u{1b}[01;34m19%"), "19% should be the blue tier: {:?}", calm);
        let mut cfg = from_json(VUE_DEFAULT_JSON);
        cfg.thresholds = StatuslineThresholds { green: 5, yellow: 10, orange: 15, red: 18 };
        let (hot, _) = run_script("ladder_hot.sh", &gen(&cfg), quota, &[]);
        assert!(hot.contains("\u{1b}[01;31m19%"), "19% should be red now: {:?}", hot);
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

    #[test]
    fn no_target_selected_is_an_error_not_a_silent_agy_write() {
        let err = build_installer_script(&test_config(), &[]).unwrap_err();
        assert!(err.to_lowercase().contains("target"), "unexpected error text: {}", err);
        let cc = build_installer_script(&test_config(), &["cc".to_string()]).unwrap();
        assert!(cc.contains("AKI_STATUSLINE_CLAUDE_EOF"));
        assert!(!cc.contains("AKI_STATUSLINE_AGY_EOF"), "cc-only apply wrote the AGY file too");
        let both = build_installer_script(&test_config(), &["cc".to_string(), "ag".to_string()]).unwrap();
        assert!(both.contains("AKI_STATUSLINE_CLAUDE_EOF") && both.contains("AKI_STATUSLINE_AGY_EOF"));
    }

    /// Writing the script is only half an install: a CLI runs nothing until its settings point at
    /// the file. Skipping this half for AGY is why an Apply ticked for AGY produced a statusline
    /// that never appeared - the file was there, `statusLine.command` was still "".
    #[test]
    fn each_target_registers_its_script_in_the_cli_settings() {
        for (alias, settings, script) in [
            ("cc", ".claude/settings.json", ".claude/statusline-command.sh"),
            (
                "ag",
                ".gemini/antigravity-cli/settings.json",
                ".gemini/antigravity-cli/statusline.sh",
            ),
        ] {
            let sh = build_installer_script(&test_config(), &[alias.to_string()]).unwrap();
            assert!(sh.contains(settings), "{} never touches {}", alias, settings);
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
            assert!(sh.contains(script), "{} does not name its script path", alias);
        }
    }

    /// The per-host indicator reads this probe, so it must answer per CLI and must treat "script on
    /// disk but nothing pointing at it" as NOT configured - the exact state an AGY Apply used to
    /// leave behind, which the old single-flag probe reported as fine.
    #[test]
    fn the_probe_reports_each_cli_separately_and_needs_both_halves() {
        let cc_sh = ".claude/statusline-command.sh";
        let cc_json = ".claude/settings.json";
        let ag_sh = ".gemini/antigravity-cli/statusline.sh";
        let ag_json = ".gemini/antigravity-cli/settings.json";
        let wired = r#"{"statusLine":{"type":"command","command":"statusline.sh"}}"#;
        let empty = r#"{"statusLine":{"type":"","command":""}}"#;

        let cases: &[(&str, &[(&str, &str)], &[&str])] = &[
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
                &[(cc_sh, "#!/bin/bash"), (cc_json, r#"{"statusLine":{"command":"statusline-command.sh"}}"#)],
                &["CC_PRESENT=1", "CC_SL=1", "AG_SL=0"],
            ),
        ];
        for (name, files, want) in cases {
            let (out, _) = run_script(&format!("probe_{}.sh", name), PROBE, "", files);
            for token in *want {
                assert!(out.contains(token), "{}: probe did not report {}: {}", name, token, out);
            }
        }
    }

    #[test]
    fn both_targets_receive_the_same_body() {
        // One physical script, installed at two paths - if these ever differ, the "$0 decides the
        // CLI" contract is broken and each CLI is back to having its own dialect.
        let both = build_installer_script(&test_config(), &["cc".to_string(), "ag".to_string()]).unwrap();
        let body = gen(&test_config());
        assert_eq!(both.matches(body.as_str()).count(), 2, "the two targets got different bodies");
    }
}
