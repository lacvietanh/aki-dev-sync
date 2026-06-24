#!/usr/bin/env python3
# @docs docs/arch/usage-claudecode.md
# Parses claude -p /usage stdout and writes usage data to rate-limits-cache.json.
# Prints a JSON diagnostic line to stdout so the caller can log what happened.
import sys, re, json, datetime, os

out = os.environ.get("CLAUDE_SYNC_OUT", "")
raw_preview = out[:300].strip()

pct_match = re.search(r'(\d+)%\s*used', out, re.IGNORECASE)
if not pct_match:
    print(json.dumps({"parsed": False, "raw_preview": raw_preview}))
    sys.exit(0)

pct = int(pct_match.group(1))
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
    except Exception:
        print(json.dumps({"parsed": False, "parse_error": True, "date_str": date_str}))
        sys.exit(0)


cache_file = os.path.expanduser("~/.claude/rate-limits-cache.json")
data = {}
if os.path.exists(cache_file):
    try:
        with open(cache_file, "r") as f:
            data = json.load(f)
    except Exception:
        pass

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
except Exception:
    pass

print(json.dumps({"parsed": True, "pct": pct, "resets_at": resets_at, "written": written}))
