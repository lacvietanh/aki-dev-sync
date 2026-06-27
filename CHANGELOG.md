# Changelog

All notable changes to this project will be documented in this file.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.0.0/) · [Semantic Versioning](https://semver.org/spec/v2.0.0.html)

---

### [1.4.2] - 2026-06-27

#### Fixed
- **UTF-8 crash on non-ASCII log preview** (`agent_usage.rs`): The `preview()` log helper sliced strings at a raw byte index (`&s[..max]`), which panicked and aborted the whole app when the cut landed mid-character — e.g. a Vietnamese Claude Code `session_name` ("Khảo sát cơ ch…") embedded in the cached usage JSON. The slice runs while building the `format!` argument, so it crashed even **without** `--debug`. Now walks back to the nearest char boundary before slicing, protecting all `preview()` call sites.
- **Non-ASCII filenames mangled in git status** (`git.rs`): `git status --porcelain` octal-escapes non-ASCII paths (`"\303\251"`), so Vietnamese/emoji filenames displayed as escape sequences. Added `-c core.quotepath=false` so git emits real UTF-8.
- **Post-build DMG rename no-op for arm-triple builds** (`scripts/post-build.js`): `tauri build --target aarch64-apple-darwin` emits the DMG to the triple-specific `target/aarch64-apple-darwin/release/bundle/dmg` dir, which the rename step did not scan — producing "No matching DMG files found to rename." Added that directory to the scan list.

---

### [1.4.1] - 2026-06-27

#### Added
- **Antigravity Plan Status Monitoring**: Natively extracts the `userTier` object from the `GetUserStatus` Connect RPC payload to display the active subscription tier (e.g. "Google AI Pro") as a styled premium badge next to the Antigravity header in `AgentUsage.vue`.
- **Last Cached state for Antigravity**: Displays the last successful quota snapshot when the local Antigravity IDE is turned off or offline, complete with an auto-updating relative time indicator (e.g. "Cached 5m ago") that refreshes every 10s.
- **Tauri Async Executor Protection**: Wrapped the blocking synchronous child processes for Antigravity and Claude Code monitoring inside `tauri::async_runtime::spawn_blocking` to prevent CPU starvation on the main async executor.
- **Stale State computation fix for AG**: Reworked the stale state check to run against `fetched_at` age instead of Claude-specific `rate_limits` structure.

### [1.4.0] - 2026-06-27

#### Added
- **Live Sorting for projects list** (`ProjectTable.vue`): Drag and drop to reorder projects, with Vue 3 `<transition-group>` for smooth transitions. Order is persisted to `projects.json` on drag end.
- **Mac/Tauri Drag & Drop Pitfall solutions** (`tauri.conf.json`, `ProjectTable.vue`):
  - Disabled `dragDropEnabled` in native window config to prevent Tauri Rust layer from swallowing HTML5 drag events.
  - Added `-webkit-user-drag: element !important` in CSS to force WebKit engine to recognize custom div rows as draggable.
  - Set `draggable="false"` on `<img>` icons and `pointer-events: none` on handle children to prevent native image dragging conflict. Added `z-index: 1` and a high-contrast dark opacity overlay on handle hover to keep the dotted drag indicator clearly visible above custom icon images.
  - Mitigated Vue reactivity delay by using static `draggable="true"` and synchronous handle state checks.
  - Solved list jittering/feedback loop with **Midpoint Geometric Threshold** (Hysteresis dead-zone) to only swap when cursor crosses target midpoint.
- **Scientific CSS Grid Layout & spacing** (`ProjectTable.vue`):
  - Unified grid column structure using CSS Custom Variables (`--grid-cols`, `--grid-gap`) and `rem` units defined on the parent container. Both header and rows inherit these variables, ensuring pixel-perfect alignment without code duplication (DRY pattern).
  - Configured desktop layout to `--grid-cols: 13.5rem 5rem 3.8rem 1fr` with `0.5rem` gap, and narrow layout (under 800px) to `--grid-cols: 11rem 4.5rem 3.5rem 1fr` with `0.25rem` gap.
  - Standardized left-alignment (`text-align: left`) on all header cells and row cells.
  - Created [docs/feat/drag-and-drop.md](docs/feat/drag-and-drop.md) to document the technical insights and solutions.
- **Manual UI improvements** (`AppHeader.vue`, `main.css`, `AgentUsage.vue`):
  - Refactored `app-version` layout into separate `version-num` and `build-time` spans for cleaner alignment.
  - Squeezed actions button texts in the header on viewport under `850px` to turn buttons into icon-only layout seamlessly.
  - Adjusted button labels (e.g. `NEW PROJECT` to `PROJECT`, `INTRO` to `INTRO`, `SSH CONFIG` to `SSH`).
  - Added clean formatting structure to the console/progress layouts.

#### Changed
- **Agent Usage Email hidden state** (`AgentUsage.vue`): Replaced `v-if` template toggle with a CSS blur filter (`filter: blur(3px)`) to maintain exact element layout sizing when email is hidden.
- **Centralized frontend logging pipeline** (`logger.rs`, `lib.rs`, `useAgentUsage.js`): All frontend usage-flow events are now forwarded to the same Rust backend pipeline (`usage.log` + stderr) via a new `log_frontend(level, tag, msg)` IPC command. Previously logs were written exclusively to `console.warn` in the Webview — invisible from terminal and interleaved out of order with Rust entries. Now all log entries (Rust + JS) appear in chronological order in a single stream when running with `--debug`. Frontend console output is still printed immediately (before IPC) to preserve DevTools source-line links; console output is fully silent in production (no `--debug`) except for genuine `error`-level failures.
- **Compact log timestamp format** (`logger.rs`, `useAgentUsage.js`): Timestamp format changed from `YYYY-MM-DD HH:MM:SS.mmm` to `YYYYMMDD.HHMMSS.mmm` (e.g. `20260627.122617.123`) — saves ~10 bytes per line, reduces log file size at scale with no loss of readability.
- **Three-level frontend log discipline** (`useAgentUsage.js`): All 29 `ulog()` call sites now carry explicit level tags — `'error'` (IPC failures, parse failures, force-sync give-up), `'info'` (key lifecycle: provision, first load, STALE_RESET, forceSync start/complete, host change, manual refresh), `'debug'` (per-poll detail: poll tick, loading transitions, guard skips, IPC invoke/returned). Redundant `console.error` calls removed — now covered by `ulog(..., 'error')` routing to both console and backend.
- **Build script aliases** (`package.json`): Added `build:rmad` (ARM64 DMG) and `build:rmud` (Universal DMG) matching the naming convention from sibling projects. Replaced the old `build:app:uni` alias. `build:app` retained as the default ARM shortcut.

---

### [1.3.3] - 2026-06-25

#### Fixed
- **Claude usage stuck "No data" after quota reset — root cause: `set -o pipefail` kills dash** (`force-sync-claudecode.sh`): Force-sync script is delivered via `ssh host sh` (= POSIX dash on most Linux remotes). The line `set -o pipefail 2>/dev/null || true` made dash exit immediately (exit 2, zero stdout/stderr) — `set` is a POSIX special built-in so a usage error exits the shell before `|| true` can run, and `2>/dev/null` hides the error. Silent death → cache never refreshed → `get-usage` returned `STALE_RESET` every poll → UI stuck on "No data" permanently after quota reset. Latent since refactor `98fa2b7` (changed `ssh host <cmd>` login-shell to `ssh host sh` POSIX dash). Fixed with dash-safe subshell probe: `( set -o pipefail ) 2>/dev/null && set -o pipefail`. Full post-mortem: `docs/research/claude-usage-dash-pipefail-regression.md`.
- **Force sync firing unconditionally on every app startup** (`useAgentUsage.js`): The `manualRefreshCount` watcher called `forceSync()` directly for claudecode. On startup, `loadData()` → `refreshAll()` → `triggerManualRefresh()` incremented the counter, firing force-sync even when the cache was valid. Fixed: the watcher now calls `checkUsage()` for all agents; force-sync only auto-triggers inside `checkUsage()` when data is genuinely null.
- **Force sync incorrectly triggered when `resets_at = 0`** (`useAgentUsage.js`): First-load with `resets_at = 0` auto-triggered force-sync, but this value means "no rate-limit event recorded in the 5h window" — the cache was read successfully. Fixed by removing this trigger; force-sync is now reserved exclusively for null results (unreadable or absent cache).

#### Added
- **Build-time lint for SSH-delivered scripts** (`scripts/lint-remote-scripts.js`): Wired into `npm run dev` and `npm run tauri`; also callable as `npm run lint:scripts`. Intentionally excluded from `npm run build` (vite-only — does not compile shell scripts, running lint there was redundant and semantically wrong). Three checks per script: (1) regex scan for runtime bashisms `dash -n` won't catch — unguarded `set -o pipefail`, `[[ ]]`, `<<<`, `function name {`, `+=`, arrays (comment lines stripped to avoid false positives, guarded idiom whitelisted); (2) `dash -n` syntax check when dash is installed; (3) `shellcheck -s sh -S error` when shellcheck is installed. One bashism → build fails — the dash/pipefail bug can never ship again.
- **SSH-script timeout + remote process cleanup** (`agent_usage.rs`): `run_remote_script_timeout()` drains stdout/stderr on dedicated threads (prevents pipe deadlock on large scripts), polls `try_wait()`, kills the local SSH process after **30 s**. On timeout: spawns a fire-and-forget `ssh host pkill -f 'claude -p'` to clean up the orphaned remote process — without this, `claude -p` keeps running in the background, consuming quota and creating unintended sessions.
- **Force sync retry with backoff** (`useAgentUsage.js`): `forceSyncFailCount` tracks consecutive failures. On each failure under the cap: clears `initialSyncDone`/`staleResetSyncDone` so the next poll tick auto-retries (poll interval = backoff). After **3** consecutive failures: stops auto-retrying and shows a clear error. Manual refresh always resets the counter and tries again.
- **Error surfacing for silent force-sync failure** (`agent_usage.rs`): Empty stdout from `force_sync_agent_usage` now returns `Err(...)` (IPC reject, visible error) instead of `Ok({parsed:false})` — the old silent no-op that masked the dash/pipefail regression for many versions.
- **Structured usage-flow logging** (`logger.rs`, `agent_usage.rs`, shell scripts, `useAgentUsage.js`): New `logger.rs` module writes to `{appdata}/usage.log` (same directory as `projects.json`). Three levels: `error` (always written — file + stderr), `info`/`debug` (debug-only, via `--debug` flag or `AKI_DEBUG=1`). Production log is silent unless something breaks; `[STARTUP]` session-boundary line always written. File auto-truncates: keeps newest **512 KB** when size exceeds **1 MB** on startup. `agent_usage.rs` emits tagged entries (`GET_USAGE`, `FORCE_SYNC`, `PROVISION`) at every decision point. Shell scripts emit timestamped `[SHELL:*]` lines to stderr, captured by Rust. `useAgentUsage.js` logs every state transition to `console.warn` (DevTools F12). Two IPC commands: `is_debug_mode()`, `get_log_path()`.
- **HHMM build number — single source of truth across filename and titlebar** (`package.json`, `vite.config.js`, `scripts/post-build.js`, `AppHeader.vue`): `BUILD_NUM=$(date +%H%M)` is exported once at shell level when `build:app`/`build:app:uni` starts; Vite reads it via `process.env.BUILD_NUM` and bakes it into the bundle, `post-build.js` reads the same env var for the DMG rename — no drift. Titlebar format changed from `v1.3.3 (2026.06.26 #2334)` to `v1.3.3 (build 2026.06.26 23:34)`. Fallback to `new Date()` for `npm run dev` and standalone vite builds.
- **Universal DMG build target** (`package.json`, `scripts/post-build.js`): `npm run build:app:uni` builds for `universal-apple-darwin` and renames to `Aki-DevSync-v<ver>.<HHMM>-uni.dmg`. `post-build.js` scans both the default arm output dir and the universal output dir in a single pass.
- **Titlebar icon menu with links** (`AppHeader.vue`): App icon in the titlebar gains a `fa-chevron-down` indicator and a hover-activated dropdown with two links — **GitHub Repository** and **Latest Release** — opened via the existing `macos_open` IPC. Pure CSS hover (`:hover > .icon-dropdown`), no JS state; `::before` pseudo-element bridges the gap so the menu stays open as the cursor travels to it. Dropdown CSS explicitly resets inherited `h1` styles (`text-shadow`, `text-transform`, `letter-spacing`, `font-weight`) to prevent the cyan glow and uppercase transform from leaking in.
- **Auto port for `tauri dev`** (`scripts/tauri-runner.js`): TCP-probes ports from 1420 upward to find a free one; passes a `--config` override to Tauri CLI at runtime so no `tauri.conf.json` edit is needed. `vite.config.js` reads `TAURI_DEV_PORT` env with fallback to 1420. Allows multiple Tauri apps to run simultaneously without port conflict.
- **Architecture docs**: `docs/arch/logger.md` (new — levels, truncation, API, level map). `docs/arch/usage-claudecode.md` §3b (mermaid flowchart of the update cycle, showing exactly where the dash bug broke the flow) + §3c (4-layer prevention architecture, all implemented).

#### Changed
- **Single ordered recovery flow — `provision` no longer races `forceSync`** (`useAgentUsage.js`): On a null result the composable previously fired `provision()` (fire-and-forget) *and* `forceSync()` at the same time, opening two concurrent SSH sessions to the same host (interleaved logs, extra load at the busiest moment). Now `provision()` runs **only** on the first-load-no-cache branch — the one case where the statusLine hook may be absent — and is `await`ed *before* `forceSync()`, giving one clean ordered flow (`PROVISION` fully completes before `FORCE_SYNC` starts). STALE_RESET no longer provisions at all: the cache was readable until that poll, so the hook is already installed. `forceSync` stays fire-and-forget by design (it ends by calling `checkUsage()`, which the outer `isChecking` guard would otherwise skip).
- **Deterministic probe-transcript cleanup** (`force-sync-claudecode.sh`): The probe session's transcript is now deleted by its exact project path (`-tmp-aki-probe-$NOW_TS`) immediately after the post-probe `/usage` re-read — no time window, no globbing, zero risk to other runs. This was the real accumulator (one dir per reset event); the previous code only swept it via a 1-day orphan window. Blank-dir `/usage` transcripts are now bounded with `-mmin +1` (old enough never to race a concurrent sync's in-flight transcript), and the orphan-probe-dir sweep dropped from `-mtime +1` (1 day) to `-mmin +60` (1 hour) since each run now cleans its own.
- **Correct probe cache-freshness diagnostic** (`force-sync-claudecode.sh`): The post-probe "did claude rewrite the cache?" check compared the cache mtime against the script start time, mis-labelling the pre-reset cache (written seconds earlier) as "fresh". It now compares mtime against the probe start (`written_after_probe=yes/no`). Diagnostic only — `usage_run2` remains the source of truth — but the log line no longer misleads.

---

### [1.3.2] - 2026-06-25

#### Added
- **`.icon-glow` utility class** (`main.css`): Single-source `filter: drop-shadow(0 0 2px rgba(255,255,255,0.18))` applied to all app icons — titlebar (`AppHeader.vue`), agent icons in usage section (`AgentUsage.vue`), project icon in table, and popup menu IDE icons (`ProjectTable.vue`). Popup insiders icon merges the glow into its existing `hue-rotate` filter chain. Removed the old per-element `box-shadow` on `.app-icon`. One edit in `main.css` now controls glow intensity everywhere.
- **Antigravity pool color-coding** (`AgentUsage.vue`): Gemini fieldset gets a blue dashed border + legend (`rgba(96,165,250)` / `#93c5fd`); Claude/OSS gets orange (`rgba(251,146,60)` / `#fdba74`) — matches brand colors, eliminates cognitive load when scanning pools at a glance. Applied to both live and skeleton states.
- **Tauri v2 project icon extraction** (`system.rs`): Added support for detecting Tauri v2 projects (by checking for `src-tauri/tauri.conf.json`) and extracting the smallest suitable standard icon file (e.g. `128x128.png`, `64x64.png`, `32x32.png`) under 150KB as a Base64 data URL for the project list.

#### Changed
- **CSS hygiene pass** (`main.css`, `AgentUsageSection.vue`, modals): Replaced 13 hardcoded `#9CA3AF` literals with `var(--text-muted)` across `main.css` and 4 Vue files. Removed 3 unused `:root` vars (`--accent-purple`, `--bg-card`, `--bg-secondary`). Deleted 3 dead rule blocks (`.col-dry`, `.log-command`, `.log-delete`). Promoted spacing utilities `.mb-3`, `.mt-2`, `.mt-3` to global `main.css`; removed scoped duplicates from `GitModal.vue` (`.mt-3`, `.mr-1`) and `IntroModal.vue` (`.mb-2`, `.mb-3`, `.mt-3`).

#### Fixed
- **Open Remote Antigravity silent fail in production build** (`system.rs`, `ProjectTable.vue`): `antigravity-ide` installs to `~/.antigravity-ide/antigravity-ide/bin/` — outside Homebrew and `/usr/local/bin`, so `create_command` (v1.2.9 fix) could never find it in the GUI app's launchd PATH. Root cause confirmed: launching from Finder/Dock fails because `$SHELL -lc` (login non-interactive) reads `~/.zprofile` but NOT `~/.zshrc` — where all `antigravity-ide` PATH entries live (confirmed lines 98, 101, 113, 116). Launching via terminal works because the process inherits the full session PATH. Fixed by using `$SHELL -ilc` (interactive + login), which forces `~/.zshrc` to be sourced. Also added `Toast.fire({ icon: 'error' })` in `openIdeRemote` catch block so spawn failures surface to the user instead of silently logging to console only.
- **`staleResetSyncDone` undeclared variable** (`useAgentUsage.js`): used in 4 places but never declared — caused a `ReferenceError` on app load that crashed `AgentUsageSection` entirely. Added `let staleResetSyncDone = false;` alongside the other plain-boolean guards. Removed the leftover `lastStaleResetSyncAt` from an earlier refactor.
- **Claude Code force-sync probe bypassed after quota reset**: `force-sync-claudecode.sh` checked only for the presence of the string `"resets"` in `/usage` output to decide whether to skip the probe. After a quota reset, `/usage` echoes back the stale `resets_at` from `rate-limits-cache.json` — output contains `"resets [past time]"` — so the check passed and the probe never fired. Python then parsed the past timestamp and wrote it back to cache, causing `get-claudecode-usage.sh` to emit `|||STALE_RESET|||` again on every poll. UI remained stuck on "No data — waiting for next session" until the user manually opened a real Claude Code session. Fixed by adding a Python inline check that parses the reset time and verifies it is actually in the future; if past (or absent), the probe fires regardless.
- **JSONL cleanup window reduced from 7 days to 1 day**: blank dir and probe orphan JSONL files in `~/.claude/projects/` are only needed for the single `/usage` call immediately following their creation. Retaining them for 7 days was unnecessary accumulation.

---

### [1.3.1] - 2026-06-24

#### Added
- **`RefreshRing.vue` component**: Extracted SVG `stroke-dashoffset` countdown ring into a standalone reusable component with `inline` (flex row, 16px) and `overlay` (position absolute over button) modes and configurable `strokeColor`. Replaces the two inline SVG blocks in `AgentUsage.vue`.
- **Countdown rings for Git & Diff** (`ProjectTable.vue`): GIT column header shows a green `RefreshRing` (git interval); ACTIONS column header shows an amber ring (remote diff interval) — both display-only, no interaction, animate to full each cycle. `useBackgroundRefresh.js` exports `gitRefreshKey` / `diffRefreshKey`, incrementing on every timer fire and on every timer restart (so ring resets immediately when interval setting changes).
- **`ChangelogModal.vue`**: Replaced ad-hoc `Swal.fire()` inline HTML changelog with a proper `BaseModal`-based component. Uses themed scoped CSS matching app dark style, `renderMarkdown` computed once, `runMermaid()` via `watch` on body ref. `AppHeader.vue` no longer imports `Swal`, `changelogText`, or `renderMarkdown` directly.

#### Changed
- **Store extraction (`sshStore.js`, `logStore.js`)**: Module-scope `ref` trong `useSsh.js` và `useLogs.js` tách ra store files riêng — HMR không còn tạo ref mới khi edit composable, giữ đúng singleton behavior.
- **Dead CSS removal (`main.css`)**: Deleted 10 unused rule blocks — `.mr-2`, `.text-center`, `.badge-sync-git`, `.badge-push-special`, `.col-log`, `.btn-log-toggle` (+ `:hover`, `.log-active`), `.btn-action-terminal` (+ `:hover`, `:active`), `.btn-action-vscode` (+ `:hover`, `:active`), `.action-vscode-icon`, `.hooks-grid`. None referenced in any Vue template.
- **`AgentUsage.vue` dead props removed**: `locationType` and `hostName` props deleted — never referenced in template or script body, were being passed from `AgentUsageSection.vue` for no purpose.
- **`AgentUsage.vue` clock timer guard**: `ccClockTimer` `setInterval` now only starts when `agentId === 'claudecode'` — was running 1-minute ticks on every Antigravity instance even though `ccNow` is unused there.
- **`ProjectTable.vue` accessibility**: Added `aria-label` to `.btn-action-git` (Git Actions) and `.btn-tech-secondary.btn-icon-only` (Edit Configuration) — both are icon-only buttons. Removed dead scoped CSS class `.popup-divider`.
- **`AgentUsage.vue` reload buttons accessibility**: Added dynamic `:aria-label` to both CC and Antigravity reload buttons (icon-only, previously had `title` only).
- **`ProjectTable.vue` layout**: GIT column header renamed from "LOCAL GIT" to "GIT" (full name in `title` attribute). Git action button (purple gradient) moved from ACTIONS column into GIT cell, positioned before the status badge — button and badge now left-aligned as a unit, visually anchored to the column.
- **`RefreshRing` interval change edge case**: Usage rings restart immediately when `usage_interval_s` changes (via `watch` on `refreshSettings` in `AgentUsage.vue`). Git/diff rings restart immediately when their timer restarts — `restartGitTimer()` and `restartDiffTimer()` now increment the respective key before starting the new interval, not just on each fire.
- **`useProjectConfig.js` `saveConfig` toast**: Save and create now fire a success toast ("Config saved" / "Project created") and an error toast on failure — previously had no user feedback.
- **`main.css` `--color-danger`**: Added to `:root` — was referenced in 2 rules but undefined, resolving to empty.
- **Toast position**: Changed from `bottom-end` to `bottom` (center) — avoids overlapping ACTIONS column buttons on the last table row.

#### Fixed
- **`sync.rs` Mutex poison panic**: Both `versions_map.lock().unwrap()` calls replaced with `.unwrap_or_else(|e| e.into_inner())` — prevents app crash if a thread panics while holding the `RSYNC_VERSIONS` lock.
- **`ssh.rs` `.expect()` crash**: `config.parent().expect(...)` replaced with `ok_or(...)` and `?` propagation — hard panic in `save_ssh_config` eliminated.
- **`useAgentUsage.js` concurrency guard**: Added `isChecking` boolean to `checkUsage()` — parallel poll ticks and `manualRefreshCount` watch can no longer spawn overlapping fetch calls. Guard resets on host change and in `finally`.
- **`provision-claudecode.sh` temp file leak**: Added `trap 'rm -f /tmp/patch.sh' EXIT` so the temp patch file is always cleaned up. Also removes `sed -i.bak` backup file (`${FILE}.bak`) after successful patch — was accumulating on remote host indefinitely.
- **Shell scripts `set -e`**: Added `set -e` to `get-claudecode-usage.sh` and `set -o pipefail` (with `|| true` fallback for POSIX) to `force-sync-claudecode.sh` — silent parse failures now abort instead of propagating empty data.
- **`auth-cache.json` corrupt file**: `get-claudecode-usage.sh` now validates JSON via `python3 -c "import json..."` before using — `cat` returning corrupt content silently no longer causes Rust parse failure downstream.
- **`agent_usage.rs` dead stderr block**: Removed the unused `if !output.stderr.is_empty()` block and its `err` variable entirely — was a no-op after `eprintln!` was removed, causing an unused-variable compiler warning.
- **`useLogs.js` silent clipboard catch**: `copyLogs` no longer swallows clipboard errors silently — logs to `console.warn`.

---

### [1.3.0] - 2026-06-24

#### Added
- **Parallel Quota Connect RPC (`get-antigravity-usage.js`)**: Upgraded the local Connect RPC backend script to execute `/GetUserStatus` (for email/status) and `/RetrieveUserQuotaSummary` (for quota summary details) concurrently, capturing 4 detailed model quota buckets of Gemini and Claude pools.
- **Circular Progress SVG Gauge (`UsageCircle.vue`)**: Designed a new reusable SVG-based circular progress component rendering Used % in a radial style with automatic color state triggers (`<= 70%` safe green, `<= 90%` warning amber, `> 90%` danger red, and gray for `N/A`) and hover tooltips.
- **Fieldset Grouping Panels (`AgentUsage.vue`)**: Structured Antigravity model circles under separate `<fieldset>` containers for Gemini and Claude/GPT pools, applying a dashed border layout with a `<legend>` header resting natively on the border line.
- **Next Reset Countdowns**: Formatted and displayed remaining quota reset timers (e.g., `2h15m`, `ready`, or `N/A`) directly below the circles.
- **Persisted Host Selector Relocation (`SshConfigModal.vue` & `AgentUsageSection.vue`)**: Removed the `HOST` select dropdown from the main Remote column header. Created a new Claude Code remote host selector row inside the **SSH Config Modal**, sharing state globally via `useSsh.js` and persisting selection choices in `localStorage`.
- **Claude Code Icon Alignment (`AgentUsage.vue`)**: Re-integrated the official `/claude-icon.png` image with `18x18px` boundaries in the Claude Code usage card header, ensuring symmetrical alignment with Antigravity.
- **CC Auth Info Pipeline**: `provision-claudecode.sh` now runs `bash -lc 'claude auth status'` at the end of each provision call and writes the result to `~/.claude/auth-cache.json` on the remote host (runs once per host session — PATH-safe via login shell). `get-claudecode-usage.sh` reads this cache file on every poll (file read only, zero extra SSH overhead) and emits it via a new `|||AUTHINFO|||` delimiter. `agent_usage.rs` parses the delimiter and injects `email` and `orgName` into the payload JSON alongside the existing `subscriptionType`/`rateLimitTier` fields.

#### Changed
- **Zero Padding & Compact Grid**: Transparentized card backgrounds, eliminated inner card padding/margins, and reduced column/header spacing (gap from 12px to 4px, header padding from 6px to 4px) to optimize space.
- **Theme CSS Variables (`main.css`)**: Defined missing `:root` CSS variables (`--text-light`, `--text-muted`, `--text-darker`, `--border-color`, `--bg-secondary`, `--bg-tertiary`) to correct reset time contrast.
- **App Version Bump**: Updated app version to `1.3.0` globally in `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json`.

#### Fixed
- **Claude Code STALE_RESET Indefinite Stuck**: `useAgentUsage.js` had no self-healing path when `get-claudecode-usage.sh` returned `|||STALE_RESET|||` (reset window expired): `data` became `null`, `UsageCircle` unmounted (killing the `@timeout` trigger), and `initialSyncDone` blocked the startup auto-sync — leaving the UI permanently stuck until manual action. Fixed by detecting the `data-present → null` transition and auto-triggering `forceSync()` once, guarded by `staleResetSyncDone` to prevent polling loops.
- **Claude Code forceSync Concurrency**: Multiple trigger sources (5H and 7D `@timeout` both firing, rapid titlebar clicks) could invoke `forceSync()` concurrently, spawning parallel SSH sessions that race-wrote `rate-limits-cache.json`. Added `isSyncing` flag to drop duplicate calls while a sync is in-flight. Flag resets on host change.
- **JSONL Session File Accumulation**: `~/.claude/projects/-tmp-aki-dev-sync-blank-dir/*.jsonl` and orphaned `~/.claude/projects/-tmp-aki-probe-*` dirs grew unboundedly (17+ files per day from force-sync runs, plus one orphan per probe). Added cleanup block at end of `force-sync-claudecode.sh` that removes JSONL files and probe dirs older than 7 days. Files inside both the 5h and 7d windows are unaffected.
- **Year-Boundary resets_at Misparse**: `force-sync-parse.py` used `datetime.now().year` to fill the missing year in `/usage` output (`resets Jun 22, 10:10pm`). On Dec 31 late at night when the reset falls on Jan 1 of the next year, the parser produced a timestamp in the past → immediate STALE_RESET loop. Fixed by detecting the >1h-in-the-past condition and bumping the year by 1 via `dt.replace(year=year+1)`. Normal cases (reset in the future, or just-expired within 1h) are unaffected.
- **Spurious 7D @timeout forceSync**: The 7D `UsageCircle` in `AgentUsage.vue` emitted `@timeout → force-sync` when the 7-day reset window passed, but `force-sync-claudecode.sh` only updates `five_hour` data — `seven_day` data comes exclusively from the passive statusline hook. The 7D timeout trigger was a no-op for its intended purpose and only accidentally refreshed 5H data as a side effect. Removed `@timeout` from the 7D circle. The 5H circle retains its own `@timeout` which is the correct auto-recovery trigger.
- **Dead code removal**: Deleted `src/components/UsageProgressBar.vue` — superseded by `UsageCircle.vue`, no imports remained anywhere in the codebase.
- **Dead computed removal (`AgentUsage.vue`)**: Removed three unused computed properties (`iconClass`, `locationIcon`, `locationName`) that had no references in the template.

#### UI Improvements
- **Claude Code bars layout**: Replaced the two radial SVG circles for CC 5H/7D with compact horizontal progress bars (`cc-bars-block`). Labels and percentage sit on the header row; a thin 5px track fills left-to-right; reset time line appears below each bar. Saves vertical space and better utilizes the wide CC column.
- **AG circle label left of ring**: Moved the `subLabel` ("5H", "7D") from below the circle to the left of it, forming a compact `circle-main-row` flex row. Reset time line stays centered below.
- **Reset time line — single combined line**: The reset countdown and absolute time are now merged into one line: `Reset <bold>4h5m</bold> (22:15 Jun24)`. "Reset " is in muted normal weight; the relative time is white bold; the absolute time appended in round brackets is muted. This avoids a second line while preserving both data points.
- **Tooltip repositioned to bottom-right**: Moved `premium-tooltip` from above (could hit titlebar) to `top: calc(100% + 6px); left: 0` (below the element, extends rightward). Triangle arrow direction inverted to point upward. Prevents crop by left edge or titlebar.
- **SVG countdown ring on reload button**: Replaced the separate 1px `refresh-drain-bar` div with an SVG `stroke-dashoffset` ring that wraps the reload button itself. The ring fills clockwise from 12 o'clock over `refreshSettings.usage_interval_s` seconds and restarts each time a refresh completes (`drainKey` increment). Uses `stroke-dasharray: 94.25` (2π × r15). No conic-gradient — fully compatible with WKWebView on macOS.
- **Reload button circular style**: Both reload buttons (CC and AG headers) now use `border-radius: 50%` and a faint border that complements the ring color.
- **Account info in CC header**: Removed misleading `Session: $USD` badge (shows hypothetical API cost meaningless for Pro/Max subscribers). CC now displays full email and org name (if custom — auto-generated `"email's Organization"` is suppressed) sourced via a new auth-cache pipeline (see Added below).
- **Full email display for both agents**: AG and CC headers now show the complete email address (e.g. `user@domain.com`) instead of the truncated username prefix. Visibility controlled per-column by the eye toggle.
- **Icon drop-shadow**: Applied `filter: drop-shadow(0 0 4px rgba(255,255,255,0.22))` to both agent icons so they stand out against the dark app background without changing icon size or shape.
- **Eye toggle per column** (`AgentUsageSection.vue`): Added a subtle eye icon button to each LOCAL/REMOTE column header to independently show/hide email. State persists in `localStorage` (`aki-show-local-email`, `aki-show-remote-email`). Implemented with plain toggle functions — no `watch`/`watchEffect` overhead.
- **`selectedSshHost` computed refactor** (`useSsh.js`): Replaced `ref` + module-level `watch` (localStorage persist) + two component-level `watch` calls (async default) with a single writable `computed` — getter falls back to `sshHosts[0]` when no stored value, setter writes localStorage directly. Removed `watch`, `useProjects` import, and `sshHosts` destructure from `AgentUsageSection.vue`.

#### Documentation
- **Architecture References**: Updated [usage-antigravity.md](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/docs/arch/usage-antigravity.md) with parallel Connect RPC sequence diagrams.
- **Research Log**: Created [antigravity-usage-new-4line.md](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/docs/research/antigravity-usage-new-4line.md) logging local Connect RPC probe trials and endpoint findings.
- **Claude Code Flow Fix**: Updated `docs/arch/usage-claudecode.md` with STALE_RESET trigger #3, concurrency guard, and JSONL cleanup notes. Completed plan moved to `docs/plan/done/`.
- **Usage doc sync**: Updated `docs/arch/usage-claudecode.md` frontend file list — replaced stale `UsageProgressBar.vue` reference with `UsageCircle.vue`.

### [1.2.9] - 2026-06-24

#### Added
- **Dev Titlebar Indicator**: Display a subtle red "DEV" badge in the custom HTML titlebar when running the application in Vite development mode (`npm run tauri dev`).
- **Claude Code Auto-Probe Session**: Integrated an automatic "Probe Session" (dummy session using Haiku with prompt "respond with ok" in a temporary directory) into `force-sync-claudecode.sh`. When a remote host has no active local Claude sessions in the current 5-hour window, `/usage` hides the `resets_at` time. The script now automatically runs a quick probe session if no reset time is found, forcing Claude CLI to output a valid `resets_at` timestamp for the UI cache.

#### Fixed
- **Open Remote Antigravity**: Fixed a path issue where launching Antigravity remotely from the ACTIONS table button failed silently when running the app as a macOS `.app` bundle. Resolved by wrapping the `antigravity-ide` command in the `create_command` helper to correctly inject `/opt/homebrew/bin:/usr/local/bin` into the process environment PATH.
- **Parser Crash on 0% Quota**: `force-sync-parse.py` regex was a single combined pattern requiring both `% used` AND `· resets <time>` together. When quota resets fully, `/usage` returns `Current session: 0% used` with **no reset timestamp**, causing parse failure → cache not updated → `STALE_RESET` loop → UI permanently stuck on "No data". Fixed by splitting into two independent regexes: `%` (required) + `resets_at` (optional, defaults to 0). Now correctly writes `{used_percentage: 0, resets_at: 0}` to cache on fresh quota.
- **Force Sync Button in Empty State**: `AgentUsage.vue` empty state ("No data — waiting for next session") had no way to trigger Force Sync, leaving users stuck. Added a Force Sync button directly in the empty state for Claude Code cards.
- **Double refreshAll() on Manual Reload**: `handleRefresh()` in `AppHeader.vue` called `refreshAll()` directly after `loadData()`, which itself already calls `refreshAll()` internally. This caused git status, sync status, and usage checks to fire twice on every manual reload. Removed the redundant external call.
- **Startup & Host Change Auto Force-Sync**: Fixed an issue where restarting the application or switching remote hosts left the UI stuck at `0%` without a reset line. Integrated an initial-load auto force-sync logic in `useAgentUsage.js` that automatically runs a remote sync when `resets_at` is 0 or cache is missing.
- **Manual Refresh Full Force-Sync**: Wired up both the card header reload button and the titlebar global reload button to trigger a full remote `forceSync()` for Claude Code rather than just re-reading the remote cache file.

#### Research
- Confirmed (2026-06-24): `claude -p /usage` **reads local JSONL session files** (`~/.claude/projects/**/*.jsonl`) and computes offline — output states *"does not include other devices or claude.ai"*. It is **P2, not P3** (OAuth API call). Previous assumption (2026-06-23) that it makes a live OAuth network call was **incorrect**. Documented in `docs/arch/usage-claudecode.md` and `docs/research/claude-usage-1.2.x-analyze.md`.
- **Auto-Probe Breakthrough (2026-06-24)**: Solved the "missing reset time on inactive session" issue. When local Claude Code logs are inactive (past 5h window), `/usage` hides `resets_at`. We discovered that executing a dummy one-turn session (`claude --model haiku -p "respond with ok" < /dev/null`) inside a unique temporary directory forces the CLI to populate the local JSONL log. This safely and natively forces the `/usage` parser to output the correct server-synchronized `resets_at` time for the UI, consuming less than $0.0001 worth of tokens (approx. 100 Haiku tokens) without the safety risks of hitting undocumented Anthropic OAuth APIs.
- Confirmed (2026-06-24): correct SSH invocation for manual testing is `ssh <host> "bash -lc 'claude --model haiku -p /usage < /dev/null'"`. Using `zsh` fails on hosts without zsh (e.g. `bien`); omitting `-lc` fails because PATH is not loaded.

---

### [1.2.8] - 2026-06-24

#### Fixed
- **Force Sync 3s Delay**: Added `< /dev/null` to the `claude --model haiku -p /usage` invocation in `force-sync-claudecode.sh`. Without it, Claude Code waited 3 seconds for stdin input before proceeding, making every Force Sync take >5s. Now completes in ~2s.
- **Force Sync Context Isolation**: Changed working directory from generic `/tmp` to a dedicated empty directory `/tmp/aki-dev-sync-blank-dir` before running `claude -p /usage`, ensuring no stray files in `/tmp` are picked up as project context.
- **Stale Cache After Reset**: `get-claudecode-usage.sh` now checks if `rate_limits.five_hour.resets_at` has already passed before serving the cache file. If the reset time is in the past, it emits a `|||STALE_RESET|||` signal instead of the stale file. The Rust backend interprets this as `Ok(None)`, causing the UI to show "No data — waiting for next session" instead of displaying a frozen progress bar with "Reset X hours ago".
- **Stale Badge on Reset**: `useAgentUsage.js` stale detection now also triggers when `five_hour.resets_at` is in the past, complementing the existing 10-minute mtime check.
- **Auto Force Sync on Reset**: `UsageProgressBar.vue` now emits a `timeout` event exactly once when the countdown timer crosses zero (reset time reached). For Claude Code bars, `AgentUsage.vue` maps `@timeout` to `force-sync` (previously mapped to `retry`), triggering an automatic quota refresh instead of just re-reading the stale cache file.

#### Research
- Confirmed via live SSH testing (2026-06-23): `claude -p /usage` makes a real network call to the Anthropic OAuth API (`~/.claude/.credentials.json`) and does **not** require an active Claude Code session. Previous assumption that it read from RAM session was incorrect. Documented in `docs/arch/usage-claudecode.md` and `docs/research/claude-usage-1.2.7-analyze.md`.is not loaded.

---

### [1.2.7] - 2026-06-23

#### Added
- **Auto-Reveal DMG**: Automatically open and highlight the built `.dmg` file in Finder via `open -R` on macOS after the build completes.

#### Changed
- **Post-Build Script**: Renamed `rename-artifacts.js` to `post-build.js` to align with its broader post-build role.

#### Fixed
- **Antigravity Inactive State**: Optimized the quota monitoring flow to return a clean `None` response instead of throwing a raw JSON error when the Antigravity IDE process is not running. The UI now displays a friendly status message: "IDE not running (Open Antigravity to monitor)".
- **Rust Test Compilation**: Restored compilation of the Rust unit tests by adding the missing `ignore_hook_errors` field to the mock `SyncHooks` struct inside `projects.rs` tests.

---

### [1.2.6] - 2026-06-23

#### Added
- **Delete on Push Toggle**: Added `delete_on_push` per-project flag in the configuration modal. When enabled, pushing will use `--delete` to remove files on the Remote that no longer exist on the Local, keeping the Remote as a perfect mirror. Defaults to OFF for safety.
- **Safety Guard for Push**: Added a protective confirmation dialog when attempting to Push with `delete_on_push` enabled while there are pending Pull changes. This prevents accidental deletion of AI-generated files on the Remote.

#### Changed
- **Native Antigravity Quota Flow**: Replaced the flaky third-party `antigravity-usage` NPM CLI tool with a custom Node.js script `scripts/get-antigravity-usage.js` compiled directly into the Tauri Rust binary. This resolves the process-matching conflict with Volar/CSS language servers and macOS command argument truncation. We also removed the legacy `50K Quota` badge (which displayed static/fake monthly credits) and simplified the Vue frontend code. Quota polling is now 100% stable, fast (takes ~40ms), runs entirely locally, and returns accurate active model telemetry. Added reference documentation in [antigravity-usage.md](docs/ref/antigravity-usage.md).

#### Fixed
- **Sync Status Deletions**: Fixed an issue where locally deleted files were ignored by the sync status checker. The `count_rsync_changes` logic now correctly accounts for `deleting ` lines from the `rsync` dry-run, ensuring the Push button accurately reflects pending deletions.

---

### [1.2.5] - 2026-06-23

#### Added
- **Open Popup Header**: Added a project title header inside the Open Popup to prevent accidental clicks.
- **Open Popup Animation**: Added a smooth fade/scale animation with dynamic `transform-origin` flipping based on the popup's vertical position.
- **Brighter Popup UI**: Slightly brightened the popup's background color for better contrast.

#### Changed
- **Rebranding**: Renamed "Project Hub" to "Open Popup" across the UI, codebase, and documentation.
- **Documentation**: Consolidated old planning docs and created a dedicated `docs/feat/open-popup.md` feature document.

#### Fixed
- **Remote `$HOME` Resolution**: Fixed a bug where remote IDEs failed to launch if the path was configured using `$HOME` instead of `~/`.
- **OpenSSH Argument Bug**: Fixed a backend issue where `Command::new("ssh")` passed separated arguments that OpenSSH incorrectly concatenated without quotes, causing remote bash scripts to fail. Scripts are now passed as a single quoted string.

---

### [1.2.4] - 2026-06-23

#### Added
- **Build Identifier**: Added `#HHMM` build identifier to the titlebar (e.g., `v1.2.4 (2026.06.23 #1430)`) to distinguish same-day builds.
- **Bundle Metadata**: Added `category`, `description`, `copyright`, and `publisher` to `tauri.conf.json` for OS-level app metadata.

#### Changed
- **Push Special UI**: Renamed the `PUSH SPECIAL` button to `SELECT`.
- **Build Date Format**: Replaced the `HH:MM` time in `buildDate` with the new `#HHMM` build identifier.

#### Fixed
- **Special Push Git Sync**: Fixed empty modal issue by explicitly showing `.git/` when git sync is enabled, allowing manual git pushing even on a clean working tree.
- **VSCode Remote SSH**: Fixed `~/` paths creating a literal `~` directory at the remote root. Paths are now automatically resolved to absolute paths via SSH before opening.
- **Force Sync Diagnostics**: `force-sync-parse.py` now emits JSON diagnostics to stdout. If Force Sync fails silently, the exact reason is now logged in the browser console.

---

### [1.2.3] - 2026-06-23

#### Added
- **Background Sync Logging**: The application now intelligently logs state transitions from the background sync checker (which polls every 60s). It logs the initial state of each project upon startup, and subsequently only emits a log when it detects a *new* pending push or pull. This turns the project log into a linear timeline of when you (locally) or the AI (remotely) finished making changes, without spamming the log with redundant checks.
- **Auto Version Sync**: Added `scripts/sync-version.js` and updated `package.json` scripts to automatically synchronize the application version from `package.json` into `src-tauri/Cargo.toml` before every `tauri dev` and `tauri build` execution.
- **Intro Modal**: Added an interactive "INTRO" button to the header with a pulsing notification badge. The modal provides a comprehensive and visually appealing explanation of the Aki Dev Sync workflow, explicitly distinguishing between the author's primary use case (Security / Claude MAX sharing) and general use cases for other developers.

#### Changed
- **Changelog UI**: Reduced the global font size and line height of the changelog content for improved readability.
- **Build Artifacts**: Modified the post-build artifact renaming script to map `aarch64` to `arm` and support `universal` in `.dmg` filenames for better clarity.

- **Project Hub UI**: Refactored the hub to trigger from a dedicated "OPEN" button in the actions column rather than the project icon. The popup layout was changed to a wider horizontal two-column format (Local and Remote) to prevent vertical overflow.
- **Project Info Layout**: Reduced the width of the Project/Path column to save space. Full paths are now available via hover tooltips. The Production URL link was moved out of the hub back to the project row, right-aligned next to the project name.
- **App Identifier**: Updated `tauri.conf.json` identifier from `com.aki.remotedevsync` to `aki.devsync`.

#### Fixed
- **Remote Terminal Fallback Issue**: Fixed an issue where opening a remote terminal would fail with `No such file or directory` and fallback to the remote `$HOME` directory if the project's remote directory had not yet been created. A `mkdir -p` command is now automatically prepended before `cd` to ensure the directory exists.
- **VSCode/Insiders Remote Open**: Fixed an issue causing a `Could not resolve hostname` error when opening a remote project in VS Code or VS Code Insiders. This was caused by a missing slash `/` separator between the hostname and the tilde-prefixed path (`~`) when constructing the `vscode-remote://` URI.
- **Antigravity IDE Remote Open**: Added automatic tilde expansion (`~/` -> `$HOME/`) for remote paths passed to the Antigravity IDE CLI to ensure correct path resolution.
- **Sync Buttons False Positive in Build**: Fixed an issue where the Push and Pull buttons would falsely light up for all projects in the compiled production build on macOS. This occurred because the production GUI app uses the ancient macOS default `/usr/bin/rsync` (v2.6.9), which outputs `building file list ... done`, `Transfer starting:`, and `Skip newer ` during dry runs. These strings are now explicitly filtered out from the `count_rsync_changes` parsing logic.
- **Homebrew PATH Injection**: Injected `/opt/homebrew/bin:/usr/local/bin` into the environment `PATH` of all Rust `Command` executions on macOS. This ensures the production build automatically uses the modern `rsync 3.x` from Homebrew (if installed) for drastically improved sync performance, matching the development environment.
- **Rsync Version Logging**: Added automatic extraction and printing of both the **Local** and **Remote** `rsync` versions to the sync logs before execution. This helps developers verify whether the local macOS is using Homebrew's `rsync` and quickly identify any version or protocol mismatches between the two environments.
- **VSCode Insiders Icon**: Updated the icon's CSS filter in the project hub to match the bright green color (`#10b981`) of a clean Git state, fixing the previously incorrect color.
- **Project Hub Popup Positioning**: Added dynamic bounding box calculation so the hover popup correctly flips upwards when near the bottom of the window, preventing it from being cut off by the global log area.
- **Antigravity IDE Compatibility**: Fixed macOS app detection to correctly locate `Antigravity IDE.app` and updated the frontend arguments (`-a 'Antigravity IDE'`) to launch it successfully from the hub.
- **DMG Icon Alignment**: Shifted the macOS DMG application folder alias 6px to the left in the installer background for perfect pixel alignment.

---

### [1.2.2] - 2026-06-23

#### Fixed
- **Push/Pull buttons no longer falsely light up on startup**: `hasPendingPush` and
  `hasPendingPull` are now initialized to `null` instead of `undefined`. Buttons
  display a visually distinct "checking" state (very faint outline) while the background
  sync-status check is in flight, then resolve to fully lit (pending changes) or muted
  (clean) once the live fetch completes. No disk caching needed — background fetch is
  the source of truth.

---

### [1.2.1] - 2026-06-23

#### Changed
- **IPC open consolidation**: replaced 7 thin Rust wrapper commands (`open_url`, `open_local_dir`,
  `open_in_terminal`, `open_antigravity_app`, `open_ide_local`, `open_ide_remote` vscode arms,
  `open_remote_terminal`) with a single `macos_open(args: Vec<String>)` command. JS now builds
  the arg list directly (`['-a', 'Visual Studio Code', path]`, `[url]`, etc.) — macOS `open` is
  called once per intent, no Rust matching required. Subprocess-only cases (AppleScript SSH
  terminal, `antigravity-ide --remote`) remain in Rust as `open_remote_subprocess`.
- **Removed dead command** `open_remote_terminal` — no callers since the hub refactor (v1.2.0).
- **Test coverage updated**: `validate_ssh_host` tests replaced by equivalent `validate_remote_host`
  tests (the active validation function); `applescript_escape` tests unchanged.

---

### [1.2.0] - 2026-06-23

#### Added
- **Project Open Hub**: hovering over the project icon now reveals a floating menu with three
  sections — LOCAL (Finder, Terminal, VSCode, VSCode Insiders, Antigravity IDE), REMOTE SSH
  (SSH Terminal, VSCode Remote, VSCode Insiders Remote, Antigravity Remote), and LINKS (Open
  Production Site). IDE items are automatically greyed out when the application is not installed,
  checked once per session via the new `check_ide_availability` Tauri command.
- **`check_ide_availability` command**: detects presence of VSCode, VSCode Insiders, and
  Antigravity in `/Applications/` on macOS. Result cached in-session — only one IPC call per
  app lifecycle regardless of how many projects are hovered.
- **`open_ide_local` command**: unified local-open replacing separate `open_in_vscode` and
  `open_antigravity_app` commands. Accepts `ide_name` (`finder` | `terminal` | `vscode` |
  `vscode_insiders` | `antigravity`) and opens the given path with the matching application.
- **`open_ide_remote` command**: opens a remote project via SSH. Terminal uses AppleScript,
  VSCode/Insiders use the `vscode://vscode-remote/ssh-remote+<host><path>` URL scheme,
  Antigravity uses `antigravity-ide --remote`. Host validated to allow `user@host` format.
- **Remote Git URL in Git modal**: the project's remote git URL is now shown as a clickable link
  inside the Git modal, replacing the icon that was previously shown next to the project name.

#### Changed
- **ACTIONS column cleaned up**: removed standalone Terminal (`>_`) and VSCode buttons — these
  actions are now available through the Project Open Hub. Remaining actions: GIT, PUSH SPECIAL,
  PUSH/DRY/PULL group, LOG, CONFIG.
- **Path labels no longer clickable**: local path and remote path text in the project row no
  longer have click handlers. All open actions are consolidated into the hub.
- **Production URL moved to hub**: the globe icon (Open Production Site) is removed from the
  project name row and now lives in the hub's LINKS section.
- **Remote path display**: the remote path label is now hidden when a project has no `remote_host`
  configured, eliminating the empty `:` display.
- **Rebranding**: Project officially renamed to **Aki Dev Sync**. All UI texts, window titles, and documentation have been updated to reflect the new identity.
- **Build Process**: Output release binaries (e.g. `.dmg`) are now automatically renamed post-build using `scripts/rename-artifacts.js` to match the standard format `Aki-DevSync-vX.X.X-arch.dmg` (Kebab-case with version) for better DX and URL distribution without spaces.
- **Source of Truth Versioning**: Updated `tauri.conf.json` to read the app version directly from `package.json` via `"version": "../package.json"`. Resolved the previous bug where compiled files were incorrectly labeled as v1.1.1.

---

### [1.1.3] - 2026-06-23

#### Added
- **Background Refresh**: a new settings panel (⚙ icon next to REFRESH) lets you configure
  independent auto-refresh intervals for Git Status, Remote Diff, and Agent Usage. Settings
  persist across sessions; set any interval to 0 to disable that type.
- **REFRESH button** (renamed from RELOAD): triggers all three refresh types simultaneously —
  git status, remote diff, and agent usage — in one click. Grouped with the ⚙ settings icon
  as a paired control.

#### Changed
- **`BaseModal` component**: extracted shared modal scaffolding (overlay, drag handle, header,
  close button, ESC listener, backdrop click) into a single reusable `BaseModal.vue`. All 5 modals
  now use it, removing ~80 lines of duplicated boilerplate each.
- **Log panel ESC**: pressing Escape in an expanded log panel now collapses the panel and returns
  to the Global Event Log in one keystroke. Has no effect when a modal is open — modal ESC takes
  priority.
- **Push button dirty state**: the Push button no longer stays permanently lit when `sync_git` is
  enabled. Directory entries (e.g. `.git/`) are now filtered from the rsync dry-run change count —
  previously a routine `git status` call was enough to flip the button to dirty.
- **UI language**: completed a full English pass — all remaining Vietnamese strings replaced across
  `AppHeader`, `UsageProgressBar`, `useSsh.js`, and `useSync.js`.
- **Version display**: `package.json` is now the single source of truth for the app version. Version
  is injected at build time via Vite (same pattern as build date), replacing a `getVersion()` call
  that read from `Cargo.toml` and required manual updates in two separate files to stay in sync.

#### Fixed
- **Agent Usage — percentage display**: fixed floating-point noise rendering values like
  `7.000000000000001%`. Percentages are now always displayed as whole numbers.
- **Agent Usage — stale indicator**: the "Stale" badge now reflects the actual current age of
  the cached data rather than its age at the time of the last fetch. The badge also no longer
  flickers (disappearing and reappearing) on every refresh cycle.
- **Agent Usage — auto-setup on first use**: when no usage cache is found on a remote host,
  the app now automatically provisions the host in the background — patching Claude Code's
  statusline hook so rate-limit data is cached on every session. No manual setup required.
- **Modal backdrop**: clicking outside any modal now dismisses it, equivalent to pressing Cancel.
- **Git modal stale data**: the Git modal now fetches fresh data from the backend on every open
  instead of showing a potentially stale cached snapshot. A loading state is shown while in flight.
- **Project Config preset notification**: the success toast after applying a preset was silently
  failing due to an unresolved reference. Now fires correctly.
- **Push Special modal width**: the modal was rendering at 800px instead of the intended 600px
  after the `BaseModal` refactor. Corrected by passing the right container class.

---

### [1.1.2] - 2026-06-23

#### Added
- **`ignore_hook_errors` flag** on `SyncHooks`: when enabled, a hook that exits non-zero emits a
  `[WARN]` log line and allows the sync to continue instead of aborting. Useful for post-sync
  scripts that may fail on the first push (e.g. directory not yet created on remote, optional
  install steps). Toggle available in Project Config modal under the hooks section.
- **Sync status indicator**: Push/Pull buttons now show visual state based on real-time rsync dry-run
  checks. Buttons appear muted (`.btn-sync-clean`) when no changes are pending in that direction.
  Background polling every 60s keeps status fresh. New `check_sync_status` Tauri command runs
  `rsync --dry-run` for both directions and returns `has_local_changes` / `has_remote_changes`.

#### Fixed
- **Titlebar sacred boundary**: Modal overlays now start at `top: 42px` instead of `top: 0` to never
  cover the custom titlebar drag region. Added `--titlebar-h` CSS variable and documentation at
  `docs/ref/titlebar-sacred-boundary.md` to enforce the rule for all future fixed-position UI.

---

### [1.1.1] - 2026-06-23

#### Changed
- Internal: major DRY pass on `sync.rs` (`spawn_and_stream`, `run_hook_phase`, `build_rsync_args`),
  `git.rs` (`git_capture`), `ssh.rs` (`ssh_config_path`), `projects.rs` (`validate_path_segment`).
- `get_project_files` moved from `projects.rs` to `git.rs` (co-located with all git porcelain parsing).
- All `scripts/` now fully external (`get-claudecode-usage.sh` extracted); `include_str!` at every call site.

#### Fixed
- **UI freeze on Push/Pull**: `run_sync` restored to `async fn` with internal `spawn_blocking` for
  subprocess work. Previous patch incorrectly changed it to a sync `fn`, causing Tauri's IPC
  dispatch to block briefly before returning a Promise to JS — making the UI appear frozen on every
  sync action. Now truly non-blocking end-to-end.
- **Corrupt projects.json now surfaces error**: previously a bad JSON file silently returned an
  empty project list, making users think all projects were lost. Now returns a clear error message.
- **Remote mkdir failure now caught**: SSH `mkdir -p` exit status was not checked — a permission
  error would silently proceed into rsync and fail with a confusing message. Now reported immediately.
- **JSON field injection** in agent usage now uses `serde_json::Value` instead of string
  concatenation, safe for values containing quotes.

---

### [1.1.0] - 2026-06-23

#### Added
- **Dry Run toggle** (default ON): each project has a `dry_run` flag persisted in config. Sync previews changes without writing until explicitly turned off.
- **Delete on Pull toggle**: `delete_on_pull` per-project flag controls whether `--delete` is passed on PULL. Default on; opt-out to preserve local-only files.
- **Parallel sync**: removed global sync lock. Each project tracks its own `syncing` state independently — multiple projects can sync simultaneously.
- **Per-project runtime state** (`projectRuntime` map): ephemeral data (`git_status`, `git_log`, `remote_url`, `syncing`) separated from persisted config. Eliminates deep-watch overhead and copy-back hacks.
- **`delete_on_pull` toggle** in Project Config modal (danger-styled, hooks section).
- **Rust unit tests**: 23 tests across `projects.rs`, `sync.rs`, `system.rs` covering `validate_project`, `expand_remote_tilde`, `validate_specific_paths`, `validate_ssh_host`, `applescript_escape`. Run with `cargo test --lib`.
- **External scripts**: `scripts/provision-claudecode.sh`, `scripts/force-sync-claudecode.sh`, `scripts/force-sync-parse.py` — embedded at compile time via `include_str!`.
- **Frontend module split**: `useProjects.js` decomposed into `store/projectStore.js` (pure state), `useGit.js`, `useProjectConfig.js`, `useSync.js`. `useProjects.js` remains as a thin re-export facade — no component changes needed.

#### Changed
- **Rust backend split**: `lib.rs` god-module → 6 domain modules (`projects`, `ssh`, `git`, `sync`, `agent_usage`, `system`). `lib.rs` now only declares modules and wires the Tauri builder.
- **`run_sync` is now a sync `fn`**: previously `async fn` with blocking `thread::spawn+join` inside, which starved the async executor. Tauri's thread pool handles blocking commands natively.
- **Remote directory creation**: replaced `--rsync-path="mkdir -p ... && rsync"` string injection with a dedicated `ssh mkdir -p` call before rsync.
- **Agent usage poll is read-only**: `checkUsage()` no longer auto-runs `provision`. Provisioning is an explicit user action via `provision()` in the UI.
- **SSH undo/redo**: both operations now share `swap_ssh_state(from, to)` helper instead of duplicated logic.
- **CSP**: `"csp": null` → `"default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self'"`.

#### Fixed
- **`include_str!` path**: scripts were referenced as `../scripts/` (resolved to `src-tauri/scripts/`) instead of `../../scripts/` (project root). Caused compile error.
- **AppleScript injection**: `open_remote_terminal` now validates SSH host (allowlist chars) and escapes path via `applescript_escape()` before interpolating into AppleScript string.
- **Path traversal check**: `validate_project()` now covers both `local_path` and `remote_path`; `validate_specific_paths()` covers partial-sync params.

---

### [1.0.1] - 2026-06-22

#### Fixed
- **PULL creates nested subdirectory**: rsync was receiving `host:path` without a trailing slash on the source, causing it to sync the *directory itself* into the destination instead of syncing its *contents*. Both local and remote paths are now normalized to always carry exactly one trailing slash at the Rust layer.

---

### [1.0.0] - 2026-06-22

#### Added
- **Global Logs**: Added explicit system logs when triggering manual Reload and when modifying Project/SSH Configurations.
- **Environment Check**: Added `check-env.js` script to warn Linux users over SSH about Tauri's GUI restrictions during `npm run dev` or `build`.
- **GUI Versioning**: Added dynamic version display and Build Date (`YYYY.MM.DD HH:MM`) directly to the App's Titlebar (`AppHeader.vue`).

#### Changed
- **Version SSOT**: Removed hardcoded `version` inside `tauri.conf.json`. `package.json` is now the Single Source of Truth for the App's version. Tauri CLI syncs the version from it during build.

#### Architecture
- Added lightweight Markdown module with Mermaid support for rendering `CHANGELOG.md` in-app via `renderMarkdown` + `runMermaid`.
