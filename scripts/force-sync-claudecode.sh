#!/usr/bin/env sh
# Runs `claude -p /usage` on the remote and exports the output for the parser.
export CLAUDE_SYNC_OUT=$(if command -v zsh >/dev/null 2>&1; then
    zsh -lc "claude --model haiku -p /usage 2>/dev/null"
else
    bash -lc "claude --model haiku -p /usage 2>/dev/null"
fi)
