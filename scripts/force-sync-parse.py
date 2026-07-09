#!/usr/bin/env python3
# @docs docs/arch/usage-claudecode.md
# Parses claude -p /usage stdout and writes usage data to rate-limits-cache.json.
# Reset time prefers the native `--output-format json` rate_limit_info.resetsAt (exported by
# force-sync-claudecode.sh as CLAUDE_SYNC_JSON_RESETS_AT / CLAUDE_SYNC_JSON_TYPE) — a real epoch,
# server-truth, no year-guessing — and falls back to scraping the /usage text. When the /usage text
# is empty (Bug B) but the JSON carries a five_hour reset, recovers (writes resets_at, preserves the
# last-known percentage) instead of giving up. Only when BOTH are empty (CLI/auth truly dead) does it
# still give up, exactly as before.
# Prints a JSON diagnostic line to stdout so the Rust logger can record exactly what happened.
import sys, re, json, datetime, os, time

out = os.environ.get("CLAUDE_SYNC_OUT", "")
raw_preview = out[:400].strip()
now_ts = int(time.time())

# ── Native JSON reset-time (authoritative for the five_hour window when present) ──────────────
json_type = os.environ.get("CLAUDE_SYNC_JSON_TYPE", "none")
try:
    json_resets_at = int(os.environ.get("CLAUDE_SYNC_JSON_RESETS_AT", "0") or "0")
except Exception:
    json_resets_at = 0
# This parser only manages the five_hour window, so trust JSON only when it reports five_hour.
json_five_hour_reset = json_resets_at if (json_type == "five_hour" and json_resets_at > 0) else 0

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
    "json_resets_at": json_resets_at,
    "json_type": json_type,
    "reset_source": None,  # "json" | "text" | None
}

cache_file = os.path.expanduser("~/.claude/rate-limits-cache.json")


def read_cache():
    if os.path.exists(cache_file):
        try:
            with open(cache_file, "r") as f:
                return json.load(f)
        except Exception as e:
            diag["cache_read_error"] = str(e)
    return {}


def write_five_hour(pct, resets_at):
    data = read_cache()
    if "rate_limits" not in data or data["rate_limits"] is None:
        data["rate_limits"] = {}
    data["rate_limits"]["five_hour"] = {
        "used_percentage": pct,
        "resets_at": resets_at,
    }
    try:
        with open(cache_file, "w") as f:
            json.dump(data, f)
        return True
    except Exception as e:
        diag["write_error"] = str(e)
        return False


pct_match = re.search(r'(\d+)%\s*used', out, re.IGNORECASE)
if not pct_match:
    # After a quota reset, Claude's /usage no longer prints "X% used" — the session window is fresh
    # with 0 usage. Treat any output that still looks like a valid /usage response as pct=0. A truly
    # EMPTY /usage (Bug B) has no context, so:
    #   - if the native JSON gave us a real five_hour reset → RECOVER (write reset, keep last pct);
    #   - otherwise → give up (the CLI/auth-dead case, same as before).
    has_usage_context = bool(re.search(r'claude|usage|resets|session|limit|token', out, re.IGNORECASE))
    if not has_usage_context or not out.strip():
        if json_five_hour_reset > 0:
            prev_5h = (read_cache().get("rate_limits", {}) or {}).get("five_hour", {}) or {}
            pct = prev_5h.get("used_percentage", 0) or 0
            resets_at = json_five_hour_reset
            diag["pct"] = pct
            diag["resets_at"] = resets_at
            diag["reset_source"] = "json"
            diag["parse_error"] = "no_pct_match:recovered_from_json"
            diag["written"] = write_five_hour(pct, resets_at)
            diag["parsed"] = True
            print(json.dumps(diag))
            sys.exit(0)
        diag["parse_error"] = "no_pct_match"
        print(json.dumps(diag))
        sys.exit(0)
    pct = 0
    diag["parse_error"] = "no_pct_match:assumed_zero"
else:
    pct = int(pct_match.group(1))
diag["pct"] = pct

# ── Reset time: prefer native JSON (five_hour), fall back to the /usage text scrape ────────────
resets_at = 0
if json_five_hour_reset > 0:
    resets_at = json_five_hour_reset
    diag["reset_source"] = "json"
else:
    # Current CLI writes "resets Jul 14 at 9:59am" (no comma, ":MM" optional — "10am" alone when
    # exactly on the hour); older CLI wrote "resets Jul 14, 9:59am". Accept both separators and
    # make minutes optional (found 2026-07-08, was silently writing resets_at=0 on every real
    # response — see docs/plan/claudecode-oauth-usage-p3.md).
    reset_match = re.search(
        r'resets\s+([a-zA-Z]+\s+\d+)(?:,|\s+at)\s+(\d+)(?::(\d+))?\s*([ap]m)',
        out,
        re.IGNORECASE,
    )
    if reset_match:
        year = datetime.datetime.now().year
        minutes = reset_match.group(3) or "00"
        date_str = f"{reset_match.group(1)} {year} {reset_match.group(2)}:{minutes}{reset_match.group(4)}"
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
            diag["reset_source"] = "text"
        except Exception as e:
            diag["parse_error"] = f"strptime_failed: {e}"
            diag["date_str_attempted"] = date_str
            print(json.dumps(diag))
            sys.exit(0)
    else:
        diag["parse_error"] = "no_reset_match (pct parsed but no resets line)"

diag["resets_at"] = resets_at
diag["written"] = write_five_hour(pct, resets_at)
diag["parsed"] = True

print(json.dumps(diag))
