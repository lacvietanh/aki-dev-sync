# Claude Code Usage Monitoring Reference

## Mechanism of Action
Claude Code (Claude.ai Pro/Max subscription) injects real rate limit data from Anthropic API headers into the `stdin` JSON of the `statusLine` script after EVERY turn (each message exchange). This data is NOT an estimation—it is real telemetry from the server.

The JSON payload structure:
```json
{
  "rate_limits": {
    "five_hour":  { "used_percentage": 42, "resets_at": 1782034800 },
    "seven_day":  { "used_percentage": 18, "resets_at": 1782288000 }
  },
  "cwd": "/home/user/project",
  "transcript_path": "/home/user/.claude/projects/..."
}
```

- `resets_at`: Unix epoch seconds, UTC.
- This functionality is exclusive to Claude.ai Pro/Max and does not apply to standard API key users.
- The `statusLine` script is invoked automatically after every turn.

## Local File Layout on Remote Machine
- `~/.claude/settings.json` → Configuration file, points to the `statusLine` script.
- `~/.claude/statusline-command.sh` → The script receiving the `stdin` JSON from Claude Code.
- `~/.claude/rate-limits-cache.json` → NOT available by default. We create this file by dumping `stdin`.
- `~/.claude/projects/**/*.jsonl` → Transcript files (fallback for token estimation).

## Self-Provisioning Logic
Since the real data only exists transiently in `stdin` during the execution of the `statusLine` script, it must be persisted. 

The self-provisioning flow works as follows:
1. SSH into the target host.
2. Read `~/.claude/statusline-command.sh`.
3. Check for the string `rate-limits-cache`.
4. If it doesn't exist, use `sed` to inject the dump command immediately after `input=$(cat)`.
   Command injected: `printf '%s' "$input" > ~/.claude/rate-limits-cache.json`
5. If it already exists, skip modification.
6. From then on, simply read `~/.claude/rate-limits-cache.json` over SSH.

## Official References
- [StatusLine Documentation](https://code.claude.com/docs/en/statusline)
- [Claude Code Changelog](https://code.claude.com/docs/en/changelog)
- [Rate Limits Documentation](https://platform.claude.com/docs/en/api/rate-limits)
