# Changelog

All notable changes to this project will be documented in this file.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.0.0/) · [Semantic Versioning](https://semver.org/spec/v2.0.0.html)

---

### [1.3.1] - 2026-06-24

#### Added
- **`RefreshRing.vue` component**: Extracted SVG `stroke-dashoffset` countdown ring into a standalone reusable component with `inline` (flex row, 16px) and `overlay` (position absolute over button) modes and configurable `strokeColor`. Replaces the two inline SVG blocks in `AgentUsage.vue`.
- **Countdown rings for Git & Diff** (`ProjectTable.vue`): GIT column header shows a green `RefreshRing` (git interval); ACTIONS column header shows an amber ring (remote diff interval) — both display-only, no interaction, animate to full each cycle. `useBackgroundRefresh.js` exports `gitRefreshKey` / `diffRefreshKey`, incrementing on every timer fire and on every timer restart (so ring resets immediately when interval setting changes).
- **`ChangelogModal.vue`**: Replaced ad-hoc `Swal.fire()` inline HTML changelog with a proper `BaseModal`-based component. Uses themed scoped CSS matching app dark style, `renderMarkdown` computed once, `runMermaid()` via `watch` on body ref. `AppHeader.vue` no longer imports `Swal`, `changelogText`, or `renderMarkdown` directly.

#### Fixed
- **`sync.rs` Mutex poison panic**: Both `versions_map.lock().unwrap()` calls replaced with `.unwrap_or_else(|e| e.into_inner())` — prevents app crash if a thread panics while holding the `RSYNC_VERSIONS` lock.
- **`ssh.rs` `.expect()` crash**: `config.parent().expect(...)` replaced with `ok_or(...)` and `?` propagation — hard panic in `save_ssh_config` eliminated.
- **`useAgentUsage.js` concurrency guard**: Added `isChecking` boolean to `checkUsage()` — parallel poll ticks and `manualRefreshCount` watch can no longer spawn overlapping fetch calls. Guard resets on host change and in `finally`.
- **`provision-claudecode.sh` temp file leak**: Added `trap 'rm -f /tmp/patch.sh' EXIT` so the temp patch file is always cleaned up. Also removes `sed -i.bak` backup file (`${FILE}.bak`) after successful patch — was accumulating on remote host indefinitely.
- **Shell scripts `set -e`**: Added `set -e` to `get-claudecode-usage.sh` and `set -o pipefail` (with `|| true` fallback for POSIX) to `force-sync-claudecode.sh` — silent parse failures now abort instead of propagating empty data.
- **`auth-cache.json` corrupt file**: `get-claudecode-usage.sh` now validates JSON via `python3 -c "import json..."` before using — `cat` returning corrupt content silently no longer causes Rust parse failure downstream.
- **`agent_usage.rs` dead stderr block**: Removed the unused `if !output.stderr.is_empty()` block and its `err` variable entirely — was a no-op after `eprintln!` was removed, causing an unused-variable compiler warning.
- **`useLogs.js` silent clipboard catch**: `copyLogs` no longer swallows clipboard errors silently — logs to `console.warn`.

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

#### Changed
- Internal: major DRY pass on `sync.rs` (`spawn_and_stream`, `run_hook_phase`, `build_rsync_args`),
  `git.rs` (`git_capture`), `ssh.rs` (`ssh_config_path`), `projects.rs` (`validate_path_segment`).
- `get_project_files` moved from `projects.rs` to `git.rs` (co-located with all git porcelain parsing).
- All `scripts/` now fully external (`get-claudecode-usage.sh` extracted); `include_str!` at every call site.

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
