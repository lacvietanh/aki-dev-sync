#!/usr/bin/env bash
# One-shot fix for the macOS ssh-agent leak: every new interactive shell starts with a clean environment, so a guard that only tests `[ -z "$SSH_AGENT_PID" ]` in ~/.zshrc always passes and spawns a fresh agent that is never reaped (ssh-agent daemonizes into its own session, outside the spawning shell's process group). See docs/ref/ssh-agent-leak.md for the full mechanism.
#
# This script replaces that guard with one that persists the agent's env vars to a file and only starts a new agent when the recorded PID is not actually an ssh-agent process. It deliberately checks liveness with `ps -p "$SSH_AGENT_PID" -o comm=` instead of `kill -0 "$SSH_AGENT_PID"`: `kill -0` only asks "does this PID exist", and a PID is recycled by the OS over time - a bare `kill -0` would pass against an unrelated process that now happens to own that PID, silently wiring every new shell to a dead, wrong socket. Matching `comm=` against `ssh-agent` closes that gap.
#
# Usage:
#   scripts/fix-ssh-agent-leak.sh              # dry run (default, safe) - prints the diff, writes nothing
#   scripts/fix-ssh-agent-leak.sh --dry-run    # same as above, explicit
#   scripts/fix-ssh-agent-leak.sh --apply      # writes the fix to ~/.zshrc, after a timestamped backup
set -euo pipefail

MODE="dry-run"
for arg in "$@"; do
    case "$arg" in
        --apply) MODE="apply" ;;
        --dry-run) MODE="dry-run" ;;
        *)
            echo "Unknown argument: $arg (expected --dry-run or --apply)" >&2
            exit 1
            ;;
    esac
done

if [ "$(uname)" != "Darwin" ]; then
    echo "This script targets macOS only (edits the owner's ~/.zshrc) - refusing to run on $(uname)." >&2
    exit 1
fi

ZSHRC="$HOME/.zshrc"
if [ ! -f "$ZSHRC" ]; then
    echo "No $ZSHRC found - nothing to fix." >&2
    exit 1
fi

MARKER="# aki-ssh-agent-fix v1"
if grep -qF "$MARKER" "$ZSHRC"; then
    echo "already applied - $ZSHRC already contains the fixed block ($MARKER). Nothing to do."
    exit 0
fi

# Match old guard dynamically by pattern rather than hardcoded line numbers to prevent file corruption.
START_LINE=$(grep -nE 'if[[:space:]]*\[[[:space:]]*-z[[:space:]]*"?\$SSH_AGENT_PID"?[[:space:]]*\]' "$ZSHRC" | head -n1 | cut -d: -f1 || true)
if [ -z "$START_LINE" ]; then
    echo "Could not find the old guard: no line matching an 'if [ -z \"\$SSH_AGENT_PID\" ]' shape in $ZSHRC." >&2
    echo "Searched for: if [[:space:]]*\[[[:space:]]*-z[[:space:]]*\"?\\\$SSH_AGENT_PID\"?[[:space:]]*\]" >&2
    echo "Refusing to guess - inspect $ZSHRC manually." >&2
    exit 1
fi

# Locate closing fi following eval "$(ssh-agent -s)" to target the exact guard block.
BLOCK=$(sed -n "${START_LINE},\$p" "$ZSHRC")
EVAL_OFFSET=$(printf '%s\n' "$BLOCK" | grep -nE 'eval[[:space:]]+"\$\(ssh-agent[[:space:]]+-s\)"' | head -n1 | cut -d: -f1 || true)
if [ -z "$EVAL_OFFSET" ]; then
    echo "Found an 'if [ -z \"\$SSH_AGENT_PID\" ]' line at $ZSHRC:$START_LINE but no 'eval \"\$(ssh-agent -s)\"' after it." >&2
    echo "Refusing to guess which block to replace - inspect $ZSHRC manually." >&2
    exit 1
fi
FI_OFFSET=$(printf '%s\n' "$BLOCK" | tail -n +"$EVAL_OFFSET" | grep -nE '^\s*fi\s*$' | head -n1 | cut -d: -f1 || true)
if [ -z "$FI_OFFSET" ]; then
    echo "Found the guard's eval line but no closing 'fi' after it in $ZSHRC." >&2
    echo "Refusing to guess - inspect $ZSHRC manually." >&2
    exit 1
fi
END_LINE=$((START_LINE + EVAL_OFFSET - 1 + FI_OFFSET - 1))

NEW_BLOCK=$(cat <<EOF
$MARKER
SSH_ENV="\$HOME/.ssh/agent.env"
[ -f "\$SSH_ENV" ] && . "\$SSH_ENV" > /dev/null 2>&1
if ! ps -p "\${SSH_AGENT_PID:-0}" -o comm= 2>/dev/null | grep -q ssh-agent; then
    ssh-agent -s > "\$SSH_ENV"
    chmod 600 "\$SSH_ENV"
    . "\$SSH_ENV" > /dev/null
fi
EOF
)

echo "Found old guard at $ZSHRC:$START_LINE-$END_LINE:"
sed -n "${START_LINE},${END_LINE}p" "$ZSHRC"
echo
echo "Would replace with:"
echo "$NEW_BLOCK"
echo

ORPHAN_COUNT=$(pgrep -c ssh-agent 2>/dev/null || echo 0)
echo "Currently alive: $ORPHAN_COUNT ssh-agent process(es)."
echo "This script does not kill anything - clean them up yourself with:"
echo "  pkill -f ssh-agent"

if [ "$MODE" = "dry-run" ]; then
    echo
    echo "Dry run only - nothing was written. Re-run with --apply to make this change."
    exit 0
fi

TIMESTAMP=$(date -u +%Y%m%dT%H%M%SZ)
BACKUP="$HOME/.zshrc.bak.$TIMESTAMP"
cp "$ZSHRC" "$BACKUP"
echo "Backed up $ZSHRC to $BACKUP"

TMP_ZSHRC=$(mktemp)
{
    [ "$START_LINE" -gt 1 ] && sed -n "1,$((START_LINE - 1))p" "$ZSHRC"
    printf '%s\n' "$NEW_BLOCK"
    sed -n "$((END_LINE + 1)),\$p" "$ZSHRC"
} > "$TMP_ZSHRC"
mv "$TMP_ZSHRC" "$ZSHRC"

mkdir -p "$HOME/.ssh"
chmod 700 "$HOME/.ssh"

if ! zsh -n "$ZSHRC"; then
    echo "zsh -n $ZSHRC failed after the edit - restoring backup and aborting." >&2
    cp "$BACKUP" "$ZSHRC"
    exit 1
fi

echo "Applied. $ZSHRC now contains the fixed block; validated with 'zsh -n'."
