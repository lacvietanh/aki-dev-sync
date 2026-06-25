#!/usr/bin/env sh
# NOTE: This script is delivered via `ssh host sh`, where `sh` is POSIX dash on most
# Linux remotes. `set -o pipefail` is a bash/zsh-ism; on dash `set` is a special
# builtin whose usage error makes a non-interactive shell EXIT IMMEDIATELY (status 2)
# — and `2>/dev/null || true` does NOT rescue it (the shell exits before `||` runs).
# That silently killed the entire force-sync (exit 2, no stdout, no stderr) on dash
# remotes, leaving usage stuck in STALE_RESET forever. Probe in a subshell first so
# only the subshell dies on dash; enable pipefail only where it is actually supported.
( set -o pipefail ) 2>/dev/null && set -o pipefail
# @docs docs/arch/usage-claudecode.md
# @docs docs/research/claude-usage-1.2.x-analyze.md
# Runs `claude -p /usage` on the remote and exports the output for the parser.
# Auto-probes with a dummy Haiku session if /usage has no resets_at or shows a past reset time.
# Writes a log to stderr so the Rust caller can record it via its own stderr capture.

BLANK_DIR="/tmp/aki-dev-sync-blank-dir"
NOW_TS=$(date +%s)

_log() {
    printf '[%s][SHELL:force-sync] %s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$1" >&2
}

# ── 1. Environment probe ───────────────────────────────────────────────────
ZSH_PATH=$(command -v zsh 2>/dev/null || echo none)
BASH_PATH=$(command -v bash 2>/dev/null || echo none)
CLAUDE_PATH=$(command -v claude 2>/dev/null || echo none)
_log "env: zsh=$ZSH_PATH bash=$BASH_PATH claude=$CLAUDE_PATH"

# Login-shell claude path (what run_usage() actually resolves)
if [ "$ZSH_PATH" != "none" ]; then
    LOGIN_CLAUDE=$(zsh -lc 'command -v claude 2>/dev/null || echo none' 2>/dev/null)
    LOGIN_SHELL="zsh"
else
    LOGIN_CLAUDE=$(bash -lc 'command -v claude 2>/dev/null || echo none' 2>/dev/null)
    LOGIN_SHELL="bash"
fi
_log "env: login_shell=$LOGIN_SHELL login_shell_claude=$LOGIN_CLAUDE"

# ── 2. BLANK_DIR setup ────────────────────────────────────────────────────
mkdir -p "$BLANK_DIR"
BLANK_MKDIR_EXIT=$?
BLANK_DIR_EXISTS=$([ -d "$BLANK_DIR" ] && echo yes || echo no)
_log "blank_dir: path=$BLANK_DIR mkdir_exit=$BLANK_MKDIR_EXIT dir_exists=$BLANK_DIR_EXISTS"

# ── 3. run_usage helper ───────────────────────────────────────────────────
run_usage() {
    if [ "$ZSH_PATH" != "none" ]; then
        _log "run_usage: using zsh -lc | cd $BLANK_DIR && claude --model haiku -p /usage < /dev/null"
        zsh -lc "cd '$BLANK_DIR' && claude --model haiku -p /usage < /dev/null 2>/dev/null"
    else
        _log "run_usage: using bash -lc | cd $BLANK_DIR && claude --model haiku -p /usage < /dev/null"
        bash -lc "cd '$BLANK_DIR' && claude --model haiku -p /usage < /dev/null 2>/dev/null"
    fi
    _log "run_usage: exit=$?"
}

# ── 4. First /usage run ───────────────────────────────────────────────────
_log "usage_run1: starting"
RUN1_START=$(date +%s)
OUT=$(run_usage)
RUN1_EXIT=$?
RUN1_END=$(date +%s)
RUN1_DUR=$((RUN1_END - RUN1_START))
OUT_LEN=$(printf '%s' "$OUT" | wc -c | tr -d ' ')
_log "usage_run1: exit=$RUN1_EXIT dur_s=$RUN1_DUR output_len=$OUT_LEN"

if [ "$OUT_LEN" -gt 0 ]; then
    PREVIEW=$(printf '%s' "$OUT" | head -c 200 | tr '\n' ' ')
    _log "usage_run1 content: $PREVIEW"
else
    _log "usage_run1: EMPTY OUTPUT — claude may not be found or returned nothing"
fi

# Content markers in first run output
HAS_PCT=$(printf '%s' "$OUT" | grep -qi '%' && echo yes || echo no)
HAS_RESETS_WORD=$(printf '%s' "$OUT" | grep -qi 'resets' && echo yes || echo no)
HAS_SESSION=$(printf '%s' "$OUT" | grep -qi 'session' && echo yes || echo no)
_log "usage_run1 markers: has_pct=$HAS_PCT has_resets=$HAS_RESETS_WORD has_session=$HAS_SESSION"

# ── 5. Parse reset time from first run ────────────────────────────────────
RESETS_CHECK=$(printf '%s' "$OUT" | python3 -c "
import re, datetime, sys, time
out = sys.stdin.read()
now = int(time.time())
m = re.search(r'resets\s*([a-zA-Z]+\s+\d+),\s*(\d+):(\d+)([ap]m)', out, re.IGNORECASE)
if not m:
    print('no_match:0')
    sys.exit(0)
year = datetime.datetime.now().year
try:
    ds = '{} {} {}:{}{}'.format(m.group(1), year, m.group(2), m.group(3), m.group(4))
    dt = datetime.datetime.strptime(ds, '%b %d %Y %I:%M%p')
    ts = int(dt.timestamp())
    overdue_s = now - ts  # positive = in past
    if ts < now - 3600:
        new_ts = int(dt.replace(year=year+1).timestamp())
        print('year_fix_future:{}:orig_overdue={}'.format(new_ts, overdue_s))
    elif ts > now:
        print('future:{}:overdue={}'.format(ts, overdue_s))
    else:
        print('past:{}:overdue={}'.format(ts, overdue_s))
except Exception as e:
    print('parse_error:{}'.format(e))
" 2>/dev/null)

_log "resets_check: raw=$RESETS_CHECK"

RESETS_IS_FUTURE=0
case "$RESETS_CHECK" in
    future:*|year_fix_future:*) RESETS_IS_FUTURE=1 ;;
    *) RESETS_IS_FUTURE=0 ;;
esac

HAS_RESETS=0
if echo "$OUT" | grep -qi "resets"; then
    HAS_RESETS=1
fi

_log "probe_decision: has_resets=$HAS_RESETS resets_is_future=$RESETS_IS_FUTURE → probe_needed=$([ $HAS_RESETS -eq 0 ] || [ $RESETS_IS_FUTURE -ne 1 ] && echo YES || echo no)"

# ── 6. Probe session (if needed) ──────────────────────────────────────────
if [ "$HAS_RESETS" = "0" ] || [ "$RESETS_IS_FUTURE" != "1" ]; then
    PROBE_DIR="/tmp/aki-probe-$NOW_TS"
    _log "probe: starting dir=$PROBE_DIR reason=has_resets=$HAS_RESETS,resets_is_future=$RESETS_IS_FUTURE"
    PROBE_START=$(date +%s)

    if [ "$ZSH_PATH" != "none" ]; then
        _log "probe: cmd=zsh -lc 'mkdir -p $PROBE_DIR && cd $PROBE_DIR && claude --model haiku -p respond_with_ok < /dev/null'"
        zsh -lc "mkdir -p '$PROBE_DIR' && cd '$PROBE_DIR' && claude --model haiku -p \"respond with ok\" < /dev/null >/dev/null 2>&1"
    else
        _log "probe: cmd=bash -lc 'mkdir -p $PROBE_DIR && cd $PROBE_DIR && claude --model haiku -p respond_with_ok < /dev/null'"
        bash -lc "mkdir -p '$PROBE_DIR' && cd '$PROBE_DIR' && claude --model haiku -p \"respond with ok\" < /dev/null >/dev/null 2>&1"
    fi
    PROBE_EXIT=$?
    PROBE_END=$(date +%s)
    PROBE_DUR=$((PROBE_END - PROBE_START))
    _log "probe: done exit=$PROBE_EXIT dur_s=$PROBE_DUR"

    # Did the probe cause claude to (re)write the rate-limit cache? The cache counts as
    # fresh only when its mtime is at/after the probe START — comparing against an
    # absolute "age since script start" would mis-label the pre-reset cache (written a
    # few seconds before the script ran) as fresh. This is diagnostic only; usage_run2
    # below remains the source of truth regardless of the outcome here.
    CACHE_FILE="$HOME/.claude/rate-limits-cache.json"
    if [ -f "$CACHE_FILE" ]; then
        CACHE_MTIME=$(stat -c %Y "$CACHE_FILE" 2>/dev/null || stat -f %m "$CACHE_FILE" 2>/dev/null)
        if [ -n "$CACHE_MTIME" ] && [ "$CACHE_MTIME" -ge "$PROBE_START" ] 2>/dev/null; then
            CACHE_WRITTEN=yes
        else
            CACHE_WRITTEN=no
        fi
        _log "probe: cache_mtime=$CACHE_MTIME probe_start=$PROBE_START written_after_probe=$CACHE_WRITTEN"
    else
        _log "probe: WARNING cache file not found at $CACHE_FILE"
    fi

    rm -rf "$PROBE_DIR"
    _log "probe: cleaned up probe_dir=$PROBE_DIR"

    # ── 7. Second /usage run (post-probe) ─────────────────────────────────
    _log "usage_run2: starting (post-probe)"
    RUN2_START=$(date +%s)
    OUT=$(run_usage)
    RUN2_EXIT=$?
    RUN2_END=$(date +%s)
    RUN2_DUR=$((RUN2_END - RUN2_START))
    OUT_LEN2=$(printf '%s' "$OUT" | wc -c | tr -d ' ')
    _log "usage_run2: exit=$RUN2_EXIT dur_s=$RUN2_DUR output_len=$OUT_LEN2"

    if [ "$OUT_LEN2" -gt 0 ]; then
        PREVIEW2=$(printf '%s' "$OUT" | head -c 200 | tr '\n' ' ')
        _log "usage_run2 content: $PREVIEW2"
    else
        _log "usage_run2: EMPTY OUTPUT after probe — cache may not have been updated by statusLine hook"
    fi

    # Verify reset time improved after probe
    HAS_RESETS2=$(printf '%s' "$OUT" | grep -qi 'resets' && echo yes || echo no)
    RESETS_CHECK2=$(printf '%s' "$OUT" | python3 -c "
import re, datetime, sys, time
out = sys.stdin.read()
now = int(time.time())
m = re.search(r'resets\s*([a-zA-Z]+\s+\d+),\s*(\d+):(\d+)([ap]m)', out, re.IGNORECASE)
if not m:
    print('no_match')
    sys.exit(0)
year = datetime.datetime.now().year
try:
    ds = '{} {} {}:{}{}'.format(m.group(1), year, m.group(2), m.group(3), m.group(4))
    dt = datetime.datetime.strptime(ds, '%b %d %Y %I:%M%p')
    ts = int(dt.timestamp())
    overdue_s = now - ts
    status = 'future' if ts > now else 'past'
    print('{}:ts={}:overdue={}'.format(status, ts, overdue_s))
except Exception as e:
    print('parse_error:{}'.format(e))
" 2>/dev/null)
    _log "usage_run2 resets_check: has_resets=$HAS_RESETS2 $RESETS_CHECK2"

    # ── 7b. Remove this run's probe transcript ────────────────────────────
    # The probe session existed only to register a rate-limit event for the
    # re-read above; its transcript is now pure garbage. Delete it deterministically
    # by the exact project path we created (claude maps cwd → projects/<cwd-with-/→->).
    # No time window, no globbing — zero risk to other runs' transcripts.
    PROBE_PROJECT_DIR="$HOME/.claude/projects/$(printf '%s' "$PROBE_DIR" | sed 's|/|-|g')"
    if [ -d "$PROBE_PROJECT_DIR" ]; then
        rm -rf "$PROBE_PROJECT_DIR"
        _log "cleanup: probe_project_dir removed=$PROBE_PROJECT_DIR"
    else
        _log "cleanup: probe_project_dir not_created=$PROBE_PROJECT_DIR"
    fi
else
    _log "probe skipped: reset is in the future — resets_check=$RESETS_CHECK"
fi

# ── 8. Export result ──────────────────────────────────────────────────────
export CLAUDE_SYNC_OUT="$OUT"
FINAL_LEN=$(printf '%s' "$CLAUDE_SYNC_OUT" | wc -c | tr -d ' ')
FINAL_HAS_PCT=$(printf '%s' "$CLAUDE_SYNC_OUT" | grep -qi '%' && echo yes || echo no)
FINAL_HAS_RESETS=$(printf '%s' "$CLAUDE_SYNC_OUT" | grep -qi 'resets' && echo yes || echo no)
FINAL_PREVIEW=$(printf '%s' "$CLAUDE_SYNC_OUT" | head -c 300 | tr '\n' ' ')
_log "export: CLAUDE_SYNC_OUT len=$FINAL_LEN has_pct=$FINAL_HAS_PCT has_resets=$FINAL_HAS_RESETS"
_log "export: content_preview=$FINAL_PREVIEW"

# ── 9. Cleanup ────────────────────────────────────────────────────────────
# Blank-dir transcripts: every /usage run leaves a session JSONL in the blank
# project dir; they are disposable once we have captured the output above. Delete
# those older than 1 minute — old enough to never race a concurrent in-flight
# sync's just-created transcript, aggressive enough to bound growth to a handful.
BLANK_PROJECT_DIR="$HOME/.claude/projects/$(echo "$BLANK_DIR" | sed 's|/|-|g')"
JSONL_DELETED=$(find "$BLANK_PROJECT_DIR" -name "*.jsonl" -mmin +1 -delete -print 2>/dev/null | wc -l | tr -d ' ')
_log "cleanup: blank_project_jsonl_deleted=$JSONL_DELETED dir=$BLANK_PROJECT_DIR"

# Orphan probe dirs: each run already deletes its own probe transcript
# deterministically (section 7b), so this sweep is only a safety net for runs that
# died before reaching that step. A 1-hour window clears them promptly without any
# chance of touching a probe dir from a still-running sync.
PROBE_DIRS_DELETED=$(find "$HOME/.claude/projects" -maxdepth 1 -type d -name '-tmp-aki-probe-*' -mmin +60 \
  -print 2>/dev/null | wc -l | tr -d ' ')
find "$HOME/.claude/projects" -maxdepth 1 -type d -name '-tmp-aki-probe-*' -mmin +60 \
  -exec rm -rf {} + 2>/dev/null || true
_log "cleanup: orphan_probe_dirs_deleted=$PROBE_DIRS_DELETED"

_log "done: total_script_dur_s=$(($(date +%s) - NOW_TS))"
