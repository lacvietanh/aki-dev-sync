#!/bin/bash
input=$(cat)

# aki-rlcache v2 - persist rate_limits across calls that omit it
rl_input=$(echo "$input" | jq -c '.rate_limits // empty')
if [ -z "$rl_input" ] && [ -f "$HOME/.claude/rate-limits-cache.json" ]; then
    input=$(echo "$input" | jq --argjson old "$(cat "$HOME/.claude/rate-limits-cache.json")" '
        if ($old.rate_limits != null) then
            .rate_limits = $old.rate_limits
        else . end
    ')
fi
printf '%s' "$input" > "$HOME/.claude/rate-limits-cache.json"

IFS=$'\x1f' read -r cwd model_name cost_usd duration_ms lines_added lines_removed \
  ctx_input ctx_output ctx_size ctx_used_pct \
  cur_input cur_output cur_cache_creation cur_cache_read \
  effort_level thinking_enabled five_used five_reset seven_used seven_reset <<< "$(echo "$input" | jq -r '[
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
    (.context_window.current_usage.input_tokens // 0),
    (.context_window.current_usage.output_tokens // 0),
    (.context_window.current_usage.cache_creation_input_tokens // 0),
    (.context_window.current_usage.cache_read_input_tokens // 0),
    (.effort.level // ""),
    (.thinking.enabled // false),
    (.rate_limits.five_hour.used_percentage // -1),
    (.rate_limits.five_hour.resets_at // ""),
    (.rate_limits.seven_day.used_percentage // -1),
    (.rate_limits.seven_day.resets_at // "")
  ] | join("")')"

# ---- palette: every color used below is named here, nothing inline past this point ----
RESET='\033[00m'
BOLD_BLUE='\033[01;34m'
BOLD_GREEN='\033[01;32m'
BOLD_YELLOW='\033[01;33m'
BOLD_ORANGE='\033[01;38;5;208m'
BOLD_RED='\033[01;31m'
CYAN='\033[36m'
GREEN='\033[32m'
RED='\033[31m'
WHITE='\033[97m'
GREY='\033[90m'

# ---- helpers ----
colored() { printf '%s%s%s' "$1" "$2" "$RESET"; }

# 4-tier usage scale: green <50%, yellow 50-70%, orange 70-85%, red >=85%
color_for_pct() {
  p="$1"
  if awk -v p="$p" 'BEGIN{exit !(p>=85)}'; then printf '%s' "$BOLD_RED"
  elif awk -v p="$p" 'BEGIN{exit !(p>=70)}'; then printf '%s' "$BOLD_ORANGE"
  elif awk -v p="$p" 'BEGIN{exit !(p>=50)}'; then printf '%s' "$BOLD_YELLOW"
  else printf '%s' "$BOLD_GREEN"; fi
}

# 999 -> "999", 90000 -> "90.0k", 1000000 -> "1M", 1500000 -> "1.5M"
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

# round a raw (possibly float-noisy, e.g. "41.99999999998") percentage to a clean integer
round_pct() {
  awk -v p="$1" 'BEGIN{printf "%.0f", p}'
}

# "5h"/"7d" white label, ":" grey, "N%" threshold-colored - no reset time
rate_block() {
  label="$1" used="$2"
  [ "$used" = "-1" ] && return
  used_int=$(round_pct "$used")
  pct_color=$(color_for_pct "$used_int")
  printf '%s%s%s' "$(colored "$WHITE" "$label")" "$(colored "$GREY" ":")" "$(colored "$pct_color" "${used_int}%")"
}

SEP="$(colored "$GREY" " | ")"

# ---- build each group, then join with " | " ----

# user (cyan) @ (white) host (green) - user/host capped at 5 chars
_user="$(whoami)"; _user="${_user:0:5}"
_host="$(hostname -s)"; _host="${_host:0:5}"
g_id="$(colored "$CYAN" "$_user")$(colored "$WHITE" "@")$(colored "$BOLD_GREEN" "$_host")"

# cwd (blue) - its own group
_cwd_dir="${cwd:-$(pwd)}"
[ "$_cwd_dir" = "$HOME" ] && _cwd_dir="~" || _cwd_dir="$(basename "$_cwd_dir")"
g_cwd="$(colored "$BOLD_BLUE" "$_cwd_dir")"

# model (cyan) + effort (grey), abbreviated: medium->med, rest unchanged (low/high/xhigh/max)
# strip trailing parenthetical (e.g. "Opus 4.8 (1M context)" -> "Opus 4.8") - context size is already shown in the ctx group
model_name="$(printf '%s' "$model_name" | sed -E 's/ *\([^)]*\)$//')"
model_lower=$(printf '%s' "$model_name" | tr 'A-Z' 'a-z')
g_model="$(colored "$CYAN" "$model_lower")"
effort_abbr="$effort_level"
[ "$effort_abbr" = "medium" ] && effort_abbr="med"
[ -n "$effort_abbr" ] && g_model="$g_model $(colored "$GREY" "$effort_abbr")"

# context window usage - "ctx" white, % threshold-colored, cyan numbers (no brackets - color alone
# already distinguishes the breakdown from surrounding groups)
# total = input+output combined (matches what used_percentage represents), not shown separately
g_ctx=""
if [ "$ctx_size" != "0" ]; then
  ctx_pct_int=$(round_pct "$ctx_used_pct")
  ctx_total=$(( ${ctx_input%.*} + ${ctx_output%.*} ))
  ctx_breakdown="$(colored "$CYAN" "$(fmt_k "$ctx_total")")$(colored "$GREY" "/")$(colored "$CYAN" "$(fmt_k "$ctx_size")")"
  g_ctx="$(colored "$WHITE" "ctx") $(colored "$(color_for_pct "$ctx_pct_int")" "${ctx_pct_int}%") $ctx_breakdown"
fi

# rate limits (Pro/Max only) - 5h/7d separated by 2 plain spaces, no " | "
g_rate=""
five_block=$(rate_block "5h" "$five_used")
seven_block=$(rate_block "7d" "$seven_used")
if [ -n "$five_block" ] && [ -n "$seven_block" ]; then
  g_rate="${five_block}  ${seven_block}"
else
  g_rate="${five_block}${seven_block}"
fi

# session group - duration (grey), lines +/-, cost (cyan), all together, placed last
g_session="$(colored "$GREY" "$(fmt_dur "$duration_ms")")"
if [ "$lines_added" != "0" ] || [ "$lines_removed" != "0" ]; then
  g_lines="$(colored "$GREEN" "+${lines_added}")$(colored "$GREY" "/")$(colored "$RED" "-${lines_removed}")"
  g_session="$g_session $g_lines"
fi
cost_fmt=$(awk -v c="$cost_usd" 'BEGIN{printf "$%.2f", c}')
g_session="$g_session $(colored "$CYAN" "$cost_fmt")"

# ---- join non-empty groups with " | " ----
out=""
for g in "$g_id" "$g_cwd" "$g_model" "$g_ctx" "$g_rate" "$g_session"; do
  [ -z "$g" ] && continue
  if [ -z "$out" ]; then out="$g"; else out="${out}${SEP}${g}"; fi
done

printf '%b' "$out"
