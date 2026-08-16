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

# ── 5. Auth info (docs/ref/multiple-account-config-dir.md) ─────────────
# Run `claude auth status` live every poll without TTL to prevent multi-process CLAUDE_CONFIG_DIR clobber races.
AUTH_CACHE="$HOME/.claude/auth-cache.json"
AUTH_CACHE_EXISTS=$([ -f "$AUTH_CACHE" ] && echo yes || echo no)
_log "auth: running live claude auth status (cache_exists=$AUTH_CACHE_EXISTS, used only as fallback if this call fails)"
AUTH_INFO=$(bash -lc "$AKI_CLAUDE_TMO'$CLAUDE_BIN' auth status 2>/dev/null" 2>/dev/null || echo '{}')
AUTH_LEN=$(printf '%s' "$AUTH_INFO" | wc -c | tr -d ' ')
_log "auth: source=claude_auth_status output_len=$AUTH_LEN"
if [ "$AUTH_INFO" != '{}' ] && [ "$AUTH_LEN" -gt 2 ]; then
    printf '%s' "$AUTH_INFO" > "$AUTH_CACHE"
    _log "auth: cached to $AUTH_CACHE (fallback only, not read unless the live call fails)"
elif [ "$AUTH_CACHE_EXISTS" = "yes" ]; then
    # Fallback to stale cache if live auth status failed to prevent blanking email; next poll retries live.
    _log "auth: WARNING claude_auth_status empty this cycle - falling back to stale cache"
    AUTH_INFO=$(python3 -c "import json,sys; d=json.load(open('$AUTH_CACHE')); print(json.dumps(d))" 2>/dev/null || echo '{}')
else
    _log "auth: WARNING output was empty or {} - not caching"
fi

# ── 5b. Identity/tiers from .claude.json (docs/ref/multiple-account-config-dir.md, §3b/§6b/§6) ─
# Uses accountUuid for stable cache-ownership match; extracts tiers unexposed by auth status.
CLAUDE_JSON_PATH="${CLAUDE_CONFIG_DIR:-$HOME/.claude}/.claude.json"
CURRENT_ACCT=""
CURRENT_ACCT_UUID=""
LIVE_ORG_TIER=""
LIVE_USER_TIER=""
LIVE_ORG_TYPE=""
LIVE_ORG_NAME=""
if [ -f "$CLAUDE_JSON_PATH" ]; then
    # organizationName is emitted last because it may contain `|` which stays in the final field.
    CLAUDE_JSON_INFO=$(AKI_CLAUDE_JSON="$CLAUDE_JSON_PATH" python3 -c '
import json, os
def one(v):
    return " ".join(str(v or "").split())
try:
    d = json.load(open(os.environ["AKI_CLAUDE_JSON"]))
    a = d.get("oauthAccount", {}) or {}
    print("{}|{}|{}|{}|{}|{}".format(
        one(a.get("emailAddress")),
        one(a.get("accountUuid")),
        one(a.get("organizationRateLimitTier")),
        one(a.get("userRateLimitTier")),
        one(a.get("organizationType")),
        one(a.get("organizationName"))))
except Exception:
    print("|||||")
' 2>/dev/null || echo '|||||')
    CURRENT_ACCT=$(printf '%s' "$CLAUDE_JSON_INFO" | cut -d'|' -f1)
    CURRENT_ACCT_UUID=$(printf '%s' "$CLAUDE_JSON_INFO" | cut -d'|' -f2)
    LIVE_ORG_TIER=$(printf '%s' "$CLAUDE_JSON_INFO" | cut -d'|' -f3)
    LIVE_USER_TIER=$(printf '%s' "$CLAUDE_JSON_INFO" | cut -d'|' -f4)
    LIVE_ORG_TYPE=$(printf '%s' "$CLAUDE_JSON_INFO" | cut -d'|' -f5)
    LIVE_ORG_NAME=$(printf '%s' "$CLAUDE_JSON_INFO" | cut -d'|' -f6-)
fi
_log "identity: source=$CLAUDE_JSON_PATH current_account='$CURRENT_ACCT' uuid='$CURRENT_ACCT_UUID' live_org_tier='$LIVE_ORG_TIER' live_user_tier='$LIVE_USER_TIER' live_org_type='$LIVE_ORG_TYPE'"

# ── 5c. Email/orgName retained from AUTH_INFO (§5) without ~/.claude.json override ──
# Avoids multi-process flush clobber race; .claude.json is used only for tiers (§5b, §6).

# ── 6. Subscription metadata priority: .claude.json -> live auth (§5) -> .credentials.json fallback ──
SUB_TYPE="Unknown"
TIER="Unknown"
if [ -n "$LIVE_ORG_TIER" ] || [ -n "$LIVE_USER_TIER" ]; then
    TIER="${LIVE_ORG_TIER:-$LIVE_USER_TIER}"
    _log "meta: tier source=claude.json tier=$TIER"
fi
# Map live organizationType (claude_max -> max, claude_pro -> pro); auth status subscriptionType is fallback.
case "$LIVE_ORG_TYPE" in
    claude_*)
        SUB_TYPE="${LIVE_ORG_TYPE#claude_}"
        _log "meta: subtype source=claude.json organizationType=$LIVE_ORG_TYPE → $SUB_TYPE"
        ;;
esac
if [ "$SUB_TYPE" = "Unknown" ]; then
    FOUND=$(printf '%s' "$AUTH_INFO" | grep -o '"subscriptionType"\s*:\s*"[^"]*"' | head -n 1 | awk -F'"' '{print $4}')
    [ -n "$FOUND" ] && SUB_TYPE="$FOUND"
fi
if [ "$TIER" = "Unknown" ]; then
    FOUND_TIER=$(printf '%s' "$AUTH_INFO" | grep -o '"rateLimitTier"\s*:\s*"[^"]*"' | head -n 1 | awk -F'"' '{print $4}')
    [ -n "$FOUND_TIER" ] && TIER="$FOUND_TIER"
fi
_log "meta: after auth_status subtype=$SUB_TYPE tier=$TIER"
if [ "$SUB_TYPE" = "Unknown" ] || [ "$TIER" = "Unknown" ]; then
    if [ -f "$CREDS" ]; then
        if [ "$SUB_TYPE" = "Unknown" ]; then
            FOUND=$(grep -o '"subscriptionType"\s*:\s*"[^"]*"' "$CREDS" | head -n 1 | awk -F'"' '{print $4}')
            [ -n "$FOUND" ] && SUB_TYPE="$FOUND"
        fi
        if [ "$TIER" = "Unknown" ]; then
            FOUND_TIER=$(grep -o '"rateLimitTier"\s*:\s*"[^"]*"' "$CREDS" | head -n 1 | awk -F'"' '{print $4}')
            [ -n "$FOUND_TIER" ] && TIER="$FOUND_TIER"
        fi
        _log "meta: creds_fallback subtype=$SUB_TYPE tier=$TIER"
    else
        _log "meta: creds_found=no - no fallback left"
    fi
fi

# ── 2-4. Quota-cache freshness (only meaningful when the cache file exists) ───────
CACHE_PRESENT=0
MTIME=0
SANITIZED_JSON=""
[ -f "$FILE" ] && CACHE_PRESENT=1
if [ "$CACHE_PRESENT" = "1" ]; then
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
" 2>/dev/null || echo '0 pct=-1 err=no_python3')

    # RESETS_AT now has format "TIMESTAMP pct=N" - split it
    RESETS_AT_VAL=$(printf '%s' "$RESETS_AT" | awk '{print $1}')
    RESETS_AT_PCT=$(printf '%s' "$RESETS_AT" | awk '{print $2}')
    _log "cache: five_hour.resets_at=$RESETS_AT_VAL $RESETS_AT_PCT now=$NOW"

    # ── 3b. Cache ownership verification (§6b, §7) ─────────────────────────
    # Non-matching owner bypasses STALE_RESET to let §6b drop stale foreign cache and §7 emit live identity.
    CACHE_OWNER_MATCHES=1
    if [ -n "$CURRENT_ACCT" ] || [ -n "$CURRENT_ACCT_UUID" ]; then
        CACHE_OWNER_MATCHES=$(AKI_RL="$FILE" AKI_ACCT="$CURRENT_ACCT" AKI_UUID="$CURRENT_ACCT_UUID" python3 -c '
import json, os
try:
    d = json.load(open(os.environ["AKI_RL"]))
except Exception:
    print(1)
    raise SystemExit
cached_acct = d.get("account", "") or ""
cached_uuid = d.get("account_uuid", "") or ""
live_acct = os.environ.get("AKI_ACCT") or ""
live_uuid = os.environ.get("AKI_UUID") or ""
if cached_uuid and live_uuid:
    print(1 if cached_uuid == live_uuid else 0)
elif cached_acct and live_acct:
    print(1 if cached_acct == live_acct else 0)
else:
    print(1)
' 2>/dev/null || echo 1)
    fi
    _log "cache: owner_matches_live_account=$CACHE_OWNER_MATCHES"

    # ── 4. Stale-reset decision ───────────────────────────────────────────
    if [ -n "$RESETS_AT_VAL" ] && [ "$RESETS_AT_VAL" -gt 0 ] 2>/dev/null; then
        if [ "$NOW" -gt "$RESETS_AT_VAL" ]; then
            OVERDUE=$((NOW - RESETS_AT_VAL))
            if [ "$CACHE_OWNER_MATCHES" = "1" ]; then
                _log "stale_check: NOW($NOW) > resets_at($RESETS_AT_VAL) → STALE overdue_s=$OVERDUE"
                _log "STALE_RESET: signaling → Rust returns null, JS keeps old data and marks it cached"
                echo "|||STALE_RESET|||"
                exit 0
            fi
            _log "stale_check: resets_at overdue by ${OVERDUE}s but this cache belongs to another account → not STALE_RESET, falling through to the account gate"
        else
            UNTIL_RESET=$((RESETS_AT_VAL - NOW))
            _log "stale_check: resets_at=$RESETS_AT_VAL still in future by ${UNTIL_RESET}s → cache valid"
        fi
    else
        _log "stale_check: resets_at=0 or empty → no stale check, treating as valid"
    fi

    # ── 6b. Read-side sanitizing (v5 gates - mirrors statusline-unified.sh, docs/arch/usage-claudecode.md §3) ─
    # DESIGN LOCK: Drops expired/mismatched caches via accountUuid; emits LOG/STATUS to stdout with `|| echo` guard.
    SANITIZED=$(python3 -c "
import json, sys
now = $NOW
current_acct = '''$CURRENT_ACCT'''
current_uuid = '''$CURRENT_ACCT_UUID'''
try:
    with open('$FILE') as f:
        d = json.load(f)
except Exception as e:
    print('LOG:cache unparseable: {}'.format(e))
    print('STATUS:PARSE_ERROR')
    sys.exit(0)

cached_acct = d.get('account', '') or ''
cached_uuid = d.get('account_uuid', '') or ''
if cached_uuid or current_uuid:
    if cached_uuid and cached_uuid != current_uuid:
        print('LOG:account mismatch cached_uuid={} current_uuid={} - cache untrusted, dropping whole cache'.format(cached_uuid, current_uuid))
        print('STATUS:ACCOUNT_MISMATCH')
        sys.exit(0)
    elif not cached_uuid:
        print('LOG:cache has no account_uuid (pre-v5 write) but a live uuid exists - falling back to email gate for this one cache')
        if cached_acct != '' and current_acct != '' and cached_acct != current_acct:
            print('LOG:account mismatch cached={} current={} - cache untrusted, dropping whole cache'.format(cached_acct, current_acct))
            print('STATUS:ACCOUNT_MISMATCH')
            sys.exit(0)
elif cached_acct == '':
    print('LOG:legacy cache has no account field (pre-v4 script) - not dropping, host should be re-applied')
elif current_acct != '' and cached_acct != current_acct:
    print('LOG:account mismatch cached={} current={} - cache untrusted, dropping whole cache'.format(cached_acct, current_acct))
    print('STATUS:ACCOUNT_MISMATCH')
    sys.exit(0)

rl = d.get('rate_limits', {})
if not isinstance(rl, dict):
    rl = {}

kept = {}
for key, entry in rl.items():
    if not isinstance(entry, dict):
        print('LOG:dropped {} (not an object)'.format(key))
        continue
    seen_at = entry.get('seen_at', 0) or 0
    try:
        seen_at = int(seen_at)
    except Exception:
        seen_at = 0
    if seen_at and (now - seen_at) >= 21600:
        print('LOG:dropped {} (unseen for {}s - field stopped being sent)'.format(key, now - seen_at))
        continue
    resets_at = entry.get('resets_at', 0) or 0
    try:
        resets_at = int(resets_at)
    except Exception:
        resets_at = 0
    if resets_at <= 0:
        print('LOG:dropped {} (resets_at=0/missing - unverifiable window)'.format(key))
        continue
    if resets_at <= now:
        print('LOG:dropped {} (expired resets_at={} now={})'.format(key, resets_at, now))
        continue
    kept[key] = entry
    print('LOG:kept {} (resets_at={} still in future)'.format(key, resets_at))

if not kept:
    print('STATUS:EMPTY_AFTER_FILTER')
    sys.exit(0)

out = dict(d)
out['rate_limits'] = kept
print('STATUS:OK:' + json.dumps(out))
" 2>/dev/null || echo 'STATUS:INTERPRETER_ERROR')

    printf '%s\n' "$SANITIZED" | while IFS= read -r _line; do
        case "$_line" in
            LOG:*) _log "sanitize: ${_line#LOG:}" ;;
        esac
    done

    SANITIZE_LAST=$(printf '%s\n' "$SANITIZED" | grep '^STATUS:' | tail -n 1)
    SANITIZE_STATUS=$(printf '%s' "$SANITIZE_LAST" | awk -F: '{print $2}')
    _log "sanitize: status=${SANITIZE_STATUS:-NONE}"

    # The STATUS line is the sole verdict; substitution guard ensures exit code is 0.
    SANITIZED_JSON=""
    if [ "$SANITIZE_STATUS" = "OK" ]; then
        SANITIZED_JSON=$(printf '%s' "$SANITIZE_LAST" | cut -d: -f3-)
    else
        _log "sanitize: no trustworthy data (status=${SANITIZE_STATUS:-NONE}) → no stdout"
    fi
fi

# ── 7. Write stdout payload ───────────────────────────────────────────────────────
# Emit identity frames even without valid quota cache so account switches update UI immediately (§6b).
emit_frames() {
    printf '%s\n' "$1"
    echo "|||MTIME|||$2"
    echo "|||SUBTYPE|||$SUB_TYPE"
    echo "|||TIER|||$TIER"
    echo "|||AUTHINFO|||$AUTH_INFO"
}

if [ -n "$SANITIZED_JSON" ]; then
    _log "stdout_write: emitting sanitized cache_json + MTIME=$MTIME SUBTYPE=$SUB_TYPE TIER=$TIER"
    emit_frames "$SANITIZED_JSON" "$MTIME"
    _log "stdout_write: done - all delimiters emitted"
elif [ -n "$CURRENT_ACCT" ]; then
    _log "stdout_write: no trustworthy quota data, but the account is known - emitting identity only (account=$CURRENT_ACCT)"
    emit_frames '{}' 0
else
    _log "stdout_write: no quota data and no account identity → no stdout → Rust returns null"
fi
