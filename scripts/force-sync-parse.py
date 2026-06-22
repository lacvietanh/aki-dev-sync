#!/usr/bin/env python3
# Parses claude -p /usage stdout and writes usage data to rate-limits-cache.json.
import sys, re, json, datetime, os

out = os.environ.get("CLAUDE_SYNC_OUT", "")
match = re.search(
    r'(\d+)%\s*used\s*.\s*resets\s*([a-zA-Z]+\s+\d+),\s*(\d+):(\d+)([ap]m)',
    out,
    re.IGNORECASE,
)
if not match:
    sys.exit(0)

pct = int(match.group(1))
year = datetime.datetime.now().year
date_str = f"{match.group(2)} {year} {match.group(3)}:{match.group(4)}{match.group(5)}"
try:
    dt = datetime.datetime.strptime(date_str, "%b %d %Y %I:%M%p")
    resets_at = int(dt.timestamp())
except Exception:
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

try:
    with open(cache_file, "w") as f:
        json.dump(data, f)
except Exception:
    pass
