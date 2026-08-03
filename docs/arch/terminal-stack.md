# Terminal stack — architecture

How the in-app terminal's frontend is layered, and how terminal v2's SCOPES (tab groups) sit on top of it without touching Rust. User-facing behaviour: `docs/feat/in-app-terminal.md`.

## The layering

Four layers, each with a single job:

- **Shared session state** — `src/store/terminalTabsStore.js`. Holds `terminalTabs` (the list), mutated only through `action()`-wrapped functions so a companion's gesture (tap a tab's ✕ on the phone) executes on the host, whose state change then mirrors back to every screen. `services/mirror.js` auto-discovers every `isRef` export under `src/store/` (SSOT-1); the two navigation refs — `activeTerminalTabId` and `activeTerminalScope` — opt out via `PER_SCREEN_KEYS`, because which tab/group a screen is looking at is that screen's own navigation, not session data.
- **Per-screen glue** — `src/composables/useTerminalTabs.js`. Liveness (`tabAlive`, re-exported from `usePtyTerminal`'s module-level tracker), `activatedTabs` (which tabs this screen has ever mounted), `lastTabByScope` (which tab each scope was last looking at, VSCode-group style), and the companion's pending-activation claim (scope-keyed, with a short TTL so a claim the host refuses cannot strand and later steal focus). None of this is mirrored — the file says why per ref, since each is either screen-local navigation or something a companion's own PTY event stream already reconstructs.
- **Panel chrome** — `DockStack.vue` (presentational base) + the specializations `dock/LogStack.vue` and `dock/TerminalStack.vue`, each owning its own per-screen collapse ref. This is **composition with a shared presentational base and slot-injected content**, not class inheritance — `DockStack` renders the header/body/peek chrome and emits `update:collapsed`; each specialization fills its own `#title` / `#actions` / `#peek` slots and owns its own script. The one configuration point terminal v2 added here is `collapseVariant` (`'chevron' | 'close'`) — `TerminalStack` passes `'close'` so its one header button reads CLOSE/EXPAND instead of COLLAPSE/EXPAND; `LogStack` keeps the default and is otherwise untouched.
- **PTY surface** — `usePtyTerminal.js` + `TerminalView.vue`. One `usePtyTerminal(term, tabId)` instance per mounted `TerminalView`, filtering Tauri events and companion frames by `tab_id`.

## Scope as the aggregate root

A **scope** is a tab group. Its key is `scopeOf(tab) = tab.projectId || GLOBAL_SCOPE` — grouping is derived entirely from a field the tab record already carried (`projectId`); nothing new went on the wire for the tab list itself. A scope owns its tabs; a tab's membership in one is nothing more than that field.

Invariants:

- **No scope has a floor, global included (2026-07-28).** Every group — a project's, or the global one — may go to zero tabs; it simply stops existing until its terminal button is clicked again, at which point `openScopeTerminal` (via `openProjectTerminal` / `openGlobalTerminal`) creates a fresh tab. Global used to be pinned to a permanent one-tab minimum (seeded at boot by `initTerminalTabs`, enforced on close by `terminalTabsStore.js`'s `closeTerminalTab`); that was removed because it was the actual mechanism behind a real bug, not just an inconsistency — a dev-server HMR reload re-runs the boot seed, and a `pty_list_tabs()` call racing the backend into reporting an empty list (even with a shell already live) would seed another tab on top of it, piling up phantom "Shell" chips across repeated reloads. Every scope is now symmetric: opened on demand, closeable to zero, no special case.
- **Two caps, not one (1.21.1).** `MAX_TABS_PER_SCOPE = 5` is the user-facing rule: a group (a project, or the global group) refuses its 6th tab, and the refusal names that number. `MAX_TABS = 16` is a separate, **global**, machine guard underneath it — it mirrors `src-tauri/src/pty.rs`'s `MAX_TABS` by comment, not a shared constant (the Rust and JS build graphs do not share one), because the Rust side has no notion of groups; it only ever sees a flat set of `tab_id`s, so the per-scope cap is a pure frontend rule layered above a backend that stays scope-blind. `addTerminalTab`/`capReached()` both check **scope first, then global** — a user in a 1-tab group who hits the global ceiling is told about the *other* groups, not told their own group is full.
- `MAX_TABS` is a **generous shared ceiling, not a per-scope multiple.** It happens to equal `1 + 3 × MAX_TABS_PER_SCOPE`, but that arithmetic is no longer load-bearing (it used to be justified by global's old guaranteed one-tab minimum, which is gone — see above). Nothing enforces "at most 3 project groups plus global" as a real limit; a 4th or 5th project group filling up alongside a full global group can legitimately hit the ceiling before its own per-scope cap. That is an accepted, rare edge of a **resource guard, never a budget the user manages** (the existing rule that no tooltip states this number ahead of time) — raising it is a one-line change plus re-checking **INVARIANT R** below, not an architectural one.
- `MAX_TABS` also binds `web_server.rs`'s `COMPANION_QUEUE_LIMIT_BYTES` via **INVARIANT R** (`MAX_TABS × (base64_len(SCROLLBACK_CAP) + ~128) ≤ COMPANION_QUEUE_LIMIT_BYTES / 2`), asserted by a Rust unit test rather than left to a comment: the two-file mirroring that sufficed when the caps were the same number cannot check arithmetic between four constants that are now deliberately different. Raising `MAX_TABS` without re-deriving the other two re-breaks a joining phone's scrollback replay; the test exists to catch exactly that.
- The three refusal strings (per-project, global-group, global-ceiling) each interpolate their own constant and never hardcode a digit; only the global-ceiling one says "in any group", because it is the one refusal whose cause genuinely lives somewhere off-screen — see `terminalTabsStore.js`.
- **Scope-empty ⇒ fall back to global — the SAME rule for every scope, global included.** Closing any scope's last tab forgets that scope's remembered tab (`forgetScopeTab`, multi-entity guard: scoped to the ONE scope, never clears the whole map), resets `activeTerminalScope` to `GLOBAL_SCOPE`, and activates the global group's remembered (or most recent) tab if it has one. Global closing its own last tab is simply the identity case of this rule: `activeTerminalScope` is already `GLOBAL_SCOPE`, and if it has no remaining tab either, the group just renders empty — the same "click the terminal button again to open a fresh one" state a project group has always had. The stale-id reconcile watcher in `useTerminalTabs.js` performs the same fallback defensively for the companion boot / cross-screen-close cases the direct `closeTab` path does not cover, and is itself a no-op when no scope anywhere has a tab left.

### Companion add is fire-and-forget — the repeat-tap guard (1.22.0)

Opening a tab from a companion never gets the tab back: `addTerminalTab`'s `action()` stub (`src/services/action.js`) sends the intent and returns `undefined` immediately, so the caller only learns a tab exists once the host's mirror echoes it back. `useTerminalTabs.js`'s `openScopeTerminal` bridges that gap with a scope-keyed "queue-of-one" claim (`pendingActivateScope`, TTL'd) that the tab-list watcher resolves once the mirror arrives — full mechanism in the doc comment at its definition.

Before 1.22.0 that round trip had a second cost: `resolveScopeTab`/`capReached` both read the still-stale mirrored list while it was in flight, so a repeat tap on the same scope's TERM button before the tab arrived passed both checks again and opened a second tab — nothing told the tapper the first one was already on its way. `openScopeTerminal` now no-ops a repeat call for a scope with a live claim already outstanding, so only the first tap sends an add.

**Any new "open/duplicate a terminal" entry point must call `openScopeTerminal`, not `addTerminalTab` directly** — the guard lives in the caller, so bypassing `openScopeTerminal` bypasses it too.

**The one deliberate exception: DEV/BUILD (`docs/plan/done/dev-build-in-app-launch.md`, #7).** `ProjectTable.vue`'s DEV/BUILD buttons — a third entry point alongside the TERM cell and the header icon — call `useTerminalTabs.js`'s `openRunCommand`, not `openScopeTerminal`. `openScopeTerminal`'s reuse check is scope-only ("does this scope already have a tab?"); DEV/BUILD needs a scope **and** `runKind` match ("does this scope already have a *dev* tab?" — a project may have an ordinary shell and a running dev server open at the same time, and pressing BUILD must never touch DEV's tab). `openRunCommand` re-implements the create-or-focus shape against that finer key, tags the new tab `runKind: 'dev' | 'build'`, and stashes the literal command in a `pendingCmd` field that `usePtyTerminal.js` sends once, on the tab's `alive` transitioning to `true` — new spawn or respawn alike, same watcher.

**Known gap the guard does not close:** two browser tabs open on the same phone are two separate page loads, so `pendingActivateScope` (module-scope, per page) is not shared between them. Same per-page-state root cause as the `invoke_result` cross-talk that per-connection addressing fixed in 1.21.1 (`docs/feat/remote-control.md`) — but unfixed here, since closing it needs the guard to live host-side, inside `addTerminalTab`'s own body, not in the composable.

## The capability pattern

`usePtyTerminal` publishes capability flags — `ownsPtySize` (does this screen decide the shared PTY's cols/rows?) and `showKeyRow` (does this surface need the synthetic Esc/Tab/arrow/Ctrl row?) — instead of exposing a role flag; `TerminalView.vue` asks the capability, never `isHost`. Scope adds no new capability: grouping is pure frontend navigation, invisible to the PTY layer entirely.

## Keyboard input: xterm owns keys, the app owns text

`TerminalView.vue`'s PTY surface splits input into two disjoint claims, enforced structurally rather than by classifying key type:

- **Keys** — arrows, modifiers, F-keys, bracketed paste, DECCKM, Alt-as-meta — stay entirely inside xterm.js's own `_keyDown` / `evaluateKeyboardEvent` pipeline, untouched. xterm already gets this right; an earlier app-owned overlay textarea that re-implemented it regressed six ways (arrows ignoring application cursor mode, F5 emitting PageUp, paste losing its bracketing, Option+word-motion sending nothing).
- **Text** — everything that lands as a rendered character — has exactly **one** path to the PTY: `useTerminalTextDrain.js`'s capture-phase `input` listener on xterm's own textarea. `customKeyEventHandler` vetoes **every** `keypress` (`return false` unconditionally, not scoped to a subset of keys), which skips xterm's own `_keyPress` **without** calling `preventDefault`; the browser still inserts the character into the textarea, and the drain reads, clears, and sends it once.

The exclusivity this buys is a DOM specification guarantee — `preventDefault` on a key event suppresses the following textarea mutation and its `input` event — not an internal xterm behaviour the app has to keep matching: any key xterm force-cancels at `_keyDown` never produces a keypress at all, so the drain never sees it; any keypress that does fire is always vetoed, so it is always the drain, never xterm, that sends the resulting text. An earlier, narrower veto (only xterm's multi-character IME carrier signature) left two key classes — space and uppercase A-Z — reaching `_keyPress` without a forced cancel, so xterm sent them AND the drain sent them again; full mechanism and file:line evidence in `docs/research/terminal-vietnamese-ime-root-cause-5.md`, decision record in `-4.md` §7. `useTerminalInput.js` (the app-owned overlay) and `useWkImeGuard.js` (a capture-phase guard on xterm's internal handlers) were the two prior approaches; both are deleted.

## PTY backend contract

One PTY per `tab_id`; `pty_spawn` is idempotent; liveness travels on `pty_output` / `pty_exit`, never on the mirror (there is deliberately no `alive` field on the `terminalTabs` record — mirroring one would be a second, competing source of truth for a fact the PTY events already carry with lower latency); `pty_close_tab` requires its `tab_id` argument (a defaultable "close" is exactly the accidental-blast-radius shape the multi-entity regression guard forbids); each live tab costs roughly three raw OS threads, bounded by `MAX_TABS` (1.21.1: 16, i.e. 48 raw threads and 2 MiB of resident ring buffer at the absolute ceiling); the app-exit hook kills every tab's process group.

**Terminal v2 required zero Rust changes.** The backend was already tab-keyed and knows nothing about "projects" or "groups" — scope is a pure frontend grouping over `projectId`, a field the tab list already carried before this work. If any part of this feature had needed a Rust change, that would have meant the frontend-only premise was wrong.

**Backend storage.** `src-tauri/src/pty.rs` keys every piece of session state by a `TabId` (`u32`): `sessions`, `inputs`, `scrollbacks` are each a `HashMap<TabId, _>` instead of a singleton, and `min_accepted` (the generation-fencing floor) is a **per-tab** map — a single global floor would let retiring one tab's generation fence off a still-live generation in another tab. Every command takes an `Option<u32> tab_id` defaulting to tab 0, the backward-compatibility seam for a companion frontend that never sends `tab_id`. The one exception is `pty_close_tab`, whose `tab_id` is **required** — a defaultable "close" is exactly the accidental-blast-radius shape the multi-entity regression guard forbids. `pty_list_tabs()` returns `{id, alive}` per tab so a reloaded frontend can re-adopt shells the backend kept running.

## Wire format

No new frame types — `pty_output` / `pty_input` / `pty_resize` / `pty_exit` all carry `tab_id` (default 0). This matters because the relay (`web_server.rs`) coalesces `pty_output` frames under congestion by tag alone, content-blind — without `tab_id` on every frame, a coalesce could silently land tab A's bytes in tab B's xterm. `pty_output` also carries an optional `reset` flag meaning "replace everything, do not append", set by RESTART and scrollback replay.

**Per-companion addressing (1.21.1).** A scrollback replay (a `pty_output` with `reset: true`) and an `invoke_result` are addressed to the one companion they are for, via an optional `to` field the relay routes on and an optional `from` field the relay stamps on the way in — see `docs/feat/remote-control.md` for the wire-level detail. Before this, both were broadcast: a second phone joining wiped and rebuilt the screen of a phone already mid-command, and two phones with an overlapping `invoke` call in flight could resolve each other's replies. Live `pty_output`, `pty_exit`, `pty_resize`, `delta` and `init` all still broadcast, since every screen genuinely needs the same bytes.

**Resync.** `pushScrollback()` became `pushAllScrollbacks(to)` in `src/services/ptyBridge.js`: on a fresh companion connect or a scheduled resync, it calls `pty_list_tabs()` then replays every tab's scrollback, not just tab 0's — the same congestion that forces a resync would otherwise leave every tab past the first silently un-replayed on the device that just reconnected. The companion-join path passes the joining device's id so only that phone receives the replay; the host's own congestion-recovery path still broadcasts, because that hole exists in every companion's byte stream at once and there is no single device to address it to.

## How the bytes move

```
[Mac] shell ──► PTY reader thread ──► ring buffer (scrollback)
                       │
                       ├─ emit('pty-output') ──────────► the Mac's own terminal (lowest latency)
                       └─ services/ptyBridge.js ──────► pty_output frame ──► phone
[phone] keystroke ──► pty_input frame ──► ptyBridge (Mac) ──┐
[Mac]   keystroke ──────────────────────────────────────────┴──► pty_write  (one authority)
```

Terminal bytes ride four top-level frames on the existing socket (`pty_input`, `pty_output`, `pty_resize`, `pty_exit`), never the state mirror or the intent registry — raw output is a firehose and does not fit a JSON-diffed state model. The first two names were reserved in `src/constants/protocol.js` a release ahead of time for exactly this. Bytes are base64 end-to-end and never treated as a Rust `String`; the frontend decodes to a `Uint8Array` and hands it to xterm.js, whose own stateful UTF-8 decoder reassembles a multi-byte character split across two PTY reads. Output is coalesced (about every 20ms, or 16KB, whichever comes first) so a chatty build does not become one network message per `read()`. `pty_exit` is a separate signal rather than something parsed out of the `[process exited]` text: driving state by pattern-matching our own cosmetic output would break the moment the wording changed.

## Never-block-the-UI, and the one place the usual rule inverts

`pty_spawn` / `pty_write` / `pty_resize` / `pty_get_scrollback` are all `async fn` wrapping their work in `spawn_blocking`, per the ABSOLUTE rule in `CLAUDE.md`.

The **read loop is a dedicated `std::thread`, and must stay one.** `spawn_blocking`'s pool is sized for bounded one-shot work; parking one of its threads forever in a `read()` loop for the app's whole lifetime starves every other blocking command (remote path resolution, update check, git info) of a slot. A tokio task would be just as wrong — `portable-pty`'s reader is a synchronous `Read`, so it would block a worker identically. This is the one spot in the app where "just wrap it in `spawn_blocking`" is the bug rather than the fix.

### Restart cannot orphan or clobber a session

Every spawn takes a generation number. A reader thread only retires the session slot if the session sitting in it is still the one it was reading. Without that, a shell killed by restart whose EOF arrives a moment late would null out the brand-new session that replaced it — leaving a terminal that is dead with no error anywhere and no way to tell why.

### Killing the shell means killing its process group

`portable_pty`'s `Child::kill()` signals only the direct child — the login shell. Everything the user started *inside* that shell is a separate process in the shell's process group and receives nothing from it. `kill_current` therefore sends SIGHUP to the whole group first (`killpg`, the same signal closing a real terminal window sends) and escalates to SIGKILL only for what survives the grace period. `portable-pty` puts the child in its own session on unix, so the child's pid is its process-group id and one `killpg` reaches every descendant.

The bigger half of the fix is *when* teardown runs: it is also wired to `RunEvent::Exit` in `lib.rs`. Every other path into `kill_current` is a user gesture (KILL, RESTART), so before that hook existed, quitting the app ran no teardown whatsoever.

**How much `killpg` itself buys, stated honestly.** The unit test `killing_the_shell_takes_processes_started_inside_it_with_it` still passes with `killpg` swapped for a plain `kill` on the shell — verified by mutation, not assumed. When a session leader holding a controlling terminal dies, the kernel SIGHUPs the foreground process group on its own, so an ordinary foreground child dies either way. `killpg` covers what the kernel does not: a process that has left the foreground group, and any teardown path where the ctty is not revoked.

The app-exit teardown investigation (orphaned `ssh` clients on the dev Mac) is tracked separately and not resolved by this mechanism — the evidence does not fit the in-app terminal's spawn shape.

## Mount semantics

`TerminalStack.vue`'s mount loop (`v-for="t in tabs"`, the FULL unfiltered list) iterates every tab regardless of scope, on purpose: `v-if="activatedTabs.has(t.id)"` mounts a `TerminalView` lazily on first activation and keeps it mounted afterward, and only `v-show="t.id === activeTabId"` changes when the visible tab or the visible scope changes. Filtering the mount loop by scope would unmount and re-spawn xterms on every group switch — the tab strip (`TerminalTabStrip.vue`) is what is scope-filtered (`scopedTabs`), not the mount loop.

A stack **collapse** no longer unmounts anything either (1.21.1). `DockStack.vue` takes a `bodyPersist` prop that only `TerminalStack.vue` passes: with it the default slot is wrapped in `.dock-stack-body` and hidden with `v-show` instead of removed with `v-if`, so a collapse→expand round-trip disposes and re-spawns nothing, and scroll position and a full-screen program's painted screen survive it. `LogStack.vue` does not pass the prop and keeps the old destroy-on-collapse path.

The re-fit on expand is not a new code path: `TerminalStack.vue` passes `active` as `t.id === activeTabId && !collapsed`, and `TerminalView.vue`'s existing `watch(() => props.active)` re-fits and refocuses on the false→true edge. While hidden the container measures 0 and `doFit()`'s `width < 40 || height < 24` floor discards the measurement, so nothing resizes the live PTY.

`TerminalStack.vue` also collapses **itself** once `tabs.value.length` (the full, unfiltered list) reaches 0 — every tab in every scope closed, by any close path (a chip's ✕, ⌘W on the last one, etc.) — a `watch` sets `collapsed.value = true` rather than a per-close-path special case, since it is state-driven, not path-driven. The collapse/expand transition itself is a `main.css` rule on `.dock-stack`: `flex-grow`/`flex-basis` (numeric, so a `transition` can ease it), not the `flex: none` shorthand (whose implicit `auto` basis a transition can't interpolate toward) — shared with `LogStack.vue`, so both stacks now ease shut/open instead of snapping.

The cost is retention: every mounted xterm keeps its 5000-line scrollback while the panel is closed, which is what `MAX_TABS` bounds.

## External `Terminal.app` count — derived, never remembered

The `TERM` cell's bottom (slate) badge is **not** part of the PTY stack at all: it counts *external* `Terminal.app` windows/tabs standing in a project's directory, and it is a **live scan, not a tally**. That is the whole architectural point — the app can observe itself opening a window but never observes the user closing one, so any remembered counter can only grow and eventually lies.

One producer, host-only (seam P): `composables/useExternalTerminals.js` calls the `count_external_terminals` command (`src-tauri/src/system.rs`) every **5 s**, plus once ~800 ms after the app opens a Terminal window. Each scan is three short local subprocesses on the blocking pool — `pgrep -x Terminal`, one `ps -axo pid=,ppid=,tty=,etime=,command=` walked into Terminal's descendant set, one batched `lsof -a -d cwd -p <pids> -F pn` (capped at 200 pids). The counting rule is **roots of matching subtrees** (`count_cwd_subtree_roots`, unit-tested with no subprocess): a process counts only if its cwd is the project directory *and its parent's cwd is not*, so one window running a dev server (shell → npm → node, all sharing the cwd) counts once. Match is exact — a subdirectory is not the project.

The result replaces `projectStore.externalTermCounts` wholesale each tick, keyed by project id. That is not a multi-entity "clear": every key is rewritten from the same single scan, and no function resets part of it. Being a store ref, the mirror carries the snapshot to every companion for free — the phone must never poll, since Terminal's process table exists only on the Mac.

### The detail modal shares the scan, not just the idea (1.22.0)

`list_external_terminals` answers "which sessions, and what is running in them" and is the same pipeline: both commands go through `scan_terminal_tree`, which owns the three subprocesses and hands back `(ppid_of, cwd_of, row_of)`. The subtree-root test is applied identically on both sides, so the modal's row count and the badge's number are the same fact computed once — a second, parallel definition of "one session" is precisely how the two would come to disagree.

The split is in *cadence*, not in logic: the badge polls every 5 s and needs only a count; the modal is on-demand and returns a command line per process, which must never ride the poll. Both are host-only and neither is in `COMPANION_ALLOWED_COMMANDS`; the badge reaches a phone through the mirrored `externalTermCounts` snapshot, and the modal's button simply does not render there (`externalTerminalsSupported`, published as a capability rather than an `isHost` read at the call site, in keeping with the capability pattern above).
