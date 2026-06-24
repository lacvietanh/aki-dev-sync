#!/usr/bin/env sh
# @docs docs/arch/usage-claudecode.md
# @docs docs/research/claude-usage-1.2.x-analyze.md
# Runs `claude -p /usage` on the remote and exports the output for the parser.
# Auto-probes with a dummy Haiku session if no local sessions exist (to force output of resets_at).

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

# If the output does not contain "resets" (meaning no active local session in the 5h window),
# trigger a dummy session to populate local JSONL logs, then fetch /usage again.
if ! echo "$OUT" | grep -q "resets"; then
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
