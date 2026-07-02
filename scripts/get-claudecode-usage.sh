# @docs docs/arch/usage-claudecode.md
set -e
FILE="$HOME/.claude/rate-limits-cache.json"
CREDS="$HOME/.claude/.credentials.json"
NOW=$(date +%s)

_log() {
    printf '[%s][SHELL:get-usage] %s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$1" >&2
}

# ── 1. File existence ─────────────────────────────────────────────────────
FILE_EXISTS=$([ -f "$FILE" ] && echo yes || echo no)
CREDS_EXISTS=$([ -f "$CREDS" ] && echo yes || echo no)
_log "start: cache_file=$FILE exists=$FILE_EXISTS creds_exists=$CREDS_EXISTS now=$NOW"

if [ -f "$FILE" ]; then
    # ── 2. File age ───────────────────────────────────────────────────────
    MTIME=$(stat -c %Y "$FILE" 2>/dev/null || stat -f %m "$FILE" 2>/dev/null)
    FILE_AGE=$((NOW - MTIME))
    FILE_SIZE=$(wc -c < "$FILE" 2>/dev/null | tr -d ' ')
    _log "cache: mtime=$MTIME age_s=$FILE_AGE size_bytes=$FILE_SIZE"

    # ── 3. Read resets_at from cache ──────────────────────────────────────
    RESETS_AT=$(python3 -c "
import json, sys
try:
    with open('$FILE') as f:
        d = json.load(f)
    rl = d.get('rate_limits', {})
    fh = rl.get('five_hour', {})
    ra = fh.get('resets_at', 0)
    pct = fh.get('used_percentage', -1)
    print('{} pct={}'.format(int(ra), pct))
except Exception as e:
    print('0 pct=-1 err={}'.format(e))
" 2>/dev/null)

    # RESETS_AT now has format "TIMESTAMP pct=N" — split it
    RESETS_AT_VAL=$(printf '%s' "$RESETS_AT" | awk '{print $1}')
    RESETS_AT_PCT=$(printf '%s' "$RESETS_AT" | awk '{print $2}')
    _log "cache: five_hour.resets_at=$RESETS_AT_VAL $RESETS_AT_PCT now=$NOW"

    # ── 4. Stale-reset decision ───────────────────────────────────────────
    if [ -n "$RESETS_AT_VAL" ] && [ "$RESETS_AT_VAL" -gt 0 ] 2>/dev/null; then
        if [ "$NOW" -gt "$RESETS_AT_VAL" ]; then
            OVERDUE=$((NOW - RESETS_AT_VAL))
            _log "stale_check: NOW($NOW) > resets_at($RESETS_AT_VAL) → STALE overdue_s=$OVERDUE"
            _log "STALE_RESET: signaling → Rust will return null, JS will trigger forceSync"
            echo "|||STALE_RESET|||"
            exit 0
        else
            UNTIL_RESET=$((RESETS_AT_VAL - NOW))
            _log "stale_check: resets_at=$RESETS_AT_VAL still in future by ${UNTIL_RESET}s → cache valid"
        fi
    else
        _log "stale_check: resets_at=0 or empty → no stale check, treating as valid"
    fi

    # ── 5. Auth info ──────────────────────────────────────────────────────
    # Fetched before subscription-metadata below because newer Claude Code versions no
    # longer keep `.credentials.json` on disk (moved to OS keychain) — `claude auth status`
    # is the one source that still works either way, and it also carries subscriptionType.
    AUTH_CACHE="$HOME/.claude/auth-cache.json"
    AUTH_CACHE_EXISTS=$([ -f "$AUTH_CACHE" ] && echo yes || echo no)
    _log "auth: cache_exists=$AUTH_CACHE_EXISTS"

    if [ -f "$AUTH_CACHE" ]; then
        AUTH_INFO=$(python3 -c "import json,sys; d=json.load(open('$AUTH_CACHE')); print(json.dumps(d))" 2>/dev/null || echo '{}')
        AUTH_EMAIL=$(python3 -c "import json,sys; d=json.loads(sys.argv[1]); print(d.get('email','none'))" "$AUTH_INFO" 2>/dev/null || echo unknown)
        AUTH_ORG=$(python3 -c "import json,sys; d=json.loads(sys.argv[1]); print(d.get('orgName','none'))" "$AUTH_INFO" 2>/dev/null || echo unknown)
        _log "auth: source=cache email=$AUTH_EMAIL org=$AUTH_ORG"
    else
        AUTH_INFO=$(bash -lc 'claude auth status 2>/dev/null' 2>/dev/null || echo '{}')
        AUTH_LEN=$(printf '%s' "$AUTH_INFO" | wc -c | tr -d ' ')
        _log "auth: source=claude_auth_status output_len=$AUTH_LEN"
        if [ "$AUTH_INFO" != '{}' ] && [ "$AUTH_LEN" -gt 2 ]; then
            printf '%s' "$AUTH_INFO" > "$AUTH_CACHE"
            _log "auth: cached to $AUTH_CACHE"
        else
            _log "auth: WARNING output was empty or {} — not caching"
        fi
    fi

    # ── 6. Read subscription metadata ─────────────────────────────────────
    SUB_TYPE="Unknown"
    TIER="Unknown"
    if [ -f "$CREDS" ]; then
        FOUND=$(grep -o '"subscriptionType"\s*:\s*"[^"]*"' "$CREDS" | head -n 1 | awk -F'"' '{print $4}')
        [ -n "$FOUND" ] && SUB_TYPE="$FOUND"
        FOUND_TIER=$(grep -o '"rateLimitTier"\s*:\s*"[^"]*"' "$CREDS" | head -n 1 | awk -F'"' '{print $4}')
        [ -n "$FOUND_TIER" ] && TIER="$FOUND_TIER"
        _log "meta: creds_found=yes subtype=$SUB_TYPE tier=$TIER"
    else
        _log "meta: creds_found=no — falling back to auth status"
    fi
    if [ "$SUB_TYPE" = "Unknown" ]; then
        FOUND=$(printf '%s' "$AUTH_INFO" | grep -o '"subscriptionType"\s*:\s*"[^"]*"' | head -n 1 | awk -F'"' '{print $4}')
        [ -n "$FOUND" ] && SUB_TYPE="$FOUND"
        _log "meta: auth_status subtype=$SUB_TYPE"
    fi

    # ── 7. Write stdout payload ───────────────────────────────────────────
    _log "stdout_write: emitting cache_json + MTIME=$MTIME SUBTYPE=$SUB_TYPE TIER=$TIER"
    cat "$FILE"
    echo "|||MTIME|||$MTIME"
    echo "|||SUBTYPE|||$SUB_TYPE"
    echo "|||TIER|||$TIER"
    echo "|||AUTHINFO|||$AUTH_INFO"
    _log "stdout_write: done — all delimiters emitted"
else
    _log "cache file missing: $FILE → no stdout output → Rust returns null → JS triggers forceSync"
fi
