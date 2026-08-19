# In-app terminal — feature

> updated 2026-08-20 · v1.28.0

A real interactive shell inside the app, on a `TERMINAL` tab next to the event log, mirrored to any paired phone. Design and the decisions behind it: `docs/plan/done/1.20.0-terminal-and-remote-sync.md` §4. The original sketch it grew from: `docs/arch/remote-views-roadmap.md` § Terminal View.

> **Status:** built in 1.20.0, extended to multiple tabs (per-tab PTY sessions, tab strip, dock split into two collapsible stacks) in a later pass documented in the "Tabs" section below, then to scoped **terminal groups** (this doc's current shape) — see "Groups" below and `docs/arch/terminal-stack.md` for the architecture. The first build was run on a Mac and hit three defects — a terminal that rendered tiny in a corner, a shell that froze silently when it exited, and no session controls at all; all three are fixed in the same 1.20.0 entry (it was never tagged, so it was not split into a follow-up version). Everything here now compiles and passes the unit suite on this Mac (`cargo check` clean, `cargo test --lib` 77/77); what remains open is live runtime behaviour — see "What is still unverified" at the bottom.

## Why it exists

Not "because VS Code has one". The app opened `Terminal.app` windows for DEV, BUILD, SSH and the AkiClaudeDoc installer — and a phone on the other end of Remote Control could see none of them. The goal is **drive a real shell on the Mac from the phone**. Every scope decision below follows from that, not from feature parity with an editor. DEV/BUILD have since moved into this terminal (`docs/plan/done/dev-build-in-app-launch.md`) and are visible from a phone as of that change; SSH now has an in-app option too (see "SSH into a remote host" below), alongside its original external-window path. The AkiClaudeDoc installer still opens an external window.

It is a real PTY, not a piped command runner: that is what makes `Ctrl+C` on a runaway build, shell history recall, interactive `y/n` prompts and full-screen programs like `vim` work at all.

## How it behaves

| | |
| :-- | :-- |
| **Sessions** | One PTY **per tab**, all shared across screens. Typing on the Mac and typing on the phone both land in the same shell for a given tab; both screens show identical output for that tab. Reaching *the terminal you are already using* was the point of the single-session design this grew from — tabs add more than one of those shared shells, not a private one per screen. |
| **Echo** | Neither screen echoes a keystroke locally. The PTY is the single source of truth for what is displayed, exactly like every other piece of state in this app. |
| **Resize** | The **Mac is the DEFAULT and automatic resize authority** — its own window/dock resize keeps driving the shared grid unprompted, exactly as always. A PTY has one `(cols, rows)`. As of `docs/plan/done/wish-terminal-manual-resize-authority.md` (1.24), a companion may take explicit, temporary authority via one deliberate "Fit to my screen" tap — never automatically or in the background — and the Mac can always reclaim it with one tap of its own. What this invariant actually prevents — a companion silently or automatically resizing the shared PTY, e.g. reshaping the Mac's build output mid-stream — still holds without exception; only the literal "sole" has been narrowed to "sole automatic". |
| **Spawn** | Lazy and idempotent. The first screen to open the tab spawns the shell; every later call from either screen is a no-op. That is how "one shared PTY" is enforced without an ownership handshake. |
| **Scrollback** | A 128KB ring buffer per tab on the Mac (1.21.1, down from 256KB — see "Tab and byte caps" below), replayed to a device when it opens the tab or reconnects — so locking your phone mid-build does not cost you the context. Nothing shrinks on the Mac itself: each mounted xterm keeps its own 5,000-line scrollback regardless: this ring only feeds a fresh mount, a companion join, or a congestion rehydrate. |
| **Tab state** | Which panel tab you are on is **per-screen** and not mirrored: it is navigation, not data (SYNC-1). |
| **Shell** | A **login shell** (`$SHELL -l`, `TERM=xterm-256color`). Without `-l` the shell skips `.zprofile`/`.bash_profile`, so nvm/rbenv/`path_helper` never run and this terminal would have a different `PATH` from every `Terminal.app` window on the machine — a silent difference that costs an hour to diagnose the first time it bites. |
| **Death** | Real state, not an absence of state. When the shell exits — by `exit`, by crashing, by KILL — the host retires the session, prints `[process exited. Press any key to start a new shell]` into the scrollback (1.21.1: the message no longer names a RESTART button, which was removed a release earlier) and emits `pty-exit`; every screen turns the tab header red. Typing anything into a dead terminal respawns it. |

## Groups

A **group** is a set of tabs that share one identity: either one project, or the **global** group (not tied to any project). Two ways to enter one:

- A project row's `TERM` cell — switches the stack to that project's group and reuses its last-active tab, or opens a fresh one already `cd`'d into that project's directory if the group is currently empty.
- The `TERM` column **header** icon — the same entry point for the **global** group.

The stack header always shows which group you are in: a project icon (or a plain terminal glyph for global) plus a 4-character name (`TERM` for global). The tab strip next to it is that group's tabs only, plus any **pinned** tab from elsewhere (see below) — chips from other, unpinned groups are not shown, but they still exist and keep running; switching groups never re-spawns or loses any shell's scrollback. `+` opens a new tab **in the current group**; a chip's ✕ kills that one shell.

**Pinning (1.24).** Each chip carries a small pin toggle at its left edge. A pinned tab shows in *every* group's strip — project or global — sorted ahead of the unpinned tabs, and `pinned: boolean` rides the tab object (`terminalTabsStore.js`) over the normal mirror, so it stays pinned for a paired phone too. Pinning is display-only: it never changes a tab's owning `projectId`, so the per-group (5) and global (16) caps stay keyed off real ownership and cannot be shrunk or bypassed by pinning a tab into a foreign group's strip.

`⌘T` / `⌘W` / `⌘⇧[` / `⌘⇧]` all act **within the current group only** — ⌘T in a project's group opens a shell already `cd`'d into that project, ⌘⇧[ / ⌘⇧] cycle only that group's tabs, and ⌘W closes only the active tab of the group you are looking at.

**CLOSE** (the panel header's one right-side button, replacing the old chevron-only affordance) hides the whole panel — every shell keeps running untouched, and (1.21.1) so does every mounted terminal view itself: collapsing no longer disposes and re-spawns each xterm, it only hides them, so scroll position and whatever a full-screen program (`vim`, a TUI) had painted survive a collapse and re-expand instead of being reconstructed from the scrollback ring. Re-open it via the same button (now reading EXPAND), any project's `TERM` cell, the header terminal icon, or the OPEN popup's **In-App Terminal** item. The event log stack's own collapse is unchanged — it still hides by unmounting.

**Right-Dock Layout (1.25).** When the window width is ≥ 900px, the terminal stack moves out of the bottom dock into a dedicated right-side column taking 100% of remaining width (`flex: 1`), with the main project list capped at 440px. In this mode, the terminal is dedicated (collapse/maximize buttons are hidden) and the LogStack docks at the bottom of the main view column. When the window is < 900px, the layout returns to the bottom dock with independent collapse/maximize controls. Detailed specification and breakpoint impact matrix: [docs/feat/right-dock.md](right-dock.md).

**The `TERM` cell's two badges** (the header's global terminal button carries the same two, see "External `Terminal.app` sessions" below), honestly labelled by what they can and cannot know:

- **Top (cyan, red if one has exited)** — how many in-app tabs exist in that project's group right now. Turns red the moment any one of them has exited; the count itself does not change (a dead shell is still a tab until you close it or a new command respawns it).
- **Bottom (slate)** — how many **external** `Terminal.app` sessions are attributed to that project **right now**: by spawn origin if this app opened the window (even if its cwd has since moved elsewhere), otherwise by which directory it currently stands in. A live count, not a tally: open a window and it rises, close that window and it falls back within a tick.

  Mechanism (scan cadence, subprocess pipeline, subtree-root counting rule): `docs/arch/terminal-stack.md` § External `Terminal.app` count — derived, never remembered. Host-only; the companion never polls, it receives the snapshot (`externalTermCounts`) over the state mirror.

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

- **Per group: 5 tabs.** The number a user is meant to have in their head — five shells is a working set, and wanting a sixth genuinely means closing one. Hitting it in a project's group shows *"This project already has 5 terminal tabs. Close one to open another."*; hitting it in the global group shows the same wording for *"The global group"*. The `TERM` cell's tooltip and the tab strip's `+` button both show the live count against this number, and the `+` dims (never hides) once the group is full.
- **Global ceiling: 16 tabs, across every group.** A resource guard, not a budget — it is never shown ahead of time, and the app is built so it should essentially never fire in normal use. It happens to equal `1 + 3 × 5`, but that arithmetic stopped being load-bearing on 2026-07-28 when the global group's own permanent one-tab minimum was removed (see below) — it is simply a generous shared ceiling, not a guarantee that any particular number of full groups can always coexist. Hit it anyway and the message is *"All 16 terminal tabs are in use. Close one in any group first."* — the only refusal that says "in any group", because it is the only one whose cause can genuinely be sitting in a group the screen is not showing.

A refusal in one group never touches any other group's tabs. Opening a project's `TERM` cell that turns out to be full also no longer strands you looking at that empty group: the screen returns to whichever group you were in before the tap.

Tapping a project's `TERM` cell (or the header's global icon) again while an earlier tap for that same group is still waiting on the Mac to answer is now a no-op instead of opening another tab (1.22.0) — a companion tap gets nothing back until the Mac's reply mirrors over, so with no visible feedback, a second tap (or an impatient few) each used to open its own tab. Mechanism: `docs/arch/terminal-stack.md`'s "Companion add is fire-and-forget" section.

Raising the global ceiling costs real resources rather than being a UI preference: each live tab is a shell process plus three raw OS threads and up to 128 KiB of scrollback ring buffer, so 16 tabs is roughly 48 threads and 2 MiB of resident buffer at the absolute ceiling — see `docs/arch/terminal-stack.md` for the full derivation, including why the per-tab scrollback ring was halved (256 KiB to 128 KiB) alongside the phone's replay budget being raised, and why a phone joining with every group full used to never fully catch up rather than simply disconnecting.

## In-App Terminal from the OPEN popup

Each project's OPEN popup carries **In-App Terminal**, which switches to that project's group and reuses/creates its tab exactly like clicking the `TERM` cell — it sits above the existing `Terminal.app` item because it is the only one of the two that does anything from a phone. The path is single-quote escaped (`'\''`), so a directory containing spaces, quotes or `$` cannot break out into command execution.

## Tabs

The dock is now two independently collapsible stacks — `TerminalStack` above `LogStack` (`src/components/DockStack.vue`) — rather than one panel with LOG/TERMINAL tabs. Each stack owns its own collapse ref and hands it to `DockStack`. Collapsing the log stack shrinks it to one live line (the latest log message); collapsing the terminal stack shows only its header. Each stack's collapse state is per-screen, matching every other dock decision above.

Backend session storage, wire format, companion addressing, resync, and restart/kill process-group mechanics are backend architecture, not feature behaviour: `docs/arch/terminal-stack.md` §§ PTY backend contract, Wire format, Never-block-the-UI.

**Liveness.** Three-state (`'unknown' | true | false`): a fresh mount or a failed invoke sets `'unknown'`, never `false`, so a terminal never paints its header red before a real exit is confirmed. Closing a tab drops only that one tab's session and scrollback — every other tab survives untouched.

**On-screen key row.** Now companion-only (`showKeyRow: !isHost` in `usePtyTerminal.js`) — the Mac has a real keyboard and never needs it; only a paired phone does.

**Tab titles follow the shell, or rename by hand (2026-07-28).** A chip's title is no longer always "Shell" — xterm parses the shell's own OSC 0/2 title escapes (the same ones an external `Terminal.app` window's titlebar already shows) and retitles the chip automatically. Right-click a chip to rename it directly in place; a manual rename sticks and is never overwritten by the shell's own retitling afterward.

## SimpleView — the phone's plain-text view (1.23.0, withdrawn in 1.24.0)

For one release a paired phone rendered the PTY byte stream as a plain scrolling text stream instead of xterm.js's grid, so a narrow viewport could not mangle output meant for the Mac's wider terminal. Its accepted cost was that full-screen TUIs (`vim`, `htop`, `claude`/`agy`'s own UI) rendered garbled, which turned out to be most of what the phone was used for.
`useTerminalViewType.js` now returns `TerminalView.vue` on every screen; resize safety comes from the explicit, revocable authority handoff instead (`docs/plan/done/wish-terminal-manual-resize-authority.md`). `SimpleView.vue` stays in the tree, unreferenced, as the candidate for a future opt-in low-bandwidth mode — which is the only reason the seam still exists rather than `TerminalStack.vue` importing `TerminalView` directly.

## Phone input

Ordinary typing needs nothing special — xterm.js's hidden textarea turns the on-screen keyboard into real per-character input. A line-buffered "type a line, press send" box was rejected outright: it cannot interrupt a process, answer a prompt, or run a full-screen program.

One slim row of icon buttons covers what a phone keyboard cannot produce:

**Esc · Tab · Shift (sticky) · Ctrl (sticky: tap Ctrl, then tap C) · ↑ ↓ ← → · Enter**

Sticky Shift (`armShift`/`shiftArmed` in `usePtyTerminal.js`, same armed styling as Ctrl) modifies the *next key-row button press*, not the next typed character — unlike Ctrl, which arms the next real keystroke via `term.onData`. Tab becomes `\x1b[Z` (backtab — Claude Code and other AI agents use Shift+Tab constantly for mode cycling); the arrows become CSI modifier-2 sequences (`\x1b[1;2A/B/C/D` for Up/Down/Right/Left). Enter/Esc and anything else pass through unaffected, and Shift disarms after any key-row press regardless of whether that key had a shift variant. Ctrl and Shift are independent: Ctrl still only ever affects a following typed letter, so "Ctrl wins for letters" is unchanged, and arming both before tapping Tab sends Shift+Tab.

Directly under the key row sits a compose input: a plain `<input type="text">` + a send button. Typing (voice dictation, an IME) composes there instead of keystroke-at-a-time into xterm; Enter or the send button does `sendRaw(text + '\r')`, clears the field, and keeps focus in it so consecutive commands can be typed without re-tapping. Enter is ignored while `isComposing` (or `keyCode === 229`), so the Enter that COMMITS an IME syllable does not fire a half-finished line.

### Vietnamese input: xterm owns keys, the app owns text

**Architecture:** `docs/research/terminal-vietnamese-ime-root-cause-4.md` §7 (decided design) and `-5.md` (the double-space blocker that corrected one of that design's implementation details); summary in `docs/arch/terminal-stack.md` § Keyboard input.

Two earlier approaches are gone, replaced rather than layered: `useWkImeGuard.js` (1.22.0) intercepted xterm.js's internal event pipeline — a capture-phase guard that classified each keydown and vetoed xterm's own handlers, fragile because it depended on xterm internals (`_keyDownSeen`, `_keyPressHandled`, textarea-diff scheduling) that change across versions. It was replaced by an app-owned overlay `<textarea>` (`useTerminalInput.js`) with `disableStdin: true`, which took xterm out of the keyboard path entirely and re-implemented its key protocol from scratch — badly enough to regress six ways (arrows ignoring application cursor mode, F5 emitting PageUp, paste losing its bracketing, Option+word-motion sending nothing). Both files are deleted.

The current approach (`useTerminalTextDrain.js`) puts `disableStdin` back to `false` and lets xterm own every key exactly as it always has — arrows, modifiers, F-keys, bracketed paste all stay on xterm's own pipeline, untouched. It claims only text: a capture-phase `input` listener on xterm's own textarea (visible again, no longer hidden) reads the committed string, strips OpenKey's invisible sentinels, and sends it once to the PTY; `customKeyEventHandler` vetoes **every** `keypress` so xterm's own `_keyPress` never also sends the character. This is what makes the split exclusive by DOM construction rather than by classifying which keys need it, and it handles both OpenKey's multi-character carrier events ("gì" as one keydown) and true composing IMEs the same way.

The compose row remains for true composing IMEs (macOS's built-in Vietnamese input) where the no-local-echo screen the preedit is drawn against is the fundamental obstacle, not a WebKit bug — companion-only, since the Mac's own keyboard now types straight into the terminal.

**Diagnostics:** `window.__akiTermInput` — `status()` / `tail(40)` / `dump()` / `debug(on)` / `clear()` / `help()`.

**No escape hatch.** The `aki-input-mode='legacy'` A/B fallback to the overlay approach was removed 2026-07-31, once real-hardware testing confirmed the drain is what makes typing work — there is now exactly one input path to read.

The research chain remains the root-cause record, now four rounds deep:
- `docs/research/terminal-vietnamese-ime-root-cause-jul27.md` — original analysis (OpenKey is not an IME, WKWebView tags synthetic keys `keyCode 229`, xterm 5.5.0 IME-fallback paths drop/collapse/duplicate them — upstream #5887/#5894, unfixed; Chromium doesn't tag, which is the entire "VS Code works" difference). Superseded by later rounds; kept for the background.
- `docs/research/terminal-vietnamese-ime-root-cause-2.md` — guard v1 post-mortem and v2 delivery-shape matrix. Superseded by the drain architecture.
- `docs/research/terminal-vietnamese-ime-root-cause-4.md` — the 229 mechanism was inverted; decides the drain architecture. §7 point 1 (a narrow keypress veto) superseded by `-5.md`; everything else stands.
- `docs/research/terminal-vietnamese-ime-root-cause-5.md` — **head of the chain.** The jul31 double-space blocker: root cause traced to `_keyPress`'s unforced `cancel()` for space and uppercase A-Z, fix broadened the veto to every keypress, confirmed on real hardware.

### Copying text out (1.28.1)

`⌘C` copies the selection. It had never done so before 1.28.1 — not under a full-screen program, not at a plain shell prompt — because xterm's only copy route is a native browser `copy` event fed from a real DOM selection, and it builds one only on Linux (primary-selection emulation); in this app's webview the event therefore never had anything to fire on. The terminal now claims the key itself and writes through the same clipboard path as every COPY button in the app (`src/composables/useTerminalCopy.js`).

Two things follow from that, both deliberate:

- **`⌘C` copies the last text you selected in this tab**, not only a selection still highlighted right now. A program that tracks the mouse (`claude`, `agy`, anything full-screen) makes xterm drop the highlight almost immediately — the text is stashed the moment it is selected, so the key still copies what you meant. With nothing ever selected in that tab, `⌘C` does nothing at all, exactly as before.
- **Selecting text under such a program still needs `⌥`** held while dragging, unchanged: the program owns the mouse, and Option is xterm's escape hatch out of it (`macOptionClickForcesSelection`, 1.23). The app additionally swallows a redundant re-arm of the program's mouse mode, which is the most likely reason the highlight used to vanish on release — see `docs/research/terminal-copy-selection-root-cause.md`.

## Panel size and font zoom (1.22.0)

Two VS Code affordances, split by which surface has a keyboard:

- **Panel height** — **each stack has its own height and its own handle**: drag the 3px edge at the top of the terminal stack or of the event log to resize that one panel (`.dock-splitter` in `DockStack.vue`, pointer events + `setPointerCapture` so one path covers trackpad, mouse and touch); double-click it to reset that panel. Collapsing or expanding one panel never resizes the other — the dock's height is simply the sum of the two, so it grows and shrinks with them. Each panel is clamped to ≥10% of the window and the two together to 85%. **MAXIMIZE** in the terminal header goes to `calc(100vh - var(--titlebar-h))`, leaving the app header on screen, and is the explicit way past that ceiling. State: `src/composables/useDockLayout.js` — per-screen (a composable, not a `src/store/*.js` module, so `mirror.js` never discovers it: a phone dragging its dock must not resize the Mac's layout). The dragged heights persist to `localStorage`; **MAXIMIZE deliberately does not**, because a dock restored maximised on launch hides the project table and reads as broken rather than configured. A collapsed panel shows no handle (its length is a fixed header row, so the drag would be inert), and MAXIMIZE hides itself when both are collapsed. Model and the coupling it replaced: `docs/arch/terminal-stack.md` § The dock is the sum of its stacks.
- **Font size** — `⌘+` / `⌘-` / `⌘0` while focus is inside the terminal (`dock/TerminalStack.vue`'s existing keydown handler, keyed on `e.code` so `⌘⇧=` and the numeric keypad both work). On a phone the same three appear as buttons **inside the key row**, which already renders only where there is no physical keyboard — so "buttons on the browser, shortcuts on the Mac" needs no second condition and costs the Mac window no pixels.

State is a **scale**, not a pixel size (`src/composables/useTerminalFont.js`), because the two surfaces still disagree about the GRID: whichever screen currently holds resize authority (the Mac by default, or a companion mid-claim — see the "Resize" row above) has its size authoritative and cols/rows follow it (zoom changes cols/rows, exactly like VS Code); every other screen's terminal is resized only by the size it receives, never derived from its own container. It is per-device and deliberately not mirrored — how big text should be is a fact about the screen you are looking at, not the project.

### Terminal font size — native, not fitted to the grid (1.22.0)

Before this, a companion's font size was *derived*: `TerminalView.vue`'s `scaleFontToFit` measured the host's cols × rows grid against the phone's own viewport and picked whatever size made that grid fill the screen, then multiplied the result by `terminalFontScale`. At the same 100% zoom, that meant the Mac and a companion never actually agreed on text size — the companion's "100%" was however big its screen happened to make the shared grid, not the same 12px base the Mac renders.

`scaleFontToFit` is removed. Both surfaces now compute font size identically: `BASE_FONT_SIZE (12px) × terminalFontScale`, clamped to `[4, 18]px` scaled by the zoom factor. `terminalFontScale` was already per-device (localStorage, never mirrored — see above), so this was already local state; the fix is that it is no longer *derived from* the other screen's grid or viewport in the first place. `⌘+`/`⌘-`/`⌘0` on the Mac and the phone's zoom buttons each move only their own device's scale, and 100% now means the same rendered size on both.

## Terminal chrome visibility (1.25)

A 3-dot drop-up in the terminal stack header (`TerminalChromeMenu.vue`) lets each device hide or show individual pieces of terminal chrome — a per-device preference, not mirrored, following the same reasoning as font zoom and panel height above. `src/composables/useTerminalChrome.js`'s `CONTROLS` list is the single place a toggleable control is named; the stack and `TerminalView.vue` read only the resulting `chromeVisible` map, never `isHost` directly (the capability pattern below).

**Effective visibility is capability × preference**: a control the current screen cannot support (e.g. the key row on the Mac) is never offered as a checkbox, and preference can never widen capability. One control is checked-and-locked rather than offered: a companion's tab strip, since a phone has no other way to open, close or switch tabs.

**Host and companion get different defaults.** On the Mac, everything defaults off except the compose input (kept on — it is still the only working path for macOS's built-in Vietnamese composing IME, since the double-space fix above did not close that gap) and the tab strip lock; on a companion, every available control defaults on. Preferences persist to `localStorage` under `aki-terminal-chrome`, sparse (only explicitly-changed controls are written, so a control added later picks up its role default with no migration) and merged over the role default at read time. A "Show all" row appears once at least one available control is hidden. Design and the acceptance criteria: `docs/plan/done/terminal-chrome-settings.md`.

**One behavioural change from before this landed:** the compose input row used to be tied to the same `showKeyRow` gate as the key row (both companion-only, both-or-neither); it is now independently toggleable and, on the Mac, defaults on where it previously did not render at all.

## External `Terminal.app` sessions (1.22.0)

A button in the stack header opens a modal listing every external `Terminal.app` window/tab: the directory it stands in, its pid/tty, how long it has run, and **what is executing inside it** — which is the actual question ("which window has the dev server?"). It cannot show their screens; no app can read another application's window contents, and the MVP does not pretend otherwise. The button's own icon is `fa-terminal` boxed in a rounded outline (distinct from the bare glyph the in-app tabs use elsewhere), and it now carries a badge of its own: the sum of every project's external count (`externalTermCounts`) plus the unowned complement (`externalTermGlobalCount`, see below) — the total external session count, not just what one project's badge already showed.

Each row's heading now names how the session was attributed: **"launched from X"** for a session tagged at launch (spawn-origin, authoritative — an SSH session opened from a project's popup reads this way even though its cwd is the local `$HOME`), **"in X's folder"** for one merely adopted by matching cwd (today's pre-existing rule), or the bare directory name for neither. Mechanism: `docs/arch/terminal-stack.md` § Spawn-origin ownership.

`describe_terminal_sessions` (`src-tauri/src/system.rs`, renamed from `list_external_terminals`) shares `scan_terminal_tree` with the badge's `list_terminal_sessions` — same `pgrep`/`ps`/`lsof` pipeline, same subtree-root definition of "one session", so the modal can never list a different number than the badge above it claims. On demand only, never on the badge's 5s cadence: it returns a command line per process, which has no business being polled. Host-only, and deliberately absent from `COMPANION_ALLOWED_COMMANDS` — the scan reads the Mac's process table, so on a phone the button would only ever open an error; it hides itself there instead.

**The global terminal button (the `TERM` column header icon) now carries both badges too (2026-07-28)** — previously silent on both, unlike every per-project button. Top (cyan) is the global group's own in-app tab count, same rule as a project's. Bottom (slate) is `globalCount` (`src/utils/terminalOwnership.js`, fed by `list_terminal_sessions`), a live count of external `Terminal.app` sessions attributed to **none** of the listed projects — the complement of every project's own bottom badge, computed as one pass over the same scan (a tagged session matches its owner's project id first; everything else falls back to `unowned = all subtree roots − roots matching any listed project`'s cwd rule), so a session can never be silently missing from both a project's badge and the global one, nor double-subtracted if two projects share a directory. **Since 2026-08-16 this honours spawn origin, not just cwd**: a window that `cd`s away from a project after opening, or an SSH session opened from a project's popup, is now attributed back to that project **on the badge**, via the same `owner` tag the sessions modal's "launched from X" label already read (`docs/plan/done/terminal-ownership-model.md` §3/S1-S6, mechanism: `docs/arch/terminal-stack.md` § Spawn-origin ownership). A session this app never opened still falls back to the directory-matching (adoption) rule, exactly as before.

Deliberately not there: function keys, Alt/Meta, a configurable key row. None are load-bearing for driving a build or a dev shell.

## SSH into a remote host (in-app)

The OPEN popup's REMOTE column now has **SSH Terminal (In-App)** above the original **SSH Terminal** (native `Terminal.app`), same ordering as LOCAL's In-App Terminal over its own native Terminal item, and for the same reason: it is the only one of the two that works from a phone. `build_remote_ssh_command` (`src-tauri/src/system.rs`) builds the exact `ssh <host> -t '...'` string the native item already launches in `Terminal.app` — same host validation, same `mkdir -p && cd` remote-side quoting — and hands it back as plain text; `openProjectRemoteTerminal` (`ProjectTable.vue`) then types that string into a fresh in-app PTY tab via `openProjectRemoteTerminal` (`useTerminalTabs.js`). Unlike DEV/BUILD, which still dedup by their own `runKind` and reuse an existing tab, SSH no longer dedups — every invocation opens a new tab, subject to the same per-scope (5) and global (16) tab caps as any other tab. Pure string construction, no subprocess, so it is companion-allowed (`COMPANION_ALLOWED_COMMANDS`).

## Auto-collapse when the last tab closes

Closing every tab everywhere — via a tab's own ✕, `⌘W` on the last one, or any other close path — now collapses the terminal stack itself instead of leaving an open panel over an empty mount area (`TerminalStack.vue` watches `tabs.value.length`). The collapse (and the reverse expand, from the `TERM` cell, the header icon, or the OPEN popup) eases via a `flex-grow`/`flex-basis` transition on `.dock-stack` (`main.css`) instead of snapping instantly — `flex: none`'s implicit `auto` basis can't be interpolated, so every endpoint is a literal length, the collapsed one matching the header's own rendered height. The rule covers both stacks, so the event log panel eases the same way.

## Security

Opening the terminal from a paired device adds **no extra confirmation step**, and that is a decision. A paired device could already invoke any command on the host, including the one that runs an arbitrary shell command for DEV and BUILD — so gating a PTY alone while that stays open would be theatre, and it would defeat the purpose, since the phone is used precisely when nobody is at the Mac. The pairing token remains the single gate and **Off** still cuts every live device instantly. This is the same posture 1.19.0 already declared, not a widening of it.

## Not in this version

- Split panes (multiple tabs within one PTY-per-tab model are covered above — a split pane would be more than one *view* onto one tab, which is a different feature).
- Theme / shell-profile configuration (the theme is hardcoded to the app's own CSS tokens). Font *size* is adjustable since 1.22.0; the font *family* is not.
- Search addon, web-links addon, ligatures.
- **REPORT** opens `REPORT.html` in the OS default browser (`resolve_report_html` in `src-tauri/src/system.rs`) — it is a static file view, not a shell, and was never a terminal-redirect candidate; grouping it with DEV/BUILD in an earlier version of this line was itself the mistake.

**DEV / BUILD now redirect into this terminal (`docs/plan/done/dev-build-in-app-launch.md`).** The two reasons this line used to give are both gone: terminal v2's SCOPES give every project its own tab group (the "no notion of which project" gap closed 2026-07-28, well before this shipped), and build/dev output fighting the user's own typing is avoided structurally — DEV/BUILD never reuse the scope's last-active shell, they open a dedicated tab tagged `runKind: 'dev' | 'build'` and dedup against THAT tag, not against "any tab in the scope". `ProjectTable.vue`'s DEV/BUILD buttons call `openRunCommand` (`useTerminalTabs.js`), which writes the command via the same `pty_write` a keystroke would, exactly once — for a fresh tab, only after both `pty_spawn` resolves and the tab's scrollback hydrate has taken its snapshot, so the write's echo can only ever arrive through the live output stream, never doubled up with the hydrate's own replay of the same ring-buffer bytes. `run_project_command`/`run_project_dev`/`run_in_project_terminal` (`src-tauri/src/system.rs`), the old external-`Terminal.app` launch path this replaced, have since been removed (2026-07-30) along with their `hostInvoke.js` allowlist entries and `lib.rs` registrations.

## What is still unverified

Compilation is no longer open: `cargo check` is clean and `cargo test --lib` passes 77/77 on this Mac, which also settled every `portable-pty` API shape the code had been written against from memory (`spawn_command`, `get_size`, `cwd`/`arg`/`env`, `kill`/`wait`/`process_id`) — the `VERIFY ON MAC` comment that used to head `src-tauri/src/pty.rs` has been replaced with a record of that result rather than left standing as a false caveat.

Still open: a `npm run build:rmud` linking `portable-pty` against `universal-apple-darwin` (the unit suite builds only the dev profile for this host), and all live behaviour — typing from each side, `Ctrl+C` via the sticky modifier, resize propagation, scrollback replay after a phone lock, and whether a real `npm run build`'s output volume stays inside the coalescing thresholds. Two things specifically want eyes because they are timing properties no unit test here covers: that a keystroke echoed less than 20ms after the previous flush now appears immediately (the flusher thread added in 1.20.0), and that quitting the app really does take an `ssh` session down with it.

Specific to the fix round: the terminal must fill the panel at first open **and** after collapsing and re-expanding the dock; `exit` must show `[process exited]` on both screens; RESTART must give a working prompt (and be safe to double-tap); CLEAR must leave the phone's screen empty too, and stay empty after the phone reconnects; OPEN must land `Terminal.app` in the directory you last `cd`'d to, which is the one path here that depends on `/usr/sbin/lsof` output format.

Specific to copy (1.28.1, `docs/plan/terminal-copy-selection.md` §6): `⌘C` copying a selection at a plain prompt and under `claude` over SSH, whether the highlight now survives the mouse release, and `⌘⇧]` cycling tabs three times in a row — none of it is checkable off the Mac, so all of it is open.

Specific to keyboard input, largely resolved by the 2026-07-31 hardware run (`docs/research/terminal-input-jul31.md`, `docs/research/terminal-vietnamese-ime-root-cause-5.md`): direct Vietnamese typing with OpenKey, plain fast English typing, arrows/Ctrl+C/Ctrl+D/F-keys, Shift+Enter in the compose box, and the sticky Ctrl/Shift latch are all confirmed on real hardware, both on the Mac and over a remote Chrome/Android session, with no double space (the blocker that stood in the way of confirming any of this — root cause and fix in `-5.md`). Still open: Option+arrow-key combinations (deliberately not exercised yet — do not read this as PASS) and the Android/Gboard defect where a corrected character arrives alongside the original it replaced (`docs/plan/done/terminal-input-jul31.md` §2.2, also tracked in `docs/plan/remaining-1.22.md`), tracked separately and not yet root-caused. Also verify: the splitter tracks the pointer without the dock oscillating, and that the terminal re-fits to each new height; that `⌘+`/`⌘-` change cols/rows on the Mac and that the phone's zoom buttons change its own font size independently, with no grid re-fit or oscillation; and that `describe_terminal_sessions` (renamed from `list_external_terminals`) parses real `ps -axo pid=,ppid=,tty=,etime=,command=` output (the column walk is unit-tested against synthetic lines, not against the real `ps`).
