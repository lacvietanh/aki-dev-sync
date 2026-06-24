#!/usr/bin/env sh
set -o pipefail 2>/dev/null || true
# @docs docs/arch/usage-claudecode.md
# @docs docs/research/claude-usage-1.2.x-analyze.md
# Runs `claude -p /usage` on the remote and exports the output for the parser.
# Auto-probes with a dummy Haiku session if /usage has no resets_at or shows a past reset time.

BLANK_DIR="/tmp/aki-dev-sync-blank-dir"
mkdir -p "$BLANK_DIR"

# Run command helper
run_usage() {
    if command -v zsh >/dev/null 2>&1; then
        zsh -lc "cd '$BLANK_DIR' && claude --model haiku -p /usage < /dev/null 2>/dev/null"
    else
        bash -lc "cd '$BLANK_DIR' && claude --model haiku -p /usage < /dev/null 2>/dev/null"
    fi
}

OUT=$(run_usage)

# If the output does not contain "resets", OR the parsed reset time is already in the past
# (stale cache echoed back by /usage), trigger a probe session and re-fetch.
RESETS_IS_FUTURE=$(printf '%s' "$OUT" | python3 -c "
import re, datetime, sys
out = sys.stdin.read()
m = re.search(r'resets\s*([a-zA-Z]+\s+\d+),\s*(\d+):(\d+)([ap]m)', out, re.IGNORECASE)
if not m:
    print(0)
    sys.exit(0)
year = datetime.datetime.now().year
try:
    ds = '{} {} {}:{}{}'.format(m.group(1), year, m.group(2), m.group(3), m.group(4))
    dt = datetime.datetime.strptime(ds, '%b %d %Y %I:%M%p')
    ts = int(dt.timestamp())
    if ts < int(datetime.datetime.now().timestamp()) - 3600:
        ts = int(dt.replace(year=year+1).timestamp())
    print(1 if ts > int(datetime.datetime.now().timestamp()) else 0)
except Exception:
    print(0)
")
if ! echo "$OUT" | grep -qi "resets" || [ "$RESETS_IS_FUTURE" != "1" ]; then
    PROBE_DIR="/tmp/aki-probe-$(date +%s)"
    if command -v zsh >/dev/null 2>&1; then
        zsh -lc "mkdir -p '$PROBE_DIR' && cd '$PROBE_DIR' && claude --model haiku -p \"respond with ok\" < /dev/null >/dev/null 2>&1"
    else
        bash -lc "mkdir -p '$PROBE_DIR' && cd '$PROBE_DIR' && claude --model haiku -p \"respond with ok\" < /dev/null >/dev/null 2>&1"
    fi
    rm -rf "$PROBE_DIR"
    # Re-run /usage
    OUT=$(run_usage)
fi

export CLAUDE_SYNC_OUT="$OUT"

# Cleanup: remove JSONL files older than 7 days from BLANK_DIR project folder
BLANK_PROJECT_DIR="$HOME/.claude/projects/$(echo "$BLANK_DIR" | sed 's|/|-|g')"
find "$BLANK_PROJECT_DIR" -name "*.jsonl" -mtime +1 -delete 2>/dev/null || true

# Cleanup: remove orphaned probe project dirs older than 1 day
find "$HOME/.claude/projects" -maxdepth 1 -type d -name '-tmp-aki-probe-*' -mtime +1 \
  -exec rm -rf {} + 2>/dev/null || true
