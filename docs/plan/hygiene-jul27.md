# Hygiene batch — jul27 (WS-E)

Status (2026-07-30, plan-consolidation pass): **2 of 3 closed, 1 unclassified — this doc stays active only for item 3.**

- **Item 1 (WebSocket reconnect noise) — closed, no code change.** The verdict was independently re-traced against `src/services/bridge.js`; nothing in app code emits the message and every close path converges through a capped backoff. Nothing to execute.
- **Item 2 (project-icon 404) — fix applied, uncommitted.** `src/utils/projectIcon.js` carries the edit in the working tree and it has been audited in §2 below. Nothing further to design.
- **Item 3 (hard-wrap sweep) — UNCLASSIFIED, owner's call (`agent.B5`).** The sweep has already run across 49 doc files (~1409 insertions / 4937 deletions) and none of it is committed. From the tree alone, finished-work-awaiting-a-commit and an abandoned mid-pass are indistinguishable, so this is not auto-classified. The one-word question for the owner: **is the uncommitted hard-wrap sweep finished, or abandoned?** If finished, this doc moves to `done/` and only its "unsafe-to-collapse" list needs preserving; if abandoned, §3's inventory is still the live work list.

This doc was deliberately **not** moved to `done/` for that reason alone. Everything below is the original triage.

Owner workstream for `docs/plan/backlog-jul27.md` WS-E. Three unrelated items triaged together because none blocks or is blocked by the other workstreams. This doc is a **triage report** (agent.B5): findings and verdicts only — nothing in the tree has been touched to produce it.

## 1. Companion WebSocket reconnect noise

**Symptom**: `ws://127.0.0.1:1421/ws?role=host&token=...` failing with "The network connection was lost."

**What was found**

- `1421` is the companion web-server port — `src-tauri/src/web_server.rs:78` (`const PORT: u16 = 1421;`), the one LAN entry axum binds in both dev and release (`src-tauri/src/web_server.rs:705`).
- `role=host` is the Mac's own Tauri webview connecting to its own embedded relay, not a phone: `src/services/bridge.js:170-173` builds that exact URL only when `isHost` is true, and the Rust side only grants the host role to a loopback peer holding the process-local `host_token` (`src-tauri/src/web_server.rs:929-955`) — a secret only the Mac's webview ever receives (`src/services/bridge.js:144-168`). So this is the app talking to itself over loopback, not a remote client.
- Nothing in app code logs the string "The network connection was lost" — that is WebKit's own native network-layer console line for a failed `WebSocket` connect, not something `console.error`/`console.warn` produced. The socket's own `error` listener (`src/services/bridge.js:440-443`) only sets `connectionState.value = 'error'`; it emits nothing to console. Confirmed by grepping the whole `src/` tree for that string and for any "failed:" pattern near bridge.js — no hits in app code (only unrelated strings in `useGit.js`/`useSync.js`).
- Reconnect is automatic and unconditional for this case: the `close` handler (`src/services/bridge.js:406-439`) falls through to `connectionState.value = 'closed'; scheduleReconnect()` for any host-side close other than `CLOSE_HOST_ROLE_REJECTED`, and `scheduleReconnect()` (`src/services/bridge.js:355-362`) retries with exponential backoff from 1s up to a 10s cap, forever. A dropped host socket cannot get "stuck" — the design comment at `web_server.rs:1013` even calls a rapid host-side reconnect racing cleanup "a known, accepted edge case."
- The 1421 axum listener is torn down and rebound on: (a) any full app-process restart (Cargo/Tauri rebuild of the Rust binary — dev-only), and (b) macOS sleep/wake dropping the loopback TCP session at the OS level. Both produce exactly this WebKit message on the client side while the reconnect loop is already retrying.

**Verdict**: **expected churn, not a real bug** — with one caveat marked unverified below. The message is the browser's own network log for a self-connection that is designed to retry, and every code path that closes this socket routes back into `scheduleReconnect()`. This matches "expected churn during dev reloads / sleep / server restart," not a broken feature.

**Unverified**: whether the reconnect actually *succeeds* every time (i.e., the relay always comes back up in time, and the webview isn't left in a stuck `'error'`/`'connecting'` state under real sleep/wake) can only be confirmed by watching `connectionState` on a live Mac across an actual sleep or dev-rebuild cycle — static reading proves the retry loop exists and is unconditional, not that it always converges. Needs runtime evidence per `coding.B3`.

**Proposed fix**: none required if the unverified point holds. If runtime evidence shows the loop ever gets stuck (not just noisy), the fix is scoped to `openSocket()`'s `error`/`close` handlers in `src/services/bridge.js` only — no blast radius beyond that one file, since `web_server.rs`'s side already restarts cleanly on its own.

**Acceptance criteria**: on a real Mac, trigger (a) a dev rebuild (`cargo`/`tauri dev` hot-restart) and (b) a sleep/wake cycle, and confirm `connectionState` returns to `'open'` within one backoff cycle (≤10s after the relay is reachable again) in both cases, with no `console.error` from `bridge.js` during the transition (only the browser's own uncontrollable network log, which is expected and out of scope to suppress).

**Status (jul28): closed, no code change.** Independently re-traced the whole reconnect flow in `src/services/bridge.js` against this section's claims — both hold: (i) grepped `src/` and `src-tauri/` for the literal string "network connection was lost" — zero hits; every `console.error`/`console.warn` in `bridge.js` (lines 158, 161, 245, 263, 280, 308, 392) is a distinct app-level message, none of them this one, confirming it really is WebKit's own network log. (ii) the backoff converges: `reconnectDelayMs` starts at 1000ms, doubles per `scheduleReconnect()` call (line 361) capped at `MAX_RECONNECT_DELAY_MS` = 10000, and resets to 1000 on the next successful `open` (line 400) — no hot loop. No leaked second socket: `connect()` only proceeds past its open/connecting guard (line 370) when the module-level `ws` is not already live, JS's single-threaded execution means that guard and the reassignment inside `openSocket()` cannot interleave, and the `if (ws !== socket) return` staleness check in both `close` (line 410) and `error` (line 441) handlers stops a superseded socket's late events from clobbering the live one's state or scheduling a redundant reconnect. `connect()` itself is called from exactly one place at boot (`src/services/index.js:41`, guarded by a `started` idempotency flag) plus internally from `scheduleReconnect()` and `pairDevice()` — no other call site exists that could race it. No defect found; verdict unchanged from the original triage.

**Still unverified**: the original caveat stands — whether the relay actually comes back up in time on a real sleep/wake or dev-rebuild cycle can only be confirmed by watching `connectionState` on the Mac live; nothing above substitutes for that runtime check.

## 2. Project-icon 404 logged as an error

**What was found**

- `src/utils/projectIcon.js:15-18` builds the host `<img>` src as `${assetBase}${id}?t=${timestamp}` unconditionally whenever `assetBase` (i.e., `isHost`) is truthy — it does **not** consult `projectStore.projectIcons`, even though that store already holds a complete, null-aware map of which projects have an icon (`src/store/projectStore.js:82-96`, populated by `refreshProjectIcons()` and called at boot in `src/App.vue:83`).
- The Rust protocol handler (`src-tauri/src/lib.rs:35-58`) returns a genuine HTTP **404** with an empty body when a project has no cached icon (`src-tauri/src/lib.rs:51-57`) — a real, correctly-shaped fallback response, not a crash or malformed reply.
- Every Vue call site (`src/components/dock/TerminalStack.vue:34`, `src/components/ProjectTable.vue:88,155`, `src/components/modals/GitModal.vue:10`, `src/components/modals/ProjectTasksModal.vue:9`) only wires `@error` to a local boolean flag that hides the `<img>` — none of them call `console.error` or `console.warn`. App code does not log this 404 anywhere.
- Conclusion on the distinction the user asked to pin down: the 404 line the user sees in the console is the **browser's own network-layer log** for a completed request that returned 404 — the same category of unsuppressible-from-JS noise as item 1's WebKit message, not an explicit `console.error` call anywhere in this app's code. There is no `console.error` to delete; nothing in JS is emitting this.

**Verdict**: **not fixable as stated** — "stop the 404 from being logged" has no code-level lever, because nothing in this codebase logs it; it's the browser's own devtools network entry for a real 404 HTTP response. The Rust fallback is correct as-is (a real 404, matching HTTP semantics) and needs no change.

The only real fix is the one the task description anticipates: **stop requesting the URL at all when the icon's absence is already known**, which is cheaply knowable here — `projectStore.projectIcons` is already populated by `refreshProjectIcons()` on the host at `src/App.vue:83`, as a complete `{ [id]: dataUri | null }` map (per the comment at `src/store/projectStore.js:82-86`, written specifically so companions never 404). `projectIcon.js`'s host branch simply never reads that map; the companion branch two lines below it already does (`src/utils/projectIcon.js:18`).

**Proposed fix**: in `src/utils/projectIcon.js`, have the host branch also consult `projectIcons.value[id]` and return `''` (render no `<img>`) when that entry is explicitly `null`/absent, mirroring the companion branch's logic instead of unconditionally building the `aki-devsync-icon://` URL. Blast radius: one function in one file (`projectIconSrc`); the four Vue call sites already handle `''` as "no image" via their existing `v-if`/`@error` guards, so they need no change. No Rust change needed.

**Acceptance criteria**: opening a project with no icon shows no `<img>` request in the network log at all (not a 404, an absent request); a project that *does* have an icon still resolves via `aki-devsync-icon://` unchanged; `refreshProjectIcons()`'s existing timing (called once at boot, and after any icon-affecting action per `src/store/remoteActions.js:401`) is not modified.

**Status (jul28): applied edit audited, comment corrected, logic kept.** The orchestrator's hand-edit to `src/utils/projectIcon.js` was reviewed on all three axes and the code change is correct as written:

- **(a) Logic vs the `projectIcons` contract** — confirmed against `src/store/projectStore.js:82-96` and the Rust side (`get_project_icons_map`, `src-tauri/src/web_server.rs:1488-1503`): the map genuinely is complete-with-explicit-null every time it is filled (Rust inserts one entry per currently-known project, `Some(uri)` or `None`, never omitting an id), and it is refilled after every project add/edit (`src/store/remoteActions.js:401`, awaited before the caller continues). The edit's "absent id must still be requested" fallback is not a hedge against a real gap — it is the correct behavior for the one window where absence is expected: boot, where `App.vue:83` calls `refreshProjectIcons()` without awaiting it, so `projectIcons.value` is still `{}` while the first render happens. During that window every id is genuinely absent and every icon request correctly still goes out unconditionally, same as pre-fix behavior. Verdict: **kept, no logic change**.
- **(b) Comment vs `agent.C3` and `docs.B3`** — measuring this file's own pre-existing comments (the header block and the `assetBase` comment, both already wrapped ~94-101 cols before this edit), the new block's wrap column was not out of line with the file's local convention, so no C3 rewrap was needed on that axis. The real problem was length and duplication: the added ~11-line block re-narrated this section's own diagnosis (WebKit's 404 log, the Rust fallback, the "already known" reasoning) inline in source, which is exactly what `docs.B3` says not to do — a doc (this one) already carries that reasoning in detail, so the comment should point at it, not restate it. Rewrote the block down to ~5 lines that state the *what* and link to `docs/plan/hygiene-jul27.md §2` for the *why*.
- **(c) Right module?** — `projectIconSrc()` is this file's own stated purpose ("one place that answers what goes in this `<img>`'s src", per the file's own header), and the fix is a pure function of `(id, timestamp, projectIcons)` with no side effects — the right place, not a call-site concern. No Rust change needed, matching the original diagnosis.

**Remains unverified**: opening a project with no icon produces no network-log entry at all (not just no console error) can only be confirmed by watching devtools' Network tab on a live run — not checked here per the no-dev-server constraint on this task.

## 3. Hard-wrap violations — inventory (no fixes performed)

**Method**: scanned `src/**/*.js`, `src/**/*.vue`, `src-tauri/src/*.rs`, and `docs/**/*.md` for runs of ≥3 consecutive comment/prose lines whose lengths cluster within 3 characters of a shared maximum ≥90 — the signature of word-wrap-to-column rather than one-sentence-per-line prose. Markdown scan excludes fenced code blocks, table rows (`|...|`), and headers.

**What was found — wrap column and scope per bucket**

| Bucket | Files flagged | Lines flagged | Predominant wrap column |
|---|---|---|---|
| `src/**/*.js` + `*.vue` | 67 | ~1,286 | 96–101 (a few 92–106 outliers) |
| `src-tauri/src/*.rs` | 13 | ~953 | 95–102 |
| `docs/**/*.md` | 44 | ~2,649 | 93–105 |
| **All flagged** | **124** | **~4,888** | **~96–102 modal** |

This is not confined to recent edits — it is a near-universal ~100-column wrap applied to almost every multi-line block comment and doc paragraph across the whole tree (examples: `src/services/bridge.js:13-17`, `src/store/projectNotesStore.js:1-12`, `src-tauri/src/web_server.rs:1-45`, `docs/plan/done/remote-control.md:34-44`). Sampled blocks read as mid-sentence breaks at the same column (e.g. `bridge.js:13-17` splits one sentence across 5 lines each ending 94–101 chars in), not independently-sized bullets — this is auto-wrap, not the file's own deliberate convention. Per `agent.C3`'s own wording, this reads exactly as "a learned training-data habit," applied so uniformly that it is effectively the *codebase's* accidental default rather than any one file's *chosen* one — so the C3 "match the file's existing convention" escape hatch does not apply; there is no deliberate convention here to match, only an unremarked one to fix.

**Bucket split — touched by the current uncommitted diff vs. untouched (legacy)**

Touched = `git diff --name-only` plus untracked files (both listed in `git status`), cross-referenced against the flagged-file list above.

| Bucket | Files | Approx. lines |
|---|---|---|
| **Touched by current diff** (JS/Vue: `App.vue`, `hostInvoke.js`, `AppConsole.vue`, `TerminalView.vue`, `ProjectTasksModal.vue`, `dock/TerminalStack.vue`, `useExternalTerminals.js`, `useProjectConfig.js`, `useDockLayout.js`, `useProjectNotes.js`, `useTerminalFont.js`, `useWkImeGuard.js`, `projectNotesStore.js`, `remoteActions.js`, `tasks/NotesField.vue`, `tasks/TaskListPanel.vue`, `ExternalTerminalsModal.vue`; Rust: `lib.rs`, `projects.rs`, `system.rs`, `project_notes.rs`; docs: `terminal-stack.md`, `in-app-terminal.md`, `project-task-list.md`, `1.22.0-notes-json-ssot.md`) | 25 | ~750 |
| **Untouched legacy sweep** (everything else flagged, e.g. `src/services/bridge.js`, `src/services/mirror.js`, `src/components/AgentUsage.vue`, `src-tauri/src/pty.rs`, `src-tauri/src/statusline.rs`, most of `docs/plan/done/*.md` and `docs/research/*.md`) | ~99 | ~4,138 |

The touched bucket is what the user is reacting to directly; the untouched bucket is roughly 5–6x larger and is a separate, much bigger legacy sweep.

**Unsafe-to-collapse flags** (for when the fix pass runs — do not act on these now):
- `docs/plan/backlog-jul27.md` contains a markdown table (workstream index) — table rows are structurally atomic; never rejoin or rewrap them.
- Several `docs/plan/done/*.md` and `docs/ref/*.md` files use `---` horizontal-rule lines as section separators (e.g. `docs/plan/done/push-only-paths.md`, `docs/ref/claudecode-cleanup-paths.md`) — verify each is a markdown thematic break, not YAML frontmatter, before any tooling treats it as a delimiter.
- Multi-line `import { ... } from '...'` blocks (e.g. `src/store/remoteActions.js:21`, `src/components/TerminalView.vue:105`) are already one-symbol-per-line and must stay that way — these are not wrapped prose and are not part of this sweep's scope.
- CHANGELOG.md's `[Unreleased]` bullet entries are one-entry-per-line by convention; do not merge multiple bullets even if adjacent ones look like continuations.

**Verdict**: real, widespread violation — confirmed by pattern (not just reported), but the fix is **not fixable in this triage run** by design (`agent.B5`); this section is inventory only.

**Proposed fix**: a follow-up rewrap pass, scoped file-by-file, that (a) rejoins each flagged comment/prose run into one logical line per sentence/bullet, (b) explicitly skips every item under "unsafe-to-collapse" above, and (c) is done in two separate PRs/commits matching the two buckets above (touched-by-this-diff first, legacy sweep second) so review stays reviewable. Blast radius: touches comment/prose text only, zero code-logic lines — but at ~4,888 lines across 124 files it will diff-collide with any other in-flight workstream touching the same files line-for-line, which is exactly why the backlog schedules it last.

**Acceptance criteria**: no file in either bucket has a comment/prose run matching the detection heuristic above; every item in "unsafe-to-collapse" is verified untouched by diffing before/after; the touched-bucket commit lands only after every other WS-A/B/C/D diff affecting the same 25 files has already merged.

## Ordering

1. **Item 2 (icon 404)** first — smallest, most precise fix (one function), zero collision risk with other workstreams, and already fully diagnosed above.
2. **Item 1 (WebSocket noise)** second, gated on the runtime evidence called out above — if the sleep/wake or dev-rebuild check shows the reconnect ever fails to converge, schedule a fix then; otherwise this item closes with no code change.
3. **Item 3 (hard-wrap sweep)** last — and explicitly **last among all code workstreams in the jul27 batch** (WS-A, WS-B, WS-C, WS-D included), not just last within WS-E, per the backlog's own dependency note: at ~4,888 lines across 124 files it will collide line-for-line with any diff that touches the same files, so every other workstream must land first.
