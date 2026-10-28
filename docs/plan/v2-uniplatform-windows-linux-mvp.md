# Plan: v2 Uniplatform Architecture & Windows / Linux MVP

**Research origin:** [docs/research/v2-uniplatform-windows-linux-architecture.md](../research/v2-uniplatform-windows-linux-architecture.md)  
**Companion plan:** [docs/plan/done/ag-plan-badge-unification.md](done/ag-plan-badge-unification.md)  
**Business spine reference:** [docs/biz/backbone.md](../biz/backbone.md) (`RULE-docs.md` §A3, `RULE-biz.md` §A3–A4)  
**Target platforms:** macOS (`.dmg`), Windows 10/11 x64/ARM64 (`.exe` NSIS / `.msi`), Linux x64/ARM64 (`.AppImage` / `.deb`)

## Status (2026-10-28)

**Not started.** The app stays macOS-only (`CLAUDE.md`, `docs/biz/backbone.md`) until the owner approves Phase 0. Windows work is open to an outside contributor: the owner has no Windows machine and cannot run a Windows VM, so Phase 6.2–6.3 (and any runtime check of Phases 3–5 on Windows) need a contributor's own machine.

| Phase | Size (estimate, unverified) | Verifiable without Windows |
|---|---|---|
| 0 — docs | < 1 hour | yes |
| 1 — compile blockers | ~1 day | compile only (6.1, or a CI `windows-latest` runner); `tauri.B8` |
| 2 — platform traits | 2–3 days | macOS/Linux branches only |
| 3 — usage probes | 3–5 days (Windows AGY probe is the largest unknown) | macOS/Linux branches only |
| 4 — terminal & keys | 1–2 days | Linux via `bien`; ConPTY needs Windows |
| 5 — capability UI | 1–2 days | yes (stub capabilities can be forced) |
| 6 — verification | contributor-dependent | 6.1 only |

**Contributor entry point:** Phase 1 is self-contained and gates everything else; a Windows contributor can start there and report `cargo test` output from a real Windows host (6.2). Contributor requirements (akirule installed, a Linux host with `claude`/`agy` reachable over SSH from the Windows machine): [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

**Corrections to the text below, checked against the code on 2026-10-28:**
- 1.2: `mod control;` is at `lib.rs:7`, not `:6`.
- 1.4: `is_pid_alive` (`pty.rs:909`) sits inside the `#[cfg(test)]` module (`pty.rs:886`), so it blocks `cargo test` on Windows, not `cargo build`; the process-group test is at `pty.rs:~1510`. The non-test `libc::killpg` calls (`pty.rs:567-583`) are already `#[cfg(unix)]`.
- `sync.rs` needs nothing: `kill_process_group`/`detach_process_group` are already `#[cfg(unix)]` with a `#[cfg(not(unix))]` stub, and `libc` is a `cfg(unix)` dependency in `Cargo.toml`.
- 3.2: the "Free-tier / Pro-tier inference" from `ag-plan-badge-unification.md` Phase 2.1 was not built (deferred, see that plan's § 4); the Windows probe gets its tier the same way as macOS/Linux or not at all.

---

## 1. Goal & MVP Scope Definition

Deliver a **Uniplatform v2 architecture** where macOS, Windows, and Linux share one codebase:
- **Core features active on ALL platforms (macOS, Windows, Linux):**
  1. **Agent Usage Monitoring (2–8 slots):** Real-time Claude Code & Antigravity quota bars, plan badges (`Pro`, `Free`, `Ultra`, `Max`), countdowns, and multi-account cache switching (`LOCAL` on all platforms + `REMOTE` whenever `ssh` is available).
  2. **Project & Task Management:** Project table, `.akidevsync/project.json`, `.akidevsync/notes.json` task list (pinned/doing/wish/done), and Global Notes.
  3. **In-App Multi-Tab Terminal (`portable-pty`):** Project-scoped & global tabs, `DEV` / `BUILD` / `DEPLOY` one-click runners, ConPTY on Windows, Unix PTY on macOS/Linux.
  4. **Git Modal Operations:** Status, diff, stage, commit, push, pull, fetch.
  5. **Statusline Customizer & Phone Remote Control (`axum`):** Local/remote statusline config and LAN/Tailscale phone companion.
- **Capability-gated features (OOP Strategy / Stub pattern when `rsync` or `ssh` is absent on Windows):**
  - When `has_rsync == false` (default on stock Windows): `SyncEngine` resolves to `StubSyncEngine`; background sync checks are skipped and `PUSH`/`PULL` sync controls hide or display a clean capability notice without errors or broken UI.
  - When `has_ssh == false` (rare; Windows 10/11 includes OpenSSH `ssh.exe` by default): `RemoteShellTransport` resolves to `StubSshTransport` and disables `REMOTE` usage slots / SSH modal cleanly.
  - External `Terminal.app` window counting (`lsof` scan): uses `StubExternalTerminalScanner` (`None`) on Windows/Linux MVP while keeping the in-app terminal badge active.

---

## 2. Execution Phases

### Phase 0 — Business Backbone & Target Context Alignment (`docs/biz/backbone.md`, `CLAUDE.md`)

**Files owned:**
- `docs/biz/backbone.md`
- `CLAUDE.md`

**Changes:**
- [ ] **0.1. Update `docs/biz/backbone.md` (`biz.A3`, `biz.A4`):**
  - Record the beachhead expansion from macOS-only (v1) to Uniplatform macOS + Windows + Linux (v2): primary audience remains the AI-agent power developer, now serving both split local-to-remote workflows (when `rsync`/`ssh` are present) and local-first Windows/Linux workflows (Usage Monitoring + Project/Task Command Center + In-App Terminal + Git).
  - Rewrite anchor stamp (`docs.A4`).
- [ ] **0.2. Update `CLAUDE.md` (`tauri.B6`, `docs.A5`):**
  - Update target platform line from `macOS-only` to `Uniplatform (macOS primary, Windows & Linux MVP in v2; capability-gated rsync/ssh)`.

---

### Phase 1 — Clear All 6 Compile-Time Unix Blockers in `src-tauri/`

**Files owned:**
- `src-tauri/src/conflict.rs`
- `src-tauri/src/control.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/src/statusline.rs`
- `src-tauri/src/system.rs`
- `src-tauri/src/pty.rs`

**Changes:**
- [ ] **1.1. `conflict.rs:246-248` (`libc::localtime_r`):**
  - Gate Unix `libc::localtime_r` with `#[cfg(unix)]` and add `#[cfg(windows)]` implementation using Windows CRT / `std` time offset so `conflict.rs` compiles and passes all timezone unit tests on Windows.
- [ ] **1.2. `lib.rs:7` & `control.rs` (`UnixListener` / `PermissionsExt`):**
  - Gate `#[cfg(unix)] mod control;` in `lib.rs` (since `control.sock` is a Unix domain socket only used in debug builds on Unix).
- [ ] **1.3. `statusline.rs:965, 1956` & `system.rs:1757` (`std::os::unix::fs::PermissionsExt`):**
  - Gate `PermissionsExt` (`0o755` chmod) and `std::os::unix::fs::symlink` behind `#[cfg(unix)]` (`tauri.B4`).
- [ ] **1.4. `pty.rs:909, 1497` (`is_pid_alive` & Unix process-group test):**
  - Test-only: both sit inside `#[cfg(test)]` (`pty.rs:886`), so they block `cargo test` on Windows, not `cargo build`.
  - Gate `libc::kill(pid, 0)` with `#[cfg(unix)]` and add `#[cfg(windows)]` process liveness check via `windows_sys::Win32::System::Threading::{OpenProcess, GetExitCodeProcess}`.
- [ ] **1.5. Prevent console window flashing on Windows (`system.rs:6` `create_command`):**
  - In `system::create_command()`, add:
    ```rust
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    ```
  - This single funnel ensures `git`, `claude`, `agy`, `ssh`, and `rsync` never pop up flashing black `cmd.exe` windows on Windows.
- [ ] **1.6. Path & file-handle safety on Windows (`tauri.B8`):**
  - Add `normalize_path_display(p: &Path) -> String` in `system.rs` to strip `\\?\` verbatim prefixes (`\\?\UNC\` $\rightarrow$ `\\`, `\\?\C:\` $\rightarrow$ `C:\`).
  - Audit `logger.rs` and `write_atomic` to ensure no file truncation uses an `OpenOptions::append(true)` handle (`tauri.B8`).

---

### Phase 2 — `src-tauri/src/platform/` OOP Strategy Traits & Runtime Capability Registry

**Files owned:**
- `src-tauri/src/platform/mod.rs` (new)
- `src-tauri/src/platform/capabilities.rs` (new)
- `src-tauri/src/platform/opener.rs` (new)
- `src-tauri/src/platform/sync_engine.rs` (new)
- `src-tauri/src/platform/external_term.rs` (new)

**Changes:**
- [ ] **2.1. `PlatformCapabilities` SSoT & `get_platform_capabilities` Tauri Command:**
  - Detect once at startup (cached in `OnceLock<PlatformCapabilities>`):
    - `os`: `"macos" | "windows" | "linux"`
    - `has_ssh`: probes `ssh -V`
    - `has_rsync`: probes `rsync --version` and checks `rsync_protects_remote_args` (>= 3.2.4)
    - `has_external_terminal_scan`: `cfg!(target_os = "macos")`
    - `file_manager_label`: `"Finder"` (macOS) | `"Explorer"` (Windows) | `"Files"` (Linux)
    - `external_terminal_label`: `"Terminal.app"` (macOS) | `"Windows Terminal / PowerShell"` (Windows) | `"System Terminal"` (Linux)
    - `mod_key_label`: `"⌘"` (macOS) | `"Ctrl"` (Windows/Linux)
- [ ] **2.2. `SyncEngine` Trait (`RsyncSyncEngine` + `StubSyncEngine`):**
  - Define `pub trait SyncEngine: Send + Sync` with `check_status(...)` and `run_sync(...)`.
  - `RsyncSyncEngine`: delegates to existing `sync.rs` logic when `has_rsync == true`.
  - `StubSyncEngine`: active when `has_rsync == false` (stock Windows); `check_status` returns clean zero-count disabled state with 0 subprocess calls, and `run_sync` returns a clear user-facing message.
- [ ] **2.3. `DesktopOpener` Trait (`MacOsOpener`, `WindowsOpener`, `LinuxOpener`):**
  - Define `pub trait DesktopOpener: Send + Sync`:
    - `detect_ides(&self) -> IdeAvailability`
    - `open_file_manager(&self, path: &str) -> Result<(), String>`
    - `open_ide(&self, ide: &str, path: &str, remote_uri: Option<&str>) -> Result<(), String>`
    - `open_external_terminal(&self, path: &str, cmd: Option<&str>) -> Result<(), String>`
  - On Windows (`WindowsOpener`):
    - Resolves CLI/IDE binaries using `PATHEXT` filtered strictly to `.com, .exe, .bat, .cmd` (`tauri.B8` rule — prevents `os error 193` on npm `.ps1`/sh shims) plus standard `%LOCALAPPDATA%\Programs\` install paths for VS Code, Cursor, and Antigravity.
    - Opens folders with `explorer.exe`.
    - Opens external terminal via `wt.exe -d <path>` (Windows Terminal) with fallback to `powershell.exe -NoExit`.
- [ ] **2.4. `ExternalTerminalScanner` Trait (`MacTerminalAppScanner`, `StubExternalTerminalScanner`):**
  - `MacTerminalAppScanner`: runs existing `lsof` + `pgrep` scan in `system.rs`.
  - `StubExternalTerminalScanner`: returns `Ok(None)` on Windows and Linux MVP.

---

### Phase 3 — Cross-Platform Usage Monitoring (`LocalUsageProbe` in Native Rust + Remote SSH Fallback)

**Files owned:**
- `src-tauri/src/agent_usage/claudecode.rs`
- `src-tauri/src/agent_usage/antigravity.rs`
- `src-tauri/src/agent_usage/local_win.rs` (new, or cross-platform local probe module)

**Changes:**
- [ ] **3.1. Native Rust Local Claude Code Usage Reader (All 3 OSes):**
  - Implement `get_claudecode_usage_local()` in pure Rust (no `sh`, no `python3` dependency!):
    - Reads `${CLAUDE_CONFIG_DIR:-~/.claude}/rate-limits/*.json` (and fallback `rate-limits-cache.json`), `.claude.json`, and `.credentials.json`.
    - Runs `claude auth status` (`claude.exe` / `claude.cmd` on Windows resolved via the `.com/.exe/.bat/.cmd` `PATHEXT` helper per `tauri.B8`) with a 3s timeout.
    - Applies the exact same expiry (`resets_at <= now`), unseen (`now - seen_at >= 21600`), and `accountUuid`/`email` ownership gates as `scripts/get-claudecode-usage.sh`.
  - Keep `scripts/get-claudecode-usage.sh` for remote hosts (`host != "local"`) over `ssh host sh`.
- [ ] **3.2. Native Cross-Platform Local Antigravity Usage Probe:**
  - On **macOS & Linux (`#[cfg(unix)]`)**: keep `scripts/get-antigravity-usage.sh` (already supports both `Darwin` `lsof` and Linux `ss`/`netstat`).
  - On **Windows (`#[cfg(windows)]`)**: implement `get_antigravity_usage_local_windows()` in Rust:
    - Finds running `language_server_windows_x64.exe` (IDE / Desktop) and `agy.exe` (CLI) processes.
    - Queries local Connect RPC (`GetUserStatus` + `RetrieveUserQuotaSummary`) via `reqwest` or runs `agy.exe -p "/usage" --output-format json --log-file <temp>` (with 60s temp cache in `std::env::temp_dir()`) + reads `~/.gemini/antigravity-cli/usage-cache/<email>.json`.
    - Feeds the frames into the shared `antigravity_payload::parse_antigravity_frames()` normalizer. The Free-tier weekly-only / Pro-tier Argon inference of [ag-plan-badge-unification.md](done/ag-plan-badge-unification.md) Phase 2.1 was not built (see its § 4).
- [ ] **3.3. Remote Usage Probe Guard (`has_ssh`):**
  - When `host != "local"` and `!capabilities().has_ssh`, return `AgentUsageResult::unreachable("OpenSSH client not found on this machine")` immediately without attempting to spawn `ssh`.

---

### Phase 4 — Cross-Platform In-App Terminal & Statusline Customizer

**Files owned:**
- `src-tauri/src/pty.rs`
- `src-tauri/src/statusline.rs`
- `src/composables/useTerminalCopy.js`
- `src/composables/useTerminalFont.js`
- `src/composables/useTerminalTabs.js`

**Changes:**
- [ ] **4.1. Default shell resolution in `pty.rs`:**
  - On `#[cfg(windows)]`: resolve `pwsh.exe` (PowerShell 7) $\rightarrow$ `powershell.exe` (Windows PowerShell) $\rightarrow$ `%COMSPEC%` (`cmd.exe`), passing `-NoLogo` for clean startup.
  - On `#[cfg(target_os = "linux")]`: resolve `$SHELL` $\rightarrow$ `/bin/bash` $\rightarrow$ `/bin/sh`, using `-l` login shell.
- [ ] **4.2. Keyboard modifier abstraction in Vue (`src/utils/platformKey.js`):**
  - Export `isPrimaryMod(ev)` (`isMac ? ev.metaKey && !ev.ctrlKey : ev.ctrlKey && !ev.metaKey`) and `modGlyph` (`⌘` vs `Ctrl+`).
  - Update `useTerminalFont.js` (`Ctrl+=` / `Ctrl+-` / `Ctrl+0`), `useTerminalTabs.js` (`Ctrl+T`), and `useTerminalCopy.js` (on Windows/Linux: `Ctrl+Shift+C` always copies, and `Ctrl+C` copies when xterm has an active selection and clears selection, otherwise passes `\x03` SIGINT through to the shell!).

---

### Phase 5 — Capability-Aware Frontend UI (`src/composables/usePlatformCapabilities.js`, `src/components/`)

**Files owned:**
- `src/composables/usePlatformCapabilities.js` (new)
- `src/components/ProjectTable.vue`
- `src/components/AgentUsageSlot.vue`
- `src/components/AppHeader.vue` (or header menu component)

**Changes:**
- [ ] **5.1. `usePlatformCapabilities()` Composable:**
  - Fetches `get_platform_capabilities` once at app boot and exposes reactive `capabilities` (`os`, `has_rsync`, `has_ssh`, `has_external_terminal_scan`, `file_manager_label`, `mod_key_label`).
- [ ] **5.2. Graceful UI adaptation when `has_rsync == false` or `has_ssh == false`:**
  - In `ProjectTable.vue`:
    - Replace hardcoded `"Finder"` label in the `OPEN` popup (`ProjectTable.vue:179`) with `capabilities.file_manager_label` (`Finder` / `Explorer` / `Files`).
    - When `!capabilities.has_rsync`, hide or disable the `PUSH`/`PULL` sync buttons and remote diff polling (`useRefreshController.js` skips `check_sync_status`), keeping `OPEN`, `DEV | BUILD | DEPLOY`, `TERM`, `TASKS`, and `GIT` front-and-center.
    - When `!capabilities.has_ssh`, hide the `Remote (SSH)` section of the `OPEN` popup.
  - In `AgentUsageSlot.vue`:
    - When `!capabilities.has_ssh`, disable the `REMOTE` scope tab with tooltip `"Install OpenSSH client to monitor remote hosts"`.

---

### Phase 6 — Cross-Compile, Native Windows VM & Linux Verification (`tauri.B8`)

**Verification steps (`coding.B3` + `tauri.B8`):**
- [ ] **6.1. Cross-compile check from macOS:**
  - Run `cargo check --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc` (or `cargo xwin check`) and `x86_64-unknown-linux-gnu` — 0 errors, 0 warnings (`tauri.B4`).
- [ ] **6.2. Native Windows test suite execution (`tauri.B8`):**
  - Build test binary and run in Windows 11 VM with `<exe>.manifest` (`Microsoft.Windows.Common-Controls` `6.0.0.0`), verifying `test result: ok. N passed; 0 failed` summary line.
- [ ] **6.3. Windows MVP end-to-end smoke check:**
  - Verify Claude Code & Antigravity local usage cards, multi-account cache, Project Table (`Explorer`, `VSCode`, `TASKS`, `GIT`), and In-App ConPTY Terminal (`PowerShell`) on Windows 11 with zero console window flashes and clean `has_rsync = false` stubbing.
