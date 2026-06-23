#!/usr/bin/env sh
# @docs docs/arch/usage-claudecode.md
# @docs docs/research/claude-usage-1.2.7-analyze.md
# Runs `claude -p /usage` on the remote and exports the output for the parser.
# Research findings (2026-06-23):
# - /usage makes a live network call using OAuth token from ~/.claude/.credentials.json
# - Does NOT require an active Claude Code session
# - < /dev/null is required to avoid a 3-second stdin wait (warning: "no stdin data received in 3s")
# - Dedicated blank dir avoids /tmp clutter that could be picked up as project context
BLANK_DIR="/tmp/aki-dev-sync-blank-dir"
mkdir -p "$BLANK_DIR"
export CLAUDE_SYNC_OUT=$(if command -v zsh >/dev/null 2>&1; then
    zsh -lc "cd '$BLANK_DIR' && claude --model haiku -p /usage < /dev/null 2>/dev/null"
else
    bash -lc "cd '$BLANK_DIR' && claude --model haiku -p /usage < /dev/null 2>/dev/null"
fi)
