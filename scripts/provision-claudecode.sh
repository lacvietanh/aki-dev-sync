#!/usr/bin/env sh
# @docs docs/arch/usage-claudecode.md
# Patches ~/.claude/statusline-command.sh on the remote to cache rate-limit data.
# Idempotent: checks for marker before patching.
FILE="$HOME/.claude/statusline-command.sh"
if [ ! -f "$FILE" ]; then exit 0; fi
if ! grep -q "rate-limits-cache" "$FILE"; then
    cat << 'EOF' > /tmp/patch.sh
rl_input=$(echo "$input" | jq -c '.rate_limits // empty')
if [ -z "$rl_input" ] && [ -f "$HOME/.claude/rate-limits-cache.json" ]; then
    input=$(echo "$input" | jq --argjson old "$(cat "$HOME/.claude/rate-limits-cache.json")" '
        if ($old.rate_limits != null) then
            .rate_limits = ($old.rate_limits | map_values(.used_percentage = 100))
        else . end
    ')
fi
printf '%s' "$input" > "$HOME/.claude/rate-limits-cache.json"
EOF
    sed -i.bak -e '/input=$(cat)/r /tmp/patch.sh' "$FILE"
    rm -f /tmp/patch.sh
fi
