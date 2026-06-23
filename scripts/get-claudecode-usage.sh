# @docs docs/arch/usage-claudecode.md
FILE="$HOME/.claude/rate-limits-cache.json"
CREDS="$HOME/.claude/.credentials.json"
if [ -f "$FILE" ]; then
    MTIME=$(stat -c %Y "$FILE" 2>/dev/null || stat -f %m "$FILE" 2>/dev/null)

    # If five_hour resets_at has passed, signal stale reset — Rust returns Ok(None)
    RESETS_AT=$(python3 -c "
import json, sys
try:
    with open('$FILE') as f:
        d = json.load(f)
    print(int(d.get('rate_limits', {}).get('five_hour', {}).get('resets_at', 0)))
except:
    print(0)
" 2>/dev/null)
    NOW=$(date +%s)
    if [ -n "$RESETS_AT" ] && [ "$RESETS_AT" -gt 0 ] 2>/dev/null && [ "$NOW" -gt "$RESETS_AT" ] 2>/dev/null; then
        echo "|||STALE_RESET|||"
        exit 0
    fi

    SUB_TYPE="Unknown"
    TIER="Unknown"
    if [ -f "$CREDS" ]; then
        FOUND=$(grep -o '"subscriptionType"\s*:\s*"[^"]*"' "$CREDS" | head -n 1 | awk -F'"' '{print $4}')
        [ -n "$FOUND" ] && SUB_TYPE="$FOUND"
        FOUND_TIER=$(grep -o '"rateLimitTier"\s*:\s*"[^"]*"' "$CREDS" | head -n 1 | awk -F'"' '{print $4}')
        [ -n "$FOUND_TIER" ] && TIER="$FOUND_TIER"
    fi
    cat "$FILE"
    echo "|||MTIME|||$MTIME"
    echo "|||SUBTYPE|||$SUB_TYPE"
    echo "|||TIER|||$TIER"
fi
