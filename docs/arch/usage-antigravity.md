# Antigravity IDE Local Proxy Quota Monitoring Reference

This reference document explains the architecture, flow, and implementation details of local quota monitoring for the Google Antigravity IDE & CLI in this project.

> **Updated 2026-07-22:** Multi-Surface Integration (v1.17.0: Antigravity Desktop App `AG`, IDE `IDE`, CLI `CLI`). See [docs/plan/done/1.17.0-ag-multi-surface.md](../plan/done/1.17.0-ag-multi-surface.md).
>
> **Updated 2026-07-30:** Entity model simplified to `(host, email)` — `sourceType` (ide/cli/desktop) is now transport metadata, not identity. A Google account has one quota; the same email on the same host has one cache slot regardless of which surface reported it. Cache key scheme bumped to v4. See §Per-Account Cache.

## Multi-Surface Directory Structure & Settings

| Surface | Data & Storage Path | Primary Settings File | Note |
| :--- | :--- | :--- | :--- |
| **AGY CLI** | `~/.gemini/antigravity-cli/` | `settings.json` | JSON format. Holds `permissions.allow` for auto-approving terminal commands. |
| **AGY Desktop App** | `~/.gemini/antigravity/` | `user_settings.pb` | Protobuf format for UI settings. Shares auth credentials with CLI. |
| **AGY IDE** | `~/.gemini/antigravity-ide/` | `user_settings.pb` | Protobuf format for IDE extension settings. |

*Shared auth credentials live at `~/.gemini/oauth_creds.json` and `~/.gemini/google_accounts.json`.*


## Mechanism of Action

The Antigravity IDE (Gemini-based desktop agent coding environment) runs a local native Language Server instance (`language_server_macos_arm`, `language_server_macos_x64`, etc.) which exposes local Connect RPC APIs. 

Instead of making external network requests to Google Cloud Code APIs (which return simulated/dead data with `0%` usage), our tool queries this local server directly to fetch real-time, accurate quota metrics (such as Gemini Pool and Claude/OSS pool status).

## Flow of Action

Quota retrieval is executed by the self-contained POSIX shell script [get-antigravity-usage.sh](../../scripts/get-antigravity-usage.sh) compiled directly into the Tauri Rust backend.

```mermaid
sequenceDiagram
    participant Rust as Tauri Rust Backend
    participant Shell as POSIX sh (sh / ssh)
    participant OS as OS Process Table (ps)
    participant PORT as lsof / ss / netstat (Port Scan)
    participant AG as Antigravity Language Server
    
    Rust->>Shell: Pipe get-antigravity-usage.sh contents
    Shell->>OS: Execute 'ps auxww'
    OS-->>Shell: Process list stdout
    Shell->>Shell: Match language_server binary & extract CSRF token / seed port
    Shell->>PORT: Execute 'lsof' (macOS) or 'ss'/'netstat' (Linux)
    PORT-->>Shell: List of listening TCP ports
    Shell->>AG: Probe ports via curl (POST /GetUnleashData with CSRF)
    AG-->>Shell: Response 200/401 (identifies active port)
    
    rect rgb(20, 30, 40)
        Note over Shell,AG: Connect RPC Queries
        Shell->>AG: Query GetUserStatus (POST via curl)
        Shell->>AG: Query RetrieveUserQuotaSummary (POST via curl)
        AG-->>Shell: Return email & plan status
        AG-->>Shell: Return detailed groups/buckets quota
    end

    Shell->>Rust: Output delimited JSON frames (|||AGPROC|||...)
    Rust->>Rust: Parse frames & serialize for Vue Frontend
```

### 1. Process Detection (Multi-Instance: IDE & CLI agy)
* **Execution:** Runs `ps auxww` on macOS/Unix to output the command list without line truncation.
* **Targeting:** Detects both Antigravity IDE and standalone `agy` CLI processes running simultaneously:
  * **IDE Language Server binaries:** `language_server_macos_arm`, `language_server_macos_x64`, `language_server_linux_x64`, `language_server_linux_arm64`, `language_server_windows_x64.exe` (with `--csrf_token` and `--extension_server_port`).
  * **AGY CLI binary:** Standalone `agy` process instances listening on local Connect RPC HTTPS ports.
* **Argument Extraction:** Parses the command arguments using regular expressions to extract:
  * `--csrf_token` (Security token required for Connect RPC queries).
  * `--extension_server_port` (Base extension communication port).
* **Both branches extract, unconditionally.** Until 2026-07-30 only the language-server branch ran the extraction; the `agy` CLI branch set `proc_type="cli"` and skipped it, so an `agy` session was probed with **no** `X-Codeium-Csrf-Token` header and no seeded port. Where the Connect API requires the token, every request 401s and the whole poll collapses into an anonymous failure - which the app renders as a reading that simply keeps getting older, with no error surfaced. This was the direct cause of "Antigravity cannot measure quota while `agy` is running", and it is worth recording *why it survived so long*: the defect was inherited from the Node probe this script was ported from, which hardcoded the token to `undefined`. A port that is checked for parity against its own predecessor cannot detect a fault the two share - so the language-server branch, not the old JS, is the specification the `agy` branch was made to match. Extraction now costs nothing when a process genuinely carries neither flag: the values come out empty and behaviour is unchanged.
* **A 401 with no `--csrf_token` in argv is logged by name**, distinguishing a permanent "this process's quota is unreadable" from a transient "the IDE is mid-restart". Both used to look identical in the log.

### 2. Port Discovery & Probing
* **TCP Port Detection:** Runs `lsof -nP -iTCP -sTCP:LISTEN -a -p <PID>` to gather active ports listening on each target process ID (both IDE and CLI).
* **Seed Ports:** Seeds candidate lists with extracted `extensionServerPort` and adjacent ports (`port + 1`).
* **Connection Probing:** Sends POST requests to `/exa.language_server_pb.LanguageServerService/GetUnleashData` (probing HTTPS and HTTP) to check for valid Connect RPC statuses (`200` or `401`).

### 3. API Query & Standardization
* **Parallel Queries:** Performs two Connect RPC queries concurrently using `Promise.all`:
  * `GetUserStatus`: Fetch account information such as `email`.
  * `RetrieveUserQuotaSummary`: Fetch the 4 detailed quota metrics (5h and Weekly buckets for Gemini and Claude/GPT pools).
* **Output Mapping:** Merges the results into a standardized JSON snapshot containing:
  * `email`: User account email.
  * `models`: Autocomplete/model metadata (for backward compatibility).
  * `quotaSummary`: Structured list of groups and buckets, including remaining fractions and reset times.

## Signed-Out Detection & usage-flow stability (fixed 2026-07-03, 1.9.1)

When the user is signed out while the language server is still running, `GetUserStatus` does **not** reliably return `401`. On the current Antigravity build it returns **HTTP `500`** with body `{"code":"unknown","message":"GetCascadeModelConfigData() is nil"}` - the server answers, but has no session, so the account-derived model config is nil (empirically verified on this machine after a real logout). The earlier code assumed signed-out == `401`, so it mislabeled this as a generic connection failure. `get-antigravity-usage.sh` sends raw RPC responses inside delimited frames,

- classic: HTTP `401` / `unauthorized`
- current: HTTP `500` matching `is nil` / `GetCascadeModelConfigData`

**Root cause of the "usage keeps erroring / unstable" report:** the probe script itself is 100% stable while the IDE runs (measured 8/8, ~175 ms). The instability was purely in error surfacing: `get_antigravity_usage` in `agent_usage/antigravity.rs` only swallowed `"is not running"` / `"Not authenticated"` / `"command not found"` to `Ok(None)`, and returned `Err` for every other transient case - port not open yet, IDE mid-restart, a single RPC timeout, and the signed-out `500`. Each `Err` set `error.value` in the frontend monitor (khi đó là `useAgentUsage.js`; sau refactor 1.20.0 là `usageMonitor.js`), flashing an error banner every poll. AG usage is a best-effort monitor and the frontend already has a graceful null path (show the last cached account), so `get_antigravity_usage` now swallows **any** non-zero script exit to `Ok(None)` and logs the reason at debug level. Result: transient/offline/signed-out states show the cached account (or the "Not connected - open & sign in to Antigravity to monitor" empty state), never a repeating banner.

## Log Out (fixed 2026-07-03, v1.9.x → next)

Antigravity's account dropdown (in `AgentUsage.vue`, opened by clicking the email) has a **Log Out** row that calls the `logout_antigravity` Tauri command.

**Where the credential actually lives (empirically verified on this machine).** The live OAuth session is stored in VS Code's globalState SQLite store, `User/globalStorage/state.vscdb`, in the `ItemTable` under two keys:

- `antigravityUnifiedStateSync.oauthToken` (~1 KB)
- `antigravityUnifiedStateSync.userStatus` (~8 KB - account/email/quota)

These values are **not** Electron `safeStorage` ciphertext - they carry no `v10`/`v11` prefix (inspected without materializing the token; first byte `0x43` = base64 protobuf, the Connect-RPC wire form). The earlier theory that the token was `safeStorage`-encrypted and that deleting the `"Antigravity IDE Safe Storage"` Keychain item made it "permanently undecryptable" was **wrong**: because the token isn't encrypted with that key, wiping cookies + the Keychain item left the token fully readable, so the IDE re-read it on next launch and silently signed back in. That was the "logout does nothing" bug in 1.9.

`logout_antigravity` now:

1. Quits the app (`osascript quit app` then `pkill -f` fallback) so nothing holds the files open.
2. Deletes the account-only Chromium files (`ANTIGRAVITY_ACCOUNT_ONLY_PATHS`).
3. **Deletes the two auth rows** (`ANTIGRAVITY_AUTH_KEYS`) from `state.vscdb` **and** `state.vscdb.backup` (Antigravity restores from the backup if the primary is missing) via the system `/usr/bin/sqlite3` - `remove_antigravity_auth_rows()`. This is what actually forces re-login. A `DELETE ... WHERE key IN (...)` touches only those two rows and leaves all other globalState intact (verified: 2 keys removed, 1632 rows preserved).
4. Deletes the `"Antigravity IDE Safe Storage"` Keychain item (defense-in-depth - harmless, and covers any future build that *does* move to `safeStorage`).

`User/` (settings, keybindings, snippets, extensions, workspaceStorage) and the rest of `globalStorage/` are never touched, so extensions, rules, and permissions survive a logout intact.

## Execution Environment

The script is compiled into the Tauri binary via `include_str!` inside [antigravity.rs](../../src-tauri/src/agent_usage/antigravity.rs) and executed in a POSIX shell using `sh` for local targets or `ssh <host> sh` for remote targets.

## Stability and Performance

* **Zero Plugin Conflicts:** By targeting the native binary `language_server_` names rather than a generic `"language-server"` search, it avoids false matches with external plugins like Volar's `language-server.js` or `cssServerMain` which run inside the Antigravity IDE directory.
* **Zero CLI Startup Latency:** Directly executing our raw shell script avoids spawning `npx` or Node interpreters over remote shells, bringing detection time down to ~40ms.

---

## Per-Account Cache (localStorage) & Account Dropdown

Antigravity can switch the logged-in account on the same machine, so usage is cached **per account per machine** in `localStorage` under `aki-antigravity-usage-cache-v4`. The whole cache is owned by `src/composables/agUsageCache.js` and reached only through its functions — no component parses it:

```json
{
  "accounts": {
    "local|user@a.com":  { "data": { ...usage, "email": "user@a.com", "sourceType": "cli" }, "fetchedAt": 1751430000, "host": "local" },
    "local|user@b.com":  { "data": { ... }, "fetchedAt": 1751420000, "host": "local" },
    "devbox|user@a.com": { "data": { ... }, "fetchedAt": 1751430500, "host": "devbox" }
  },
  "lastActiveKeyByHost": { "local": "user@a.com", "devbox": "user@a.com" }
}
```

* **Account identity = email, nothing else (v4, 2026-07-30).** A Google account has ONE quota regardless of which local surface (IDE, CLI, desktop) is running. `sourceType` is transport metadata — it records HOW the data was collected (icon/badge), not WHICH quota it belongs to. One email = one cache entry = one quota. Two entries for the same email on the same host are the same quota: the cache and the dedup in `antigravity_payload.rs` both key on email alone.
* **Why the host is in the key** (v3, 1.20.0): the same Google account is routinely signed in on the Mac and on a remote host at once. v2 keyed on `email` (with host as advisory metadata), so both machines wrote the one key — whichever polled last overwrote the other's reading. Two records that share a key cannot be told apart by metadata; only by the key. `lastActiveKeyByHost` (per-host pointer) replaced the former global `lastActiveEmail` for the same reason.
* **Migration chain:** `aki-antigravity-usage-cache` (v1 single blob) → v2 (per-account) → v3 (`host|email:sourceType`) → v4 (`host|email`). Each step runs once on load and removes the old key. v3→v4 collapses sourceType variants keeping the freshest entry per (host, email) — no reading is lost.
* **`sourceType` in the dropdown row** is the most-recently-observed surface — drives the icon (terminal = CLI, logo = IDE). It updates on every live fetch and is NOT used for deduplication or as a cache key.
* **Account dropdown:** clicking the email in the AG header (`AgentUsage.vue`) opens a dropdown listing every cached account (one row per email), newest first, with its cached-ago time and a «live» dot on the active one. Selecting a non-active account pins the view (`slotViewingEmail` in `AgentUsageSlot.vue`, persisted per slot). **The pin is an email-only handle** — it matches any surface (IDE or CLI) running that account, so switching between surfaces does not strand the card on a stale cache entry. **The monitor's `data` is the full parsed payload** (including `allAccounts`); `AgentUsageSlot.vue`'s `slotAccountInfo` computed resolves the right account for display — that separation is what lets two slots show two different accounts from the same monitor.
* **Sau refactor 1.20.0** mỗi monitor là một entity riêng theo `monitorId(agentId, host)` (`usageMonitorRegistry.js`), nên mỗi `antigravity@<host>` giữ danh sách account của riêng nó.

> **Contrast with Claude Code:** CC deliberately has **no** multi-account cache - exactly one account per remote host by design (see `usage-claudecode.md`). Only Antigravity uses this store.

## Smart Multi-Environment Logout (`logout_antigravity` vs `logout_antigravity_cli`)

A Google account can be simultaneously signed into both the Antigravity IDE and the AGY CLI on the same machine — these are two **surfaces of the same account**, not two different accounts. Logout is surface-aware because the credential stores differ:

1. **Antigravity IDE surface:**
   - Command: `logout_antigravity`
   - Action: Quits `Antigravity IDE.app` (`osascript` / `pkill`) and wipes OAuth session rows (`antigravityUnifiedStateSync.oauthToken` and `.userStatus`) from SQLite `state.vscdb` (`~/Library/Application Support/antigravity-ide/User/globalStorage/state.vscdb`).
   - UI: Dropdown displays `<i class="fa-solid fa-right-from-bracket"></i> Log Out IDE`.
   - `sourceType` in the dropdown row will be `"ide"` when the IDE surface was the most recently observed one.
2. **AGY CLI surface:**
   - Command: `logout_antigravity_cli`
   - Action: Terminates `agy` CLI binary processes (`pkill -f agy`) and removes CLI credential files (`oauth_creds.json`, `google_accounts.json`, `state.json`) from `~/.gemini/`.
   - UI: Dropdown displays `<i class="fa-solid fa-terminal"></i> Log Out CLI`.
   - `sourceType` in the dropdown row will be `"cli"` when the CLI surface was the most recently observed one.

Both commands trigger `@logout-success`, causing all active usage slots to self-heal and fall back cleanly to remaining active accounts.

## Dynamic N-Tier Grid Architecture (`usageTierStore.js` & `AgentUsageSection.vue`)

The usage section uses a declarative, standardized N-Tier slot architecture:

* **Declarative Schema (`ALL_TIER_ROWS`):** Slots are modeled declaratively as rows of slot configurations (`Slot A` & `Slot B` for Tier 1; `Slot C` & `Slot D` for Tier 2).
* **Zero Template Duplication:** A nested `v-for` renders `activeTierRows` dynamically based on `tierCount`. Adding N tiers requires zero HTML template edits.
* **User Control:** The App Titlebar Menu (☰) includes a 1 Tier / 2 Tiers toggle, persisting the preference in `localStorage` (`aki-usage-tier-count`).

## Log Out behavior & cache retention (PO decision, chốt 2026-07-07 - nguồn chân lý)

**Mục tiêu của cache multi-account**: xem được hiện trạng lần cuối (last-known state) của **từng** account, vì AG native chỉ hiển thị được 1 account tại 1 thời điểm. Cache tồn tại để bù đắp đúng giới hạn đó - không phải để "dọn dẹp" hay "ẩn" tài khoản không còn active.

- **Sau khi Log Out 1 account trong app**: header hiện **như bình thường** - không có xử lý đặc biệt, không blank/reset về trạng thái trống. `resetAccount()` chỉ kích một lần `checkUsage()` ngay (đẩy nhanh việc phát hiện account mới nếu người dùng sắp login lại) - **không** xóa `data`, `activeEmail`, `viewingEmail`, hay bất kỳ entry nào trong `accounts`. Chấm "live" (`ag-live-dot`) đã tự nhiên di chuyển sang account mới active ngay khi nó có live fetch đầu tiên - không cần thêm indicator/badge riêng cho "account này vừa logout".
- **Thời hạn giữ trong dropdown: vô hạn.** Không có cơ chế dọn theo thời gian/số lượng. Một account đã từng xuất hiện thì luôn có mặt trong dropdown cho tới khi bị dọn thủ công (không có UI cho việc này ở v1 - nếu cần, đó là yêu cầu riêng).
- **Hiển thị "hiện trạng lần cuối": tooltip hiện tại là đủ** (`cachedAgo`/`cachedAbsTime` trong `AgentUsage.vue`) - không cần thêm timestamp tường minh ở dòng dropdown.

### Lịch sử - vì sao mục này tồn tại (tránh tái phạm)

1.9.3 (`a26b8f5` -> `b082d0d`) từng thêm `resetAccount()` gọi `clearAgStore()` để giải quyết vấn đề "header vẫn hiện account vừa logout" - nhưng đó **không phải là bug** theo mục tiêu thật của tính năng (xem đầu mục này), và cách giải quyết (xóa toàn bộ store) là một regression nghiêm trọng: mỗi lần logout xóa sạch lịch sử mọi account, không chỉ account vừa logout. Đã sửa 2026-07-07: `resetAccount()` không còn xóa gì, chỉ trigger recheck. Xem mục "Regression Guard - Multi-entity State" trong `CLAUDE.md` (repo root) cho nguyên tắc chung rút ra từ sự việc này.

### Design locks (by design - do not "fix")

- **CC has no multi-account cache.** One account per remote host; do not add an AG-style store to CC.
- **The per-slot account pin IS persisted, and self-heals rather than being cleared at boot.** Each slot stores its pin under `aki-usage-slot-<id>-viewing-account` and restores it on reload (1.18.0, "Persistent slot account selection") - that is what lets a two-slot side-by-side comparison of two Antigravity accounts survive a restart. Do **not** add a boot-time unpin: the failure it would be aimed at (a pin restored onto an account that no longer exists anywhere) is already handled, scoped to the one slot, by the `slotAccountInfo.isMissing` watcher in `AgentUsageSlot.vue`, which clears only that slot's key once the first fetch settles. A pin onto an account that is merely *not live* but still cached deliberately keeps showing that account's last-known state - the whole purpose of the cache.
- **Header shows a character-truncated email.** `truncEmail()` in `AgentUsage.vue` cắt theo **số ký tự** (12, hoặc 7 ở breakpoint narrow `window.innerWidth <= 700`) rồi thêm `…` - **không** phải lấy phần trước dấu `@`. Mục đích là giữ width header ổn định khi đổi account active/cached; the full email is shown in the dropdown rows and the tooltip. (Ghi chú design lock nằm ngay trên hàm.)
- **AG payload always has an email.** Antigravity authenticates via Google, so a live payload always carries `email`; no empty-email guard is added in the live cache path.
- **Logout never clears the account cache or blanks the header.** See "Log Out behavior & cache retention" above - this is a deliberate product decision, not a gap to "fix" later.
- **10-Day Eviction TTL on cached accounts (v1.16.1):** Account records with `fetchedAt` older than 10 days (`> 864,000s`) are automatically evicted in `prune()`, gọi từ `loadStore()` (cả hai là private trong `agUsageCache.js`), to prevent indefinite accumulation of obsolete sessions.

---

## Related Source Files

- **Backend / Scripts:**
  - [get-antigravity-usage.sh](../../scripts/get-antigravity-usage.sh) - POSIX shell script to probe and fetch Connect RPC metrics.
  - [remote_shell.rs](../../src-tauri/src/remote_shell.rs) - Shared script-transport funnel and SSH lock management.
  - [agent_usage/](../../src-tauri/src/agent_usage/) - Domain modules for usage IPC handlers, probe result types, and Antigravity/Claude Code probes.
- **Frontend Stores & Composables:**
  - [usageMonitor.js](../../src/composables/usageMonitor.js) - One monitor entity: poll loop, circuit breaker, wake self-heal, multi-account view state. Its agent and machine are immutable identity.
  - [usageMonitorRegistry.js](../../src/composables/usageMonitorRegistry.js) - Multiton keyed `agentId@host`; two slots naming the same pair share one monitor and one poll.
  - [agUsageCache.js](../../src/composables/agUsageCache.js) - The per-account, per-host cache above; sole owner of the localStorage key.
  - [usageMonitorStore.js](../../src/store/usageMonitorStore.js) - Which monitors are switched on, keyed by monitor id (mirrored).
  - [usageSlotStore.js](../../src/store/usageSlotStore.js) - What each display slot points at, including its own remote host (mirrored).
  - [usageTierStore.js](../../src/store/usageTierStore.js) - Reactive store managing 1 Tier / 2 Tiers layout preference.
- **UI Components:**
  - [AgentUsageSection.vue](../../src/components/AgentUsageSection.vue) - Pure N-Tier slot layout; owns no usage state.
  - [AgentUsageSlot.vue](../../src/components/AgentUsageSlot.vue) - Independent display slot component supporting per-slot account viewing state.
  - [AgentUsage.vue](../../src/components/AgentUsage.vue) - Usage card component featuring dynamic IDE/CLI icon, cyan/purple live dots, and environment-aware logout.
  - [UsageCircle.vue](../../src/components/UsageCircle.vue) - SVG radial progress circle used for Gemini and Claude/GPT quota buckets.
  - [RefreshRing.vue](../../src/components/RefreshRing.vue) - Countdown ring on the reload button (overlay mode).
