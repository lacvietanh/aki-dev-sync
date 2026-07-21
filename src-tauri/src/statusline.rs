// @docs docs/plan/statusline-customizer.md
// Generates a Claude Code statusline-command.sh from a user-editable field/order/color config,
// then pushes it (+ patches settings.json's statusLine key) to local and/or remote hosts.
//
// Field catalog is intentionally NOT the full 41-key statusLine schema (see the plan doc) — it
// covers the groups already present in the hand-tuned reference script this was built from, plus
// one defensive git-branch field. Adding a field later means one new match arm in
// `render_field()` + one catalog entry in `default_config()`, nothing structural.

use crate::agent_usage::run_remote_script;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct StatuslineField {
    pub key: String,
    pub enabled: bool,
    /// Only honored for fields whose label color is user-editable (see FIELD_COLOR_EDITABLE).
    /// Fields with intrinsic meaning (identity, %, +/-) keep their locked-in colors regardless
    /// of what's stored here.
    pub color: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct StatuslineThresholds {
    pub yellow: u8,
    pub orange: u8,
    pub red: u8,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct StatuslineConfig {
    pub fields: Vec<StatuslineField>,
    pub thresholds: StatuslineThresholds,
}

#[derive(Serialize, Clone)]
pub struct HostApplyResult {
    pub host: String,
    pub ok: bool,
    pub message: String,
}

/// Fields whose label color the user may override. Everything else keeps its locked-in,
/// semantically-meaningful color (identity user/@/host, %, +/-).
fn field_color_editable(key: &str) -> bool {
    matches!(key, "cwd" | "model" | "session" | "git_branch" | "cache_tokens")
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

/// Default catalog + order, matching the hand-tuned preset already locked in during the planning
/// chat (see docs/plan/statusline-customizer.md). Used as the Vue-side fallback default too (kept
/// in sync manually — small, stable list).
pub fn default_config() -> StatuslineConfig {
    let f = |key: &str, enabled: bool, color: &str| StatuslineField {
        key: key.to_string(),
        enabled,
        color: color.to_string(),
    };
    StatuslineConfig {
        fields: vec![
            f("identity", true, "cyan"),
            f("cwd", true, "blue"),
            f("model", true, "cyan"),
            f("context", true, "white"),
            f("rate_limits", true, "white"),
            f("rate_reset", false, "grey"),
            f("cache_pct", false, "white"),
            f("cache_tokens", false, "cyan"),
            f("session", true, "grey"),
            f("git_branch", false, "magenta"),
        ],
        thresholds: StatuslineThresholds { yellow: 50, orange: 70, red: 85 },
    }
}

fn sanitized_thresholds(t: &StatuslineThresholds) -> (u8, u8, u8) {
    let mut v = [t.yellow.min(100), t.orange.min(100), t.red.min(100)];
    v.sort_unstable();
    (v[0], v[1], v[2])
}

const HELPERS: &str = r#"
colored() { printf '%s%s%s' "$1" "$2" "$RESET"; }

fmt_k() {
  awk -v t="$1" 'BEGIN {
    if (t >= 1000000) { m = t/1000000; if (m == int(m)) printf "%dM", m; else printf "%.1fM", m }
    else if (t >= 1000) { printf "%.1fk", t/1000 }
    else { printf "%d", t }
  }'
}

fmt_dur() {
  ms="$1"
  s=$(( ${ms%.*} / 1000 ))
  h=$(( s / 3600 ))
  m=$(( (s % 3600) / 60 ))
  if [ "$h" -gt 0 ]; then printf '%dh%dm' "$h" "$m"; else printf '%dm' "$m"; fi
}

round_pct() {
  awk -v p="$1" 'BEGIN{printf "%.0f", p}'
}

fmt_eta() {
  epoch="$1"
  [ -z "$epoch" ] || [ "$epoch" = "0" ] && return
  now=$(date +%s)
  diff=$(( epoch - now ))
  [ "$diff" -le 0 ] && return
  d=$(( diff / 86400 ))
  h=$(( (diff % 86400) / 3600 ))
  m=$(( (diff % 3600) / 60 ))
  if [ "$d" -gt 0 ]; then printf '%dd%dh' "$d" "$h"; else printf '%dh%dm' "$h" "$m"; fi
}

rate_block() {
  label="$1" used="$2" reset="$3"
  [ "$used" = "-1" ] && return
  used_int=$(round_pct "$used")
  pct_color=$(color_for_pct "$used_int")
  block="$(colored "$COLOR_rate_limits" "$label")$(colored "$GREY" ":")$(colored "$pct_color" "${used_int}%")"
  if [ -n "$reset" ]; then
    eta=$(fmt_eta "$reset")
    [ -n "$eta" ] && block="${block}$(colored "$GREY" " ⟳")$(colored "$GREY" "$eta")"
  fi
  printf '%s' "$block"
}
"#;

/// Builds the full `~/.claude/statusline-command.sh` body (no shebang concerns beyond the
/// standard `#!/bin/bash`) from `config`. The `aki-rlcache v2` block is always emitted — the
/// app's rate-limit caching depends on it and it must not be user-togglable (see plan doc).
pub fn generate_statusline_script(config: &StatuslineConfig) -> String {
    let (yellow, orange, red) = sanitized_thresholds(&config.thresholds);

    let mut s = String::new();
    s.push_str("#!/bin/bash\n");
    s.push_str("# Generated by Aki Dev Sync — Statusline Customizer. Manual edits survive until the\n");
    s.push_str("# next Apply from the app (a .bak of your previous file is kept alongside this one).\n");
    s.push_str("input=$(cat)\n\n");

    s.push_str(
        r#"# aki-rlcache v2 — persist rate_limits across calls that omit it
rl_input=$(echo "$input" | jq -c '.rate_limits // empty')
if [ -z "$rl_input" ] && [ -f "$HOME/.claude/rate-limits-cache.json" ]; then
    input=$(echo "$input" | jq --argjson old "$(cat "$HOME/.claude/rate-limits-cache.json")" '
        if ($old.rate_limits != null) then
            .rate_limits = $old.rate_limits
        else . end
    ')
fi
printf '%s' "$input" > "$HOME/.claude/rate-limits-cache.json"

"#,
    );

    s.push_str(
        r#"IFS=$'\x1f' read -r cwd model_name cost_usd duration_ms lines_added lines_removed \
  ctx_input ctx_output ctx_size ctx_used_pct effort_level five_used seven_used git_branch five_reset seven_reset \
  cache_read cache_creation cache_input <<< "$(echo "$input" | jq -r '[
    (.cwd // .workspace.current_dir // ""),
    (.model.display_name // ""),
    (.cost.total_cost_usd // 0),
    (.cost.total_duration_ms // 0),
    (.cost.total_lines_added // 0),
    (.cost.total_lines_removed // 0),
    (.context_window.total_input_tokens // 0),
    (.context_window.total_output_tokens // 0),
    (.context_window.context_window_size // 0),
    (.context_window.used_percentage // 0),
    (.effort.level // ""),
    (.rate_limits.five_hour.used_percentage // -1),
    (.rate_limits.seven_day.used_percentage // -1),
    (.workspace.git_branch // .git.branch // ""),
    (.rate_limits.five_hour.resets_at // 0),
    (.rate_limits.seven_day.resets_at // 0),
    (.context_window.current_usage.cache_read_input_tokens // 0),
    (.context_window.current_usage.cache_creation_input_tokens // 0),
    (.context_window.current_usage.input_tokens // 0)
  ] | join("")')"

"#,
    );

    // ---- palette ----
    s.push_str("RESET='\\033[00m'\n");
    s.push_str("BOLD_GREEN='\\033[01;32m'\n");
    s.push_str("BOLD_YELLOW='\\033[01;33m'\n");
    s.push_str("BOLD_ORANGE='\\033[01;38;5;208m'\n");
    s.push_str("BOLD_RED='\\033[01;31m'\n");
    s.push_str("GREEN='\\033[32m'\n");
    s.push_str("RED='\\033[31m'\n");
    s.push_str("WHITE='\\033[97m'\n");
    s.push_str("GREY='\\033[90m'\n");
    s.push_str("CYAN='\\033[36m'\n");
    s.push_str("BOLD_BLUE='\\033[01;34m'\n");
    // per-field editable label colors, exposed as COLOR_<key>
    for field in &config.fields {
        if field_color_editable(&field.key) {
            s.push_str(&format!(
                "COLOR_{}='{}'\n",
                field.key,
                ansi_for(&field.color)
            ));
        }
    }
    // fixed colors for non-editable fields that still need a named var (rate labels, identity host)
    s.push_str("COLOR_rate_limits='\\033[97m'\n"); // white, locked
    s.push_str("COLOR_identity_host='\\033[01;32m'\n"); // bold green, locked
    s.push_str("COLOR_identity_user='\\033[36m'\n"); // cyan, locked

    s.push_str(HELPERS);

    s.push_str(&format!(
        r#"
# 4-tier usage scale: green <{yellow}%, yellow {yellow}-{orange}%, orange {orange}-{red}%, red >={red}%
color_for_pct() {{
  p="$1"
  if awk -v p="$p" 'BEGIN{{exit !(p>={red})}}'; then printf '%s' "$BOLD_RED"
  elif awk -v p="$p" 'BEGIN{{exit !(p>={orange})}}'; then printf '%s' "$BOLD_ORANGE"
  elif awk -v p="$p" 'BEGIN{{exit !(p>={yellow})}}'; then printf '%s' "$BOLD_YELLOW"
  else printf '%s' "$BOLD_GREEN"; fi
}}

# Inverse of color_for_pct: for metrics where HIGH is good (cache hit rate), not bad — reuses the
# same threshold ladder against (100 - p) instead of duplicating the tier logic.
color_for_pct_inv() {{
  p="$1"
  inv=$(awk -v p="$p" 'BEGIN{{printf "%.0f", 100 - p}}')
  color_for_pct "$inv"
}}

"#,
        yellow = yellow,
        orange = orange,
        red = red
    ));

    s.push_str("SEP=\"$(colored \"$GREY\" \" | \")\"\n\n");

    // cache_pct and cache_tokens both need the same total — computed once here (SSoT) so either
    // field can be toggled independently without depending on the other's match arm having run.
    s.push_str(
        r#"_cache_total=$(( ${cache_read%.*} + ${cache_creation%.*} + ${cache_input%.*} ))

"#,
    );

    // ---- group builders (only for enabled fields; each var is empty string if unused) ----
    for field in &config.fields {
        match field.key.as_str() {
            "identity" => s.push_str(
                r#"_user="$(whoami)"; _user="${_user:0:5}"
_host="$(hostname -s)"; _host="${_host:0:5}"
g_identity="$(colored "$COLOR_identity_user" "$_user")$(colored "$WHITE" "@")$(colored "$COLOR_identity_host" "$_host")"

"#,
            ),
            "cwd" => s.push_str(
                r#"_cwd_dir="${cwd:-$(pwd)}"
[ "$_cwd_dir" = "$HOME" ] && _cwd_dir="~" || _cwd_dir="$(basename "$_cwd_dir")"
g_cwd="$(colored "$COLOR_cwd" "$_cwd_dir")"

"#,
            ),
            "model" => s.push_str(
                r#"model_name="$(printf '%s' "$model_name" | sed -E 's/ *\([^)]*\)$//')"
model_lower=$(printf '%s' "$model_name" | tr 'A-Z' 'a-z')
g_model="$(colored "$COLOR_model" "$model_lower")"
effort_abbr="$effort_level"
[ "$effort_abbr" = "medium" ] && effort_abbr="med"
[ -n "$effort_abbr" ] && g_model="$g_model $(colored "$GREY" "$effort_abbr")"

"#,
            ),
            "context" => s.push_str(
                r#"g_context=""
if [ "$ctx_size" != "0" ]; then
  ctx_pct_int=$(round_pct "$ctx_used_pct")
  ctx_total=$(( ${ctx_input%.*} + ${ctx_output%.*} ))
  ctx_breakdown="$(colored "$CYAN" "$(fmt_k "$ctx_total")")$(colored "$GREY" "/")$(colored "$CYAN" "$(fmt_k "$ctx_size")")"
  g_context="$(colored "$WHITE" "ctx") $(colored "$(color_for_pct "$ctx_pct_int")" "${ctx_pct_int}%") $ctx_breakdown"
fi

"#,
            ),
            "rate_limits" => {
                let reset_enabled = config.fields.iter().any(|f| f.key == "rate_reset" && f.enabled);
                if reset_enabled {
                    s.push_str(
                        r#"five_block=$(rate_block "5h" "$five_used" "$five_reset")
seven_block=$(rate_block "7d" "$seven_used" "$seven_reset")
if [ -n "$five_block" ] && [ -n "$seven_block" ]; then
  g_rate_limits="${five_block}  ${seven_block}"
else
  g_rate_limits="${five_block}${seven_block}"
fi

"#,
                    );
                } else {
                    s.push_str(
                        r#"five_block=$(rate_block "5h" "$five_used")
seven_block=$(rate_block "7d" "$seven_used")
if [ -n "$five_block" ] && [ -n "$seven_block" ]; then
  g_rate_limits="${five_block}  ${seven_block}"
else
  g_rate_limits="${five_block}${seven_block}"
fi

"#,
                    );
                }
            }
            "session" => s.push_str(
                r#"g_session="$(colored "$COLOR_session" "$(fmt_dur "$duration_ms")")"
if [ "$lines_added" != "0" ] || [ "$lines_removed" != "0" ]; then
  g_lines="$(colored "$GREEN" "+${lines_added}")$(colored "$GREY" "/")$(colored "$RED" "-${lines_removed}")"
  g_session="$g_session $g_lines"
fi
cost_fmt=$(awk -v c="$cost_usd" 'BEGIN{printf "$%.2f", c}')
g_session="$g_session $(colored "$CYAN" "$cost_fmt")"

"#,
            ),
            "git_branch" => s.push_str(
                r#"g_git_branch=""
[ -n "$git_branch" ] && g_git_branch="$(colored "$COLOR_git_branch" "$git_branch")"

"#,
            ),
            "cache_pct" => s.push_str(
                r#"g_cache_pct=""
if [ "$_cache_total" -gt 0 ]; then
  _cache_pct=$(awk -v r="$cache_read" -v t="$_cache_total" 'BEGIN{printf "%.0f", r/t*100}')
  g_cache_pct="$(colored "$WHITE" "cache") $(colored "$(color_for_pct_inv "$_cache_pct")" "${_cache_pct}%")"
fi

"#,
            ),
            "cache_tokens" => s.push_str(
                r#"g_cache_tokens=""
[ "$_cache_total" -gt 0 ] && g_cache_tokens="$(colored "$COLOR_cache_tokens" "$(fmt_k "$cache_read")")$(colored "$GREY" "/")$(colored "$COLOR_cache_tokens" "$(fmt_k "$_cache_total")")"

"#,
            ),
            _ => {}
        }
    }

    // ---- join enabled groups, in configured order, with " | " ----
    let enabled_keys: Vec<&str> = config
        .fields
        .iter()
        .filter(|f| f.enabled)
        .map(|f| f.key.as_str())
        .collect();
    let word_list = if enabled_keys.is_empty() {
        "\"\"".to_string()
    } else {
        enabled_keys
            .iter()
            .map(|k| format!("\"$g_{}\"", k))
            .collect::<Vec<_>>()
            .join(" ")
    };
    s.push_str("out=\"\"\n");
    s.push_str(&format!("for g in {}; do\n", word_list));
    s.push_str(
        r#"  [ -z "$g" ] && continue
  if [ -z "$out" ]; then out="$g"; else out="${out}${SEP}${g}"; fi
done

printf '%b' "$out"
"#,
    );

    s
}

const INSTALLER_HEADER: &str = r#"set -e
mkdir -p "$HOME/.claude"
FILE="$HOME/.claude/statusline-command.sh"
if [ -f "$FILE" ] && [ ! -f "$FILE.aki-bak" ]; then cp "$FILE" "$FILE.aki-bak"; fi
cat > "$FILE" <<'AKI_STATUSLINE_EOF_9f3'
"#;

const INSTALLER_FOOTER: &str = r#"
AKI_STATUSLINE_EOF_9f3
chmod +x "$FILE"
SETTINGS="$HOME/.claude/settings.json"
[ -f "$SETTINGS" ] || echo '{}' > "$SETTINGS"
tmp=$(mktemp)
jq '.statusLine.type = "command" | .statusLine.command = "~/.claude/statusline-command.sh"' "$SETTINGS" > "$tmp" && mv "$tmp" "$SETTINGS"
"#;

fn build_installer_script(body: &str) -> String {
    format!("{}{}{}", INSTALLER_HEADER, body, INSTALLER_FOOTER)
}

#[tauri::command]
pub fn get_default_statusline_config() -> StatuslineConfig {
    default_config()
}

#[derive(Serialize, Clone)]
pub struct StatuslineHostStatus {
    pub host: String,
    pub claude_installed: bool,
    pub statusline_configured: bool,
}

/// Detects, per host, whether Claude Code is present and whether our statusline script is
/// already wired into `settings.json`. Reuses the same `$CLAUDE_BIN` resolver preamble every
/// other remote script gets (see `agent_usage::run_remote_script`), so detection isn't racing
/// the same PATH-sourcing timing issue described in CLAUDE.md.
///
/// `run_remote_script` is fully synchronous (blocking `Command`/poll loop, one host after
/// another). Per CLAUDE.md's blocking-UI rule, that must never run on the command-dispatch
/// thread directly — `spawn_blocking` offloads it to Tauri's blocking thread-pool.
#[tauri::command]
pub async fn check_statusline_status(hosts: Vec<String>) -> Vec<StatuslineHostStatus> {
    const PROBE: &str = r#"
if command -v "$CLAUDE_BIN" >/dev/null 2>&1 || [ -d "$HOME/.claude" ]; then echo "CLAUDE=1"; else echo "CLAUDE=0"; fi
if [ -f "$HOME/.claude/statusline-command.sh" ] && [ -f "$HOME/.claude/settings.json" ] && grep -q "statusline-command.sh" "$HOME/.claude/settings.json" 2>/dev/null; then echo "SL=1"; else echo "SL=0"; fi
"#;
    tauri::async_runtime::spawn_blocking(move || {
        hosts
            .into_iter()
            .map(|host| {
                let (claude_installed, statusline_configured) = match run_remote_script(&host, PROBE) {
                    Ok(out) => {
                        let stdout = String::from_utf8_lossy(&out.stdout);
                        (stdout.contains("CLAUDE=1"), stdout.contains("SL=1"))
                    }
                    Err(_) => (false, false),
                };
                StatuslineHostStatus { host, claude_installed, statusline_configured }
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

/// Pushes the generated statusline script + settings.json patch to every host in `target_hosts`
/// ("local" for this machine, otherwise an ssh host string — same convention as the rest of the
/// app's remote infra). Each host is applied independently; one failing host doesn't block others.
///
/// `spawn_blocking`-wrapped for the same reason as `check_statusline_status` above — see
/// CLAUDE.md's blocking-UI rule. This was the actual bug behind the Statusline Customizer
/// freezing the whole app on open: the auto-install path calls this immediately, and it used to
/// run its blocking SSH loop straight on the async executor thread.
#[tauri::command]
pub async fn apply_statusline_config(
    config: StatuslineConfig,
    target_hosts: Vec<String>,
) -> Result<Vec<HostApplyResult>, String> {
    let body = generate_statusline_script(&config);
    let installer = build_installer_script(&body);

    tauri::async_runtime::spawn_blocking(move || {
        let mut results = Vec::new();
        for host in target_hosts {
            let outcome = run_remote_script(&host, &installer);
            let result = match outcome {
                Ok(output) => {
                    let ok = output.status.success();
                    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    HostApplyResult {
                        host: host.clone(),
                        ok,
                        message: if ok { "Applied".to_string() } else { stderr },
                    }
                }
                Err(e) => HostApplyResult { host: host.clone(), ok: false, message: e },
            };
            crate::logger::info(
                "STATUSLINE",
                &format!("apply host={} ok={} msg={}", result.host, result.ok, preview(&result.message, 200)),
            );
            results.push(result);
        }
        results
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
