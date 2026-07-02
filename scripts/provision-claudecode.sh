#!/usr/bin/env sh
# @docs docs/arch/usage-claudecode.md
# Patches ~/.claude/statusline-command.sh on the remote to cache rate-limit data.
# Idempotent: checks for marker before patching.
FILE="$HOME/.claude/statusline-command.sh"
if [ ! -f "$FILE" ]; then exit 0; fi
# Version-aware patching. v2 preserves the previous rate_limits verbatim when a turn omits
# rate_limits (v1 wrongly forced used_percentage=100, producing transient false "100% red"
# because ordinary turns — not just 429 exhaustion — legitimately lack the key).
if grep -q "aki-rlcache v2" "$FILE"; then
    :  # already up to date — nothing to do
else
    if grep -q "rate-limits-cache" "$FILE"; then
        # Old (v1, unmarked) block present → delete it before injecting v2.
        # v1 spans from the `rl_input=` line through the trailing `printf ... rate-limits-cache.json`
        # line. The `if [ -z ... ]` line also contains "rate-limits-cache.json", so the range END
        # pattern matches the printf line specifically, not any occurrence.
        sed -i.bak '/^rl_input=/,/printf .*rate-limits-cache\.json/d' "$FILE"
        rm -f "${FILE}.bak"
    fi
    trap 'rm -f /tmp/patch.sh' EXIT
    cat << 'EOF' > /tmp/patch.sh
# aki-rlcache v2
rl_input=$(echo "$input" | jq -c '.rate_limits // empty')
if [ -z "$rl_input" ] && [ -f "$HOME/.claude/rate-limits-cache.json" ]; then
    input=$(echo "$input" | jq --argjson old "$(cat "$HOME/.claude/rate-limits-cache.json")" '
        if ($old.rate_limits != null) then
            .rate_limits = $old.rate_limits
        else . end
    ')
fi
printf '%s' "$input" > "$HOME/.claude/rate-limits-cache.json"
EOF
    sed -i.bak -e '/input=$(cat)/r /tmp/patch.sh' "$FILE"
    rm -f "${FILE}.bak"
fi
# Cache auth info (email, orgName) for UI — runs once per host session
AUTH_CACHE="$HOME/.claude/auth-cache.json"
AUTH_JSON=$(bash -lc 'claude auth status 2>/dev/null' 2>/dev/null || echo '{}')
[ "$AUTH_JSON" != '{}' ] && printf '%s' "$AUTH_JSON" > "$AUTH_CACHE"
