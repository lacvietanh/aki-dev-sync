# Plan 1.25

New work plus everything the 2026-08-15 sweep of the active plan docs turned up. Written 2026-08-15. Continues `docs/plan/remaining-1.22.md` (that file's own still-open items — GBOARD, HARDWRAP, WS-B, WS-A S3-S7, TOAST — stand untouched, unrelated to 1.25, not duplicated here) and `docs/plan/done/verify-pending.md` (its existing rows are unaffected except item 5 below, which added one — since confirmed resolved 2026-08-15).

## Ranked table

| # | ID | Description | Kind | Owning file |
|---|---|---|---|---|
| 1 | PIN-DONE | Investigated per the owner's report of "done tasks still pinned" — **already fixed in code, no action** | resolved | `src/composables/useTaskCollection.js:43-52` |
| 2 | PROJ-TOGGLE | Per-project sync/git-check disable toggle — **landed** | resolved | `src-tauri/src/projects.rs`, `src/composables/useBackgroundRefresh.js`, `src/composables/useSyncStatus.js` |
| 3 | TERM-STACK-R | Terminal stack right-dock — built, needs on-Mac runtime verification | feat | `docs/research/terminal-stack-right-dock.md`, `src/composables/useRightDockLayout.js` |
| 4 | PERF-IDLE | High GPU/CPU usage while idle — code fixes built, magnitude saved still unmeasured | improve | `docs/research/perf-idle-gpu-cpu.md` |
| 5 | RSZ-STEP8 | Manual-resize-authority Step 8 on-device test had no tracking row — added, confirmed passed by owner on Mac 2026-08-15 | resolved | `docs/plan/done/verify-pending.md` (new row), `docs/plan/done/wish-terminal-manual-resize-authority.md` |

## Items

### 1. PIN-DONE — done-task auto-unpin, already fixed

The owner's task-note report (`.akidevsync/notes.json`, read 2026-08-15) showed 5 tasks with `done: true` **and** `pin: true` at once, which read as a live bug: marking a task done should clear its pin.

**Investigated, not a code bug.** `toggleProp(task, prop)` (`src/composables/useTaskCollection.js:43-52`) already force-clears `pin` the moment `done` is set true — `if (prop === 'done' && updated.done) updated.pin = false` — and carries a header comment saying so (lines 41-42). This landed in commit `6389010` (2026-07-27), before any of the 5 flagged rows were last touched (`updated_at: 1785512158000` = 2026-07-31). Confirmed with `git blame` and `git log` on the guard line.

It is also functionally inert everywhere pin is read: `countPinned()` and `sortTasks()` (`src/utils/tasks.js:43-65`) both gate on `!t.done` before looking at `pin`, so a done+pinned row never counts or sorts as pinned. The stale flag has no user-visible effect.

**Residual, not scheduled:** the 5 rows in the local `.akidevsync/notes.json` still carry the stale `pin: true` on disk — all 5 share the exact same `updated_at`, meaning they were set in one batch write, not through individual `toggleProp` calls (which each stamp their own `Date.now()`). No code path that writes tasks in bulk with a shared timestamp was found in this pass. Root cause of that one batch write is unresolved and not chased further here — it is cosmetic-only data, not a defect, so not release-scoped. If it recurs after a manual edit of the 5 rows, that would be a real signal worth a follow-up investigation.

### 2. PROJ-TOGGLE — per-project disable toggle

From the owner's pinned note `task-1785805733110`: "toggle disable per projects... để giúp tạm thời không sync, không check git,.. giảm tải hệ thống" (temporarily stop sync/git-check per project to reduce system load).

**Current state — greenfield, no existing per-project flag.** `SyncProject` (`src-tauri/src/projects.rs:41-97`) has no `enabled`/`disabled`/`paused` field. The only existing on/off switch is `syncCheckEnabled` (`src/store/syncCheckStore.js:14-33`) — a single **global** flag, gated into the poll loop at `src/composables/useBackgroundRefresh.js:84` (`if (s > 0 && syncCheckEnabled.value)`) and read again in `useSync.js:43` / `useSyncStatus.js:12`.

**Shape to build:** add `disabled: bool` (default `false`, `#[serde(default)]` per the project's serde-default rule — old `projects.json` records must not break) to `SyncProject`. Gate `useBackgroundRefresh.js`'s per-project loop body (where it currently only checks the global `syncCheckEnabled`) on `!project.disabled` as well, so a disabled project is skipped from both the sync-status check and the git-status check (`useGit.js:32`'s `get_git_info` call) without touching any other project — this is a multi-entity store, so the CLAUDE.md regression guard applies: verify with ≥2 projects that disabling one leaves the other's sync/git polling untouched.

**UI, per this project's Extreme Narrow rule:** no new row or label — communicate the disabled state via the existing project-row chrome (dim/outline the row, or repurpose an existing icon state) rather than adding a visible toggle switch element, unless the existing pattern for other per-project settings already has a slot for one.

**Result:** built as specified. `SyncProject` (`src-tauri/src/projects.rs`) carries `disabled: bool` (`#[serde(default)]`, old `projects.json` records unaffected). `useBackgroundRefresh.js`'s git tick and `useSyncStatus.js`'s `checkAllSyncStatus()` both filter `!project.disabled` before fanning out, alongside the existing global `syncCheckEnabled` gate. The toggle lives as a checkbox in the Project Configuration modal; a disabled row is dimmed with a tooltip, per the Extreme Narrow rule (no new row/label). **What still works on a disabled project:** the per-project Refresh button, PUSH/PULL, and the git modal — all manual paths go through `fetchGitStatus`/`checkProjectSyncStatus` directly and never consult `disabled`. Doc-synced: README.md, `IntroModal.vue`, `docs/feat/background-refresh.md`.

### 3. TERM-STACK-R — terminal stack right-dock

From the owner's pinned note `task-1786581837403`: the terminal stack should stop being a "stack that flies over the top" of the main project list when the app is narrow, and become a right-side column when the app is wide — main view capped at ~900px, terminal taking the extension space, with a defined breakpoint between the two layouts.

**Entry points, current layout:** `src/components/AppConsole.vue:4-16` — top-level dock container (`.dashboard-bottom`), stacks `<TerminalStack />` then `<LogStack />` vertically. `src/components/dock/TerminalStack.vue:8-58` and the shared chrome in `src/components/DockStack.vue:10-39` (splitter, header, collapsible body) back both stacks today. `src/composables/useDockLayout.js` holds the height/state logic — it was just reworked in 1.24.0 for independent per-stack height (`docs/plan/done/dock-stack-independent-height.md`), so a right-dock redesign needs to read that doc first to avoid re-fighting the same layout state.

**Research stage done:** the breakpoint behavior and how `useDockLayout.js`'s current height-sum model maps onto a width-based right-column mode are scoped in [docs/research/terminal-stack-right-dock.md](../research/terminal-stack-right-dock.md). **Still pending:** owner input on the design question (exact breakpoint px, whether `DockStack.vue`'s shared chrome fits a right-column presentation or needs its own variant) before a `docs/plan/wish-*.md` doc and code can follow — same two-step shape as `restore-terminal-mobile-ux.md` → `wish-terminal-manual-resize-authority.md`.

**Result (revised 2026-08-16 after Mac test):** The initial build had MAIN_VIEW_MAX_WIDTH=900 and a draggable width splitter — on the real app at 1920px the main view occupied too much of the screen. Revised spec from owner: main view capped at **420px**, terminal column fills **all remaining space** via `flex:1` (no fixed width, no drag splitter), trigger at **900px** window width. Implemented: `src/composables/useRightDockLayout.js` now exports `MAIN_VIEW_MAX_WIDTH=420` and `RIGHT_DOCK_BREAKPOINT=900`, all width-drag/persistence code removed; `AppConsole.vue` simplified (no `consoleStyle` width branch, no splitter div, no pointer handlers); `main.css`'s `.dashboard-main.is-right-dock > .dashboard-bottom` adds `flex: 1`. **Unverified — needs a runtime re-check on Mac**: resizing across the 900px breakpoint and confirming 420px main / fill-rest terminal.

### 4. PERF-IDLE — high GPU/CPU usage on idle

From the owner's pinned note `task-1786650061322`: the app appears to consume significant GPU even while idle; asks for a flow/code investigation of what is running and why, not a blind fix.

**Not diagnosed yet — this is the candidate list a future investigation starts from**, gathered by grepping for anything that could run continuously regardless of visibility (`requestAnimationFrame`, short-interval `setInterval`, unconditional CSS `animation: … infinite`, unbounded `ResizeObserver`):

- `src/assets/main.css:420` — `.skeleton-box { animation: skeleton-pulse 1.5s infinite }`, a shared class used app-wide; runs continuously for as long as any skeleton stays mounted. If a load ever silently fails to resolve and leaves a skeleton mounted permanently, this animates forever. Highest-suspicion candidate.
- `src/components/AgentUsage.vue:1231,1237,1411,1419,1427` — same `pulse 1.5s infinite` pattern on its own skeleton elements.
- `src/components/AppHeader.vue:1121` — `animation: pulse-red 2s infinite` on a notification dot; needs checking whether its parent is conditionally rendered or always in the DOM.
- `src/assets/main.css:837` — `.status-dot { animation: pulse 1s infinite alternate }`, used by `src/components/dock/LogStack.vue:24` inside a "SYNCING..." label; needs checking whether that label unmounts when not syncing or just hides visually (hidden-but-mounted still animates).
- `src/components/TerminalView.vue:364,486` — `ResizeObserver` driving a coalesced `requestAnimationFrame` fit; bursty on resize, not a standing loop — lower suspicion, listed for completeness.
- `src/services/bridge.js:305` (15s), `src/components/UsageCircle.vue:123` (10s), `src/components/AgentUsage.vue:266,698` (10s + a second independent clock timer) — all ≥1s intervals, unlikely GPU contributors on their own but worth confirming none run when their owning component should be unmounted.
- 14 `transition: all` declarations not scoped to a specific property (`src/components/tasks/TaskListPanel.vue:313,368`, `src/components/modals/ClaudeCleanupModal.vue:290`, `src/components/modals/SshConfigModal.vue:91`, `src/components/AppHeader.vue:1234`, `src/components/AgentUsage.vue:1105`, `src/assets/main.css:275,310,519,686,997,1060,1254,1275`) — costs compositor work per triggered transition, not idle-standing, but `transition: all` is worth narrowing to the actual animated properties regardless of the GPU question (Law 1/7 hygiene).

**Research stage done:** the candidate sweep and confirm-or-rule-out order are recorded in [docs/research/perf-idle-gpu-cpu.md](../research/perf-idle-gpu-cpu.md). **Still pending:** the on-Mac runtime check (Activity Monitor GPU column / Web Inspector Timelines) to isolate the real cost, and a code decision, both owner input.

**Result:** the code-decision half is done, the magnitude question is not. The header notification dot's always-on `animation: pulse-red 2s infinite` (`AppHeader.vue`) is dropped — confirmed by reading the rule directly, it now reads `animation: pulse-red 2s;` (no `infinite`), so it plays once instead of standing forever. The skeleton animations (`.skeleton-box`, `AgentUsage.vue`'s own skeleton blocks) were confirmed by reading the surrounding markup to be gated behind a loading-state `v-if`/`v-else-if`, not permanently mounted, so they cannot be the "runs forever" case this item worried about. Of the 14 `transition: all` sites this item's candidate list named, 2 remain by design — `.btn-tech` and `.btn-cell-trigger` in `main.css`, shared base classes with heterogeneous per-variant modifiers, interaction-only (hover/focus) so they carry no idle cost — every other site now names the specific properties it animates; verified directly by grepping every file this item listed. **Still unmeasured, explicitly not claimed as measured:** how much GPU/CPU any of this actually saved. That needs a Mac profiler run (Activity Monitor GPU column or Web Inspector Timelines) against the built app, which has not happened.

### 5. RSZ-STEP8 — missing verify-pending row for manual-resize-authority Step 8

Found during the 2026-08-15 sweep of active plans: `wish-terminal-manual-resize-authority.md`'s 8-step "smallest correct shippable V1" is fully landed and shipped in `1.24.0`. Step 8 (the on-device runtime test — Mac auto-drive unchanged, phone "Fit to my screen" tap on both a plain shell and a TUI, Mac reclaim pill) had no row in `docs/plan/done/verify-pending.md`, unlike every other shipped-but-unverified item in this repo, so a row was added there (Terminal section, T6).

**Result:** confirmed passed by the owner on a real Mac+phone, 2026-08-15. Both `docs/plan/done/verify-pending.md` and `docs/plan/done/wish-terminal-manual-resize-authority.md` are closed.

## Not in scope / deliberately not doing

- **Re-diagnosing the PIN-DONE batch-write source** (item 1) — cosmetic-only, no user-visible effect, not chased further this round.
- **Measuring PERF-IDLE's actual GPU/CPU savings** — the code fixes are built (item 4) but the magnitude they bought is not measured; that needs a Mac profiler run, owner input.

## Mac handoff — what genuinely needs a human on a Mac

Everything in this round that static reading, typecheck or a rule audit can settle **has been settled and is not listed here** (`coding.B3`: never park finished work behind a manual test an automated tier already proves). What follows is only what depends on a compiler, a live macOS API, a real second device, or a human eye. Ordered so a failure early stops the rest.

### M1 — `cargo` has never run against any of this round's Rust · BLOCKING

No Rust in this round has ever been compiled; this box cannot run `cargo` (`CLAUDE.local.md`). New or changed: the spawn-origin ownership registry and `osascript` tty readback in `src-tauri/src/system.rs`, the `disabled` field on `SyncProject` in `src-tauri/src/projects.rs`, and the command registrations in `src-tauri/src/lib.rs`.

```bash
cd ~/aki/app/Aki-Dev-Sync && cargo check --manifest-path src-tauri/Cargo.toml
```

**Result (2026-08-16, Mac):** ✅ Passed — `Finished dev profile [unoptimized + debuginfo] target(s) in 5.54s`. All Rust in this round compiles clean.

Every item below assumes this passes.

### M2 — terminal ownership: does the tty readback actually return a tty?

The one mechanism the design doc itself flags as unverifiable off-Mac (`docs/plan/done/terminal-ownership-model.md` §8 — feasibility, verified vs unverified). `osascript` may return `""` for the new window's tty, in which case every session silently falls back to cwd-matching and the feature is inert rather than wrong.

- Open a project's OPEN popup → Terminal (local). Within one poll tick that project's `TERM` badge should increment, **and** the External Terminals modal should say "launched from &lt;project&gt;", not "in &lt;project&gt;'s folder".
- Then `cd` that same window somewhere unrelated. The badge must stay on the launching project (this is the whole point of spawn-origin over cwd).
- Open a plain `⌘T` Terminal.app tab by hand in the same folder. It must **not** claim to be launched from the project — it should read as a cwd match only.
- Open an SSH session from the popup. Its cwd is the local `$HOME`, so before this round it counted on nobody; it must now count on its project.

### M3 — right-side dock, the whole geometry path

Built and doc-synced. `src/composables/useRightDockLayout.js` is the single source of the trigger (900px breakpoint) and max-width (420px).

- Drag the window across 900px in both directions: above it TerminalStack moves to the right column taking remaining width (`flex: 1`), LogStack docks in `dashboard-left` under ProjectTable, and the project table caps at 420px; below it the layout returns to the bottom dock as in 1.24.0.
- When right-docked: confirm terminal mode is dedicated and clean — no redundant minimize/collapse buttons, and the LogStack below ProjectTable collapses to 44px (header + peek line) and expands to 20vh without duplicating DOM peek lines.
- In bottom dock mode (< 900px): confirm both TerminalStack and LogStack retain independent collapse/maximize/splitter controls.

### M4 — per-project disable toggle, with ≥2 projects present

Mandated by this project's multi-entity regression guard (`CLAUDE.md`), which the 1.9.3 incident wrote: verifying only the entity the change targets is exactly the gap that shipped that bug.

- With at least two projects configured, disable one from its Settings gear. Confirm the **other** project's background sync and git polling keep running untouched.
- On the disabled project, confirm the manual paths still work: the per-project Refresh button, PUSH/PULL, and opening the Git modal. Only the two background timers are supposed to skip it.

### M5 — terminal chrome menu across two devices

Preferences are per-device `localStorage` and are deliberately never mirrored, so one device proves nothing about the other.

- On the Mac: toggle each piece of chrome from the 3-dot drop-up; confirm the compose input row is now its own toggle and defaults **on** (it is the only working path for macOS's Vietnamese composing IME — worth typing a Vietnamese line through it).
- On a paired phone: confirm the tab strip is checked and locked (it is the only way to switch tabs there), and that toggling anything on the phone leaves the Mac's own settings alone.

### M6 — toast position, by eye

Moved from bottom-center to top-end below the titlebar. Trigger any toast while the ProjectTable ACTIONS column is visible and again with a modal open: it must cover neither the ACTIONS column nor the modal's footer buttons.

### M7 — `scripts/fix-ssh-agent-leak.sh`, never executed anywhere

Written this round, run on no machine. It edits a real `~/.zshrc`, so it defaults to a dry run (`docs/ref/ssh-agent-leak.md` — what the leak is and why the app amplifies it).

```bash
bash scripts/fix-ssh-agent-leak.sh            # dry run, prints the diff it would make
bash scripts/fix-ssh-agent-leak.sh --apply    # only after reading that diff; writes a timestamped backup first
```

Afterwards, confirm new login shells still get a working agent (`ssh-add -l`) and that orphan `ssh-agent` processes stop accumulating.

### M8 — PERF-IDLE magnitude, still unmeasured (not a gate)

The open half of item 4. Activity Monitor's GPU column, or a Web Inspector Timelines capture, against the built app while idle — to find out whether this round's animation and `transition: all` cleanup actually bought anything.

### Repo state at handoff

Shipped as `1.25.0` (tag pushed, GitHub Release published, universal dmg built and verified locally — `cargo build --release` succeeded for both architectures). M1 passed on this Mac as part of that build. **M2–M7 above are still outstanding** — they need a human on the built app (and a paired phone for M5) and were not run as part of the build/release itself.

### Older backlog, not this round

`docs/plan/remaining-1.22.md`'s own open items (GBOARD, U3, HARDWRAP, WS-B, WS-A S3-S7) are untouched and unrelated to 1.25. Do not fold them into this handoff.

## Prompt for the Mac session

Paste this to Claude Code on the Mac. It carries only what a fresh session cannot read off disk; everything else it is told to read itself.

```
Đây là Aki-DevSync (Tauri v2 + Vue 3, ship macOS-only). Toàn bộ thay đổi của đợt 1.25 đang nằm trong working tree, chưa commit, trên nền tag 1.24.0. Code đã viết xong và đã qua audit tĩnh trên máy dev Linux; thứ chưa từng chạy là phần cần Mac.

Đọc `docs/plan/remaining-1.25.md`, mục "Mac handoff — what genuinely needs a human on a Mac" (M1–M8). Đó là danh sách duy nhất cần làm, đã lọc bỏ những kiểm tra mà đọc tĩnh/typecheck đã kết luận — đừng thêm việc ngoài danh sách đó.

Chạy M1 trước (`cargo check` — chưa dòng Rust nào của đợt này từng được compile). M1 hỏng thì dừng, sửa, báo tôi; M1 xanh thì đi tiếp M2→M7 theo thứ tự, M8 là đo đạc không phải cổng pass/fail.

M2, M4, M5 cần tôi thao tác tay và nhìn kết quả — nêu rõ từng bước bạn cần tôi bấm gì, rồi chờ tôi báo lại, đừng tự kết luận thay. M7 sửa `~/.zshrc` thật: dry-run trước, cho tôi xem diff, chỉ `--apply` khi tôi đồng ý.

Sửa lỗi phát sinh thì sửa tại gốc và cập nhật `Result` của hạng mục tương ứng trong `docs/plan/remaining-1.25.md` cùng `CHANGELOG.md` nếu hành vi đổi. Không commit, không tag, không push — tôi tự quyết.

Luật dự án ở `CLAUDE.md` và bộ rule chung ở `~/.aki/akidevrule/`; đọc `index.md` rồi nạp rule liên quan trước khi sửa code.
```
