# WS-A — External terminal ownership model

**Status**: MVP floor shipped (2026-07-28), S3-S7 still planning · **Batch**: `docs/plan/backlog-jul27.md` (WS-A), `docs/plan/done/backlog-jul28-terminal-ux.md` (WS-N) · **Version state**: `package.json` is `1.21.0`; this accumulates in `CHANGELOG.md`'s `[Unreleased]`. No version number appears in this doc or in any file it creates.

**2026-07-28 note — what actually shipped is NOT §10's S1/S2/S5 as written.** The user's immediate complaint was narrower than this whole doc: the global button showed neither badge at all. Rather than build the full session-inventory/pure-selector/component-rename shape (§10 S1, S2, S5), a smaller command was added instead:
- `src-tauri/src/system.rs`'s new `count_external_terminals_global(paths)` computes §5's complement (`unowned = all subtree roots − roots matching any of paths`) directly in Rust and returns one `u32` — no `list_terminal_sessions()`, no `owner` field, no `src/utils/terminalOwnership.js` pure module. It is the §5 formula, computed in a different shape than §10 specifies.
- `src/store/projectStore.js` gained `externalTermGlobalCount` (mirrored, sibling of `externalTermCounts`), polled by `useExternalTerminals.js` on the same 5s cadence.
- `ProjectTable.vue`'s header button gained a `globalTabCount`/`globalHasExited`/`globalTermTitle` computed block **inline**, not via the `TerminalCell.vue` → `TerminalScopeButton.vue` rename §7/§10-S5 designs — Rule-of-Three still only finds two call sites, and duplicating ~15 lines of computed logic was judged cheaper right now than carrying the rename through every reference. The button now uses the shared `.btn-cell-trigger` class and `TerminalCountBadges` component, so it is visually and structurally identical to the per-project instance even though the file itself was not renamed.

Net effect: the visible complaint (global button silently missing both badges) is fixed, using the adoption-only reading §3/§5 already describe as the correct MVP floor if spawn-origin tagging never lands. **Everything in §10 marked S3 onward (tty capture and tagging, `owner` provenance, the modal's provenance labels, the `TerminalScopeButton` rename) is still exactly as designed and NOT implemented** — a future session picking this up should treat S1/S2/S5 as superseded by the paragraph above, and S3-S7 as accurate and still open.

Answers the request: *"external terminal: không hề quản được… cơ chế đếm external terminal được tính là 'nơi được bấm bắt đầu' chứ không phải 'quét tất cả rồi phân loại theo cwd'… badge external của nút terminal global đếm toàn bộ cửa sổ terminal CÒN LẠI không thuộc về project nào."*

## 1. Non-goals (MVP boundary)

Stated first, because the imagined final version is much larger than what should ship.

- **No actuation.** The MVP counts, lists and attributes windows. It does not raise, focus, close, or send input to a `Terminal.app` window. The tty handle this design captures is what would later make actuation possible — that is a follow-up, not this workstream.
- **No persistence of the ownership registry across an app restart.** The registry is RAM-only. §6 defines exactly what happens on restart, and it is a defined, non-lossy degradation — not an omission.
- **No terminal emulator other than `Terminal.app`.** iTerm2, Ghostty, Warp, WezTerm are out. The app only ever *opens* `Terminal.app`, so spawn-origin ownership cannot exist for the others, and scanning them would produce a global badge whose meaning silently differs from the per-project one.
- **No change to the cwd matching rule itself** (still exact-match on the project root, no subdirectory rollup). That is `docs/arch/terminal-stack.md`'s existing v1 semantics and changing it in the same diff would make the badge move for two unrelated reasons at once.
- **No companion-side scanning.** The phone continues to receive tallies through the state mirror. Nothing new enters `COMPANION_ALLOWED_COMMANDS` except the `owner` argument on commands already in it.
- **No in-app PTY tab changes.** Scopes, caps, liveness and the tab strip are untouched; `src-tauri/src/pty.rs` is not opened by this workstream.
- **No new modal, panel, row, banner or label.** Both new facts (global external count, per-session provenance) ride existing elements — a badge overlay and a modal line (CLAUDE.md, Extreme Narrow).

## 2. What is true today — read from source

| Fact | Where |
|---|---|
| The badge is re-derived, never remembered. A 5s host-only poll calls `count_external_terminals(paths)` and replaces `externalTermCounts` wholesale. | `src/composables/useExternalTerminals.js`, `src/store/projectStore.js` |
| The scan is `pgrep -x Terminal` → one `ps -axo pid=,ppid=,tty=,etime=,command=` → one batched `lsof -a -d cwd` (≤200 pids), shared by badge and modal through `scan_terminal_tree`. | `src-tauri/src/system.rs` |
| "One session" = the root of a cwd subtree (`count_cwd_subtree_roots`), promoted past macOS's `login -pf` wrapper. One rule, two callers, so badge and modal cannot disagree. | `src-tauri/src/system.rs` |
| The derived design deliberately replaced a session counter that only grew and never saw a window close. | header comment of `useExternalTerminals.js`, § "External `Terminal.app` count — derived, never remembered" in `docs/arch/terminal-stack.md` |
| Every Terminal window the app opens goes through one funnel, `open_terminal_with_command`, which `.spawn()`s `osascript` and **reads nothing back** — the app currently retains zero identity for what it launched. | `src-tauri/src/system.rs:186` |
| Two launch paths reach that funnel: `open_local_terminal` and `open_remote_subprocess("terminal")` (SSH). | `src-tauri/src/system.rs` |
| Two frontend call sites launch one of those: the OPEN popup's `Terminal` item and the in-app tab's "open externally". Both then call `pokeExternalTermCounts()`, which carries **no owner information**. | `src/components/ProjectTable.vue`, `src/composables/usePtyTerminal.js`, `src/store/projectStore.js:44` |
| Both scan commands are deliberately absent from the companion allowlist; `open_local_terminal` and `open_remote_subprocess` are present. | `src/services/hostInvoke.js` |
| `ps` already reports a tty per row and it is already parsed into `PsRow.tty`. | `src-tauri/src/system.rs` |
| There is no JS test runner in the repo (`package.json` has no vitest/jest). Rust has `#[cfg(test)] mod tests` in `system.rs`. | `package.json`, `src-tauri/src/system.rs:1333` |

### 2a. Amendment applied — the launch-path count shrank when DEV/BUILD moved in-app

**Applied 2026-07-30.** `docs/plan/done/dev-build-in-app-launch.md` shipped and was verified on macOS by the owner: DEV and BUILD now run in the in-app PTY and no longer open `Terminal.app`. `run_project_command`, `run_project_dev` and `run_in_project_terminal` were removed outright from `src-tauri/src/system.rs` in the same batch, so this is a completed retirement, not a scheduled one. §2's table and §5's attribution row above are already updated to match; this section records why they changed.

- **§2's "four launch paths" row dropped to two** — `open_local_terminal` and `open_remote_subprocess("terminal")`.
- **§5's `DEV / BUILD window` attribution row is now vacuous** — it described a window this app no longer opens.

**WS-A narrowed; it was not cancelled.** This is the point most at risk of being misread. Everything that makes this workstream worth doing survives untouched: the spawn-origin registry, §3's reconciliation rule, the global complement in §5, the per-project versus global split, and the SSH-terminal attribution change that is the user's own stated request. What changed is the *size of the input set* — two launch paths instead of four — which makes the registry smaller and simpler, not less necessary. A reader who takes this note as "WS-A is superseded by the in-app terminal work" has read it backwards.

**Ordering held.** The council's execution order put `dev-build-in-app-launch.md` before WS-A executes, precisely so WS-A is designed against the final launch-path set rather than one it was about to lose. That order was followed, so no stale rows were left behind for a later reader to trip over.

## 3. The crux — how registry and scan reconcile

The two failure modes are symmetric and each is fatal alone:

- **A registry alone re-creates the 1.x bug.** The app observes itself opening a window; it never observes the user closing one. A remembered set only grows.
- **A scan alone cannot answer "who launched this".** Bucketing by cwd is a *guess* about origin, and it is wrong the moment the user `cd`s away, opens an SSH session, or has two projects in one directory.

**The reconciliation rule, stated once and referenced everywhere:**

> **The scan is the sole authority on which sessions EXIST. The registry is the sole authority on who LAUNCHED an existing session. A registry entry is an attribute of a live session, never a session in its own right — it can never make a session appear, and it is discarded the moment its session stops appearing in the scan.**

That is why the registry can never inflate a count: the count is `sessions.length`, a scan quantity. Ownership only ever decides *which bucket* an already-counted session falls into. Removing the entire registry at any moment changes attribution and changes no total.

Attribution, in priority order, evaluated per session per tick:

1. **Tagged** — the session carries an owner token recorded at launch. Authoritative; cwd is not consulted at all, so `cd`-ing away, running `ssh`, or the project folder becoming unreachable changes nothing.
2. **Adopted** — untagged, and its cwd exact-matches a listed project's `local_path` (today's rule, unchanged). Covers windows opened by hand, opened before the app started, and every session after an app restart.
3. **Unowned** — neither. This is the global complement.

Adoption is not a compromise bolted on; it is what makes the tty capture (§8, the one unverified mechanism) a pure enhancement rather than a load-bearing dependency. If the tty read never works on a real Mac, every session falls to rule 2 and the app behaves exactly as it does today, plus a correct global badge.

## 4. The ownership model

**The entity.** One `Terminal.app` session — one window or one tab, the same unit `count_cwd_subtree_roots` already counts.

**Identity.** The session's **tty** (`ps` prints `s004`; AppleScript reports `/dev/ttys004`; normalized to the bare form on both sides), pinned to the **pid** of the oldest live process on that tty, discovered on the first reconcile after the tag is recorded. The tty alone is not enough: macOS recycles tty numbers, so a closed tab's number can be handed to a new one, and the pid pin is what stops a stale owner riding along. The pin costs about five lines and closes the only misattribution class the design otherwise has.

**Where it lives: Rust, in Tauri managed state.** Not the frontend, for three reasons drawn from this repo:
- A JS registry dies on any webview reload; the Terminal windows do not. The Rust process outlives the webview, so the registry must too.
- Reconcile must happen in the same tick as the scan, against the same `TerminalTree`. Split across the IPC boundary, the two are always one round-trip stale.
- `system.rs` already has `#[cfg(test)] mod tests`; the frontend has no test runner at all. Attribution logic that can be unit-tested belongs on the side that can test it.

**The registry stores an opaque owner token; Rust never interprets it.** This mirrors the discipline `docs/arch/terminal-stack.md` records for the PTY layer ("the backend knows nothing about projects or groups") — the frontend passes `project.id`, or the global-scope token, and the backend treats it as a string. Which tokens correspond to *listed* projects is a frontend question, because only the frontend holds `projects`.

**Shape:**

```rust
struct OwnedSession { owner: String, pid: Option<u32>, tagged_at: SystemTime }
struct TerminalOwnership { by_tty: Mutex<HashMap<String, OwnedSession>> }  // app.manage(...)
```

**Lifecycle:**

| Event | Effect |
|---|---|
| A launch command opens a window and reads back a tty | `record_terminal_owner(tty, owner)` — inserts exactly one entry |
| Launch succeeds but no tty comes back | Nothing recorded; the session falls to adoption (rule 2) |
| Every scan (5s poll, and the on-demand detail call) | `reconcile_terminal_owners`: pin the pid on first sight; drop each entry whose tty has no live process in the Terminal tree, or whose pinned pid is gone — **one entry at a time, never a bulk clear** |
| The owning project is removed from the list | **Nothing.** The entry stays; "listed" is resolved at read time against the current `projects`, so removing and re-adding a project restores attribution instead of destroying it |
| App restart | Registry is empty; every surviving session is adopted by cwd. Degrades to today's behavior, not to zero |
| Frontend reload | Registry survives — it is Rust-side state |

**Commands after this work** (both host-only, both stay out of `COMPANION_ALLOWED_COMMANDS`, both zero-argument — dropping the `paths` argument is the concrete architectural statement that classification is no longer a cwd bucketing job):

- `list_terminal_sessions() -> Vec<{ pid, ppid, tty, cwd, owner: Option<String> }>` — pollable, compact, no command lines. Replaces `count_external_terminals`.
- `describe_terminal_sessions() -> Vec<ExternalTerminalSession>` — on-demand, adds the command line and running descendants. Renames `list_external_terminals`; the two old names differed by one word for two very different cadences, which is a naming hazard worth retiring while the file is open (`design.A7`).

Both continue to share `scan_terminal_tree` and the single subtree-root rule, so badge and modal remain the same fact computed once.

**What crosses to the phone.** Unchanged in kind: the derived tallies, as mirrored store refs. `externalTermCounts` (per project id) keeps its meaning and gains `externalTermGlobalCount` beside it in `src/store/projectStore.js`. The registry itself is an input, not shared state, and never crosses. A phone that taps OPEN → Terminal passes an owner token like any other caller, so phone-launched windows are owned identically — `open_local_terminal` is already in the allowlist and only gains an argument.

## 5. The global complement, defined

> **Global external count = (number of live sessions) − (number of live sessions attributed to at least one listed project).**

Computed over the **session set**, not by summing the per-project badges. The distinction is load-bearing: an adopted session in a directory two projects share is attributed to both badges but subtracted once, so the complement can never go negative.

Every ambiguous case, answered:

| Case | Answer | Why |
|---|---|---|
| Window opened before the app started | Adopted by cwd if it stands in a listed project's root; otherwise global | The app cannot have tagged it. Adoption is the honest best evidence, and it is exactly today's behavior |
| Window opened from a project later removed from the list | **Global.** The entry is kept, not deleted | The user's definition is "not launched from a project *in the list*". Resolving "listed" at read time makes re-adding the project restore attribution, and makes project removal a pure list operation with no hidden side effect on the registry (multi-entity guard, §9) |
| Two projects pointing at the same directory | A **tagged** session counts only for the project it was launched from — an improvement over today, where both show the same number. An **adopted** session counts for both badges, and once in the owned set | Tagging is per project id; cwd genuinely cannot distinguish, and inventing a tiebreak would be a guess |
| The owning project's folder is now unreachable (unmounted volume, deleted) | The session stays owned and keeps counting on that project's badge | Ownership is keyed on tty+pid+token, never on the path. Today `canonicalize` fails and the badge silently drops to 0, which reads as "you have nothing open" when the truth is "we cannot resolve the path" |
| SSH terminal opened from a project's OPEN popup | **Owned by that project** | Its cwd is the local `$HOME`, so today it is counted nowhere. Under spawn-origin, "where you pressed the button" is the whole rule. This is a deliberate, visible behavior change — listed in S3's acceptance criteria so a reviewer sees it rather than discovers it |
| ~~DEV / BUILD window~~ | — | **Vacuous since 2026-07-30**: DEV/BUILD run in the in-app PTY and open no `Terminal.app` window at all. Kept struck through rather than deleted so a reader of an older revision can see the row was retired, not overlooked. See §2a |
| Window launched from the in-app terminal's "open externally" button | Owned by that tab's scope — the project id, or the global token for a global-scope tab | The tab already knows its scope; nothing is inferred |
| Scan fails (`pgrep`/`ps`/`lsof` error) | Previous snapshot stands, both badges | Already the rule in `useExternalTerminals.js`: a failed scan means "we don't know", not "everything closed". Do not regress it |
| `Terminal.app` not running | All counts 0, no error | Already the rule (`scan_terminal_tree` returns `Ok(None)`) |

## 6. Restart, stated plainly rather than assumed away

Spawn-origin tracking **does not survive an app restart**, and the design does not pretend otherwise. On restart the registry is empty and every live session falls to adoption:

- Sessions standing in a project root reappear on that project's badge (correct answer, arrived at by weaker evidence).
- Sessions that had `cd`'d elsewhere, or SSH sessions, move to the global complement. They were owned before the restart and are unowned after it.

This is accepted for the MVP over persisting the registry to disk, because a persisted entry must be re-validated against a pid whose meaning does not survive a reboot, and a stale on-disk owner is precisely the "counter that lies" failure this whole design exists to prevent. Persistence, if it is ever wanted, is a follow-up with its own validation rule (pid + process start time + tty must all match), not an MVP line item.

## 7. The button abstraction decision

**Rule-of-Three evidence, counted honestly** (`design.A2`):

| Site | Shape | Same thing? |
|---|---|---|
| `src/components/TerminalCell.vue` — TERMINAL column cell | `button.btn-cell-trigger` + terminal icon + `TerminalCountBadges` (tabs, external, exited) + composed multi-line tooltip → `openProjectTerminal(project)` | The reference instance |
| `src/components/ProjectTable.vue:25` — TERMINAL column header | `button.th-term-btn` + terminal icon, **no badges, no composed tooltip, different CSS class** → `openGlobalTerminal()` | The same button, drifted. It is missing both badges the cell has, including the in-app tab count for the global group |
| `src/components/ProjectTable.vue:180` — OPEN popup rows | `div.popup-item` menu rows (`In-App Terminal`, `Terminal`) | **Not an instance.** A menu row, not a badged icon button — different genus, no counts, no state |
| `src/components/dock/TerminalStack.vue:46` — stack header action | `button.btn-tech-secondary` opening the sessions **modal** | **Not an instance.** A different verb (inspect, not open) on a different object |

So the honest count is **two**, not three. The user's read is right in substance — the global button *should* be the same thing as the project button, differing only in scope — and wrong in one detail: today it is not "three copies", it is one implementation plus one drifted stub.

**Decision — no new abstraction, no class hierarchy.**

- Rejected: a `TerminalButton` base with `ProjectTerminalButton` / `GlobalTerminalButton` subclasses. Rule of Three has no evidence for it, Vue has no idiom for it, and it would add an indirection layer to serve two call sites (`design.A2`, `design.B3` steelman: keeping two files would be cheaper than a hierarchy — but not cheaper than one parameter).
- Adopted: **widen the component that already exists by exactly one axis.** `TerminalCell.vue` is renamed `src/components/TerminalScopeButton.vue` (`design.A7` — "Cell" names a position in a table, "ScopeButton" names the role) and takes a `scope` prop that is either a project object or the global-scope token. The TERMINAL column header renders the same component with the global scope, and `th-term-btn` is deleted.
- The count derivation moves out of the component into one scope-keyed selector, so the two buttons cannot disagree (`design.A1`). Neither button computes a number itself.
- Net file count: one rename, one deletion of a CSS rule, zero new abstractions. `openProjectTerminal` / `openGlobalTerminal` in `useTerminalTabs.js` stay exactly as they are — they are already the scope-parameterized pair, and the component picks between them by scope.

Result the user actually asked for: the global button gains the slate external badge (every unowned window) **and** the cyan tab badge for the global group, both of which it silently lacks today, for free.

## 8. Feasibility — verified vs unverified

**Verified by reading source in this repo:**

- Every Terminal launch already funnels through exactly one function, `open_terminal_with_command`. There is one place to add tagging, not four (`system.rs:186`).
- That function currently `.spawn()`s and discards `osascript`'s stdout, so today nothing is retained about a launched window. Reading a value back requires changing `.spawn()` → `.output()`.
- `ps` already emits a tty column and it is already parsed into `PsRow.tty`. No new subprocess and no new parser are needed to match a tty back to a live process.
- `open_remote_subprocess` is a **plain synchronous `pub fn`**, not `async fn` + `spawn_blocking`. It survives today only because it never waits on its child. Switching to `.output()` would make it a blocking subprocess wait on the IPC dispatch thread — a direct `tauri.A1` / CLAUDE.md never-block-the-UI violation. It **must** become `async fn` + `spawn_blocking` in the same step. `open_local_terminal` is already correctly wrapped.
- Both scan commands are already excluded from `COMPANION_ALLOWED_COMMANDS`, and both launch commands are already included, so the companion seam needs no new entry — only a new optional argument on commands already allowed.
- `externalTermCounts` is a `ref` export of a `src/store/` module, so `services/mirror.js` auto-discovers and mirrors it. A sibling `externalTermGlobalCount` placed in the same file is mirrored on the same mechanism with no transport work.

**Unverified — cannot be checked without running on a Mac** (`coding.B3`; this machine cannot build Tauri/Rust and has no `osascript`):

- **That `do script` returns a tab object whose `tty` property can be read back in the same script.** This is the single load-bearing assumption of the tagging half. It is documented Terminal.app scripting behavior and the intended script is `set t to do script "…" \n return tty of t`, but it has not been executed here. **§3's adoption fallback exists precisely so that this failing degrades to today's behavior rather than to a broken feature** — S3 is written to be revertible without touching S1/S2/S5.
- Whether the tty is populated **immediately** on return or only after the shell has started. If it lags, the tag has to be recorded on the first scan that shows a new tty instead of at launch, which is a small change inside `record_terminal_owner`'s caller and does not alter the model.
- The added latency of `.output()` over `.spawn()`. The AppleScript contains a cold-start poll of up to ~2s. On the blocking pool this is acceptable; it does mean the OPEN → Terminal invoke resolves later than it does today, which the caller must not treat as a hang. Must be observed on a real Mac before the toast timing is judged.
- Whether tty recycling is fast enough on real usage to matter. The pid pin is designed to make this irrelevant, but the frequency is unmeasured.
- All UI geometry claims: that `btn-cell-trigger` (32px wide, `--control-h` tall) sits in the TERMINAL grid header without changing the header row's height, and that the badge overlays do not clip against the header's padding. Layout risk lives only at runtime.
- That `ps -axo … tty=` and AppleScript's `/dev/ttysNNN` normalize to the same string in every case (including a session with no controlling terminal, which `ps` prints as `??`).

**Checked and found to need no change:** `src-tauri/capabilities/default.json` governs core/plugin APIs, not the app's own `#[tauri::command]`s; renaming a command requires updating `lib.rs`'s `invoke_handler` list only. Confirm this against `lib.rs` in S1 rather than assuming it.

## 9. Multi-entity regression guard (CLAUDE.md, ABSOLUTE)

The registry is a **map of entities keyed by tty** — exactly the shape the 1.9.3 regression destroyed.

- There is **no** `clear_terminal_ownership()` and none may be added. Reconcile removes entries **one key at a time**, in a loop over keys it has individually proven dead.
- Every removal function is named by its true scope: `forget_terminal_session(tty)` removes one session's tag. A name like `reset_terminal_owners()` is forbidden — a vague name is what let the wrong blast radius pass review last time.
- **Removing a project must not touch the registry at all.** Attribution resolves "listed" at read time (§5), so project removal has zero write access to ownership state. This is the flow-shape choice (`design.A8`) that removes the whole class of bug rather than guarding against it.
- The wholesale replacement of `externalTermCounts` each tick stays a *rewrite from one scan*, not a clear — the existing comment in `projectStore.js` already records why that is not a multi-entity violation, and it must survive the edit.
- **Verification with ≥2 entities is mandatory before this ships**, per CLAUDE.md: with two projects each holding a live external window, close one project's window and confirm the other project's count is untouched; then remove one project from the list and confirm the other's tagged sessions survive.
- The `CHANGELOG.md` entry must state what was **preserved**, not only what was fixed — e.g. "closing one project's Terminal window leaves every other project's ownership entries intact; removing a project from the list does not delete any registry entry."

## 10. Execution order

Each step is independently reviewable and independently revertible. **S1 + S2 alone already deliver the global badge** (via adoption only), which is the MVP floor if S3's unverified mechanism does not hold.

### S1 — Rust: publish a session inventory, no behavior change

Add `list_terminal_sessions()` returning `{ pid, ppid, tty, cwd, owner: null }` for each subtree root, derived from the existing `scan_terminal_tree` and the existing root rule. Rename `list_external_terminals` → `describe_terminal_sessions` and drop its `paths` argument. Leave `count_external_terminals` in place and untouched.

*Acceptance*: the root-selection code is shared, not copied — one function computes roots for both commands, and the diff shows the existing root test moved rather than duplicated. New unit tests in `system.rs`'s `mod tests` cover tty normalization (`s004`, `/dev/ttys004`, `??`) with no subprocess. `lib.rs`'s `invoke_handler` lists the new and renamed commands. No frontend file changes in this step.

### S2 — Frontend: attribution moves here; global count appears

New pure module `src/utils/terminalOwnership.js` — no Vue, no `invoke`: given `(sessions, projects)` it returns `{ byProjectId, globalCount, ownerOf }` implementing §3's three-rule priority and §5's dedup-by-session complement. `useExternalTerminals.js` polls `list_terminal_sessions()` instead of `count_external_terminals(paths)` and feeds the pure module. `externalTermGlobalCount` is added to `src/store/projectStore.js` beside `externalTermCounts`. `count_external_terminals` and its Rust code are deleted.

*Acceptance*: for every project, the badge shows the same number as before this step (all sessions are untagged, so rule 2 reproduces the old cwd behavior exactly). `externalTermGlobalCount` equals total sessions minus those matching a listed project path, deduped. A failed scan still leaves the previous snapshot standing on both refs. `useExternalTerminals.js` still describes one job; the attribution rule is not in it.

### S3 — Rust: capture the tty and tag it

`open_terminal_with_command` returns `Result<String, String>` (the tty, or `""`), reading `osascript` stdout via `.output()`. `open_remote_subprocess` becomes `async fn` + `spawn_blocking` **in this step, not later**. Both launch commands take an `owner: Option<String>` and record `(tty → owner)` when both are present. `TerminalOwnership` is `app.manage`d in `lib.rs`. `reconcile_terminal_owners` runs at the top of both scan commands: pin the pid on first sight, drop dead entries one at a time. `owner` is populated on the session payload.

*Acceptance*: `grep -n "#\[tauri::command\]" -A2 src-tauri/src/system.rs` shows every command that waits on a subprocess is `async fn` with `spawn_blocking` — specifically `open_remote_subprocess`. No function clears the registry map; removal is per-key only, and the removal function is named for one session. An empty tty is a no-op, never an error, and the window still opens. A unit test drives `reconcile_terminal_owners` against a synthetic tree and asserts that (a) an entry whose tty vanished is dropped, (b) an entry whose tty exists but whose pinned pid is gone is dropped, (c) no other entry is touched by either.

### S4 — Frontend: pass the owner at every launch site

`pokeExternalTermCounts()` is replaced by one funnel, `registerExternalTerminalLaunch({ owner, path })`, which passes the owner through to the launch command and schedules the same 800ms rescan. Wired at both remaining call sites: the OPEN popup's `Terminal` item (owner = `project.id`) and the in-app tab's "open externally" (owner = that tab's scope). DEV/BUILD is no longer one of them — it opens no external window.

*Acceptance*: no call site passes an owner it had to infer — each one already holds the project or the scope. The `action()`-wrapped funnel still works from a companion (the phone's OPEN → Terminal tags identically). No call site calls `invoke('open_local_terminal', …)` directly any more.

### S5 — UI: one button, two scopes

`TerminalCell.vue` → `src/components/TerminalScopeButton.vue`, taking a `scope` prop (project object or global token) and reading its counts from the S2 selector. The TERMINAL column header renders it with the global scope; `th-term-btn` is deleted from `main.css`.

*Acceptance*: the header button shows a cyan badge for the global group's in-app tab count and a slate badge for the global external count, both as `position:absolute` overlays and with no new DOM element in the flow (Extreme Narrow). The TERMINAL header row's height is unchanged. Every project row behaves exactly as before. `TerminalCountBadges.vue` is unchanged — it was already scope-agnostic. Grep confirms no remaining reference to `TerminalCell` or `th-term-btn`.

### S6 — Modal: show provenance

`ExternalTerminalsModal.vue` labels each session `launched from <project>` / `in <project>'s folder` / no label, from the `owner` field plus the same selector. One existing line changes; no new row.

*Acceptance*: an SSH session launched from project A reads "launched from A" even though its cwd is `$HOME`. A hand-opened window in A's folder reads "in A's folder". The modal's per-project grouping still matches the badges exactly.

### S7 — Docs and changelog

Rewrite § "External `Terminal.app` count — derived, never remembered" in `docs/arch/terminal-stack.md` to state §3's reconciliation rule (the existing section's core claim — the scan owns existence — survives and is *strengthened*, not overturned; say so, so a later reader does not think the derived design was abandoned). Update `docs/feat/in-app-terminal.md`'s badge table and `docs/feat/open-popup.md`'s Terminal entries. `CHANGELOG.md` `[Unreleased]`, with the §9 preservation claim spelled out. Check `README.md` and `IntroModal.vue` per CLAUDE.md.

*Acceptance*: no doc still describes the badge as "bucketed by cwd" without qualification; the arch doc names which side owns existence and which owns origin; the changelog states what survives, not only what changed.

**Hard-wrap discipline** (`agent.C3`): every file this workstream touches — plan doc, Rust, Vue, JS, markdown — leaves long lines long. Do not reflow a comment or a bullet because it exceeds a column. WS-E's sweep runs after this workstream, per the backlog's dependency order.

## 11. Open questions for the reviewer

1. Should an SSH window opened from a project's popup really count on that project's badge (§5)? The spawn-origin rule says yes and this doc adopts it, but it makes the badge mean "windows I started from here", not "windows standing here" — a genuine change in what the number claims.
2. Is one number enough for the global badge, or does the user expect the global button to open the sessions modal filtered to unowned sessions? The modal is currently reached from the stack header. Adding a second verb to the global button would violate the "same class, different scope" premise, so this doc leaves it out.
3. Persisting the registry across restarts (§6) — deferred, with a stated validation rule. Confirm the degradation is acceptable before S3 lands.
