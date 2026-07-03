#!/usr/bin/env python3
# @docs docs/arch/usage-claudecode.md
# Parses claude -p /usage stdout and writes usage data to rate-limits-cache.json.
# Prints a JSON diagnostic line to stdout so the Rust logger can record exactly what happened.
import sys, re, json, datetime, os, time

out = os.environ.get("CLAUDE_SYNC_OUT", "")
raw_preview = out[:400].strip()
now_ts = int(time.time())

diag = {
    "ts": now_ts,
    "parsed": False,
    "pct": None,
    "resets_at": None,
    "written": False,
    "raw_preview": raw_preview,
    "raw_len": len(out),
    "year_fix_applied": False,
    "parse_error": None,
    "resets_at_overdue_s": None,  # positive = resets_at is in the past by this many seconds
}

pct_match = re.search(r'(\d+)%\s*used', out, re.IGNORECASE)
if not pct_match:
    # After a quota reset, Claude's /usage no longer prints "X% used" — the session
    # window is fresh with 0 usage. Treat any output that looks like a successful /usage
    # response (contains "Claude" or "Usage" or a resets/session line) as pct=0 rather
    # than a hard parse failure, so the cache still gets a valid resets_at written.
    has_usage_context = bool(re.search(r'claude|usage|resets|session|limit|token', out, re.IGNORECASE))
    if not has_usage_context or not out.strip():
        diag["parse_error"] = "no_pct_match"
        print(json.dumps(diag))
        sys.exit(0)
    pct = 0
    diag["parse_error"] = "no_pct_match:assumed_zero"
else:
    pct = int(pct_match.group(1))
diag["pct"] = pct
resets_at = 0

reset_match = re.search(
    r'resets\s*([a-zA-Z]+\s+\d+),\s*(\d+):(\d+)([ap]m)',
    out,
    re.IGNORECASE,
)
if reset_match:
    year = datetime.datetime.now().year
    date_str = f"{reset_match.group(1)} {year} {reset_match.group(2)}:{reset_match.group(3)}{reset_match.group(4)}"
    try:
        dt = datetime.datetime.strptime(date_str, "%b %d %Y %I:%M%p")
        resets_at = int(dt.timestamp())
        overdue = now_ts - resets_at  # positive = in the past
        diag["resets_at_raw"] = resets_at
        diag["resets_at_overdue_s"] = overdue

        # Year-boundary fix: if parsed time is >1h in the past, assume next year.
        # NOTE: this only applies for genuine Dec→Jan crossings. If /usage is returning
        # truly stale cache (reset was hours ago), this fix will incorrectly push the date
        # forward 1 year. The probe logic in force-sync-claudecode.sh should have prevented
        # stale output from reaching here, but log when fix is applied for traceability.
        if resets_at < now_ts - 3600:
            dt = dt.replace(year=year + 1)
            resets_at = int(dt.timestamp())
            diag["year_fix_applied"] = True
            diag["year_fix_from"] = diag["resets_at_raw"]
            diag["year_fix_to"] = resets_at
    except Exception as e:
        diag["parse_error"] = f"strptime_failed: {e}"
        diag["date_str_attempted"] = date_str
        print(json.dumps(diag))
        sys.exit(0)
else:
    diag["parse_error"] = "no_reset_match (pct parsed but no resets line)"

diag["resets_at"] = resets_at

cache_file = os.path.expanduser("~/.claude/rate-limits-cache.json")
data = {}
if os.path.exists(cache_file):
    try:
        with open(cache_file, "r") as f:
            data = json.load(f)
    except Exception as e:
        diag["cache_read_error"] = str(e)

if "rate_limits" not in data or data["rate_limits"] is None:
    data["rate_limits"] = {}

data["rate_limits"]["five_hour"] = {
    "used_percentage": pct,
    "resets_at": resets_at,
}

written = False
try:
    with open(cache_file, "w") as f:
        json.dump(data, f)
    written = True
except Exception as e:
    diag["write_error"] = str(e)

diag["parsed"] = True
diag["written"] = written

print(json.dumps(diag))
