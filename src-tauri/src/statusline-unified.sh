#!/bin/bash
# Aki Dev Sync - UNIFIED statusline. ONE physical file, installed verbatim at BOTH paths:
#   ~/.claude/statusline-command.sh          (Claude Code)
#   ~/.gemini/antigravity-cli/statusline.sh  (AGY CLI)
# It self-identifies from $0 - see the case block below.
#
# THIS FILE IS THE GENERATOR TEMPLATE - it is source, not documentation, which is why it lives beside
# statusline.rs inside the crate rather than under docs/. statusline.rs pulls it in with include_str!
# and, on Apply, replaces everything between the AKI-GENERATED-CONFIG markers with the values from
# the Statusline Customizer (Vue is the SSOT for those values; Rust holds no defaults of its own).
# Everything outside that region is shipped byte-for-byte, so this file is simultaneously the
# reference implementation and the thing that actually runs. Edit the body here, never in a copy.
# As checked in, the region holds the same defaults the Vue UI ships with, so the file is runnable
# and testable on its own without ever going through the generator.
input=$(cat)

# Which CLI is running us. One physical script is installed at both paths, so the invocation path
# is the identity: only AGY installs under ~/.gemini/. This never guesses from payload fields.
case "$0" in
  */.gemini/*) CLI="AG" ;;
  *)           CLI="CC" ;;
esac

# Raw payload dump, BEFORE the rlcache merge - a post-merge dump always looks like it had
# .rate_limits even on turns where the CLI did not send it.
echo "$input" > /tmp/statusline_stdin_dump.json 2>/dev/null

# aki-rlcache v4 - persist rate_limits across calls that omit it. Claude Code only: AGY carries
# its quota in .quota on every turn and has no .rate_limits to persist, so an AGY run must not
# read or write Claude Code's cache file.
#
# v2 lost data three ways v3 fixed: a corrupt cache file killed the whole line (--argjson on
# invalid JSON), a payload carrying only five_hour overwrote seven_day, and the write was not
# atomic. So: validate before reading, merge instead of replace, write via tmp+mv.
#
# DESIGN LOCK - the cache must NEVER be merged or displayed without checking (1) resets_at is
# still in the future and (2) the entry belongs to the CURRENT account. That bug shipped from
# 1.10.0 through 1.17.0: an object-merge that only ever adds/overwrites keys present in the live
# payload (and never drops absent ones) keeps a stale field alive forever, so a host showed a
# phantom "7d 45%" for an account that has no weekly limit at all. v4 closes both halves.
# See docs/plan/1.18.0-statusline-apply-correctness.md, section P0-5 (data integrity).
if [ "$CLI" = "CC" ]; then
  RLCACHE="$HOME/.claude/rate-limits-cache.json"
  mkdir -p "$HOME/.claude" 2>/dev/null
  _rl_now=$(date +%s)
  _rl_acct=""
  if [ -f "$HOME/.claude.json" ]; then
      _rl_acct=$(jq -r '.oauthAccount.emailAddress // ""' "$HOME/.claude.json" 2>/dev/null)
  fi
  # Drop every entry that is past its reset. resets_at == 0 means "unknown", not "expired".
  _rl_prune() {
      jq -c -n --argjson rl "$1" --argjson now "$_rl_now" '
        $rl | with_entries(select(
          ((.value | type) != "object")
          or ((.value.resets_at // 0) <= 0)
          or ((.value.resets_at) > $now)
        ))' 2>/dev/null
  }
  rl_input=$(printf '%s' "$input" | jq -c '.rate_limits // empty' 2>/dev/null)
  rl_cached=""
  if [ -f "$RLCACHE" ] && jq -e . "$RLCACHE" >/dev/null 2>&1; then
      # Account gate: a cache written by another account is not ours to display.
      _rl_cached_acct=$(jq -r '.account // ""' "$RLCACHE" 2>/dev/null)
      if [ "$_rl_cached_acct" = "$_rl_acct" ]; then
          rl_cached=$(jq -c '.rate_limits // empty' "$RLCACHE" 2>/dev/null)
      fi
  fi
  rl_merged="$rl_cached"
  if [ -n "$rl_input" ]; then
      rl_merged="$rl_input"
      if [ -n "$rl_cached" ]; then
          _merged=$(jq -c -n --argjson old "$rl_cached" --argjson new "$rl_input" '$old * $new' 2>/dev/null)
          [ -n "$_merged" ] && rl_merged="$_merged"
      fi
  fi
  if [ -n "$rl_merged" ]; then
      _pruned=$(_rl_prune "$rl_merged")
      [ -n "$_pruned" ] && rl_merged="$_pruned"
      [ "$rl_merged" = "{}" ] && rl_merged=""
  fi
  if [ -n "$rl_merged" ]; then
      _with_rl=$(printf '%s' "$input" | jq -c --argjson rl "$rl_merged" '.rate_limits = $rl' 2>/dev/null)
      [ -n "$_with_rl" ] && input="$_with_rl"
      _rl_tmp=$(mktemp "$HOME/.claude/.rate-limits-cache.XXXXXX" 2>/dev/null)
      if [ -n "$_rl_tmp" ]; then
          if printf '{"account":%s,"rate_limits":%s}\n' "$(jq -n --arg a "$_rl_acct" '$a')" "$rl_merged" > "$_rl_tmp" 2>/dev/null; then
              mv -f "$_rl_tmp" "$RLCACHE" 2>/dev/null || rm -f "$_rl_tmp"
          else
              rm -f "$_rl_tmp"
          fi
      fi
  elif [ -f "$RLCACHE" ]; then
      # Nothing survived the account/expiry gates - do not leave the stale file behind to be
      # re-read (and re-trusted) on the next call.
      rm -f "$RLCACHE" 2>/dev/null
  fi
fi

if [ -n "$input" ]; then
  eval $(echo "$input" | jq -r '
    # Model name and the quota pool it selects, resolved ONCE up front.
    # .model is an object on both CLIs today but was a plain string on older ones, and a single
    # unguarded .model.display_name against a string aborts the WHOLE jq program - every JSON_*
    # comes back empty and the statusline renders blank. Guard it here, in one place, rather than
    # repeating the same conditional at each quota lookup.
    (if (.model | type) == "object" then (.model.display_name // .model.id // "") else (.model // "") end | tostring) as $model_name
    | (if ($model_name | ascii_downcase | contains("gemini")) then "gemini" else "3p" end) as $pool
    | (if (.quota | type) == "object" then .quota else {} end) as $quota
    # A quota entry is either an object or a bare number meaning "fraction remaining"; a bare
    # number carries no reset time, so qreset yields 0 for it rather than reusing the fraction.
    | def qfrac($k): $quota[$k] as $v
        | if ($v | type) == "object" then ($v.remaining_fraction // -1)
          elif ($v | type) == "number" then $v
          else -1 end;
      def qreset($k): $quota[$k] as $v
        | if ($v | type) == "object" then ($v.reset_in_seconds // 0) else 0 end;
    [
      "JSON_CWD=" + ((.cwd // .workspace.current_dir // "") | @sh),
      "JSON_MODEL=" + ($model_name | @sh),
      "JSON_EFFORT=" + ((if (.effort | type) == "object" then .effort.level elif (.effort | type) == "string" then .effort elif (.model | type) == "object" then .model.effort else null end // "") | @sh),
      "JSON_COST=" + ((.cost.total_cost_usd // 0) | @sh),
      "JSON_DUR=" + ((.cost.total_duration_ms // .duration_ms // 0) | @sh),
      "JSON_ADDED=" + ((.cost.total_lines_added // .lines_added // 0) | @sh),
      "JSON_REMOVED=" + ((.cost.total_lines_removed // .lines_removed // 0) | @sh),
      "JSON_BRANCH=" + ((.workspace.git_branch // .git.branch // "") | @sh),
      "JSON_CTX_INPUT=" + ((.context_window.total_input_tokens // 0) | @sh),
      "JSON_CTX_OUTPUT=" + ((.context_window.total_output_tokens // 0) | @sh),
      "JSON_CTX_SIZE=" + ((.context_window.context_window_size // 0) | @sh),
      "JSON_CACHE_READ=" + ((.context_window.current_usage.cache_read_input_tokens // 0) | @sh),
      "JSON_CACHE_CREATE=" + ((.context_window.current_usage.cache_creation_input_tokens // 0) | @sh),
      "JSON_CACHE_INPUT=" + ((.context_window.current_usage.input_tokens // 0) | @sh),

      "JSON_5H_REM=" + (qfrac($pool + "-5h") | @sh),
      "JSON_5H_SEC=" + (qreset($pool + "-5h") | @sh),
      "JSON_7D_REM=" + (qfrac($pool + "-weekly") | @sh),
      "JSON_7D_SEC=" + (qreset($pool + "-weekly") | @sh),
      "JSON_CLAUDE_5H_USED=" + (if (.rate_limits | type) == "object" and (.rate_limits.five_hour | type) == "object" then (.rate_limits.five_hour.used_percentage // -1) else -1 end | @sh),
      "JSON_CLAUDE_7D_USED=" + (if (.rate_limits | type) == "object" and (.rate_limits.seven_day | type) == "object" then (.rate_limits.seven_day.used_percentage // -1) else -1 end | @sh),
      "JSON_CLAUDE_5H_RESET=" + ((.rate_limits.five_hour.resets_at // 0) | @sh),
      "JSON_CLAUDE_7D_RESET=" + ((.rate_limits.seven_day.resets_at // 0) | @sh),
      "JSON_ACCOUNT_EMAIL=" + ((.account.email // .user.email // .email // "") | @sh)
    ] | join("\n")
  ' 2>/dev/null)
fi

if [ -n "$JSON_CWD" ] && [ "$JSON_CWD" != "null" ]; then
  [ "$JSON_CWD" = "$HOME" ] && cwd_dir="~" || cwd_dir="$(basename "$JSON_CWD")"
fi

# ANSI Colors. Fixed palette - these are the script's own vocabulary (a label is WHITE, a
# secondary reading is GREY, the ladder owns the five BOLD_* tiers) and are deliberately NOT
# user-configurable. Only the six COLOR_* variables in the generated region below are.
RESET_ALL='\033[00m'
RESET_FG='\033[22;39m'  # reset bold+fg, keep bg
BOLD_GREEN='\033[01;32m'
BOLD_YELLOW='\033[01;33m'
BOLD_ORANGE='\033[01;38;5;208m'
BOLD_RED='\033[01;31m'
BOLD_BLUE='\033[01;34m'
WHITE='\033[97m'
GREY='\033[90m'
COST_FULL_USD=30

# >>> AKI-GENERATED-CONFIG >>> (everything between these two markers is rewritten on Apply)
# Enable flags, already dependency-resolved (see DEPENDS in ClaudeSettingModal.vue).
EN_cli_tag=1
EN_account=1
EN_identity_user=1
EN_identity_host=1
EN_cwd=1
EN_model=1
EN_effort=1
EN_context=1
EN_cache=1
EN_cache_pct=1
EN_cache_tokens=0
EN_rate_limits_5h=1
EN_rate_reset_5h=1
EN_rate_limits_7d=1
EN_rate_reset_7d=1
EN_session=1
EN_git_branch=1
EN_ram=1

# User-chosen colors. Every other color is a fixed label color or comes from the ladder.
COLOR_identity_user='\033[97m'
COLOR_identity_host='\033[97m'
COLOR_cwd='\033[35m'
COLOR_model='\033[36m'
COLOR_git_branch='\033[35m'
COLOR_account='\033[90m'

# Dynamic-color ladder, ascending. Below THRESH_GREEN is the calm blue tier.
THRESH_GREEN=20
THRESH_YELLOW=51
THRESH_ORANGE=75
THRESH_RED=90

# Truncate widths. The template clamps these itself (floor 3, per-field ceiling).
TRUNC_ACCOUNT=4   # applied AFTER stripping everything from '@' onwards
TRUNC_USER=5
TRUNC_HOST=6
TRUNC_CWD=12
TRUNC_BRANCH=10

# Zebra background shades, from the neutral ramp only.
BG_ZEBRA_A=16
BG_ZEBRA_B=235

# One space either side of every zebra block. The tag cluster is never padded.
SEPARATE_BLOCKS=1

# Print order, by block, taken from the row order in the Statusline Customizer.
BLOCK_ORDER="identity cwd model context cache quota session git_branch ram"
# <<< AKI-GENERATED-CONFIG <<<

# Clamp so a bad generated value degrades instead of rendering garbage. The ceiling is deliberately
# not uniform: a directory or branch name needs more room before it stops being recognisable than a
# user name does, so those two go to 15 while the rest stay at 12.
for _pair in TRUNC_ACCOUNT:12 TRUNC_USER:12 TRUNC_HOST:12 TRUNC_CWD:15 TRUNC_BRANCH:15; do
  _t="${_pair%%:*}"; _max="${_pair##*:}"
  eval "_v=\$$_t"
  [ "$_v" -lt 3 ] 2>/dev/null && eval "$_t=3"
  [ "$_v" -gt "$_max" ] 2>/dev/null && eval "$_t=$_max"
done

colored() { printf '%s%s%s' "$1" "$2" "$RESET_FG"; }

round_pct() {
  awk -v p="$1" 'BEGIN{printf "%.0f", p}' 2>/dev/null || echo "0"
}

color_for_pct() {
  p="$1"
  if [ "$p" -ge "$THRESH_RED" ]; then printf '%s' "$BOLD_RED"
  elif [ "$p" -ge "$THRESH_ORANGE" ]; then printf '%s' "$BOLD_ORANGE"
  elif [ "$p" -ge "$THRESH_YELLOW" ]; then printf '%s' "$BOLD_YELLOW"
  elif [ "$p" -ge "$THRESH_GREEN" ]; then printf '%s' "$BOLD_GREEN"
  else printf '%s' "$BOLD_BLUE"; fi
}

fmt_k() {
  awk -v t="$1" 'BEGIN {
    if (t >= 1000000) { printf "%.0fM", t/1000000 }
    else if (t >= 1000) { printf "%.0fk", t/1000 }
    else { printf "%d", t }
  }' 2>/dev/null
}

fmt_dur() {
  ms="${1:-0}"
  s=$(( ${ms%.*} / 1000 ))
  h=$(( s / 3600 ))
  m=$(( (s % 3600) / 60 ))
  if [ "$h" -gt 0 ]; then printf '%dh%dm' "$h" "$m"; else printf '%dm' "$m"; fi
}

# The model name as one compact token: drop the trailing "(...)" note and the vendor word, glue what
# is left. Removing a word from the middle of an id leaves its separators orphaned
# ("gemini-2.5-flash" -> "-2.5-flash"), so the last step trims the edges and squeezes any doubled
# run - otherwise the line opens with a stray dash.
compact_model() {
  printf '%s' "$1" \
    | sed -E 's/ *\([^)]*\)$//' \
    | sed -E 's/[Cc][Ll][Aa][Uu][Dd][Ee]//g; s/[Gg][Ee][Mm][Ii][Nn][Ii]//g' \
    | tr -d ' ' \
    | sed -E 's|[-_./]{2,}|-|g; s|^[-_./]+||; s|[-_./]+$||'
}

fmt_sec_eta() {
  sec="$1"
  [ -z "$sec" ] || [ "$sec" -le 0 ] 2>/dev/null && return
  d=$(( sec / 86400 ))
  h=$(( (sec % 86400) / 3600 ))
  m=$(( (sec % 3600) / 60 ))
  if [ "$d" -gt 0 ]; then printf '%dd%dh' "$d" "$h"; else printf '%dh%dm' "$h" "$m"; fi
}

# A quota reading is either a remaining *fraction* (0..1) or a remaining *percent* (>1); both mean
# "how much is left", so used% is 100 minus the reading on the same scale.
quota_used_pct() {
  awk -v rem="$1" 'BEGIN{
    p = (rem <= 1) ? (1 - rem) * 100 : 100 - rem
    if (p < 0) p = 0
    if (p > 100) p = 100
    printf "%.0f", p
  }' 2>/dev/null
}

# Identity
g_identity=""
if [ "$EN_identity_user" = 1 ] || [ "$EN_identity_host" = 1 ]; then
  u_part=""
  h_part=""
  if [ "$EN_identity_user" = 1 ]; then
    user="$(whoami)"; user="${user:0:$TRUNC_USER}"
    u_part="$(colored "$COLOR_identity_user" "$user")"
  fi
  if [ "$EN_identity_host" = 1 ]; then
    host="$(hostname -s)"; host="${host:0:$TRUNC_HOST}"
    h_part="$(colored "$COLOR_identity_host" "$host")"
  fi
  if [ -n "$u_part" ] && [ -n "$h_part" ]; then
    g_identity="${u_part}$(colored "$GREY" "@")${h_part}"
  else
    g_identity="${u_part}${h_part}"
  fi
fi

# CWD
g_cwd=""
if [ "$EN_cwd" = 1 ]; then
  _cwd_dir="${cwd_dir:-$(pwd)}"
  [ "$_cwd_dir" = "$HOME" ] && _cwd_dir="~" || _cwd_dir="$(basename "$_cwd_dir")"
  _cwd_dir="${_cwd_dir:0:$TRUNC_CWD}"
  g_cwd="$(colored "$COLOR_cwd" "$_cwd_dir")"
fi

# Model & Effort
g_model=""
if [ "$EN_model" = 1 ] || [ "$EN_effort" = 1 ]; then
  raw_model="$JSON_MODEL"
  effort_found="$JSON_EFFORT"
  if [ -z "$effort_found" ]; then
    effort_found=$(echo "$raw_model" | sed -n -E 's/.*\(([^\)]+)\).*/\1/p')
    [ "$(echo "$effort_found" | tr 'A-Z' 'a-z')" = "thinking" ] && effort_found=""
  fi
  clean_model=$(compact_model "$raw_model")
  effort_lower=$(printf '%s' "$effort_found" | tr 'A-Z' 'a-z')
  [ "$effort_lower" = "medium" ] && effort_lower="med"

  m_part=""
  e_part=""
  if [ "$EN_model" = 1 ] && [ -n "$clean_model" ]; then m_part="$(colored "$COLOR_model" "$clean_model")"; fi
  if [ "$EN_effort" = 1 ] && [ -n "$effort_lower" ]; then e_part="$(colored "$GREY" "$effort_lower")"; fi
  # Glued, no separator: model and effort read as one token ("Opus4.8med").
  g_model="${m_part}${e_part}"
fi

# Context Window
g_context=""
if [ "$EN_context" = 1 ] && [ -n "$JSON_CTX_SIZE" ] && [ "$JSON_CTX_SIZE" -gt 0 ] 2>/dev/null; then
  ctx_total=$(( ${JSON_CTX_INPUT%.*} + ${JSON_CTX_OUTPUT%.*} ))
  [ "$ctx_total" -eq 0 ] && ctx_total=${JSON_CTX_INPUT%.*}

  ctx_fmt="$(fmt_k "$ctx_total")"
  size_fmt="$(fmt_k "$JSON_CTX_SIZE")"

  # Trong quá trình sử dụng thì % này không được chú ý lắm.
  # Quan trọng là số context đối với bất kì model nào cũng sẽ bắt đầu bị nặng và giảm chất lượng ở khoảng 185k.
  # Do đó, lược bỏ % và tô màu số token theo thang tối đa 200,000 (200k = 100%).
  dynamic_pct=$(awk -v used="$ctx_total" 'BEGIN{p=used/200000*100; if(p>100)p=100; printf "%.0f", p}' 2>/dev/null)
  pct_color=$(color_for_pct "$dynamic_pct")

  # Không còn % nữa, thay vào đó tô màu pct_color thẳng vào biến ctx_fmt
  g_context="$(colored "$WHITE" "ctx")$(colored "$pct_color" "${ctx_fmt}")/$(colored "$GREY" "${size_fmt}")"
fi

# Quota / Rate Limits
g_quota=""
rate_parts=""

if [ "$EN_rate_limits_5h" = 1 ]; then
  five_used_pct=""
  five_sec="$JSON_5H_SEC"
  # Claude Code ships an absolute epoch (.rate_limits.*.resets_at), AGY a relative countdown
  # (.quota[*].reset_in_seconds). Normalise CC to the same "seconds from now" scale.
  if [ "${five_sec:-0}" -le 0 ] 2>/dev/null && [ "${JSON_CLAUDE_5H_RESET:-0}" -gt 0 ] 2>/dev/null; then
    five_sec=$(( JSON_CLAUDE_5H_RESET - $(date +%s) ))
  fi
  if [ "$JSON_5H_REM" != "-1" ] && [ -n "$JSON_5H_REM" ]; then
    five_used_pct=$(quota_used_pct "$JSON_5H_REM")
  elif [ "$JSON_CLAUDE_5H_USED" != "-1" ] && [ -n "$JSON_CLAUDE_5H_USED" ]; then
    five_used_pct=$(round_pct "$JSON_CLAUDE_5H_USED")
  fi

  if [ -n "$five_used_pct" ]; then
    five_color=$(color_for_pct "$five_used_pct")
    b5="$(colored "$WHITE" "5h:")$(colored "$five_color" "${five_used_pct}%")"
    if [ "$EN_rate_reset_5h" = 1 ] && [ -n "$five_sec" ] && [ "$five_sec" -gt 0 ] 2>/dev/null; then
      eta5=$(fmt_sec_eta "$five_sec")
      [ -n "$eta5" ] && b5="${b5}$(colored "$GREY" "$eta5")"
    fi
    rate_parts="$b5"
  fi
fi

if [ "$EN_rate_limits_7d" = 1 ]; then
  seven_used_pct=""
  seven_sec="$JSON_7D_SEC"
  if [ "${seven_sec:-0}" -le 0 ] 2>/dev/null && [ "${JSON_CLAUDE_7D_RESET:-0}" -gt 0 ] 2>/dev/null; then
    seven_sec=$(( JSON_CLAUDE_7D_RESET - $(date +%s) ))
  fi
  if [ "$JSON_7D_REM" != "-1" ] && [ -n "$JSON_7D_REM" ]; then
    seven_used_pct=$(quota_used_pct "$JSON_7D_REM")
  elif [ "$JSON_CLAUDE_7D_USED" != "-1" ] && [ -n "$JSON_CLAUDE_7D_USED" ]; then
    seven_used_pct=$(round_pct "$JSON_CLAUDE_7D_USED")
  fi

  if [ -n "$seven_used_pct" ]; then
    seven_color=$(color_for_pct "$seven_used_pct")
    b7="$(colored "$WHITE" "7d:")$(colored "$seven_color" "${seven_used_pct}%")"
    if [ "$EN_rate_reset_7d" = 1 ] && [ -n "$seven_sec" ] && [ "$seven_sec" -gt 0 ] 2>/dev/null; then
      eta7=$(fmt_sec_eta "$seven_sec")
      [ -n "$eta7" ] && b7="${b7}$(colored "$GREY" "$eta7")"
    fi
    if [ -z "$rate_parts" ]; then rate_parts="$b7"; else rate_parts="$rate_parts $b7"; fi
  fi
fi

g_quota="$rate_parts"

# Session Stats
g_session=""
if [ "$EN_session" = 1 ]; then
  # Nếu JSON_DUR rỗng/bằng 0 (như AGY), đọc trực tiếp tuổi thọ tiến trình Terminal ($PPID) từ OS Kernel
  if [ -z "$JSON_DUR" ] || [ "$JSON_DUR" -le 0 ] 2>/dev/null; then
    dur_sec=$(ps -o etime= -p "$PPID" 2>/dev/null | awk -F: '{if (NF==3) print $1*3600+$2*60+$3; else if (NF==2) print $1*60+$2; else print $1}')
    [ -n "$dur_sec" ] && JSON_DUR=$(( dur_sec * 1000 ))
  fi

  if [ -n "$JSON_DUR" ] && [ "$JSON_DUR" -ge 0 ] 2>/dev/null; then
    g_session="$(colored "$WHITE" "ss")$(colored "$GREY" "$(fmt_dur "$JSON_DUR")")"
    if [ "${JSON_ADDED:-0}" != "0" ] || [ "${JSON_REMOVED:-0}" != "0" ]; then
      g_session="$g_session $(colored "$BOLD_GREEN" "+${JSON_ADDED}")$(colored "$GREY" "/")$(colored "$BOLD_RED" "-${JSON_REMOVED}")"
    fi
    if [ -n "$JSON_COST" ] && [ "$JSON_COST" != "0" ]; then
      cost_fmt=$(awk -v c="$JSON_COST" 'BEGIN{printf "$%.2f", c}' 2>/dev/null)
      cost_pct=$(awk -v c="$JSON_COST" -v full="$COST_FULL_USD" 'BEGIN{p=(full>0)?c/full*100:0; if(p>100)p=100; printf "%.0f", p}' 2>/dev/null)
      cost_col=$(color_for_pct "${cost_pct:-0}")
      g_session="$g_session $(colored "$cost_col" "$cost_fmt")"
    fi
  fi
fi

# Cache Stats
g_cache=""
if [ "$EN_cache" = 1 ]; then
  cache_read=${JSON_CACHE_READ%.*}
  cache_create=${JSON_CACHE_CREATE%.*}
  cache_in=${JSON_CACHE_INPUT%.*}
  cache_total=$(( ${cache_read:-0} + ${cache_create:-0} + ${cache_in:-0} ))
  if [ "$cache_total" -gt 0 ]; then
    if [ "$EN_cache_pct" = 1 ]; then
      _pct=$(awk -v r="$cache_read" -v t="$cache_total" 'BEGIN{printf "%.0f", r/t*100}' 2>/dev/null)
      # Static grey, deliberately off the ladder: a high cache hit rate is good news and must not
      # shout in red the way a high quota reading should.
      g_cache="$(colored "$WHITE" "↬")$(colored "$GREY" "${_pct}%")"
    fi
    if [ "$EN_cache_tokens" = 1 ]; then
      _tok="$(colored "$GREY" "$(fmt_k "${cache_read:-0}")")"
      if [ -n "$g_cache" ]; then g_cache="$g_cache $_tok"; else g_cache="$_tok"; fi
    fi
  fi
fi

# Git Branch
g_git_branch=""
if [ "$EN_git_branch" = 1 ]; then
  git_branch="${JSON_BRANCH:-$(git branch --show-current 2>/dev/null)}"
  [ -n "$git_branch" ] && g_git_branch="$(colored "$COLOR_git_branch" "${git_branch:0:$TRUNC_BRANCH}")"
fi

# Account Email & Unified Tag
g_cli_tag=""
if [ "$EN_cli_tag" = 1 ]; then
  acc_name=""
  if [ "$EN_account" = 1 ]; then
    # Neither CLI puts an email in the payload, so each falls back to its own on-disk account file.
    # These must NOT be tried in sequence: on a machine that has both, whichever file exists would
    # win and the tag would show the other CLI's account.
    if [ -z "$JSON_ACCOUNT_EMAIL" ]; then
      if [ "$CLI" = "CC" ]; then
        [ -f "$HOME/.claude.json" ] && \
          JSON_ACCOUNT_EMAIL=$(jq -r '.oauthAccount.emailAddress // ""' "$HOME/.claude.json" 2>/dev/null)
      else
        [ -f "$HOME/.gemini/google_accounts.json" ] && \
          JSON_ACCOUNT_EMAIL=$(jq -r '.active // ""' "$HOME/.gemini/google_accounts.json" 2>/dev/null)
      fi
    fi
    # The domain is dropped FIRST - AGY puts a full address in the payload, so a raw 4-char cut of
    # "lva@akitao.com" would print the useless "lva@". Cut the local part, THEN truncate.
    acc_name="${JSON_ACCOUNT_EMAIL%%@*}"
    acc_name="${acc_name:0:$TRUNC_ACCOUNT}"
  fi

  if [ "$CLI" = "CC" ]; then
    cli_label="CC"
    cli_col="\033[1;38;5;208m"  # Màu Cam đặc trưng của Claude Code
  else
    cli_label="AG"
    cli_col="\033[1;38;5;33m"   # Màu Xanh Royal của AGY
  fi

  g_cli_tag="\033[48;5;255m${cli_col}${cli_label}"
  # \033[22m before the account color: the CLI label above turned bold on, and the account name is
  # a secondary reading on a light plate - left bold it washes out whatever color is chosen.
  [ -n "$acc_name" ] && g_cli_tag="${g_cli_tag}\033[48;5;252m\033[22m${COLOR_account} ${acc_name}"
  g_cli_tag="${g_cli_tag}${RESET_ALL}"
fi

# System RAM Usage
g_ram=""
if [ "$EN_ram" = 1 ]; then
  if [[ "$OSTYPE" == "darwin"* ]]; then
    mem_pct=$(memory_pressure 2>/dev/null | awk '/System-wide memory free percentage:/ {print 100 - $5}')
  elif [[ "$OSTYPE" == "linux-gnu"* ]] || [[ "$OSTYPE" == "linux" ]]; then
    mem_pct=$(free 2>/dev/null | awk '/Mem:/ {printf "%.0f", $3/$2 * 100}')
  fi
  # Static grey like the cache reading: whole-machine RAM is context, not something the ladder
  # should escalate about mid-session.
  [ -n "$mem_pct" ] && g_ram="$(colored "$WHITE" "⚅")$(colored "$GREY" "${mem_pct}%")"
fi

# Assemble output. The tag cluster leads and is outside the zebra - it paints its own background.
out="$g_cli_tag"

_zebra=0
for _name in $BLOCK_ORDER; do
  eval "item=\$g_$_name"
  [ -z "$item" ] && continue
  if [ $(( _zebra % 2 )) -eq 0 ]; then _bg="$BG_ZEBRA_A"; else _bg="$BG_ZEBRA_B"; fi
  _zebra=$(( _zebra + 1 ))
  if [ "$SEPARATE_BLOCKS" = "1" ]; then
    out="${out}\033[48;5;${_bg}m ${item}${RESET_FG} "
  else
    out="${out}\033[48;5;${_bg}m${item}"
  fi
done
out="${out}${RESET_ALL}"

printf '%b\n' "$out"
