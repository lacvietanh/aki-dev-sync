# In-app terminal — feature

A real interactive shell inside the app, on a `TERMINAL` tab next to the event log, mirrored to any paired phone. Design and the decisions behind it: `docs/plan/1.20.0-terminal-and-remote-sync.md` §4. The original sketch it grew from: `docs/plan/remote-views-roadmap.md` § Terminal View.

> **Status:** built in 1.20.0. The first build was run on a Mac and hit three defects — a terminal that rendered tiny in a corner, a shell that froze silently when it exited, and no session controls at all; all three are fixed in the same 1.20.0 entry (it was never tagged, so it was not split into a follow-up version). Everything here now compiles and passes the unit suite on this Mac (`cargo check` clean, `cargo test --lib` 77/77); what remains open is live runtime behaviour — see "What is still unverified" at the bottom.

## Why it exists

Not "because VS Code has one". The app already opens `Terminal.app` windows for DEV, BUILD, SSH and the AkiClaudeDoc installer — and a phone on the other end of Remote Control can see none of them. The goal is **drive a real shell on the Mac from the phone**. Every scope decision below follows from that, not from feature parity with an editor.

It is a real PTY, not a piped command runner: that is what makes `Ctrl+C` on a runaway build, shell history recall, interactive `y/n` prompts and full-screen programs like `vim` work at all.

## How it behaves

| | |
| :-- | :-- |
| **Sessions** | Exactly **one**, shared. Typing on the Mac and typing on the phone both land in the same shell; both screens show identical output. Reaching *the terminal you are already using* was the point — a second shell with its own cwd and history would not have been. |
| **Echo** | Neither screen echoes a keystroke locally. The PTY is the single source of truth for what is displayed, exactly like every other piece of state in this app. |
| **Resize** | The **Mac is the sole resize authority.** A PTY has one `(cols, rows)`; if a phone could set it, a 40-column phone viewport would reshape the Mac's build output mid-stream. The phone fits its own rendering box locally, but its terminal is resized only by the authoritative size echoed from the host. |
| **Spawn** | Lazy and idempotent. The first screen to open the tab spawns the shell; every later call from either screen is a no-op. That is how "one shared PTY" is enforced without an ownership handshake. |
| **Scrollback** | A ~256KB ring buffer on the Mac, replayed to a device when it opens the tab or reconnects — so locking your phone mid-build does not cost you the context. |
| **Tab state** | Which panel tab you are on is **per-screen** and not mirrored: it is navigation, not data (SYNC-1). |
| **Shell** | A **login shell** (`$SHELL -l`, `TERM=xterm-256color`). Without `-l` the shell skips `.zprofile`/`.bash_profile`, so nvm/rbenv/`path_helper` never run and this terminal would have a different `PATH` from every `Terminal.app` window on the machine — a silent difference that costs an hour to diagnose the first time it bites. |
| **Death** | Real state, not an absence of state. When the shell exits — by `exit`, by crashing, by KILL — the host retires the session, prints `[process exited]` into the scrollback and emits `pty-exit`; every screen turns the tab header red and lights RESTART. Typing anything into a dead terminal respawns it. |

## Session controls

They live in the console panel header that already exists, not in a toolbar of their own (Extreme Narrow). All four work identically from the phone, since they route through the `invoke` seam.

| | |
| :-- | :-- |
| **CLEAR** | Wipes the **host's** ring buffer, not just the local screen, and broadcasts a reset. A purely local clear looks broken: the output comes straight back on the next reconnect, and the other screen never clears at all. |
| **RESTART** | Kill + wipe + fresh login shell, as one gesture. Safe to spam — see the generation counter below. |
| **KILL** | Ends the shell and leaves the terminal in the same exited state a voluntary `exit` produces. Nothing downstream special-cases it. |
| **OPEN** | Hands the shell's **current** directory (read via `lsof` — macOS has no `/proc`) to `Terminal.app`. The point is to hand off where you are standing, not where you started. Falls back to `$HOME` if the cwd cannot be read. |

Each project's OPEN popup also carries **In-App Terminal**, which opens this tab and `cd`s the shared shell into that project. It sits above the existing `Terminal.app` item because it is the only one of the two that does anything from a phone. The path is single-quote escaped (`'\''`), so a directory containing spaces, quotes or `$` cannot break out into command execution.

### Restart cannot orphan or clobber a session

Every spawn takes a generation number. A reader thread only retires the session slot if the session sitting in it is still the one it was reading. Without that, a shell killed by RESTART whose EOF arrives a moment late would null out the brand-new session that replaced it — leaving a terminal that is dead with no error anywhere and no way to tell why. This is why the button is safe to double-tap.

### Killing the shell means killing its process group

`portable_pty`'s `Child::kill()` signals only the direct child — the login shell. Everything the user started *inside* that shell is a separate process in the shell's process group and receives nothing from it. `kill_current` therefore sends SIGHUP to the whole group first (`killpg`, the same signal closing a real terminal window sends) and escalates to SIGKILL only for what survives the grace period. `portable-pty` puts the child in its own session on unix, so the child's pid is its process-group id and one `killpg` reaches every descendant.

The bigger half of the fix is *when* teardown runs: it is now also wired to `RunEvent::Exit` in `lib.rs`. Every other path into `kill_current` is a user gesture (KILL, RESTART), so before that hook existed, quitting the app ran no teardown whatsoever.

**How much `killpg` itself buys, stated honestly.** The unit test `killing_the_shell_takes_processes_started_inside_it_with_it` still passes with `killpg` swapped for a plain `kill` on the shell — verified by mutation, not assumed. That is a fact about unix rather than a weak test: when a session leader holding a controlling terminal dies, the kernel SIGHUPs the foreground process group on its own, so an ordinary foreground child dies either way. `killpg` covers what the kernel does not: a process that has left the foreground group, and any teardown path where the ctty is not revoked. Treat it as belt-and-braces on top of the exit hook, not as the load-bearing part.

This was written while investigating orphaned `ssh` clients (`ppid=1`) on the dev Mac that were holding remote `sshd` sessions — and with them several hundred MB each of `agy`/`claude` — alive on `akicloud`. **That investigation is not closed and this fix should not be recorded as its resolution.** The evidence does not fit the in-app terminal: the orphans' `stdout` is `/dev/null` while `stdin`/`stderr` are revoked devices, which is not the shape a shell hands a foreground `ssh`, and no code path in this app spawns a bare `ssh <host>` with no remote command. The real source is still unidentified.

## Phone input

Ordinary typing needs nothing special — xterm.js's hidden textarea turns the on-screen keyboard into real per-character input. A line-buffered "type a line, press send" box was rejected outright: it cannot interrupt a process, answer a prompt, or run a full-screen program.

One slim row of icon buttons covers what a phone keyboard cannot produce:

**Esc · Tab · Ctrl (sticky: tap Ctrl, then tap C) · ↑ ↓ ← → · Enter**

Deliberately not there: function keys, Alt/Meta, a configurable key row. None are load-bearing for driving a build or a dev shell.

## How the bytes move

```
[Mac] shell ──► PTY reader thread ──► ring buffer (scrollback)
                       │
                       ├─ emit('pty-output') ──────────► the Mac's own terminal (lowest latency)
                       └─ services/ptyBridge.js ──────► pty_output frame ──► phone
[phone] keystroke ──► pty_input frame ──► ptyBridge (Mac) ──┐
[Mac]   keystroke ──────────────────────────────────────────┴──► pty_write  (one authority)
```

Terminal bytes ride **four top-level frames on the existing socket** (`pty_input`, `pty_output`, `pty_resize`, `pty_exit`), never the state mirror or the intent registry. Raw output is a firehose; it does not fit a JSON-diffed state model. The first two names were reserved in `src/constants/protocol.js` a release ahead of time for exactly this.

Bytes are base64 end-to-end and are never treated as a Rust `String`. The frontend decodes to a `Uint8Array` and hands it to xterm.js, whose own stateful UTF-8 decoder reassembles a multi-byte character split across two PTY reads — so no app code has to buffer split sequences. Output is coalesced (about every 20ms, or 16KB, whichever comes first) so a chatty build does not become one network message per `read()`.

## Never-block-the-UI, and the one place the usual rule inverts

`pty_spawn` / `pty_write` / `pty_resize` / `pty_get_scrollback` are all `async fn` wrapping their work in `spawn_blocking`, per the ABSOLUTE rule in `CLAUDE.md`.

The **read loop is a dedicated `std::thread`, and must stay one.** `spawn_blocking`'s pool is sized for bounded one-shot work; parking one of its threads forever in a `read()` loop for the app's whole lifetime starves every other blocking command (remote path resolution, update check, git info) of a slot. A tokio task would be just as wrong — `portable-pty`'s reader is a synchronous `Read`, so it would block a worker identically. This is the one spot in the app where "just wrap it in `spawn_blocking`" is the bug rather than the fix.

## Security

Opening the terminal from a paired device adds **no extra confirmation step**, and that is a decision. A paired device could already invoke any command on the host, including the one that runs an arbitrary shell command for DEV and BUILD — so gating a PTY alone while that stays open would be theatre, and it would defeat the purpose, since the phone is used precisely when nobody is at the Mac. The pairing token remains the single gate and **Off** still cuts every live device instantly. This is the same posture 1.19.0 already declared, not a widening of it.

`pty_output` carries an optional `reset` flag meaning "replace everything, do not append" — set by CLEAR, RESTART and scrollback replay. `pty_exit` is a separate signal rather than something parsed out of the `[process exited]` text: driving state by pattern-matching our own cosmetic output would break the moment the wording changed.

## Not in this version

- Multiple tabs or sessions, and split panes.
- Font / theme / shell-profile configuration (the theme is hardcoded to the app's own CSS tokens).
- Search addon, web-links addon, ligatures.
- SSH-into-a-remote-host inside this view.
- **Redirecting DEV / BUILD / REPORT into it.** Those open a disposable, per-invocation window scoped to one project's directory, with real polish already invested (double-window avoidance, the 124-column top-right auto-snap). This terminal is one persistent general-purpose shell with no notion of "which project". Merging them would either drag multi-session + per-project cwd into the first version, or make build output fight your own typing in one PTY. `run_in_project_terminal` in `src-tauri/src/system.rs` is already the single funnel, so the redirect stays a one-function change when the groundwork exists. `pty_spawn` already accepts an optional `cwd` for that reason.

## What is still unverified

Compilation is no longer open: `cargo check` is clean and `cargo test --lib` passes 77/77 on this Mac, which also settled every `portable-pty` API shape the code had been written against from memory (`spawn_command`, `get_size`, `cwd`/`arg`/`env`, `kill`/`wait`/`process_id`) — the `VERIFY ON MAC` comment that used to head `src-tauri/src/pty.rs` has been replaced with a record of that result rather than left standing as a false caveat.

Still open: a `npm run build:rmud` linking `portable-pty` against `universal-apple-darwin` (the unit suite builds only the dev profile for this host), and all live behaviour — typing from each side, `Ctrl+C` via the sticky modifier, resize propagation, scrollback replay after a phone lock, and whether a real `npm run build`'s output volume stays inside the coalescing thresholds. Two things specifically want eyes because they are timing properties no unit test here covers: that a keystroke echoed less than 20ms after the previous flush now appears immediately (the flusher thread added in 1.20.0), and that quitting the app really does take an `ssh` session down with it.

Specific to the fix round: the terminal must fill the panel at first open **and** after collapsing and re-expanding the dock; `exit` must show `[process exited]` on both screens; RESTART must give a working prompt (and be safe to double-tap); CLEAR must leave the phone's screen empty too, and stay empty after the phone reconnects; OPEN must land `Terminal.app` in the directory you last `cd`'d to, which is the one path here that depends on `/usr/sbin/lsof` output format.
