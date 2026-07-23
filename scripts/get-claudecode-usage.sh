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

    # RESETS_AT now has format "TIMESTAMP pct=N" - split it
    RESETS_AT_VAL=$(printf '%s' "$RESETS_AT" | awk '{print $1}')
    RESETS_AT_PCT=$(printf '%s' "$RESETS_AT" | awk '{print $2}')
    _log "cache: five_hour.resets_at=$RESETS_AT_VAL $RESETS_AT_PCT now=$NOW"

    # ── 4. Stale-reset decision ───────────────────────────────────────────
    if [ -n "$RESETS_AT_VAL" ] && [ "$RESETS_AT_VAL" -gt 0 ] 2>/dev/null; then
        if [ "$NOW" -gt "$RESETS_AT_VAL" ]; then
            OVERDUE=$((NOW - RESETS_AT_VAL))
            _log "stale_check: NOW($NOW) > resets_at($RESETS_AT_VAL) → STALE overdue_s=$OVERDUE"
            _log "STALE_RESET: signaling → Rust returns null, JS keeps old data and marks it cached"
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
    # longer keep `.credentials.json` on disk (moved to OS keychain) - `claude auth status`
    # is the one source that still works either way, and it also carries subscriptionType.
    AUTH_CACHE="$HOME/.claude/auth-cache.json"
    AUTH_REFRESH_AGE_S=300
    AUTH_CACHE_EXISTS=$([ -f "$AUTH_CACHE" ] && echo yes || echo no)
    AUTH_CACHE_AGE=999999
    if [ "$AUTH_CACHE_EXISTS" = "yes" ]; then
        AUTH_MTIME=$(stat -c %Y "$AUTH_CACHE" 2>/dev/null || stat -f %m "$AUTH_CACHE" 2>/dev/null)
        AUTH_CACHE_AGE=$((NOW - AUTH_MTIME))
    fi
    _log "auth: cache_exists=$AUTH_CACHE_EXISTS age=${AUTH_CACHE_AGE}s force=${AKI_FORCE_AUTH_REFRESH:-0}"

    # AKI_FORCE_AUTH_REFRESH=1 is set by the Rust caller exactly once per host per app launch
    # (see cc_auth_force_needed in agent_usage.rs). Without this, a CC account switch made
    # while the cache is still <5min old would keep showing the old email even right after
    # reopening the app - the TTL alone can't tell "still valid" apart from "just went stale
    # because the user switched accounts". Forcing one real check on app open closes that gap
    # without adding any extra polling for the (rare) mid-session switch case.
    if [ "$AUTH_CACHE_EXISTS" = "yes" ] && [ "$AUTH_CACHE_AGE" -lt "$AUTH_REFRESH_AGE_S" ] && [ "${AKI_FORCE_AUTH_REFRESH:-0}" != "1" ]; then
        AUTH_INFO=$(python3 -c "import json,sys; d=json.load(open('$AUTH_CACHE')); print(json.dumps(d))" 2>/dev/null || echo '{}')
        _log "auth: source=cache (fresh, age=${AUTH_CACHE_AGE}s)"
    else
        # Re-run whenever the cache is missing OR older than AUTH_REFRESH_AGE_S - NOT only on
        # the very first run. Previously this branch only fired when the file didn't exist at
        # all, so once written, auth-cache.json echoed the SAME email forever even after the
        # user logged into a different CC account on this host (bug reported in
        # docs/research/claudecode-usage-FINAL.md: "email hiển thị sai khi đổi tài khoản" - usage
        # % updated correctly but the header email stayed stuck on the old account). Bounded to
        # once per AUTH_REFRESH_AGE_S so a normal 30s poll interval doesn't spawn `claude auth
        # status` every tick.
        AUTH_INFO=$(bash -lc "$AKI_CLAUDE_TMO'$CLAUDE_BIN' auth status 2>/dev/null" 2>/dev/null || echo '{}')
        AUTH_LEN=$(printf '%s' "$AUTH_INFO" | wc -c | tr -d ' ')
        _log "auth: source=claude_auth_status output_len=$AUTH_LEN"
        if [ "$AUTH_INFO" != '{}' ] && [ "$AUTH_LEN" -gt 2 ]; then
            printf '%s' "$AUTH_INFO" > "$AUTH_CACHE"
            _log "auth: cached to $AUTH_CACHE"
        elif [ "$AUTH_CACHE_EXISTS" = "yes" ]; then
            # claude auth status failed/empty this cycle - fall back to the last-known cache
            # instead of blanking the email display; next cycle (in AUTH_REFRESH_AGE_S) retries.
            _log "auth: WARNING claude_auth_status empty this cycle - falling back to stale cache"
            AUTH_INFO=$(python3 -c "import json,sys; d=json.load(open('$AUTH_CACHE')); print(json.dumps(d))" 2>/dev/null || echo '{}')
        else
            _log "auth: WARNING output was empty or {} - not caching"
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
        _log "meta: creds_found=no - falling back to auth status"
    fi
    if [ "$SUB_TYPE" = "Unknown" ]; then
        FOUND=$(printf '%s' "$AUTH_INFO" | grep -o '"subscriptionType"\s*:\s*"[^"]*"' | head -n 1 | awk -F'"' '{print $4}')
        [ -n "$FOUND" ] && SUB_TYPE="$FOUND"
        _log "meta: auth_status subtype=$SUB_TYPE"
    fi

    # ── 6b. Read-side sanitizing (v4 gates - mirrors statusline.rs writer) ─
    # DESIGN LOCK - same two invariants as the `aki-rlcache v4` writer block in
    # src-tauri/src/statusline.rs: an entry whose resets_at has passed must never be shown, and a
    # cache written by a different account must never be shown. The writer only protects hosts
    # that already received the new script; this reader protects the app even against a host still
    # running an older statusline hook. Read-only: never touches $FILE. See docs/arch/usage-claudecode.md §3.
    CURRENT_ACCT=""
    if [ -f "$HOME/.claude.json" ]; then
        CURRENT_ACCT=$(python3 -c "
import json
try:
    d = json.load(open('$HOME/.claude.json'))
    print(d.get('oauthAccount', {}).get('emailAddress', '') or '')
except Exception:
    print('')
" 2>/dev/null)
    fi
    _log "sanitize: current_account='$CURRENT_ACCT'"

    # Sanitizer emits one "LOG:<message>" line per decision, followed by exactly one final
    # "STATUS:<code>[:<json>]" line. Everything goes to stdout (no stray files, no reliance on a
    # writable $HOME/.claude for a scratch stderr file) so a single `python3 -c` failure can never
    # abort the outer `set -e` script - a non-zero exit or empty output is simply treated as
    # "no trustworthy data".
    SANITIZED=$(python3 -c "
import json, sys
now = $NOW
current_acct = '''$CURRENT_ACCT'''
try:
    with open('$FILE') as f:
        d = json.load(f)
except Exception as e:
    print('LOG:cache unparseable: {}'.format(e))
    print('STATUS:PARSE_ERROR')
    sys.exit(0)

cached_acct = d.get('account', '') or ''
if cached_acct == '':
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
" 2>/dev/null)
    SANITIZE_RC=$?

    printf '%s\n' "$SANITIZED" | while IFS= read -r _line; do
        case "$_line" in
            LOG:*) _log "sanitize: ${_line#LOG:}" ;;
        esac
    done

    SANITIZE_LAST=$(printf '%s\n' "$SANITIZED" | grep '^STATUS:' | tail -n 1)
    SANITIZE_STATUS=$(printf '%s' "$SANITIZE_LAST" | awk -F: '{print $2}')
    _log "sanitize: exit=$SANITIZE_RC status=${SANITIZE_STATUS:-NONE}"

    SANITIZED_JSON=""
    if [ "$SANITIZE_RC" -eq 0 ] && [ "$SANITIZE_STATUS" = "OK" ]; then
        SANITIZED_JSON=$(printf '%s' "$SANITIZE_LAST" | cut -d: -f3-)
    else
        _log "sanitize: no trustworthy data (status=${SANITIZE_STATUS:-NONE}, rc=$SANITIZE_RC) → no stdout"
    fi

    # ── 7. Write stdout payload ───────────────────────────────────────────
    if [ -z "$SANITIZED_JSON" ]; then
        _log "stdout_write: nothing to emit after sanitizing → Rust returns null, same as missing-file branch"
    else
        _log "stdout_write: emitting sanitized cache_json + MTIME=$MTIME SUBTYPE=$SUB_TYPE TIER=$TIER"
        printf '%s\n' "$SANITIZED_JSON"
        echo "|||MTIME|||$MTIME"
        echo "|||SUBTYPE|||$SUB_TYPE"
        echo "|||TIER|||$TIER"
        echo "|||AUTHINFO|||$AUTH_INFO"
        _log "stdout_write: done - all delimiters emitted"
    fi
else
    _log "cache file missing: $FILE → no stdout output → Rust returns null → JS shows empty state"
fi
