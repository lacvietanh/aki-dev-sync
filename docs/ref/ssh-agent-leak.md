# ssh-agent leak on the owner's Mac

Reference doc, not a code-change record — this is a machine-config problem, no app code changed.

## The mechanism

The owner's `~/.zshrc` auto-starts an `ssh-agent` behind a guard that only tests an environment variable:

```sh
if [ -z "$SSH_AGENT_PID" ]; then
    eval "$(ssh-agent -s)" > /dev/null 2>&1
fi
```

Every new interactive shell starts with a clean environment, so `$SSH_AGENT_PID` is always unset and the guard always passes — every new shell spawns a fresh agent. `ssh-agent` calls `setsid` and daemonizes into its own session, so it is never a child of the shell's process group: when the shell (or its terminal tab) dies, the kernel has nothing to reap the agent with. Over ~11 days this produced ~270 orphaned `ssh-agent` processes (~212MB RAM) on the owner's Mac.

## Why this app amplifies the rate

Aki Dev Sync spawns a fresh **login shell** at two sites, each of which sources `~/.zshrc` and therefore re-triggers the broken guard:

- `src-tauri/src/pty.rs:479-480` — every in-app terminal tab spawns `$SHELL -l` (a login shell, so `.zprofile`/`.zshrc` are sourced — see the comment there on why `-l` is required for PATH parity with `Terminal.app`).
- `src-tauri/src/system.rs:406-408` — every remote-IDE launch (e.g. Antigravity over SSH) runs `$SHELL -ilc "..."` (interactive + login, for the same PATH-sourcing reason).

Neither call is a bug: both need a login/interactive shell for correct PATH resolution, which is a documented, deliberate choice (`CLAUDE.md`'s cold-start PATH-race guidance). The leak is entirely a property of the owner's `~/.zshrc`, and any tool that opens a login shell — this app, a new Terminal.app tab, a script — triggers it identically. The app simply opens more shells per session than a human clicking "New Tab" would (one per terminal tab, one per remote-IDE launch), so it raises the rate at which the pre-existing leak accumulates.

## Why the app's existing PTY teardown does not and cannot reap these

The app already kills the process **group** of each shell it spawns:

- `pty.rs:543-554` (`kill_process_group`) — `killpg(pid, SIGHUP)` then `SIGKILL` after a grace period, reaching every descendant still in the shell's process group.
- `pty.rs:602-606` (`shutdown`, wired to `RunEvent::Exit`) — runs that teardown for every live tab when the app quits.

This cannot reach `ssh-agent` because `ssh-agent` daemonizes: `setsid` moves it into a **new** session with its own process-group id, distinct from the login shell's group. `killpg` on the shell's pgid never touches it, by construction — the same reason a real terminal window closing (a SIGHUP to the foreground group) also never kills an already-daemonized agent. This is a structural property of `ssh-agent`, not a gap in the app's teardown logic.

See also `docs/arch/terminal-stack.md:102` — that note documents a **different** orphan class (hand-typed `ssh` clients left running in closed `Terminal.app` tabs), traced and excluded from the in-app PTY's spawn shape before this investigation started. `ssh-agent` orphans are a separate mechanism (daemonization vs. a foreground child surviving its parent) and are not covered by, or caused by, that investigation.

## Fix

Machine-config fix, run once on the Mac — see `scripts/fix-ssh-agent-leak.sh` (dry-run by default, `--apply` to write). It replaces the PID-only guard with one that persists the agent's env vars to `~/.ssh/agent.env` and reuses a live agent across shells instead of starting a new one every time. No app code change is needed or intended.
