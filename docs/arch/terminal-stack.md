# Terminal stack — architecture

How the in-app terminal's frontend is layered, and how terminal v2's SCOPES (tab groups) sit on
top of it without touching Rust. User-facing behaviour: `docs/feat/in-app-terminal.md`.

## The layering

Four layers, each with a single job:

- **Shared session state** — `src/store/terminalTabsStore.js`. Holds `terminalTabs` (the list),
  mutated only through `action()`-wrapped functions so a companion's gesture (tap a tab's ✕ on the
  phone) executes on the host, whose state change then mirrors back to every screen.
  `services/mirror.js` auto-discovers every `isRef` export under `src/store/` (SSOT-1); the two
  navigation refs — `activeTerminalTabId` and `activeTerminalScope` — opt out via
  `PER_SCREEN_KEYS`, because which tab/group a screen is looking at is that screen's own
  navigation, not session data.
- **Per-screen glue** — `src/composables/useTerminalTabs.js`. Liveness (`tabAlive`, re-exported
  from `usePtyTerminal`'s module-level tracker), `activatedTabs` (which tabs this screen has ever
  mounted), `lastTabByScope` (which tab each scope was last looking at, VSCode-group style), and
  the companion's pending-activation claim (scope-keyed, with a short TTL so a claim the host
  refuses cannot strand and later steal focus). None of this is
  mirrored — the file says why per ref, since each is either screen-local navigation or something
  a companion's own PTY event stream already reconstructs.
- **Panel chrome** — `DockStack.vue` (presentational base) + the specializations `dock/LogStack.vue`
  and `dock/TerminalStack.vue`, each owning its own per-screen collapse ref.
  This is **composition with a shared presentational base and slot-injected content**, not class
  inheritance — `DockStack` renders the header/body/peek chrome and emits `update:collapsed`;
  each specialization fills its own `#title` / `#actions` / `#peek` slots and owns its own script.
  The one configuration point terminal v2 added here is `collapseVariant` (`'chevron' | 'close'`) —
  `TerminalStack` passes `'close'` so its one header button reads CLOSE/EXPAND instead of
  COLLAPSE/EXPAND; `LogStack` keeps the default and is otherwise untouched.
- **PTY surface** — `usePtyTerminal.js` + `TerminalView.vue`. One `usePtyTerminal(term, tabId)`
  instance per mounted `TerminalView`, filtering Tauri events and companion frames by `tab_id`.

## Scope as the aggregate root

A **scope** is a tab group. Its key is `scopeOf(tab) = tab.projectId || GLOBAL_SCOPE` — grouping
is derived entirely from a field the tab record already carried (`projectId`); nothing new went on
the wire for the tab list itself. A scope owns its tabs; a tab's membership in one is nothing more
than that field.

Invariants:

- The **global** scope (`projectId == null`) is the only one with a non-empty floor: it must always
  keep at least one tab (`terminalTabsStore.js`'s `closeTerminalTab` guard). A **project** scope may
  go to zero — the group simply stops existing until its TERM cell is clicked again, at which point
  `openProjectTerminal` creates a fresh tab in that project's directory.
- `MAX_TABS = 8` is **global across all scopes** — it mirrors `src-tauri/src/pty.rs`'s `MAX_TABS`
  by comment, not by a shared constant (the Rust and JS build graphs do not share one), because the
  Rust side has no notion of groups; it only ever sees a flat set of `tab_id`s.
  `terminalTabsStore.js`'s Toast on hitting the cap says "in any group" for exactly this reason.
- **Scope-empty ⇒ fall back to global.** Closing a project scope's last tab forgets that scope's
  remembered tab (`forgetScopeTab`, multi-entity guard: scoped to the ONE scope, never clears the
  whole map), resets `activeTerminalScope` to `GLOBAL_SCOPE`, and activates the global group's
  remembered (or most recent) tab. The stale-id reconcile watcher in `useTerminalTabs.js` performs
  the same fallback defensively for the companion boot / cross-screen-close cases the direct
  `closeTab` path does not cover.

## The capability pattern

`usePtyTerminal` publishes capability flags — `ownsPtySize` (does this screen decide the shared
PTY's cols/rows?) and `showKeyRow` (does this surface need the synthetic Esc/Tab/arrow/Ctrl row?) —
instead of exposing a role flag; `TerminalView.vue` asks the capability, never `isHost`. Scope adds
no new capability: grouping is pure frontend navigation, invisible to the PTY layer entirely.

## PTY backend contract

One PTY per `tab_id`; `pty_spawn` is idempotent; liveness travels on `pty_output` / `pty_exit`,
never on the mirror (there is deliberately no `alive` field on the `terminalTabs` record —
mirroring one would be a second, competing source of truth for a fact the PTY events already carry
with lower latency); `pty_close_tab` requires its `tab_id` argument (a defaultable "close" is
exactly the accidental-blast-radius shape the multi-entity regression guard forbids); each live tab
costs roughly three raw OS threads, bounded by `MAX_TABS`; the app-exit hook kills every tab's
process group.

**Terminal v2 required zero Rust changes.** The backend was already tab-keyed and knows nothing
about "projects" or "groups" — scope is a pure frontend grouping over `projectId`, a field the tab
list already carried before this work. If any part of this feature had needed a Rust change, that
would have meant the frontend-only premise was wrong.

## Mount semantics

`TerminalStack.vue`'s mount loop (`v-for="t in tabs"`, the FULL unfiltered list) iterates every tab
regardless of scope, on purpose: `v-if="activatedTabs.has(t.id)"` mounts a `TerminalView` lazily on
first activation and keeps it mounted afterward, and only `v-show="t.id === activeTabId"` changes
when the visible tab or the visible scope changes. Filtering the mount loop by scope would unmount
and re-spawn xterms on every group switch — the tab strip (`TerminalTabStrip.vue`) is what is scope-
filtered (`scopedTabs`), not the mount loop.

A stack **collapse** still unmounts everything (`DockStack.vue`'s default slot is `v-if`, not
`v-show`) — accepted as-is, unrelated to scope: `pty_spawn` is idempotent and scrollback rehydrates
on remount, so a collapse→expand round-trip costs re-mounting N xterms, never lost session state.

## External `Terminal.app` count — derived, never remembered

The `TERM` cell's bottom (slate) badge is **not** part of the PTY stack at all: it counts *external*
`Terminal.app` windows/tabs standing in a project's directory, and it is a **live scan, not a
tally**. That is the whole architectural point — the app can observe itself opening a window but
never observes the user closing one, so any remembered counter can only grow and eventually lies.

One producer, host-only (seam P): `composables/useExternalTerminals.js` calls the
`count_external_terminals` command (`src-tauri/src/system.rs`) every **5 s**, plus once ~800 ms after
the app opens a Terminal window. Each scan is three short local subprocesses on the blocking pool —
`pgrep -x Terminal`, one `ps -axo pid=,ppid=` walked into Terminal's descendant set, one batched
`lsof -a -d cwd -p <pids> -F pn` (capped at 200 pids). The counting rule is **roots of matching
subtrees** (`count_cwd_subtree_roots`, unit-tested with no subprocess): a process counts only if its
cwd is the project directory *and its parent's cwd is not*, so one window running a dev server
(shell → npm → node, all sharing the cwd) counts once. Match is exact — a subdirectory is not the
project.

The result replaces `projectStore.externalTermCounts` wholesale each tick, keyed by project id. That
is not a multi-entity "clear": every key is rewritten from the same single scan, and no function
resets part of it. Being a store ref, the mirror carries the snapshot to every companion for free —
the phone must never poll, since Terminal's process table exists only on the Mac.
