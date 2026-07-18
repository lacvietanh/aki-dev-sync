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

# ── P3: OAuth usage poll ──────────────────────────────────────────────────
# Primitive the rest of the pipeline is missing: ask the server "usage now?"
# without a Claude Code turn. Runs BEFORE the stale-check below so that when
# it succeeds, the cache is already fresh — the null-path/STALE_RESET branches
# (which trigger forceSync/probe) become naturally unreachable, no flag needed.
# Fail-open at every step; never prints to stdout (Rust's stdout parser must
# never see anything but the existing delimiter chain).
OAUTH_REFRESH_AGE_S=120
OAUTH_MARKER="$HOME/.claude/aki-oauth-last-attempt"

oauth_should_run() {
    [ ! -f "$FILE" ] && return 0
    M=$(stat -c %Y "$FILE" 2>/dev/null || stat -f %m "$FILE" 2>/dev/null)
    A=$((NOW - M))
    [ "$A" -gt "$OAUTH_REFRESH_AGE_S" ] && return 0
    R=$(python3 -c "
import json
try:
    d = json.load(open('$FILE'))
    ra = d.get('rate_limits', {}).get('five_hour', {}).get('resets_at', 0)
    print(int(ra))
except Exception:
    print(0)
" 2>/dev/null)
    [ -n "$R" ] && [ "$R" -gt 0 ] 2>/dev/null && [ "$NOW" -gt "$R" ] && return 0
    return 1
}

if oauth_should_run; then
    _log "oauth: age_gate=${OAUTH_REFRESH_AGE_S}s starting"
    python3 << 'PYEOF' || _log "oauth: block failed (python error)"
import json, os, sys, time, subprocess, urllib.request, urllib.error, datetime

HOME_DIR = os.path.expanduser('~')
CREDS = os.path.join(HOME_DIR, '.claude', '.credentials.json')
CACHE = os.path.join(HOME_DIR, '.claude', 'rate-limits-cache.json')
MARKER = os.path.join(HOME_DIR, '.claude', 'aki-oauth-last-attempt')
# Verified against docs/plan/claudecode-oauth-usage-p3.md Bước 0 recon command.
# Bump here if a live host reports 429 (missing/stale UA is the known cause).
UA = 'claude-code/2.1.0'
HTTP_TIMEOUT_S = 8
GATE_S = 60

def log(msg):
    print('[SHELL:get-usage] oauth: ' + msg, file=sys.stderr)

def marker_age():
    try:
        with open(MARKER) as f:
            return time.time() - int(f.read().strip() or 0)
    except Exception:
        return GATE_S + 1  # no marker → treat as long-elapsed, not gated

def touch_marker():
    with open(MARKER, 'w') as f:
        f.write(str(int(time.time())))

def read_token():
    with open(CREDS) as f:
        d = json.load(f)
    o = d.get('claudeAiOauth', {})
    return o.get('accessToken'), o.get('expiresAt', 0)

if not os.path.isfile(CREDS):
    log('no token')
    sys.exit(0)

try:
    token, expires_at = read_token()
except Exception as e:
    log('creds unreadable: ' + str(e))
    sys.exit(0)

if not token:
    log('no token')
    sys.exit(0)

if expires_at and time.time() * 1000 > expires_at:
    # Ca chính, không phải edge (xem plan Hiệu chỉnh 1) — nhưng KHÔNG tự POST refresh
    # (rủi ro rotation phá auth CC). Chỉ đi qua `claude auth status` để CC tự refresh
    # credential của chính nó, gated bởi cùng marker 60s vì nó cũng gọi mạng.
    if marker_age() < GATE_S:
        log('token expired, gated')
        sys.exit(0)
    touch_marker()
    try:
        _claude_bin = os.environ.get('CLAUDE_BIN', 'claude')
        subprocess.run(['bash', '-lc', "'" + _claude_bin + "' auth status"], capture_output=True, timeout=15)
    except Exception as e:
        log('auth status failed: ' + str(e))
    try:
        token, expires_at = read_token()
    except Exception:
        pass
    if expires_at and time.time() * 1000 > expires_at:
        log('token expired')
        sys.exit(0)

if marker_age() < GATE_S:
    log('gated (60s)')
    sys.exit(0)
touch_marker()

req = urllib.request.Request(
    'https://api.anthropic.com/api/oauth/usage',
    headers={
        'Authorization': 'Bearer ' + token,
        'anthropic-beta': 'oauth-2025-04-20',
        'User-Agent': UA,
    },
)
try:
    with urllib.request.urlopen(req, timeout=HTTP_TIMEOUT_S) as resp:
        body = resp.read().decode('utf-8')
except urllib.error.HTTPError as e:
    log('http_error status=' + str(e.code))
    sys.exit(0)
except Exception as e:
    log('timeout/network: ' + str(e))
    sys.exit(0)

try:
    payload = json.loads(body)
except Exception as e:
    log('bad json: ' + str(e))
    sys.exit(0)

def to_epoch(v):
    # Schema not fully confirmed yet (plan Bước 0 checklist) — accept either an
    # epoch number or an ISO8601 string so whichever the server actually sends works.
    if v is None:
        return 0
    if isinstance(v, (int, float)):
        return int(v)
    try:
        return int(datetime.datetime.fromisoformat(str(v).replace('Z', '+00:00')).timestamp())
    except Exception:
        return 0

def to_window(w):
    # Window absent (account idle) → resets_at=0, which get-usage's own stale-check
    # already treats as "valid, no active window" — no limbo, no probe needed.
    if not w:
        return {'used_percentage': 0, 'resets_at': 0}
    pct = w.get('utilization', w.get('used_percentage'))
    pct = round(pct) if isinstance(pct, (int, float)) else 0
    return {'used_percentage': pct, 'resets_at': to_epoch(w.get('resets_at'))}

try:
    with open(CACHE) as f:
        existing = json.load(f)
except Exception:
    existing = {}

existing.setdefault('rate_limits', {})
five = to_window(payload.get('five_hour'))
seven = to_window(payload.get('seven_day'))
existing['rate_limits']['five_hour'] = five
existing['rate_limits']['seven_day'] = seven

tmp = CACHE + '.tmp'
with open(tmp, 'w') as f:
    json.dump(existing, f)
os.replace(tmp, CACHE)  # atomic — statusLine hook (aki-rlcache v3+) also writes atomically now

log('ok pct_5h=' + str(five['used_percentage']) + ' resets_at=' + str(five['resets_at']))
PYEOF
    _log "oauth: block done"
else
    _log "oauth: skip (cache fresh)"
fi

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
    # reopening the app — the TTL alone can't tell "still valid" apart from "just went stale
    # because the user switched accounts". Forcing one real check on app open closes that gap
    # without adding any extra polling for the (rare) mid-session switch case.
    if [ "$AUTH_CACHE_EXISTS" = "yes" ] && [ "$AUTH_CACHE_AGE" -lt "$AUTH_REFRESH_AGE_S" ] && [ "${AKI_FORCE_AUTH_REFRESH:-0}" != "1" ]; then
        AUTH_INFO=$(python3 -c "import json,sys; d=json.load(open('$AUTH_CACHE')); print(json.dumps(d))" 2>/dev/null || echo '{}')
        _log "auth: source=cache (fresh, age=${AUTH_CACHE_AGE}s)"
    else
        # Re-run whenever the cache is missing OR older than AUTH_REFRESH_AGE_S — NOT only on
        # the very first run. Previously this branch only fired when the file didn't exist at
        # all, so once written, auth-cache.json echoed the SAME email forever even after the
        # user logged into a different CC account on this host (bug reported in
        # docs/plan/claudecode-oauth-usage-p3.md: "email hiển thị sai khi đổi tài khoản" — usage
        # % updated correctly but the header email stayed stuck on the old account). Bounded to
        # once per AUTH_REFRESH_AGE_S so a normal 30s poll interval doesn't spawn `claude auth
        # status` every tick.
        AUTH_INFO=$(bash -lc "'$CLAUDE_BIN' auth status 2>/dev/null" 2>/dev/null || echo '{}')
        AUTH_LEN=$(printf '%s' "$AUTH_INFO" | wc -c | tr -d ' ')
        _log "auth: source=claude_auth_status output_len=$AUTH_LEN"
        if [ "$AUTH_INFO" != '{}' ] && [ "$AUTH_LEN" -gt 2 ]; then
            printf '%s' "$AUTH_INFO" > "$AUTH_CACHE"
            _log "auth: cached to $AUTH_CACHE"
        elif [ "$AUTH_CACHE_EXISTS" = "yes" ]; then
            # claude auth status failed/empty this cycle — fall back to the last-known cache
            # instead of blanking the email display; next cycle (in AUTH_REFRESH_AGE_S) retries.
            _log "auth: WARNING claude_auth_status empty this cycle — falling back to stale cache"
            AUTH_INFO=$(python3 -c "import json,sys; d=json.load(open('$AUTH_CACHE')); print(json.dumps(d))" 2>/dev/null || echo '{}')
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
