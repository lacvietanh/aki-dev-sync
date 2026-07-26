# In-app terminal — feature

A real interactive shell inside the app, on a `TERMINAL` tab next to the event log, mirrored to any paired phone. Design and the decisions behind it: `docs/plan/done/1.20.0-terminal-and-remote-sync.md` §4. The original sketch it grew from: `docs/plan/remote-views-roadmap.md` § Terminal View.

> **Status:** built in 1.20.0, extended to multiple tabs (per-tab PTY sessions, tab strip, dock split into two collapsible stacks) in a later pass documented in the "Tabs" section below, then to scoped **terminal groups** (this doc's current shape) — see "Groups" below and `docs/arch/terminal-stack.md` for the architecture. The first build was run on a Mac and hit three defects — a terminal that rendered tiny in a corner, a shell that froze silently when it exited, and no session controls at all; all three are fixed in the same 1.20.0 entry (it was never tagged, so it was not split into a follow-up version). Everything here now compiles and passes the unit suite on this Mac (`cargo check` clean, `cargo test --lib` 77/77); what remains open is live runtime behaviour — see "What is still unverified" at the bottom.

## Why it exists

Not "because VS Code has one". The app already opens `Terminal.app` windows for DEV, BUILD, SSH and the AkiClaudeDoc installer — and a phone on the other end of Remote Control can see none of them. The goal is **drive a real shell on the Mac from the phone**. Every scope decision below follows from that, not from feature parity with an editor.

It is a real PTY, not a piped command runner: that is what makes `Ctrl+C` on a runaway build, shell history recall, interactive `y/n` prompts and full-screen programs like `vim` work at all.

## How it behaves

| | |
| :-- | :-- |
| **Sessions** | One PTY **per tab**, all shared across screens. Typing on the Mac and typing on the phone both land in the same shell for a given tab; both screens show identical output for that tab. Reaching *the terminal you are already using* was the point of the single-session design this grew from — tabs add more than one of those shared shells, not a private one per screen. |
| **Echo** | Neither screen echoes a keystroke locally. The PTY is the single source of truth for what is displayed, exactly like every other piece of state in this app. |
| **Resize** | The **Mac is the sole resize authority.** A PTY has one `(cols, rows)`; if a phone could set it, a 40-column phone viewport would reshape the Mac's build output mid-stream. The phone fits its own rendering box locally, but its terminal is resized only by the authoritative size echoed from the host. |
| **Spawn** | Lazy and idempotent. The first screen to open the tab spawns the shell; every later call from either screen is a no-op. That is how "one shared PTY" is enforced without an ownership handshake. |
| **Scrollback** | A 128KB ring buffer per tab on the Mac (1.21.1, down from 256KB — see "Tab and byte caps" below), replayed to a device when it opens the tab or reconnects — so locking your phone mid-build does not cost you the context. Nothing shrinks on the Mac itself: each mounted xterm keeps its own 5,000-line scrollback regardless: this ring only feeds a fresh mount, a companion join, or a congestion rehydrate. |
| **Tab state** | Which panel tab you are on is **per-screen** and not mirrored: it is navigation, not data (SYNC-1). |
| **Shell** | A **login shell** (`$SHELL -l`, `TERM=xterm-256color`). Without `-l` the shell skips `.zprofile`/`.bash_profile`, so nvm/rbenv/`path_helper` never run and this terminal would have a different `PATH` from every `Terminal.app` window on the machine — a silent difference that costs an hour to diagnose the first time it bites. |
| **Death** | Real state, not an absence of state. When the shell exits — by `exit`, by crashing, by KILL — the host retires the session, prints `[process exited. Press any key to start a new shell]` into the scrollback (1.21.1: the message no longer names a RESTART button, which was removed a release earlier) and emits `pty-exit`; every screen turns the tab header red. Typing anything into a dead terminal respawns it. |

## Groups

A **group** is a set of tabs that share one identity: either one project, or the **global** group
(not tied to any project). Two ways to enter one:

- A project row's `TERM` cell — switches the stack to that project's group and reuses its
  last-active tab, or opens a fresh one already `cd`'d into that project's directory if the group
  is currently empty.
- The `TERM` column **header** icon — the same entry point for the **global** group.

The stack header always shows which group you are in: a project icon (or a plain terminal glyph for
global) plus a 4-character name (`TERM` for global). The tab strip next to it is that group's tabs
only — chips from other groups are not shown, but they still exist and keep running; switching
groups never re-spawns or loses any shell's scrollback. `+` opens a new tab **in the current group**;
a chip's ✕ kills that one shell.

`⌘T` / `⌘W` / `⌘⇧[` / `⌘⇧]` all act **within the current group only** — ⌘T in a project's group opens
a shell already `cd`'d into that project, ⌘⇧[ / ⌘⇧] cycle only that group's tabs, and ⌘W closes only
the active tab of the group you are looking at.

**CLOSE** (the panel header's one right-side button, replacing the old chevron-only affordance)
hides the whole panel — every shell keeps running untouched, and (1.21.1) so does every mounted
terminal view itself: collapsing no longer disposes and re-spawns each xterm, it only hides them, so
scroll position and whatever a full-screen program (`vim`, a TUI) had painted survive a collapse and
re-expand instead of being reconstructed from the scrollback ring. Re-open it via the same button
(now reading EXPAND), any project's `TERM` cell, the header terminal icon, or the OPEN popup's
**In-App Terminal** item. The event log stack's own collapse is unchanged — it still hides by
unmounting.

**The `TERM` cell's two badges**, honestly labelled by what they can and cannot know:

- **Top (cyan, red if one has exited)** — how many in-app tabs exist in that project's group right
  now. Turns red the moment any one of them has exited; the count itself does not change (a dead
  shell is still a tab until you close it or a new command respawns it).
- **Bottom (slate)** — how many **external** `Terminal.app` windows/tabs are standing in that
  project's directory **right now**. A live count, not a tally: open a window and it rises, close
  that window and it falls back within a tick.

  *Mechanism.* The host runs `count_external_terminals` (`src-tauri/src/system.rs`) every **5 s**,
  plus once ~800 ms after the app itself opens a Terminal window so the badge moves immediately.
  One scan is three short local subprocesses: `pgrep -x Terminal` (absent → every count is 0), one
  `ps -axo pid=,ppid=` to walk Terminal's whole descendant tree, and one batched
  `lsof -a -d cwd -p <pids> -F pn` (capped at 200 pids) for their working directories. The counting
  rule is **roots of matching subtrees**: a process counts only if its cwd is the project directory
  *and its parent's cwd is not* — so one window running `npm run dev` (shell → npm → node, all
  sharing the cwd) counts once, not three times, without having to know which executables are
  shells. Match is **exact** in v1: a shell in `<project>/src` does not count.

  Host-only. The scan needs `Terminal.app`'s process tree, which exists only on the Mac; the
  companion never polls, it receives the snapshot (`externalTermCounts`) over the state mirror.

**Moved / removed vs. the old single-panel version:**

| Old | Now |
| :-- | :-- |
| KILL (active tab) | The tab chip's own ✕. |
| RESTART (active tab) | ✕ the tab, then `+` a new one — two clicks, same directory. |
| CLEAR (host scrollback) | Dropped from the UI entirely. A fresh tab starts with empty scrollback; `clear` inside the shell clears the visible screen. |
| OPEN (active shell → `Terminal.app`) | The OPEN popup's **Terminal** item, per project — the same funnel that pokes the external-window scan. A global-group shell with no project has no replacement for this. |
| The old external-terminal button on the `TERM` cell | Same OPEN popup item. |
| `< 2 tabs` plain "TERMINAL" / "TERMINAL - EXITED" title | Group identity (icon + name), always shown, plus each chip's own exited tint. |

## Tab and byte caps (1.21.1)

Two caps, checked in this order, and both reachable from a paired phone as well as the Mac:

- **Per group: 5 tabs.** The number a user is meant to have in their head — five shells is a working
  set, and wanting a sixth genuinely means closing one. Hitting it in a project's group shows *"This
  project already has 5 terminal tabs. Close one to open another."*; hitting it in the global group
  shows the same wording for *"The global group"*. The `TERM` cell's tooltip and the tab strip's `+`
  button both show the live count against this number, and the `+` dims (never hides) once the group
  is full.
- **Global ceiling: 16 tabs, across every group.** A resource guard, not a budget — it is never shown
  ahead of time, and the app is built so it should essentially never fire in normal use: it is derived
  from the per-group cap (`1 + 3 × 5`, the one tab the global group can never drop below, plus three
  full project groups) so that a *third* project's terminal never refuses on its very first tap. Hit
  it anyway and the message is *"All 16 terminal tabs are in use. Close one in any group first."* —
  the only refusal that says "in any group", because it is the only one whose cause can genuinely be
  sitting in a group the screen is not showing.

A refusal in one group never touches any other group's tabs. Opening a project's `TERM` cell that
turns out to be full also no longer strands you looking at that empty group: the screen returns to
whichever group you were in before the tap.

Raising the global ceiling costs real resources rather than being a UI preference: each live tab is a
shell process plus three raw OS threads and up to 128KB of scrollback ring buffer, so 16 tabs is
roughly 48 threads and 2MB of resident buffer at the absolute ceiling — see
`docs/arch/terminal-stack.md` for the full derivation, including why the per-tab scrollback ring was
halved (256KB to 128KB) alongside the phone's replay budget being raised, and why a phone joining with
every group full used to never fully catch up rather than simply disconnecting.

## In-App Terminal from the OPEN popup

Each project's OPEN popup carries **In-App Terminal**, which switches to that project's group and reuses/creates its tab exactly like clicking the `TERM` cell — it sits above the existing `Terminal.app` item because it is the only one of the two that does anything from a phone. The path is single-quote escaped (`'\''`), so a directory containing spaces, quotes or `$` cannot break out into command execution.

## Tabs

The dock is now two independently collapsible stacks — `TerminalStack` above `LogStack` (`src/components/DockStack.vue`) — rather than one panel with LOG/TERMINAL tabs. Each stack owns its own collapse ref and hands it to `DockStack`. Collapsing the log stack shrinks it to one live line (the latest log message); collapsing the terminal stack shows only its header. Each stack's collapse state is per-screen, matching every other dock decision above.

**Backend.** `src-tauri/src/pty.rs` keys every piece of session state by a `TabId` (`u32`): `sessions`, `inputs`, `scrollbacks` are each a `HashMap<TabId, _>` instead of a singleton, and `min_accepted` (the generation-fencing floor) is a **per-tab** map — a single global floor would let retiring one tab's generation fence off a still-live generation in another tab. Every command takes an `Option<u32> tab_id` defaulting to tab 0, which is the backward-compatibility seam: a companion running an older frontend build that never sends `tab_id` keeps landing on exactly the session it always drove. The one exception is `pty_close_tab`, whose `tab_id` is **required** — a defaultable "close" is exactly the accidental-blast-radius shape the multi-entity regression guard forbids. `pty_list_tabs()` returns `{id, alive}` per tab so a reloaded frontend can re-adopt shells the backend kept running. A `MAX_TABS = 16` global cap (1.21.1, up from 8) bounds the real resource cost — each live tab is a shell process plus three raw threads — and is derived from the frontend's per-group cap of 5; see "Tab and byte caps" above and `docs/arch/terminal-stack.md` for the full derivation.

**Wire format.** No new frame types — `pty_output` / `pty_input` / `pty_resize` / `pty_exit` all now carry `tab_id` (default 0). This matters because the relay (`web_server.rs`) coalesces `pty_output` frames under congestion by tag alone, content-blind — without `tab_id` on every frame, a coalesce could silently land tab A's bytes in tab B's xterm.

**Per-companion addressing (1.21.1).** A scrollback replay (a `pty_output` with `reset: true`) and an `invoke_result` are now addressed to the one companion they are for, via an optional `to` field the relay routes on and an optional `from` field the relay stamps on the way in — see `docs/feat/remote-control.md` for the wire-level detail. Before this, both were broadcast: a second phone joining wiped and rebuilt the screen of a phone already mid-command, and two phones with an overlapping `invoke` call in flight (each numbering requests from 1 on its own page) could resolve each other's replies. Live `pty_output`, `pty_exit`, `pty_resize`, `delta` and `init` all still broadcast, unchanged, because every screen genuinely needs the same bytes.

**Resync.** `pushScrollback()` became `pushAllScrollbacks(to)` in `src/services/ptyBridge.js`: on a fresh companion connect or a scheduled resync, it calls `pty_list_tabs()` then replays every tab's scrollback, not just tab 0's — the same congestion that forces a resync would otherwise leave every tab past the first silently un-replayed on the device that just reconnected. The companion-join path passes the joining device's id so only that phone receives the replay; the host's own congestion-recovery path still broadcasts, because that hole exists in every companion's byte stream at once and there is no single device to address it to.

**Frontend.** `usePtyTerminal(term, tabId)` takes a tab id and filters both its Tauri-event and companion-frame listeners by it (`if ((payload.tab_id ?? 0) !== tabId) return`) — one listener is wired per mounted `TerminalView`, and every one of them otherwise receives every tab's bytes. Liveness is now three-state (`'unknown' | true | false`): a fresh mount or a failed invoke sets `'unknown'`, never `false`, so a terminal never paints its header red before a real exit is confirmed — the "TERMINAL - EXITED" false-positive flash this replaced. `terminalTabsStore.js` holds the shared tab list (mirrored, since which tabs exist is genuinely cross-screen state); `useTerminalTabs.js` holds the per-screen liveness map and "has this tab ever been shown" bookkeeping locally, never mirrored, since each screen's PTY event stream is its own. Closing a tab (`closeTerminalTab`) splices exactly one entry and tells the backend to drop that one tab's session + scrollback (`drop_tab_state`) — every other tab's shell and scrollback survive untouched. The floor is now **scope-aware** (see "Groups" above): the global group never drops below one tab, but a project's group may empty out entirely — it simply stops existing until its `TERM` cell is clicked again.

**On-screen key row.** Now companion-only (`showKeyRow: !isHost` in `usePtyTerminal.js`) — the Mac has a real keyboard and never needs it; only a paired phone does.

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

**Esc · Tab · Shift (sticky) · Ctrl (sticky: tap Ctrl, then tap C) · ↑ ↓ ← → · Enter**

Sticky Shift (`armShift`/`shiftArmed` in `usePtyTerminal.js`, same armed styling as Ctrl) modifies the *next key-row button press*, not the next typed character — unlike Ctrl, which arms the next real keystroke via `term.onData`. Tab becomes `\x1b[Z` (backtab — Claude Code and other AI agents use Shift+Tab constantly for mode cycling); the arrows become CSI modifier-2 sequences (`\x1b[1;2A/B/C/D` for Up/Down/Right/Left). Enter/Esc and anything else pass through unaffected, and Shift disarms after any key-row press regardless of whether that key had a shift variant. Ctrl and Shift are independent: Ctrl still only ever affects a following typed letter, so "Ctrl wins for letters" is unchanged, and arming both before tapping Tab sends Shift+Tab.

Directly under the key row sits a compose input: a plain `<input type="text">` + a send button. Typing (voice dictation, the Telex IME) composes there instead of keystroke-at-a-time into xterm; Enter or the send button does `sendRaw(text + '\r')`, clears the field, and keeps focus in it so consecutive commands can be typed without re-tapping.

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

- Split panes (multiple tabs within one PTY-per-tab model are covered above — a split pane would be more than one *view* onto one tab, which is a different feature).
- Font / theme / shell-profile configuration (the theme is hardcoded to the app's own CSS tokens).
- Search addon, web-links addon, ligatures.
- SSH-into-a-remote-host inside this view.
- **Redirecting DEV / BUILD / REPORT into it.** Those open a disposable, per-invocation window scoped to one project's directory, with real polish already invested (double-window avoidance, the 124-column top-right auto-snap). This terminal is one persistent general-purpose shell with no notion of "which project". Merging them would either drag multi-session + per-project cwd into the first version, or make build output fight your own typing in one PTY. `run_in_project_terminal` in `src-tauri/src/system.rs` is already the single funnel, so the redirect stays a one-function change when the groundwork exists. `pty_spawn` already accepts an optional `cwd` for that reason.

## What is still unverified

Compilation is no longer open: `cargo check` is clean and `cargo test --lib` passes 77/77 on this Mac, which also settled every `portable-pty` API shape the code had been written against from memory (`spawn_command`, `get_size`, `cwd`/`arg`/`env`, `kill`/`wait`/`process_id`) — the `VERIFY ON MAC` comment that used to head `src-tauri/src/pty.rs` has been replaced with a record of that result rather than left standing as a false caveat.

Still open: a `npm run build:rmud` linking `portable-pty` against `universal-apple-darwin` (the unit suite builds only the dev profile for this host), and all live behaviour — typing from each side, `Ctrl+C` via the sticky modifier, resize propagation, scrollback replay after a phone lock, and whether a real `npm run build`'s output volume stays inside the coalescing thresholds. Two things specifically want eyes because they are timing properties no unit test here covers: that a keystroke echoed less than 20ms after the previous flush now appears immediately (the flusher thread added in 1.20.0), and that quitting the app really does take an `ssh` session down with it.

Specific to the fix round: the terminal must fill the panel at first open **and** after collapsing and re-expanding the dock; `exit` must show `[process exited]` on both screens; RESTART must give a working prompt (and be safe to double-tap); CLEAR must leave the phone's screen empty too, and stay empty after the phone reconnects; OPEN must land `Terminal.app` in the directory you last `cd`'d to, which is the one path here that depends on `/usr/sbin/lsof` output format.
