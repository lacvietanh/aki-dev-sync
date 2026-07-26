# Terminal cap — UX audit

> **Status: reconciled finding-by-finding in `docs/plan/1.21.1-terminal-limits-and-structure.md` §2.6.**
> Accepted and shipped: S1 (raise the global ceiling to 16), S2 (restore the prior scope on a capped
> `openScopeTerminal`), S3 (the exit notice's dead RESTART reference), M1 (both refusals reachable
> from a companion, including the pending-claim TTL actually speaking), M2 (the per-group count in the
> two existing tooltips), M3 (three distinct refusal strings). Deferred, with its own reasoning: M4
> (tinting the global group's `TERM` column header on a dead shell) — see the plan's §6.3. Left
> alone, per this doc's own rule that `research/` content should not be edited to match new code: the
> findings and numbers below describe the state *before* 1.21.1 and are not updated to the new ones.
>
> Separately, worth raising with the doc's owner rather than acting on unilaterally: by `RULE-docs`
> A2 this is an audit — an event record of a point-in-time investigation — which belongs in
> `docs/research/`, not `docs/feat/` (which holds only current/target state). Not moved as part of
> this pass; flagging only.

An audit of the in-app terminal's tab cap as an *experience*, ahead of the change from one global cap of 8 to **5 per project** plus a global ceiling. Behaviour today: `docs/feat/in-app-terminal.md`. Architecture: `docs/arch/terminal-stack.md`.

**Conclusion: the flow does need a spec, and it is a small one.** Nothing here asks for a new component or a new DOM element; every fix is a number, a string, or a colour on an element that already exists. But two of the findings are dead ends rather than polish, and one of them is arithmetic: with `MAX_TABS = 8` unchanged, "5 per project" is a promise the app cannot keep past the second project.

Verified against the code as it stands. Anything that needs a running Mac and a real phone is in "Unverified" at the bottom and is not counted as a finding.

---

## Severe

### S1. With the ceiling left at 8, the per-project cap is a rule the app breaks immediately

`MAX_TABS = 8` is global across every group (`src/store/terminalTabsStore.js`, mirrored by comment into `src-tauri/src/pty.rs`), and the global group can never drop below one tab. That leaves 7 for projects. A per-project cap of 5 therefore describes a budget that two projects cannot both have: 5 tabs in project A plus the mandatory global tab leaves 2 for everything else, and project B's `TERM` cell refuses on the **first** tap, when B's group is empty.

That is the worst possible shape for a refusal. The user has never opened a terminal for this project, sees a `TERM` cell with no badge on it, taps it, and is told no. Nothing on screen explains why, because the cause is five tabs sitting in a different project's group that this screen is not showing.

**Fix (decision, not copy).** Pick the ceiling from the per-project number, not independently. If 5 per project is the rule, the ceiling has to be high enough that a second and third project can both hold a working set: raise `MAX_TABS` to **16** in both `terminalTabsStore.js` and `pty.rs`. The resource argument still holds at 16 (three raw threads plus up to 256KB of ring buffer per live tab is roughly 48 threads and 4MB at the absolute ceiling, which is not the constraint the number was protecting). The ceiling then becomes what it is honestly for — a guard against a runaway, not a budget the user is expected to manage.

If the ceiling stays at 8, then the per-project cap is not 5, it is "5 or fewer depending on what else is open", and the copy must say so. That is a worse product; recommend against.

**Behavioural signal:** a user opening a terminal for a third project never sees a refusal Toast.

### S2. A refused tap leaves the user standing in an empty group they did not ask to be in

`openScopeTerminal` (`src/composables/useTerminalTabs.js`) sets `activeTerminalScope.value = scope` **before** `capReached()` runs, and `expandTerminalStack()` runs before that. So a refused `TERM`-cell tap expands the dock, switches the visible group to the project that just refused, and shows a tab strip containing nothing but a `+` that will refuse too. Every other group's tabs still exist but none of them are visible from where the user now is. The only route out is the `TERM` column behind the dock the app just expanded.

**Fix.** On a refusal, restore the scope the screen was on. Capture `activeTerminalScope.value` at the top of `openScopeTerminal` and write it back in the `if (capReached()) return` branch. Zero DOM, one local variable. The user stays where they were, with a Toast that names the problem.

**Behavioural signal:** after a refusal the tab strip still shows tabs.

### S3. The `[process exited]` notice points at a button that no longer exists

`src-tauri/src/pty.rs`'s `EXIT_NOTICE` reads:

```
[process exited - press any key or click RESTART to start a new shell]
```

RESTART was removed from the UI in the same pass that introduced groups (`docs/feat/in-app-terminal.md`'s migration table). The one place a user is guaranteed to be looking when a shell dies now tells them to click something that is not on screen. That is worse than saying nothing: it makes the user hunt for a control, fail, and conclude the terminal is stuck.

**Fix — the exact string:**

```
[process exited. Press any key to start a new shell]
```

Problem first, then the one next action (`RULE-content-write.md` B1), and no reference to anything that is not on screen. This is also the entire fix for "is respawn-on-typing discoverable" — the notice is in-band, in the scrollback, on both screens, and costs no DOM at all.

---

## Material

### M1. Both refusal reasons must be checked on both surfaces, or the phone gets a silent tap

`capReached()` in `useTerminalTabs.js` exists specifically because a companion's `addTerminalTab` never runs its own body — `action()` replaces it with an RPC stub, so the store's Toast fires **on the Mac** and the phone learns nothing. The pre-check on the companion is the only thing that makes the refusal visible on a phone.

That pre-check currently tests one condition. After the change there are two, and if only the new per-project one is replicated companion-side, every global-ceiling refusal becomes host-only and the phone's tap silently does nothing for 15 seconds and then nothing at all (`pendingClaimLive`'s TTL expires without saying anything). Both checks go in the same function, in the same order the host applies them, so the phone and the Mac refuse for the same reason with the same words.

There is a residual race even then: the mirrored tab list is one round-trip stale, so the host can refuse an add the companion's pre-check let through. Today that path ends in silence.

**Fix.** Two parts. (a) `capReached()` checks the per-project count *and* the global length, and returns which one fired so the caller can pick the message. (b) When a pending claim expires without a tab arriving, say so instead of clearing it quietly — the expiry point in `pendingClaimLive()` already runs at exactly the right moment.

**Copy for the TTL expiry:**

```
No terminal tab opened on the Mac. It may have reached a terminal limit.
```

Hedged because it is honestly a guess (the refusal reason never crossed the wire). Better than the current silence, and it is not a substitute for (a) — it is the backstop for the race only.

### M2. Neither number is visible before it is hit, and there are two places it costs nothing to show

Recognition over recall (`METHOD-ux-psych.md` A2): a cap the user can only discover by hitting it is a cap they will hit repeatedly. Two elements already on screen can carry it inside their existing `title` attributes, adding no element and no row.

**`TERM` cell** (`src/components/TerminalCell.vue`, `cellTitle`) — the first line changes from `In-app terminal — N tab(s) in this group`:

```
In-app terminal, 3 of 5 tabs in this group
```

At the per-project cap, the same line reads:

```
In-app terminal, 5 of 5 tabs in this group. Close one to open another.
```

The other lines it composes (external windows, "a shell in this group has exited") are unchanged and still only appear when non-zero.

**The `+` chip** (`src/components/TerminalTabStrip.vue`, `term-tab-add`) — today `New terminal tab in this group (⌘T)`:

```
New terminal tab in this group, 3 of 5 (⌘T)
```

and at the cap:

```
This group is full, 5 of 5. Close a tab to open another.
```

At the cap, the `+` chip also drops to the same muted treatment it already uses at rest and takes `cursor: not-allowed` — a state change on an existing element, per the Extreme Narrow rule. Do **not** hide it: a hidden `+` reads as a bug, a dimmed one reads as a limit.

The global ceiling is deliberately **not** shown anywhere ahead of time. See the disagreement section.

### M3. The two refusal messages

Current single string (`TAB_LIMIT_MESSAGE`): `Terminal tab limit reached (8) — close a tab in any group first`. It becomes two, because there are two different problems with two different next actions. Both state the problem first, then the action, and neither uses an em dash (`RULE-content-write.md` B2).

**Per-project cap:**

```
This project already has 5 terminal tabs. Close one to open another.
```

**Global ceiling:**

```
All 16 terminal tabs are in use. Close one in any group first.
```

"in any group" is load-bearing here and only here: the global ceiling is the one refusal whose cause genuinely lives somewhere the user cannot see, and the `TERM` column's cyan count badges are where they can go find it. Both numbers interpolate from their constants, never hardcoded in the string.

Keep both strings exported from `terminalTabsStore.js` as today — one wording, two checkers (the store's own and the companion pre-check) — so the phone and the Mac cannot drift apart.

### M4. A dead shell in the *global* group is invisible from anywhere else

A project's dead shell tints that project's `TERM` cell badge red (`TerminalCell.vue`'s `hasExited`), which is discoverable from the table even with the dock closed. The global group has no such surface: the `TERM` column **header** button (`src/components/ProjectTable.vue`, `.th-term-btn`) is a plain terminal glyph with no badge and no state. A global shell that died while you were in a project group announces itself nowhere.

**Fix.** Tint `.th-term-btn`'s icon with `--accent-red` when any tab with `projectId == null` has `tabAlive === false`, and add one line to its existing `title`: `A shell in the global group has exited`. Colour plus tooltip on an element that already exists — the same mechanism the project cells already use, which is also what keeps the two readings consistent (`METHOD-ux-psych.md` A6).

---

## Trivial, or deliberately left alone

- **`⌘T` has no visible affordance and only fires while focus is inside the terminal** (`hasTerminalFocus` gate in `dock/TerminalStack.vue`). Verified, and correct as designed: a global `⌘T` would fight the rest of the app, and the `+` chip's tooltip already names the shortcut for anyone who has the strip open. No change.
- **`⌘W` on the last global tab** already says `The last global terminal tab stays open` instead of doing nothing. Fine as is.
- **Losing track of which group you are in.** The stack header always shows the group icon plus a 4-character name and the strip is scope-filtered, so the current group is stated, not remembered. No finding.

---

## The disagreement: do not show the global ceiling before it is hit

The obvious "complete" design is to surface both numbers so the user can always see where they stand. I think that is wrong and would argue against it if it is proposed.

The per-project cap is a **rule about this project** — five shells is a working set, and a user who wants a sixth genuinely should close one. It belongs in the user's model and it belongs in a tooltip. The global ceiling is a **resource guard about the machine**; it exists to bound OS threads, and it maps onto nothing the user is thinking about. Putting "16 total" in front of them turns a guard into a budget they now believe they have to manage across projects, and invites exactly the wrong behaviour: closing a useful shell in project A to make room in project B, when the right answer was that the ceiling should have been higher.

So the ceiling stays invisible until it fires, and the correct response to it firing often is S1's fix (raise it), not better signage. A limit the user has to be taught about is a limit that was set wrong.

The corollary is a constraint on the copy, not a licence: when the ceiling *does* fire, its message must not pretend to be the per-project rule. That is why M3 is two strings and not one parameterised one.

---

## Collapse: hide-not-unmount, from the user's side

Today `DockStack.vue`'s default slot is `v-if`, so collapsing the terminal stack unmounts every `TerminalView` and expanding re-mounts and re-hydrates them from the host's 256KB-per-tab ring buffer (`SCROLLBACK_CAP` in `pty.rs`). The change to `v-show` makes the panel purely a visibility toggle.

What is verifiable from the code:

- **Nothing about the shell changes either way.** The PTY, its process group and its scrollback live on the Mac, independent of any mount. The CLOSE button's existing tooltip, `Close panel (shells keep running)`, is already true today and stays true.
- **A half-typed command is held by the shell's own line editor**, not by xterm, so it survives both designs on the Mac. What differs is whether the *display* of it survives, and that is the same question as scrollback replay below.
- **Scroll position is xterm-local and is therefore lost today** on every collapse and restored by the change. This is the difference a user will actually notice: today, closing the panel to read the table and reopening it drops you at the bottom of the buffer; after the change you are exactly where you left off.
- **Replay is byte replay into a fresh xterm, and the ring is capped and trimmed from the front** (`append_scrollback` drops the excess). Any escape sequence older than the trim point is gone, which is why a full-screen program's state after a collapse is a real risk today rather than a theoretical one. The change removes the risk entirely by never replaying.

So the change is a strict improvement with no state the user could be surprised to *lose*. The one thing that could surprise them in the other direction: a `vim` session left open across a collapse now genuinely still being there, painted, when the panel comes back. That is the good surprise, and it needs no announcement.

The cost is the one this doc should name plainly: N xterm instances stay mounted and rendering-capable while the panel is closed. With the ceiling at 16 (S1) that is 16 mounted terminals instead of 8. Worth a look at idle CPU with a chatty shell running behind a closed panel.

---

## Failure paths, walked

| Path | Today | After this spec |
| :-- | :-- | :-- |
| Last tab of a project group closed | Falls back to the global group and activates its remembered tab (`closeTab`, plus the reconcile watcher for the cross-screen case). Verified, has a way forward. | Unchanged. |
| Shell exited | Chip tints red, tooltip says `title — exited`, typing respawns. The in-band notice points at a removed RESTART button (S3). | Notice rewritten; chip tooltip gains the same next action. |
| Cap hit while the stack is collapsed | Stack expands, group switches to the refusing project, empty strip, Toast (S2). | Scope restored, Toast names which of the two limits fired. |
| Phone hits the cap while the Mac holds the tabs | Companion pre-check Toasts locally if its mirrored list is current; otherwise the Mac Toasts and the phone gets 15 seconds of nothing followed by nothing (M1). | Both limits pre-checked on the phone; the TTL expiry speaks. |
| Intent never reached the Mac | Already handled by `action.js`'s `Not sent — no connection to the Mac`. | Unchanged. |

---

## Unverified — needs a real Mac and a real phone

Not findings. Each one is a claim this audit could not settle by reading code.

1. Whether a `vim` session survives a collapse→expand *today* in practice, or whether ring-buffer trimming already breaks it. The change makes it moot, but it determines whether S-tier urgency applies before the change ships.
2. Idle CPU and memory with 16 tabs mounted behind a closed panel, at least one of them running a chatty process. This is the only real cost of hide-not-unmount and the only thing that could argue against raising `MAX_TABS`.
3. Whether the companion's mirrored tab list is stale often enough in practice for M1's race to be a routine experience or a rare one. That decides whether the TTL-expiry Toast is a backstop or a primary path.
4. Whether a Toast fired on the phone is actually legible over an expanded terminal panel on a phone-sized viewport.
5. Whether the `TERM` cell's tooltip is reachable at all on a phone (it is a `title` attribute; touch has no hover). If it is not, M2's per-project number is a Mac-only affordance and the phone learns the cap only from the refusal message — acceptable, but it should be a known asymmetry rather than a surprise.
