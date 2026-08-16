#!/usr/bin/env sh
# @docs docs/arch/usage-claudecode.md
# Patches ~/.claude/statusline-command.sh on the remote to cache rate-limit data.
# Idempotent: checks for marker before patching.
FILE="$HOME/.claude/statusline-command.sh"
if [ ! -f "$FILE" ]; then exit 0; fi
# Version-aware patch: deep-merges rate limits and avoids clobbering newer writers (v4+). See docs/arch/usage-claudecode.md §3 (provision).
INSTALLED_RLCACHE_V=$(grep -o '# aki-rlcache v[0-9][0-9]*' "$FILE" 2>/dev/null | sed 's/.*v//' | sort -n | tail -n 1)
if [ -n "$INSTALLED_RLCACHE_V" ] && [ "$INSTALLED_RLCACHE_V" -ge 3 ] 2>/dev/null; then
    # Safe repair: remove stacked v3 block only when both anchors exist to avoid EOF truncation.
    if grep -q '^# aki-rlcache v3$' "$FILE" && grep -q '^printf .*\$RL_TMP' "$FILE" && [ "$INSTALLED_RLCACHE_V" -gt 3 ]; then
        RL_FIX_TMP="${FILE}.aki-rlfix.$$"
        if awk '
            /^# aki-rlcache v3$/ && !closed && !skip { skip = 1; next }
            skip && /^printf .*\$RL_TMP/ { skip = 0; closed = 1; next }
            !skip { print }
            END { exit (closed ? 0 : 1) }
        ' "$FILE" > "$RL_FIX_TMP" 2>/dev/null; then
            mv -f "$RL_FIX_TMP" "$FILE"
            printf '[SHELL:provision] removed a stacked aki-rlcache v3 block; the newer statusline writer (v%s) now owns the cache\n' "$INSTALLED_RLCACHE_V" >&2
        else
            rm -f "$RL_FIX_TMP"
            printf '[SHELL:provision] a v3 block is stacked in front of v%s but its end anchor was not found - left untouched, re-Apply the statusline\n' "$INSTALLED_RLCACHE_V" >&2
        fi
    fi
else
    if grep -q "rate-limits-cache" "$FILE"; then
        # Strip older v1/v2 block (from rl_input to printf) and remove marker comment before injecting v3.
        sed -i.bak '/^rl_input=/,/printf .*rate-limits-cache\.json/d' "$FILE"
        sed -i.bak2 '/^# aki-rlcache v[0-9]*$/d' "$FILE"
        rm -f "${FILE}.bak" "${FILE}.bak2"
    fi
    trap 'rm -f /tmp/patch.sh' EXIT
    cat << 'EOF' > /tmp/patch.sh
# aki-rlcache v3
OLD_RL_CACHE='{}'
if [ -f "$HOME/.claude/rate-limits-cache.json" ]; then
    OLD_RL_CACHE=$(cat "$HOME/.claude/rate-limits-cache.json")
    echo "$OLD_RL_CACHE" | jq -e . >/dev/null 2>&1 || OLD_RL_CACHE='{}'
fi
input=$(echo "$input" | jq -c --argjson old "$OLD_RL_CACHE" '
    (($old.rate_limits // {}) * (.rate_limits // {})) as $merged
    | if ($merged | length) > 0 then .rate_limits = $merged else . end
')
RL_TMP="$HOME/.claude/rate-limits-cache.json.tmp.$$"
printf '%s' "$input" > "$RL_TMP" && mv "$RL_TMP" "$HOME/.claude/rate-limits-cache.json"
EOF
    sed -i.bak -e '/input=$(cat)/r /tmp/patch.sh' "$FILE"
    rm -f "${FILE}.bak"
fi
# Best-effort auth caching: must always exit 0 to prevent caller retry storm on empty auth.
AUTH_CACHE="$HOME/.claude/auth-cache.json"
AUTH_JSON=$(bash -lc "$AKI_CLAUDE_TMO'$CLAUDE_BIN' auth status 2>/dev/null" 2>/dev/null || echo '{}')
if [ "$AUTH_JSON" != '{}' ]; then
    printf '%s' "$AUTH_JSON" > "$AUTH_CACHE"
else
    # Log empty auth to stderr as diagnostic for usage triage without failing exit code.
    printf '[SHELL:provision] claude auth status returned empty ({}) - CLI may be unable to authenticate\n' >&2
fi
exit 0
