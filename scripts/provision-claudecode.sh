#!/usr/bin/env sh
# @docs docs/arch/usage-claudecode.md
# Patches ~/.claude/statusline-command.sh on the remote to cache rate-limit data.
# Idempotent: checks for marker before patching.
FILE="$HOME/.claude/statusline-command.sh"
if [ ! -f "$FILE" ]; then exit 0; fi
# Version-aware patching.
# v2 preserved the previous rate_limits verbatim, but ONLY when a turn omitted rate_limits
# entirely - a turn that HAS rate_limits (the normal case on current CC: just `five_hour`, no
# `seven_day`) still overwrote the whole cache file, silently clobbering a `seven_day` that the
# OAuth poll (Rust side, separate recovery layer) had written moments earlier. v3 fixes this by
# deep-merging .rate_limits (jq's `*`, right side wins per-key, recursive) instead of an
# all-or-nothing swap: a turn with only five_hour now updates five_hour and leaves a
# previously-cached seven_day untouched. Also switches the write to atomic (temp file + mv)  - 
# the OAuth poll's os.replace() comment already assumed this hook wrote non-atomically; matching
# that closes the last read-half-written-file race between the two writers.
# See docs/arch/usage-claudecode.md §3 (provision).
# The guard used to be `grep -q "aki-rlcache v3"` - it knew only its OWN version, so a file already
# carrying a NEWER writer (v4/v5, installed by a Statusline Apply) did not match and control fell
# through to the removal below. That removal is anchored on `^rl_input=`, and the unified template
# indents that line, so the delete was a no-op and v3 got prepended on top of the newer block:
# provisioning silently DOWNGRADED a good install and left two cache writers stacked, v3 running
# first and writing a payload with no `account_uuid` and no `seen_at` - which is exactly the field
# the account-integrity gate in get-claudecode-usage.sh is built on. Read the installed version
# number instead of testing for one literal, and never write over something newer.
INSTALLED_RLCACHE_V=$(grep -o '# aki-rlcache v[0-9][0-9]*' "$FILE" 2>/dev/null | sed 's/.*v//' | sort -n | tail -n 1)
if [ -n "$INSTALLED_RLCACHE_V" ] && [ "$INSTALLED_RLCACHE_V" -ge 3 ] 2>/dev/null; then
    # Already at or newer than the block this script injects - nothing to do.
    # Repair the one state that provisioning itself used to create: a v3 block stacked in front of
    # a newer one. Deleting the v3 block is safe only when BOTH of its own anchors are present, and
    # the awk below refuses to write anything unless it actually found the closing anchor - a sed
    # range that misses its end pattern deletes to EOF, which is how a statusline gets destroyed.
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
        # Old (v1 unmarked, or v2 marked) block present → delete it before injecting v3.
        # The functional block always spans from the `rl_input=` line through the trailing
        # `printf ... rate-limits-cache.json` line regardless of version (same shape) - the
        # `if [ -z ... ]` line also contains "rate-limits-cache.json", so the range END pattern
        # matches the printf line specifically, not any occurrence. A leading `# aki-rlcache vN`
        # marker comment (v2+) sits one line above that range and is stripped separately so no
        # orphan comment survives the upgrade.
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
# Cache auth info (email, orgName) for UI - runs once per host session.
# BEST-EFFORT: this must NOT decide the script's exit code. The provisioning contract is the
# statusline patch above (already done by here); auth caching is a side task. Previously the final
# `[ "$AUTH_JSON" != '{}' ] && printf ...` line made the script's exit status hostage to the test,
# so an empty auth ('{}') returned exit 1 → the JS caller flipped `provisioned=false` and retried
# every 30s forever (retry storm). We now always `exit 0` and surface an empty-auth as a diagnostic.
AUTH_CACHE="$HOME/.claude/auth-cache.json"
AUTH_JSON=$(bash -lc "$AKI_CLAUDE_TMO'$CLAUDE_BIN' auth status 2>/dev/null" 2>/dev/null || echo '{}')
if [ "$AUTH_JSON" != '{}' ]; then
    printf '%s' "$AUTH_JSON" > "$AUTH_CACHE"
else
    # Empty auth is a REAL signal, not noise: it correlates with `/usage` returning empty (Bug B).
    # Surface it to stderr (the Rust side logs non-empty provision stderr at ERROR) but do not fail.
    printf '[SHELL:provision] claude auth status returned empty ({}) - CLI may be unable to authenticate\n' >&2
fi
exit 0
