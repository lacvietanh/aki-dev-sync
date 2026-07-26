# Aki Dev Sync 🚀

> MacOS App (tauri v2) for my workflow: rsync code between local-remote. Antigravity IDE for local with .git source-of-truth, ClaudeCode on remote with shared Claude MAX plan. Live monitor Local AG & remote CC quota limit

<img width="1920" height="1080" alt="Aki-Dev-Sync-1 18 0" src="https://github.com/user-attachments/assets/b37fbe39-7836-4c92-ae87-64c9710909a9" />

## 🧭 The Model: Local ↔ Remote

Aki Dev Sync solves one problem: keeping a **split development environment** in sync. You code on one machine and let an AI agent work on another - without committing noise to Git just to move files around.

```
                       PUSH  ───────────────►
   ┌───────────────────────┐         ┌───────────────────────┐
   │   LOCAL                │         │   REMOTE               │
   │   Source of Truth      │         │   AI Workspace         │
   │   • Git history        │         │   • Claude Code / MAX  │
   │   • Antigravity IDE    │         │   • Heavy builds / GPU │
   └───────────────────────┘         └───────────────────────┘
                       ◄───────────────  PULL
```

- **LOCAL - Source of Truth.** Your Git history lives here. You review, commit, and edit in a personal IDE (e.g. Antigravity Pro).
- **REMOTE - AI Workspace.** A stronger box reachable over SSH where an AI agent (e.g. Claude Code / Claude MAX) reads the full project context and generates code at scale.
- **PUSH** sends your local changes up so the AI sees everything; **PULL** brings the AI's work back for review and commit - closing the loop.

## 👥 Who is this for?

This tool was built for a specific way of working - you'll feel at home if you:

- **Code on a weak machine, run on a strong server** - keep the laptop light, push heavy builds / AI to a server.
- **Need to protect your source** - work machine locked down? Keep the core code on your own remote server.
- **Switch between devices** - sync fast across PC, laptop, and server without dumping junk commits on GitHub.
- **Feed a full project to an AI** - push everything (including `.git/`) so the agent has complete context.

## ✨ Features

### ⚡ Sync

| Feature | What it does |
|---|---|
| **PUSH** | Push Local → Remote, carrying everything not listed in that project's `push_excludes`. `.git/` ships by default so the AI gets full history - drop it from PULL's list only, and it becomes a **push-only path**: pushed up, never pulled back, and never counted as "changed" (no more badge lighting up from git housekeeping). Add `.git/` to `push_excludes` to skip it entirely. |
| **SELECT** (Push Special) | Native OS file picker (multi-select, starts in project root). If any selected file already exists on remote, shows a local-vs-remote mtime conflict table before confirming the push. |
| **PULL** | Pull what the AI just wrote on Remote straight back to Local for a quick review & commit. |
| **Mirror / Delete** (per project) | Optional `--delete` mode for Push and Pull. Off by default for Push (it never deletes on the remote); when on, pushing over pending AI changes triggers a confirm dialog first. |
| **DRY RUN** | Preview the exact rsync changes without writing a single byte. |
| **Sync Status** | PUSH/PULL buttons light up automatically when the two sides drift; background polling keeps it current. |
| **Pre / Post Hooks** | Run scripts before/after each push & pull (build, restart a service, notify…), locally or on the remote. |

### 🛠 Tools & Monitor

| Feature | What it does |
|---|---|
| **Project Tasks** | Per-project task list in a centered modal dialog (TASKS column, right before GIT). Track active, pinned (📌), and wish (🕒) tasks with independent toggles. Pinned tasks sort to the top, wish tasks sink to the bottom, and completed tasks sink to the absolute bottom with a smooth Vue transition. Marking a task as completed automatically unpins it. Includes a project notes card at the top with native height autogrow (`field-sizing: content`) and auto-trim. Stored in `projects.json`. Runs on the same shared task engine as Global Note's task list (see below) - one list panel, one notes field, two independent data sources. |
| **Open Popup** | One menu to open a project - hover the `OPEN` button on the Mac, or **tap it** on a paired phone (tap again, Esc, or a tap outside closes it) - **Local:** Finder, In-App Terminal, Terminal, VSCode, VSCode Insiders, Antigravity IDE; **Remote (SSH):** SSH Terminal, VSCode (Remote SSH), VSCode Insiders (Remote), Antigravity (Remote), Upload (select files) - a native picker that pushes just the files you tick. The popup header also carries **REPORT**, which opens the project's `REPORT.html` (pulling a newer copy off the remote first if there is one). Two inline **DEV** (green) and **BUILD** (amber) buttons auto-detected by stack (Tauri / Nuxt / Node) with tooltip showing the exact command - both just open Terminal with that command (DEV's earlier auto-open-in-browser behavior was removed, see [Open Popup](docs/feat/open-popup.md)); per-project overrides in Settings. Unavailable IDEs are dimmed automatically. Every Terminal window this app opens (local, SSH, DEV/BUILD, AkiClaudeDoc install) auto-snaps to 124 columns in the top-right corner of the main display. |
| **Global Note** | A persistent free-form notepad in the titlebar (sticky note icon, turns amber when non-empty), now with a full task list alongside the notes - pin/wish/done, same behavior as per-project tasks. Not tied to any project - jot down anything across sessions. Auto-saves with 500ms debounce, stored as `{content, tasks}` in `{appDataDir}/globalnote.json` (an older content-only file still loads, tasks start empty). Header badges show pinned/open task counts. |
| **Agent Usage** | **Real** quota - not estimates. **Claude Code** reads the number Anthropic's own CLI already computed via the `statusLine` hook cache (`rate_limits`) - locally on this Mac or on any selected SSH host - showing plan tier, email, and org name. **Antigravity** supports all 3 execution surfaces: Desktop App (`AG` - white icon), IDE (`IDE` - VS Code extension), and CLI (`CLI` - terminal), querying local Connect RPC endpoints with 1-pass process detection (~40ms) and smart session deduplication (`AG`). Both agents can be watched **locally or on any number of SSH hosts at once** - each display slot carries its own host picker, so one slot can watch Claude Code on server A while another watches it on server B under a different account. Each monitor has its own power icon; two slots pointed at the same agent-and-machine share one poll rather than doubling it. One bar per quota bucket the account actually has - 5-Hour and 7-Day today, plus any per-model weekly (Opus, Sonnet, …) rendered automatically the moment Anthropic starts reporting it, with no app update. When a pool's shared 7-day quota hits 100% its own 5-hour reading is dimmed off the colour ladder - the pool is spent either way, so the number that still matters is the one left in colour. Contextual Log Out clears the specific auth domain (`state.vscdb` for IDE vs `~/.gemini/` for AG & CLI, see [Antigravity quota](docs/arch/usage-antigravity.md)). |
| **Sync Check switch** | One power icon in the SYNC column header turns project sync on/off app-wide: PUSH/PULL/SELECT buttons, the Open popup's **Upload (select files)** item, and background + manual remote-diff checks. It no longer hides the popup's REMOTE column - SSH Terminal, the remote IDE entries and COPY are ways to reach the server, not rsync traffic, so they stay usable with sync off. Independent of the Claude Code remote monitor below - muting one no longer mutes the other (see [Sync check & usage switches](docs/feat/sync-check-and-usage-switches.md)). |
| **Remote monitor switches** | The usage widget's REMOTE tab carries the same `AG` / `CC` pair as LOCAL, each with its own power icon: Antigravity and Claude Code remote polling turn on and off separately, and both are independent of the Sync Check switch above. The host dropdown beside them belongs to that slot alone, so different slots can watch different machines simultaneously; every monitor is on by default and switching one off leaves the others untouched. |
| **SSH Config Editor** | Edit `~/.ssh/config` in-app with a built-in undo/backup safety net - auto-loads your hosts. |
| **Git Actions** | Unified Git modal: status (Clean / Dirty / Ahead / No Git), remote URL, and commit log. Supports colored terminal log consoles (ANSI parser), stage & commit, fetch, push, Vietnamese accents (quotepath=false), and visual project changelog Markdown preview. |
| **App Update Check** | Automatically checks for app updates silently on launch or manually from the app-icon dropdown menu, displaying version badges. |
| **Project Config** | Per-direction rsync excludes with one-click presets (**Nuxt 4 / Tauri v2 / Aki Default**) in a side-by-side PUSH/PULL layout. Per-project DEV/BUILD command overrides. Production URL quick-open, run-hooks-local-or-remote, and ignore-hook-errors toggles. |
| **Background Refresh** | One unit of work (`refreshProject`) runs a project's git status, remote diff, and dev/build stack detection in parallel; the same unit backs the two background timers, the per-project Refresh button, and the global Refresh button - clicking global Refresh no longer reloads the whole app. Visual countdown rings on the GIT and ACTIONS column headers show live refresh progress; per-type intervals are configurable. See [Refresh controller](docs/arch/refresh-controller.md). |
| **Narrow Mode** | The window stays usable resized down to 420px wide. One shared 700px breakpoint drives every component; labels hide first (icons + tooltips stay), never the reverse. |
| **App-icon menu** | Titlebar dropdown (☰) - GitHub/release links, manual update check, SSH config editor, Enable SSH Terminal Color, Statusline Customizer, Claude Code Profile (Local), Remote Control (toggle + pair code + address rows + an **HTTPS (PWA)** toggle), AkiClaudeDoc install, a `Usage row:` picker - **1 row** (2 slots side by side) or **2 rows** (4 slots) - plus window presets under `AppWindow:` - **Narrow** (420px), **Wide** (768px), **Stick Top-Left** (snaps to the top-left monitor, auto-fits height to the project list), **Center Primary**. `⌘1` applies Narrow + Stick Top-Left, `⌘2` applies Wide + Center Primary. Tick **remember** and the width and placement you pick are re-applied on the next launch. |
| **Remote Control** *(preview)* | Control this Mac from a phone browser on the same LAN or over Tailscale. Menu → **Remote Control** → **On** shows a 6-digit pair code and the `IP:PORT` rows to open on the phone (click to copy); the phone pairs once and reconnects silently after that. The Mac stays the single source of truth - the phone mirrors its state and sends back gestures over one WebSocket. **Off** cuts every live phone and stops serving anything on that port; 10 wrong codes disable it automatically. Same address in dev and release (`:1421`). An **HTTPS (PWA)** row serves the same page over HTTPS via Tailscale, which is what lets the phone Install it as a standalone app (needs HTTPS certs enabled once in the Tailscale admin console). A confirmation the phone triggers (the typed `--delete` confirm, Remove Project, the missing-SSH-host picker, the "preview failed - continue anyway?" prompt) now appears on **both** screens and can be answered from either. Note the LAN address is plain HTTP, so the pairing token travels unencrypted and anyone on that network can read it - the **On** toggle turns amber to say so; the Tailscale HTTPS address does not have this problem. See [Remote Control](docs/feat/remote-control.md). |
| **In-app Terminal** | A `TERMINAL` stack above the event log (the bottom dock is now two independently collapsible stacks - collapse the log and it shrinks to a single live line, latest message only) running a real interactive shell on this Mac - a genuine PTY, so history recall, `Ctrl+C`, interactive prompts and `vim` all work. **Groups**: tabs are scoped into one **group** per project plus one **global** group not tied to any project - a project's `TERM` cell or the column header's terminal icon switches the stack to that group and shows only its tabs (other groups' shells keep running untouched). `⌘T` opens a new tab / `⌘W` closes / `⌘⇧[`/`⌘⇧]` cycle - all within the **current group only**. Every project's `TERMINAL` column (replacing the old LAST ACTION column - see below) opens or reactivates that project's own tab, starting in its directory with no `cd` if the group is empty, or reusing the group's live tab untouched (no `cd`) if one is already open - so a long-running command in that shell is never interrupted. Whatever you type on the Mac or on a paired phone lands in the same shell per tab and both screens show identical output, which is the point - `Terminal.app` renders nowhere but the Mac, so a phone could never drive it. A slim key row (Esc, Tab, sticky Shift, sticky Ctrl, arrows, Enter) covers what a phone keyboard can't send and now shows only on the phone, not on the Mac's own screen - sticky Shift turns Tab into backtab (`Shift+Tab`, handy for Claude Code's mode cycling) and the arrows into their shifted CSI sequences. A compose input sits right under the key row so voice dictation or the phone's IME can type a full command before sending it in one go. Recent output replays per tab when you reopen the tab or your phone reconnects. The shell is a login shell, so it has the same `PATH` as every other terminal on the machine. The panel header now carries one button - **CLOSE** (hides the panel, every shell keeps running) / **EXPAND** - replacing the old CLEAR/RESTART/KILL/OPEN toolbar: KILL and RESTART are now the tab chip's own ✕ (✕ then + for restart); OPEN moved to each project's OPEN popup. When a shell exits, only its own tab/badge turns red instead of going quiet; typing anything starts a new one. See [In-app Terminal](docs/feat/in-app-terminal.md). |
| **Project TERMINAL column** | Replaces the old LAST ACTION column on the project table: one button per row - opens/reuses that project's terminal **group** (works from a phone) - with two absolute-overlay badges: top (cyan, red if a shell in the group has exited) = in-app tab count for that group; bottom (slate) = external `Terminal.app` windows standing in that project's directory **right now** (a live count: the host re-scans the process table every 5s, so closing a window drops the badge). External `Terminal.app` itself now opens only from the OPEN popup's **Terminal** item. LAST ACTION itself moved under the sync buttons as two small muted lines (action + time, host below), absent entirely for a project that has never synced. |
| **Statusline Customizer** | Visually build **one** statusline script for both CLIs - toggle fields (identity, model, context, cache, 5h/7d rate limits + reset ETA, session, git, RAM), drag to reorder, recolor, set per-field truncate widths and the alternating block background - and push the result to local and/or any configured remote host. The same file is installed for Claude Code and AGY CLI - each CLI also gets pointed at it in its own `settings.json` - and it identifies which one is running it from its own path. Every host row shows a `CC` / `AG` tag per CLI found there - filled once that CLI's statusline actually renders, hollow while it is still unwired. Colours in the modal are the terminal's real colours: every tier and picker swatch comes from one `{ ansi, hex }` table and names its ANSI code in the tooltip, so what you see is what the terminal prints. |

## 🔬 Under the Hood

The parts I'm quietly proud of - the clever bits that make the boring stuff "just work":

- **ANSI Terminal status colors & Unicode Vietnamese.** Force git to output color and raw paths via `-c color.status=always -c core.quotepath=false`. An extremely lightweight client-side Regex ANSI parser converts terminal escapes into styled HTML spans, displaying files in native terminal colors and rendering Vietnamese accents instead of obscure octal escapes.
- **Smart Stack Launcher & Lockfile Analyzer.** Inspects files to check if the project uses Tauri or Node, then scans lockfiles (`pnpm`, `yarn`, `bun`, or `npm`) to dynamically execute the correct start command inside the native macOS terminal.
- **Zero-JS Auto-growing Textarea.** Modern WebKit (Tauri/macOS) supports the CSS property `field-sizing: content;`. Using this eliminates all need for heavy JS resize keypress listeners and calculations, allowing tasks and notes inputs to auto-grow natively and smoothly.
- **Inherited Visual Changelog Preview.** We pass a `projectId` down to the existing `ChangelogModal` to fetch and render the project's own changelog file in clean Markdown and Mermaid, reusing the core layout.
- **Real quota, not guesses.** Claude Code's `statusLine` hook emits Anthropic's actual `rate_limits` after every turn. We persist it by idempotently patching `statusline-command.sh` over SSH - so the numbers are server truth, not token estimates.
- **Hybrid Patching survives the 100% blackout.** When you hit your limit, the Claude CLI *drops* the `rate_limits` block entirely (the 429 quirk) and the progress bar would vanish. Our injected jq+bash merges the last known reset time and pins `100%`, so the UI never breaks exactly when you most need to see it.
- **Antigravity quota, reverse-engineered.** Google's cloud endpoints return dead `0%` data. Instead we read the IDE's **local Language Server** directly: scan the process table for the native binary, extract its CSRF token, find the listening port via `lsof`, then query the `GetUserStatus` Connect RPC. Raw JS, no `npx` - **~40ms**.
- **Antigravity Log Out actually logs out.** Deleting the Chromium session files (Cookies, Local/Session Storage) alone does nothing - the OAuth token is encrypted at rest by Electron's `safeStorage` API, whose AES key lives in exactly one macOS Keychain item (`"Antigravity IDE Safe Storage"`). Log Out quits the app and deletes that single, precisely-named item - not a keychain scan - which permanently invalidates the stored token. Settings, extensions, and rules live in separate files and are never touched.
- **Claude Code quota has one source of truth, on purpose.** An earlier design ran a headless `claude -p /usage` probe to force a fresh reading; it was deleted after leaving 19 orphaned sessions (6GB RAM + 4GB swap) on a remote host, and a live measurement showed a headless turn only ever returns the reset boundary, never a percentage - the probe wasn't buying anything the passive `statusLine` cache doesn't already give. The app never runs `claude` itself to fetch usage; see [Claude Code quota](docs/arch/usage-claudecode.md) §5 for the full invariant list.
- **One refresh controller, one unit of work.** Every trigger that can refresh a project - the two background timers, the per-project Refresh button, the global Refresh button, saving a project's config - calls the same `refreshProject()` (git status + remote diff + stack detection, in parallel). Busy state lives on the check itself via a per-project counter, not on whichever button was clicked, so the spinner never lies about what's actually running; an in-flight check is cancelled with a generation token, never by touching a live rsync. See [Refresh controller](docs/arch/refresh-controller.md).
- **The `.git/` mtime trap.** `git status` rewrites `.git/index`, bumping the `.git/` directory mtime, which made rsync think there was always something to push - button permanently lit. We filter directory-only entries from the dry-run count, so PUSH lights up for real changes, not git housekeeping.
- **Bidirectional EC-3 disambiguation.** rsync is stateless - it cannot tell "remote created file X" from "local deleted file X", or "Mac created file Y" from "remote deleted file Y". After every full sync a local file-list snapshot (the *baseline*) is written to `{appDataDir}/baselines/`. On the next status check both PUSH and PULL lists are filtered: pull_files ∩ baseline ∩ absent-locally → Mac deleted → push_count; push_files ∩ baseline → remote deleted → suppress from push_count. This covers the dominant real-world case where most coding happens on the remote server.

→ Deep dives: [Claude Code quota](docs/arch/usage-claudecode.md) · [Antigravity quota](docs/arch/usage-antigravity.md) · [Refresh controller](docs/arch/refresh-controller.md) · [Background refresh](docs/feat/background-refresh.md) · [104-agent quota-measurement research](docs/ref/deepresearch-claudecode-antigravity-quota-measurement.md)

## 📦 Install (macOS)

1. Download the latest `.dmg` from the [**Releases**](https://github.com/lacvietanh/aki-dev-sync/releases) page
   (`Aki-DevSync-vX.X.X-arm.dmg` for Apple Silicon, `-universal.dmg` for Intel + Apple Silicon).
2. Open the `.dmg` and drag the app to `Applications`.
3. The build is unsigned - on first launch macOS Gatekeeper will block it. **Right-click the app → Open**, then confirm. (Or run `xattr -dr com.apple.quarantine "/Applications/Aki Dev Sync.app"`.)

**Requirements:** `rsync` and `ssh` available on your `PATH` (preinstalled on macOS), plus an SSH host you can reach.

## 🛠 Tech Stack

- **Frontend:** Vue 3 + Vite, vanilla CSS
- **Backend:** Rust + Tauri v2
- **Core engine:** native `rsync` + `ssh`

## 🔨 Build from source

```bash
npm install
npm run tauri dev    # first run compiles Rust (~5-10 min)
npm run build:rmud   # release build: universal (Intel + Apple Silicon) .dmg + artifact rename
npm run build:app    # local .app bundle (Apple Silicon only) + artifact rename
```

Full prerequisites (macOS & Linux), build conventions, and Tauri gotchas are in **[CONTRIBUTING.md](CONTRIBUTING.md)**.

## 📚 Documentation

- **[docs/index.md](docs/index.md)** - full documentation index
- [Sync flow](docs/feat/sync-flow.md) · [Open Popup](docs/feat/open-popup.md) · [Background refresh](docs/feat/background-refresh.md) · [Sync check & usage switches](docs/feat/sync-check-and-usage-switches.md) · [Refresh controller](docs/arch/refresh-controller.md)
- Agent usage internals: [Claude Code](docs/arch/usage-claudecode.md) · [Antigravity](docs/arch/usage-antigravity.md)
- Research: [quota measurement methods](docs/ref/deepresearch-claudecode-antigravity-quota-measurement.md)

## ☕ Donate

If you find this tool helpful, consider supporting its development!  
[**Donate to AkiDevSync**](https://app.akinet.me/en/qr-bank/?bank=970422&acc=0869297957&tpl=print&amount=0&info=Donate+AkiDevSync&name=LacVietAnh&view=1) ❤️

---

*Built for speed and the Lạc Việt Anh Workflow.*
