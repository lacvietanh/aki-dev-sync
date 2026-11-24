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
#
# AGY kills a statusline that runs past 5 s, so every turn spawns as few processes as it can: one jq pass reads the payload, the rest is shell builtins (docs/feat/statusline-customizer.md § Process budget).
input=$(cat)

# Which CLI is running us. One physical script is installed at both paths, so the invocation path is the identity: only AGY installs under ~/.gemini/. This never guesses from payload fields.
case "$0" in
  */.gemini/*) CLI="AG" ;;
  *)           CLI="CC" ;;
esac

# Characters an account address may keep in a per-account file name; every other character becomes "_". The usage probe finds the files by this same rule.
ACCOUNT_FILE_UNSAFE='[^A-Za-z0-9@._+-]'

# aki-rlcache v10 - one quota cache per account, CC only. Expiry/account/staleness gates are a DESIGN LOCK: docs/feat/statusline-customizer.md § Quota cache.
if [ "$CLI" = "CC" ]; then
  _rl_config_dir="${CLAUDE_CONFIG_DIR:-$HOME/.claude}"
  _rl_now=$(date +%s)
  # Sets RL_MTIME, empty for a non-numeric result. BSD stat on macOS; elsewhere the GNU form first, since GNU `stat -f` succeeds with the wrong output (--file-system).
  _rl_mtime() {
      case "$OSTYPE" in
        darwin*) RL_MTIME=$(stat -f %m "$1" 2>/dev/null) ;;
        *)       RL_MTIME=$(stat -c %Y "$1" 2>/dev/null || stat -f %m "$1" 2>/dev/null) ;;
      esac
      case "$RL_MTIME" in ''|*[!0-9]*) RL_MTIME="" ;; esac
  }
  # One config dir can hold two logins, slotted like Claude Code's Keychain item: docs/arch/usage-claudecode.md § Nhiều account trên một máy.
  _cc_slot=default
  [ -n "${CLAUDE_CONFIG_DIR:-}" ] && _cc_slot=$(printf '%s' "$CLAUDE_CONFIG_DIR" | { shasum -a 256 2>/dev/null || sha256sum 2>/dev/null; } | cut -c1-8)
  _cc_auth_cache="$_rl_config_dir/auth-cache-${_cc_slot:-default}.json"
  AUTH_TTL_S=15
  _cc_auth=""
  _auth_cache_age=999999
  if [ -f "$_cc_auth_cache" ]; then
      _rl_mtime "$_cc_auth_cache"
      [ -n "$RL_MTIME" ] && _auth_cache_age=$(( _rl_now - RL_MTIME ))
  fi
  if [ "$_auth_cache_age" -lt "$AUTH_TTL_S" ]; then
      _cc_auth=$(cat "$_cc_auth_cache" 2>/dev/null)
  else
      _auth_bound=""
      if command -v timeout >/dev/null 2>&1; then _auth_bound="timeout 3"; elif command -v gtimeout >/dev/null 2>&1; then _auth_bound="gtimeout 3"; fi
      _auth_live=$($_auth_bound claude auth status 2>/dev/null)
      if [ -n "$(printf '%s' "$_auth_live" | jq -r '.email // empty' 2>/dev/null)" ]; then
          _cc_auth="$_auth_live"
          mkdir -p "$_rl_config_dir" 2>/dev/null
          printf '%s' "$_auth_live" > "$_cc_auth_cache" 2>/dev/null
      elif [ -f "$_cc_auth_cache" ]; then
          _cc_auth=$(cat "$_cc_auth_cache" 2>/dev/null)
      fi
  fi
  # Multi-value reads go through eval of @sh output: a heredoc would cost a temp file under /tmp on bash 3.2.
  _rl_acct=""
  _rl_org_id=""
  _rl_acct_file=""
  eval "$(printf '%s' "$_cc_auth" | jq -r --arg unsafe "$ACCOUNT_FILE_UNSAFE" '@sh "_rl_acct=\(.email // "") _rl_org_id=\(.orgId // "") _rl_acct_file=\(.email // "" | gsub($unsafe; "_"))"' 2>/dev/null)"
  RLCACHE=""
  if [ -n "$_rl_acct" ]; then
      _rl_dir="$HOME/.claude/rate-limits"
      mkdir -p "$_rl_dir" 2>/dev/null
      RLCACHE="$_rl_dir/$_rl_acct_file.json"
  fi
  # The plan tier is only in .claude.json, which every account in the dir overwrites: trust it only while it names this account, else keep the tier cached before.
  _rl_tier=""
  if [ -f "$_rl_config_dir/.claude.json" ]; then
      _rl_tier=$(jq -r --arg a "$_rl_acct" 'if .oauthAccount.emailAddress == $a then (.oauthAccount.organizationRateLimitTier // .oauthAccount.userRateLimitTier // "") else "" end' "$_rl_config_dir/.claude.json" 2>/dev/null)
  fi
  # Drops entries past resets_at (0 = unknown, kept) or unseen in a live payload for 6 h (a plan change stopped sending that tier).
  _rl_prune() {
      jq -c -n --argjson rl "$1" --argjson now "$_rl_now" --argjson maxAge 21600 '
        $rl | with_entries(select(
          ((.value | type) != "object")
          or (
            (((.value.resets_at // 0) <= 0) or ((.value.resets_at) > $now))
            and (((.value.seen_at // 0) <= 0) or (($now - .value.seen_at) < $maxAge))
          )
        ))' 2>/dev/null
  }
  _rl_transcript=""
  rl_input=""
  eval "$(printf '%s' "$input" | jq -r '@sh "_rl_transcript=\(.transcript_path // "") rl_input=\(.rate_limits // "" | if . == "" then . else tojson end)"' 2>/dev/null)"
  rl_cached=""
  if [ -n "$RLCACHE" ] && [ -f "$RLCACHE" ]; then
      _rl_cached_org=""
      _rl_cached_tier=""
      _rl_cached_limits=""
      eval "$(jq -r '@sh "_rl_cached_org=\(.org_id // "") _rl_cached_tier=\(.rate_limit_tier // "") _rl_cached_limits=\(.rate_limits // "" | if . == "" then . else tojson end)"' "$RLCACHE" 2>/dev/null)"
      # Account gate: the file is named by email, so only the org id is left to compare - an account deleted and recreated under the same address gets a new org and a new plan.
      if [ -z "$_rl_cached_org" ] || [ -z "$_rl_org_id" ] || [ "$_rl_cached_org" = "$_rl_org_id" ]; then
          rl_cached="$_rl_cached_limits"
          [ -z "$_rl_tier" ] && _rl_tier="$_rl_cached_tier"
      fi
  fi
  rl_merged="$rl_cached"
  if [ -n "$rl_input" ]; then
      # A session writes its transcript on every turn, so the transcript's mtime dates the reading this payload carries: docs/feat/statusline-customizer.md § Quota cache, rule 4.
      _rl_fresh_at=0
      if [ -n "$_rl_transcript" ] && [ -f "$_rl_transcript" ]; then
          _rl_mtime "$_rl_transcript"
          _rl_fresh_at="${RL_MTIME:-0}"
      fi
      # seen_at marks every entry this payload sent, so _rl_prune can tell "still being sent" from "not sent for a while".
      _rl_input_seen=$(jq -c -n --argjson rl "$rl_input" --argjson now "$_rl_now" --argjson fresh "$_rl_fresh_at" '
        $rl | with_entries(if (.value | type) == "object" then (.value.seen_at = $now | if $fresh > 0 then .value.fresh_at = $fresh else . end) else . end)' 2>/dev/null)
      [ -n "$_rl_input_seen" ] && rl_input="$_rl_input_seen"
      rl_merged="$rl_input"
      if [ -n "$rl_cached" ]; then
          # Inside one window (same positive resets_at) the more recently active session's reading wins, so an idle session cannot overwrite a newer one and a mid-window reset can lower the reading; an undated payload keeps the larger reading.
          _merged=$(jq -c -n --argjson old "$rl_cached" --argjson new "$rl_input" '
            $old * $new | with_entries(
              .key as $k
              | if (($old[$k] | type) == "object") and (($new[$k] | type) == "object") and (($new[$k].resets_at // 0) > 0) and ($old[$k].resets_at == $new[$k].resets_at)
                then
                  if ($new[$k].fresh_at // 0) > 0 then
                    (if $new[$k].fresh_at >= ($old[$k].fresh_at // 0) then . else .value = ($old[$k] | .seen_at = $new[$k].seen_at) end)
                  else
                    (([$old[$k].used_percentage, $new[$k].used_percentage] | map(numbers) | max) as $top | if $top == null then . else .value.used_percentage = $top end)
                  end
                else . end)' 2>/dev/null)
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
      _rl_tmp=""
      [ -n "$RLCACHE" ] && _rl_tmp=$(mktemp "$_rl_dir/.rate-limits.XXXXXX" 2>/dev/null)
      if [ -n "$_rl_tmp" ]; then
          if printf '%s' "$_cc_auth" | jq -c --argjson rl "$rl_merged" --arg tier "$_rl_tier" '{account: .email, org_id: (.orgId // ""), org_name: (.orgName // ""), subscription_type: (.subscriptionType // ""), rate_limit_tier: $tier, rate_limits: $rl}' > "$_rl_tmp" 2>/dev/null; then
              mv -f "$_rl_tmp" "$RLCACHE" 2>/dev/null || rm -f "$_rl_tmp"
          else
              rm -f "$_rl_tmp"
          fi
      fi
  elif [ -n "$RLCACHE" ] && [ -f "$RLCACHE" ]; then
      # Nothing survived the account/expiry/staleness gates - do not leave the stale file behind to be re-read (and re-trusted) on the next call.
      rm -f "$RLCACHE" 2>/dev/null
  fi
fi

# aki-agquota v4 - every payload field the render reads, in one jq pass: AGY pool routing, the plan-tier record and the per-account quota frame below; bump on any change to either: docs/feat/statusline-customizer.md § Quota routing (AGY).
if [ -n "$input" ]; then
  # Held in a variable: bash 3.2 brace-expands "{2,}" inside a single-quoted program written straight into "$(...)".
  _payload_fields='
    # A field of the wrong type must blank only itself, never abort the whole program (blank line).
    def num: if type == "number" then . else (tonumber? // 0) end;
    # Resolved once: .model is a plain string on older CLIs, and an unguarded .model.display_name on a string aborts the whole program.
    (if (.model | type) == "object" then (.model.display_name // .model.id // "") else (.model // "") end | tostring) as $model_name
    # Argon models draw from the Gemini pool: docs/ref/fact-antigravity-quota-tiers.md § F-AG3.
    | (if ($model_name | ascii_downcase | contains("gemini")) then "gemini" else "3p" end) as $pool
    | (if (.quota | type) == "object" then .quota else {} end) as $quota
    | (if (.rate_limits | type) == "object" then .rate_limits else {} end) as $rl
    | (.context_window | if type == "object" then . else {} end) as $ctx
    | ($ctx.current_usage | if type == "object" then . else {} end) as $cur
    | (now | floor) as $now
    | ((.account.email // .user.email // .email // "") | tostring) as $email
    | ((.plan_tier // "") | tostring) as $tier
    # A quota entry is either an object or a bare number meaning "fraction remaining"; a bare number carries no reset time.
    | [ $quota | to_entries[]
        | select((.value | type) == "object" or (.value | type) == "number")
        | { key, frac: (if (.value | type) == "object" then .value.remaining_fraction else .value end),
            reset: (if (.value | type) == "object" then (.value.reset_in_seconds // 0 | num) else 0 end) }
        | select(.frac != null) ] as $entries
    | def qfrac($k): ([$entries[] | select(.key == $k) | .frac | num] | first) // -1;
      def qreset($k): ([$entries[] | select(.key == $k) | .reset] | first) // 0;
      # A reading is a remaining fraction (0..1) or a remaining percent (>1); used% is 100 minus it on the same scale.
      def used_pct($rem): (if $rem <= 1 then (1 - $rem) * 100 else 100 - $rem end) | if . < 0 then 0 elif . > 100 then 100 else round end;
      # [used%, seconds to reset] of one window: AGY sends remaining + a countdown per pool, Claude Code used% + an absolute epoch.
      def window($win; $cc):
        ($cc | if type == "object" then . else {} end) as $c
        | [ (qfrac($pool + "-" + $win) as $f
              | if $f != -1 then used_pct($f) elif ($c.used_percentage | type) == "number" then ($c.used_percentage | round) else "" end),
            (qreset($pool + "-" + $win) as $s
              | if $s > 0 then $s elif ($c.resets_at | num) > 0 then (($c.resets_at | num) - $now | floor) else 0 end) ];
    window("5h"; $rl.five_hour) as [$used5, $reset5]
    | window("weekly"; $rl.seven_day) as [$used7, $reset7]
    | ([$cur.input_tokens, $cur.cache_creation_input_tokens, $cur.cache_read_input_tokens] | map(num) | add) as $prompt
    # The context window holds this turn only: the cumulative session totals never shrink after /compact.
    | (($prompt + ($cur.output_tokens | num)) as $window_now
        | if $window_now > 0 then $window_now else ($ctx.total_input_tokens | num) + ($ctx.total_output_tokens | num) end) as $ctx_used
    | ((if (.effort | type) == "object" then .effort.level elif (.effort | type) == "string" then .effort elif (.model | type) == "object" then .model.effort else null end) // "" | tostring) as $effort_field
    | [
      "JSON_NOW=" + ($now | @sh),
      "JSON_CWD=" + ((.cwd // .workspace.current_dir // "") | tostring | @sh),
      # One compact token: drop the trailing "(...)" and the vendor word, then squeeze orphaned separators ("gemini-2.5-flash" must not open with "-").
      "JSON_MODEL=" + ($model_name | sub(" *\\([^)]*\\)$"; "") | gsub("claude|gemini"; ""; "i") | gsub(" "; "") | gsub("[-_./]{2,}"; "-") | sub("^[-_./]+"; "") | sub("[-_./]+$"; "") | @sh),
      "JSON_EFFORT=" + (if $effort_field != "" then $effort_field
          else ([$model_name | capture(".*\\((?<e>[^)]+)\\)") | .e] | first // "" | if ascii_downcase == "thinking" then "" else . end) end
        | ascii_downcase | if . == "medium" then "med" else . end | @sh),
      "JSON_COST_CENTS=" + (.cost.total_cost_usd | num | . * 100 | round | @sh),
      "JSON_DUR=" + ((.cost.total_duration_ms // .duration_ms) | num | floor | @sh),
      "JSON_ADDED=" + ((.cost.total_lines_added // .lines_added) | num | floor | @sh),
      "JSON_REMOVED=" + ((.cost.total_lines_removed // .lines_removed) | num | floor | @sh),
      "JSON_BRANCH=" + ((.workspace.git_branch // .git.branch // "") | tostring | @sh),
      "JSON_CTX_USED=" + ($ctx_used | floor | @sh),
      "JSON_CTX_SIZE=" + ($ctx.context_window_size | num | floor | @sh),
      "JSON_CACHE_READ=" + ($cur.cache_read_input_tokens | num | floor | @sh),
      "JSON_CACHE_TOTAL=" + ($prompt | floor | @sh),
      "JSON_5H_USED=" + ($used5 | @sh),
      "JSON_5H_SEC=" + ($reset5 | @sh),
      "JSON_7D_USED=" + ($used7 | @sh),
      "JSON_7D_SEC=" + ($reset7 | @sh),
      "JSON_ACCOUNT_EMAIL=" + ($email | @sh),
      "JSON_ACCOUNT_FILE=" + ($email | gsub($unsafe; "_") | @sh),
      "JSON_PLAN_TIER=" + ($tier | @sh),
      # STATUS and SUMMARY frame bodies of this account for the usage probe, the SUMMARY in `agy /usage --output-format json` shape (bucket ids are the quota keys): docs/arch/usage-antigravity.md § Per-account quota cache.
      "JSON_AG_STATUS=" + (if $email == "" then "" else ({email: $email} + (if $tier == "" then {} else {userTier: {name: $tier}} end) | tojson) end | @sh),
      "JSON_AG_SUMMARY=" + (
        [ $entries[]
          | .pool = (if (.key | startswith("gemini")) then "Gemini Models"
                     elif (.key | startswith("3p")) then "Claude and GPT models"
                     else (.key | sub("-(5h|weekly)$"; "")) end) ]
        | if length == 0 then "" else
            { command: { name: "usage", data: { groups: (group_by(.pool) | map({
                name: .[0].pool,
                buckets: map({ id: .key, window: (if (.key | test("5h")) then "5h" else "weekly" end), remaining_fraction: .frac }
                  + (if .reset > 0 then { reset_time: (($now + .reset) | floor | todate) } else {} end))
              })) } } } | tojson
          end | @sh),
      # What the frame says, minus its reset times, which shift by a second between turns.
      "JSON_AG_FP=" + ([$email, $tier] + ([$entries[] | "\(.key)=\(.frac)"] | sort) | join("|") | @sh)
    ] | join("\n")
  '
  eval "$(printf '%s' "$input" | jq -r --arg unsafe "$ACCOUNT_FILE_UNSAFE" "$_payload_fields" 2>/dev/null)"
fi

# Headless `agy /usage` carries no plan tier, so each AGY turn's tier is kept per account for the usage probe: docs/arch/usage-antigravity.md § Plan tier.
if [ "$CLI" = "AG" ] && [ -n "$JSON_PLAN_TIER" ] && [ -n "$JSON_ACCOUNT_FILE" ]; then
  _ag_tier_dir="$HOME/.gemini/antigravity-cli/aki-plan-tier"
  _ag_tier_file="$_ag_tier_dir/$JSON_ACCOUNT_FILE"
  _ag_tier_old=""
  [ -f "$_ag_tier_file" ] && IFS= read -r _ag_tier_old < "$_ag_tier_file"
  if [ "$_ag_tier_old" != "$JSON_PLAN_TIER" ]; then
    mkdir -p "$_ag_tier_dir" 2>/dev/null
    _ag_tier_tmp=$(mktemp "$_ag_tier_dir/.tier.XXXXXX" 2>/dev/null) \
      && { { printf '%s\n' "$JSON_PLAN_TIER" > "$_ag_tier_tmp" && mv -f "$_ag_tier_tmp" "$_ag_tier_file"; } || rm -f "$_ag_tier_tmp"; }
  fi
fi

# Headless `agy /usage` sees only the Keychain account, so each AGY turn leaves its own account's quota for the probe: docs/arch/usage-antigravity.md § Per-account quota cache.
# Rewritten only when the quota changed or the last write is AG_FRAME_REFRESH_S old: the probe drops frames older than 6 h, so a live session's frame never ages out.
AG_FRAME_REFRESH_S=300
if [ "$CLI" = "AG" ] && [ -n "$JSON_AG_STATUS" ] && [ -n "$JSON_AG_SUMMARY" ]; then
  _ag_rl_dir="$HOME/.gemini/antigravity-cli/rate-limits"
  _ag_frame="$_ag_rl_dir/$JSON_ACCOUNT_FILE.frame"
  _ag_fp_file="$_ag_rl_dir/.$JSON_ACCOUNT_FILE.fp"
  _ag_fp_at=0
  _ag_fp_old=""
  [ -f "$_ag_frame" ] && [ -f "$_ag_fp_file" ] && IFS=' ' read -r _ag_fp_at _ag_fp_old < "$_ag_fp_file"
  case "$_ag_fp_at" in ''|*[!0-9]*) _ag_fp_at=0 ;; esac
  _ag_fp_age=$(( JSON_NOW - _ag_fp_at ))
  if [ "$_ag_fp_old" != "$JSON_AG_FP" ] || [ "$_ag_fp_age" -ge "$AG_FRAME_REFRESH_S" ] || [ "$_ag_fp_age" -lt 0 ]; then
    mkdir -p "$_ag_rl_dir" 2>/dev/null
    _ag_rl_tmp=$(mktemp "$_ag_rl_dir/.frame.XXXXXX" 2>/dev/null) \
      && { { printf '%s\n%s\n' "$JSON_AG_STATUS" "$JSON_AG_SUMMARY" > "$_ag_rl_tmp" \
        && mv -f "$_ag_rl_tmp" "$_ag_frame" \
        && printf '%s %s\n' "$JSON_NOW" "$JSON_AG_FP" > "$_ag_fp_file"; } || rm -f "$_ag_rl_tmp"; }
  fi
fi

_cwd="${JSON_CWD:-$PWD}"
if [ "$_cwd" = "$HOME" ]; then
  cwd_dir="~"
else
  while [ "$_cwd" != "/" ] && [ "${_cwd%/}" != "$_cwd" ]; do _cwd="${_cwd%/}"; done
  cwd_dir="${_cwd##*/}"
  [ -z "$cwd_dir" ] && cwd_dir="/"
fi

# ANSI colors: a fixed palette, only the generated pickers are user-set. MIRROR of src/utils/statuslineColors.js, which changes first (docs/feat/statusline-customizer.md § The dynamic-color ladder).
RESET_ALL='\033[00m'
RESET_FG='\033[22;39m'  # reset bold+fg, keep bg
BOLD_GREEN='\033[01;32m'
BOLD_YELLOW='\033[01;33m'
BOLD_ORANGE='\033[01;38;5;208m'
BOLD_RED='\033[01;31m'
# The bottom tier, named for its role rather than a hue: it was bold blue (01;34), too dark to read on a dark terminal. xterm 86 aquamarine is brighter and still colder than the green tier above it.
BOLD_CALM='\033[01;38;5;86m'
WHITE='\033[97m'
GREY='\033[90m'
COST_FULL_USD=30
# Readings a busy machine keeps asking for are cached this long, across every session sharing the cache file.
RAM_CACHE_TTL_S=15

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

# Dynamic-color ladder, ascending. Below THRESH_GREEN is the calm aqua tier.
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

# Clamp so a bad generated value degrades; cwd and branch get 15, the rest 12 (docs/feat/statusline-customizer.md § Truncate widths).
for _pair in TRUNC_ACCOUNT:12 TRUNC_USER:12 TRUNC_HOST:12 TRUNC_CWD:15 TRUNC_BRANCH:15; do
  _t="${_pair%%:*}"; _max="${_pair##*:}"
  eval "_v=\$$_t"
  [ "$_v" -lt 3 ] 2>/dev/null && eval "$_t=3"
  [ "$_v" -gt "$_max" ] 2>/dev/null && eval "$_t=$_max"
done

# Formatters set a variable instead of printing, so no call costs a subshell.
set_pct_color() {
  if [ "$1" -ge "$THRESH_RED" ]; then PCT_COLOR="$BOLD_RED"
  elif [ "$1" -ge "$THRESH_ORANGE" ]; then PCT_COLOR="$BOLD_ORANGE"
  elif [ "$1" -ge "$THRESH_YELLOW" ]; then PCT_COLOR="$BOLD_YELLOW"
  elif [ "$1" -ge "$THRESH_GREEN" ]; then PCT_COLOR="$BOLD_GREEN"
  else PCT_COLOR="$BOLD_CALM"; fi
}

fmt_k() {
  if [ "$1" -ge 1000000 ]; then FMT_OUT="$(( ($1 + 500000) / 1000000 ))M"
  elif [ "$1" -ge 1000 ]; then FMT_OUT="$(( ($1 + 500) / 1000 ))k"
  else FMT_OUT="$1"; fi
}

fmt_dur() {
  FMT_OUT="$(( $1 / 3600000 ))h$(( ($1 % 3600000) / 60000 ))m"
  [ "$1" -lt 3600000 ] && FMT_OUT="${FMT_OUT#*h}"
}

fmt_eta() {
  if [ "$1" -ge 86400 ]; then FMT_OUT="$(( $1 / 86400 ))d$(( ($1 % 86400) / 3600 ))h"
  else FMT_OUT="$(( $1 / 3600 ))h$(( ($1 % 3600) / 60 ))m"; fi
}

# `ps -o etime` reads [[dd-]hh:]mm:ss; 10# keeps 08 and 09 from parsing as octal. Sets ETIME_S, empty when unreadable.
etime_seconds() {
  local t="${1// /}" d=0 h=0 m=0 s
  ETIME_S=""
  case "$t" in *-*) d="${t%%-*}"; t="${t#*-}" ;; esac
  s="${t##*:}"
  case "$t" in
    *:*) t="${t%:*}"; m="${t##*:}"
         case "$t" in *:*) h="${t%:*}" ;; esac ;;
  esac
  case "$d:$h:$m:$s" in *::*|:*|*:|*[!0-9:]*) return ;; esac
  ETIME_S=$(( 10#$d * 86400 + 10#$h * 3600 + 10#$m * 60 + 10#$s ))
}

# One quota window: label, used%, seconds to reset, reset flag. Sets QUOTA_OUT, empty when there is no reading.
quota_block() {
  QUOTA_OUT=""
  [ -n "$2" ] || return
  set_pct_color "$2"
  QUOTA_OUT="${WHITE}$1:${RESET_FG}${PCT_COLOR}$2%${RESET_FG}"
  if [ "$4" = 1 ] && [ "$3" -gt 0 ] 2>/dev/null; then
    fmt_eta "$3"
    QUOTA_OUT="${QUOTA_OUT}${GREY}${FMT_OUT}${RESET_FG}"
  fi
}

# Identity
g_identity=""
if [ "$EN_identity_user" = 1 ] || [ "$EN_identity_host" = 1 ]; then
  u_part=""
  h_part=""
  if [ "$EN_identity_user" = 1 ]; then
    user="${USER:-$(id -un 2>/dev/null)}"
    u_part="${COLOR_identity_user}${user:0:$TRUNC_USER}${RESET_FG}"
  fi
  if [ "$EN_identity_host" = 1 ]; then
    host="${HOSTNAME%%.*}"
    [ -z "$host" ] && host=$(hostname -s 2>/dev/null)
    h_part="${COLOR_identity_host}${host:0:$TRUNC_HOST}${RESET_FG}"
  fi
  if [ -n "$u_part" ] && [ -n "$h_part" ]; then
    g_identity="${u_part}${GREY}@${RESET_FG}${h_part}"
  else
    g_identity="${u_part}${h_part}"
  fi
fi

# CWD
g_cwd=""
[ "$EN_cwd" = 1 ] && g_cwd="${COLOR_cwd}${cwd_dir:0:$TRUNC_CWD}${RESET_FG}"

# Model & Effort, glued with no separator: model and effort read as one token ("Opus4.8med").
g_model=""
[ "$EN_model" = 1 ] && [ -n "$JSON_MODEL" ] && g_model="${COLOR_model}${JSON_MODEL}${RESET_FG}"
[ "$EN_effort" = 1 ] && [ -n "$JSON_EFFORT" ] && g_model="${g_model}${GREY}${JSON_EFFORT}${RESET_FG}"

# Context Window, colored on a fixed 200k scale with no % printed (docs/feat/statusline-customizer.md § The dynamic-color ladder).
g_context=""
if [ "$EN_context" = 1 ] && [ "${JSON_CTX_SIZE:-0}" -gt 0 ] 2>/dev/null; then
  ctx_pct=$(( (JSON_CTX_USED + 1000) / 2000 ))
  [ "$ctx_pct" -gt 100 ] && ctx_pct=100
  set_pct_color "$ctx_pct"
  fmt_k "$JSON_CTX_USED"; ctx_fmt="$FMT_OUT"
  fmt_k "$JSON_CTX_SIZE"
  g_context="${WHITE}ctx${RESET_FG}${PCT_COLOR}${ctx_fmt}${RESET_FG}/${GREY}${FMT_OUT}${RESET_FG}"
fi

# Quota / Rate Limits
g_quota=""
if [ "$EN_rate_limits_5h" = 1 ]; then
  quota_block 5h "$JSON_5H_USED" "$JSON_5H_SEC" "$EN_rate_reset_5h"
  g_quota="$QUOTA_OUT"
fi
if [ "$EN_rate_limits_7d" = 1 ]; then
  quota_block 7d "$JSON_7D_USED" "$JSON_7D_SEC" "$EN_rate_reset_7d"
  [ -n "$QUOTA_OUT" ] && g_quota="${g_quota:+$g_quota }$QUOTA_OUT"
fi

# Session Stats
g_session=""
if [ "$EN_session" = 1 ] && [ -n "$JSON_DUR" ]; then
  # AGY sends no duration, so the age of the terminal process ($PPID) stands in.
  if [ "$JSON_DUR" -le 0 ]; then
    etime_seconds "$(ps -o etime= -p "$PPID" 2>/dev/null)"
    [ -n "$ETIME_S" ] && JSON_DUR=$(( ETIME_S * 1000 ))
  fi
  fmt_dur "$JSON_DUR"
  g_session="${WHITE}ss${RESET_FG}${GREY}${FMT_OUT}${RESET_FG}"
  if [ "$JSON_ADDED" != "0" ] || [ "$JSON_REMOVED" != "0" ]; then
    g_session="$g_session ${BOLD_GREEN}+${JSON_ADDED}${RESET_FG}${GREY}/${RESET_FG}${BOLD_RED}-${JSON_REMOVED}${RESET_FG}"
  fi
  if [ "${JSON_COST_CENTS:-0}" -gt 0 ]; then
    cost_pct=$(( (JSON_COST_CENTS + COST_FULL_USD / 2) / COST_FULL_USD ))
    [ "$cost_pct" -gt 100 ] && cost_pct=100
    set_pct_color "$cost_pct"
    g_session="$g_session ${PCT_COLOR}\$$(( JSON_COST_CENTS / 100 )).$(( JSON_COST_CENTS % 100 / 10 ))$(( JSON_COST_CENTS % 10 ))${RESET_FG}"
  fi
fi

# Cache Stats
g_cache=""
if [ "$EN_cache" = 1 ] && [ "${JSON_CACHE_TOTAL:-0}" -gt 0 ]; then
  if [ "$EN_cache_pct" = 1 ]; then
    # Static grey, deliberately off the ladder: a high cache hit rate is good news and must not shout in red the way a high quota reading should.
    g_cache="${WHITE}↬${RESET_FG}${GREY}$(( (JSON_CACHE_READ * 100 + JSON_CACHE_TOTAL / 2) / JSON_CACHE_TOTAL ))%${RESET_FG}"
  fi
  if [ "$EN_cache_tokens" = 1 ]; then
    fmt_k "$JSON_CACHE_READ"
    g_cache="${g_cache:+$g_cache }${GREY}${FMT_OUT}${RESET_FG}"
  fi
fi

# Git Branch: read from .git/HEAD when the CLI runs at a repository root, git itself otherwise (subfolder, worktree, detached HEAD).
g_git_branch=""
if [ "$EN_git_branch" = 1 ]; then
  git_branch="$JSON_BRANCH"
  if [ -z "$git_branch" ]; then
    _head=""
    [ -f "$PWD/.git/HEAD" ] && IFS= read -r _head < "$PWD/.git/HEAD"
    case "$_head" in
      "ref: refs/heads/"*) git_branch="${_head#ref: refs/heads/}" ;;
      *) git_branch=$(git branch --show-current 2>/dev/null) ;;
    esac
  fi
  [ -n "$git_branch" ] && g_git_branch="${COLOR_git_branch}${git_branch:0:$TRUNC_BRANCH}${RESET_FG}"
fi

# Account Email & Unified Tag
g_cli_tag=""
if [ "$EN_cli_tag" = 1 ]; then
  acc_name=""
  if [ "$EN_account" = 1 ]; then
    # Each CLI reads its OWN account source: tried in sequence, a machine with both would show the other CLI's account. CC's is this session's slot, resolved in the aki-rlcache block.
    if [ "$CLI" = "CC" ]; then
      [ -n "$_rl_acct" ] && JSON_ACCOUNT_EMAIL="$_rl_acct"
    elif [ -z "$JSON_ACCOUNT_EMAIL" ]; then
      [ -f "$HOME/.gemini/google_accounts.json" ] && \
        JSON_ACCOUNT_EMAIL=$(jq -r '.active // ""' "$HOME/.gemini/google_accounts.json" 2>/dev/null)
    fi
    # The domain is dropped FIRST - AGY puts a full address in the payload, so a raw 4-char cut of
    # "lva@akitao.com" would print the useless "lva@". Cut the local part, THEN truncate.
    acc_name="${JSON_ACCOUNT_EMAIL%%@*}"
    acc_name="${acc_name:0:$TRUNC_ACCOUNT}"
  fi

  if [ "$CLI" = "CC" ]; then
    cli_label="CC"
    cli_col="\033[1;38;5;208m"  # Claude Code orange
  else
    cli_label="AG"
    cli_col="\033[1;38;5;33m"   # AGY royal blue
  fi

  g_cli_tag="\033[48;5;255m${cli_col}${cli_label}"
  # \033[22m before the account color: the CLI label above turned bold on, and the account name is a secondary reading on a light plate - left bold it washes out whatever color is chosen.
  [ -n "$acc_name" ] && g_cli_tag="${g_cli_tag}\033[48;5;252m\033[22m${COLOR_account} ${acc_name}"
  g_cli_tag="${g_cli_tag}${RESET_ALL}"
fi

# System RAM, shared through a short-lived cache file: docs/feat/statusline-customizer.md § Process budget.
g_ram=""
if [ "$EN_ram" = 1 ]; then
  if [ "$CLI" = "AG" ]; then _ram_cache="$HOME/.gemini/antigravity-cli/.aki-ram-cache"; else _ram_cache="$HOME/.claude/.aki-ram-cache"; fi
  _now="${JSON_NOW:-$(date +%s)}"
  _ram_at=0
  _ram_old=""
  [ -f "$_ram_cache" ] && IFS=' ' read -r _ram_at _ram_old < "$_ram_cache"
  case "$_ram_at$_ram_old" in ''|*[!0-9]*) _ram_at=0; _ram_old="" ;; esac
  mem_pct="$_ram_old"
  _ram_age=$(( _now - _ram_at ))
  if [ -z "$_ram_old" ] || [ "$_ram_age" -ge "$RAM_CACHE_TTL_S" ] || [ "$_ram_age" -lt 0 ]; then
    [ -n "$_ram_old" ] && printf '%s %s\n' "$_now" "$_ram_old" 2>/dev/null > "$_ram_cache"
    _ram_new=""
    if [[ "$OSTYPE" == "darwin"* ]]; then
      _mp=$(memory_pressure 2>/dev/null)
      case "$_mp" in
        *"System-wide memory free percentage: "*)
          _free="${_mp##*System-wide memory free percentage: }"
          _free="${_free%%%*}"
          case "$_free" in ''|*[!0-9]*) ;; *) _ram_new=$(( 100 - 10#$_free )) ;; esac
          ;;
      esac
    elif [[ "$OSTYPE" == linux* ]] && [ -r /proc/meminfo ]; then
      _mt=0; _ma=0
      while IFS=': ' read -r _k _v _; do
        case "$_k" in MemTotal) _mt="$_v" ;; MemAvailable) _ma="$_v" ;; esac
      done < /proc/meminfo
      [ "$_mt" -gt 0 ] && [ "$_ma" -gt 0 ] && _ram_new=$(( ((_mt - _ma) * 100 + _mt / 2) / _mt ))
    fi
    if [ -n "$_ram_new" ]; then
      mem_pct="$_ram_new"
      mkdir -p "${_ram_cache%/*}" 2>/dev/null
      printf '%s %s\n' "$_now" "$mem_pct" 2>/dev/null > "$_ram_cache"
    fi
  fi
  # Static grey like the cache reading: whole-machine RAM is context, not something the ladder should escalate about mid-session.
  [ -n "$mem_pct" ] && g_ram="${WHITE}⚅${RESET_FG}${GREY}${mem_pct}%${RESET_FG}"
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
