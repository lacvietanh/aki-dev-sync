FILE="$HOME/.claude/rate-limits-cache.json"
CREDS="$HOME/.claude/.credentials.json"
if [ -f "$FILE" ]; then
    MTIME=$(stat -c %Y "$FILE" 2>/dev/null || stat -f %m "$FILE" 2>/dev/null)
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
