# Aki Dev Sync 🚀

> MacOS App (tauri v2) for my workflow: rsync code between local-remote. Antigravity IDE for local with .git source-of-truth, ClaudeCode on remote with shared Claude MAX plan. Live monitor Local AG & remote CC quota limit

<img width="1046" height="814" alt="2026-06-24_22-19-54" src="https://github.com/user-attachments/assets/96f41605-6a88-4cea-86db-ea0399769b24" />

https://github.com/lacvietanh/aki-dev-sync/releases/latest


## 🧭 The Model: Local ↔ Remote

Aki Dev Sync solves one problem: keeping a **split development environment** in sync. You code on one machine and let an AI agent work on another — without committing noise to Git just to move files around.

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

- **LOCAL — Source of Truth.** Your Git history lives here. You review, commit, and edit in a personal IDE (e.g. Antigravity Pro).
- **REMOTE — AI Workspace.** A stronger box reachable over SSH where an AI agent (e.g. Claude Code / Claude MAX) reads the full project context and generates code at scale.
- **PUSH** sends your local changes up so the AI sees everything; **PULL** brings the AI's work back for review and commit — closing the loop.

## 👥 Who is this for?

This tool was built for a specific way of working — you'll feel at home if you:

- **Code on a weak machine, run on a strong server** — keep the laptop light, push heavy builds / AI to a server.
- **Need to protect your source** — work machine locked down? Keep the core code on your own remote server.
- **Switch between devices** — sync fast across PC, laptop, and server without dumping junk commits on GitHub.
- **Feed a full project to an AI** — push everything (including `.git/`) so the agent has complete context.

## ✨ Features

### ⚡ Sync

| Feature | What it does |
|---|---|
| **PUSH** (`.git` toggle) | Push Local → Remote. Toggle `.git/` on to give the AI full history, off to skip it. |
| **SELECT** (Push Special) | Pick individual changed files (Modified / Untracked / Deleted) from Git status and push just those — no full-tree scan. |
| **PULL** | Pull what the AI just wrote on Remote straight back to Local for a quick review & commit. |
| **Mirror / Delete** (per project) | Optional `--delete` mode for Push and Pull. Off by default for Push (it never deletes on the remote); when on, pushing over pending AI changes triggers a confirm dialog first. |
| **DRY RUN** | Preview the exact rsync changes without writing a single byte. |
| **Sync Status** | PUSH/PULL buttons light up automatically when the two sides drift; background polling keeps it current. |
| **Pre / Post Hooks** | Run scripts before/after each push & pull (build, restart a service, notify…), locally or on the remote. |

### 🛠 Tools & Monitor

| Feature | What it does |
|---|---|
| **Open Popup** | One menu to open a project — **Local:** Finder, Terminal, VSCode, VSCode Insiders, Antigravity; **Remote (SSH):** SSH Terminal, VSCode Remote, VSCode Insiders Remote, Antigravity Remote. Unavailable IDEs are dimmed automatically. |
| **Agent Usage** | **Real** quota — not estimates. **Claude Code** (Remote) read from Anthropic's own `rate_limits` (5-hour + 7-day), with plan tier and last-session cost. **Antigravity** (Local) pulled from the IDE's native Language Server, showing the Gemini and Claude/OSS pools. Relative-time reset countdowns. |
| **Force Sync Quota** (↻) | Re-read local usage data by running `claude --model haiku -p /usage` on the remote. This reads local JSONL session logs on that machine (P2, not a network call to Anthropic). Returns `0%` if no local session has run in the current 5h window. |
| **SSH Config Editor** | Edit `~/.ssh/config` in-app with a built-in undo/backup safety net — auto-loads your hosts. |
| **Git Actions** | Unified Git modal: status (Clean / Dirty / Ahead / No Git), remote URL, commit log, and commit-and-push — all from one native scan. |
| **Project Config** | Per-direction rsync excludes with one-click presets (**Nuxt 4 / Tauri v2 / Aki Default**), Production URL quick-open, run-hooks-local-or-remote, and ignore-hook-errors toggles. |
| **Background Refresh** | Polls remote sync diff (60s) and agent usage (30s) in the background; per-type intervals are configurable. |

## 🔬 Under the Hood

The parts I'm quietly proud of — the clever bits that make the boring stuff "just work":

- **Real quota, not guesses.** Claude Code's `statusLine` hook emits Anthropic's actual `rate_limits` after every turn. We persist it by idempotently patching `statusline-command.sh` over SSH — so the numbers are server truth, not token estimates.
- **Hybrid Patching survives the 100% blackout.** When you hit your limit, the Claude CLI *drops* the `rate_limits` block entirely (the 429 quirk) and the progress bar would vanish. Our injected jq+bash merges the last known reset time and pins `100%`, so the UI never breaks exactly when you most need to see it.
- **Antigravity quota, reverse-engineered.** Google's cloud endpoints return dead `0%` data. Instead we read the IDE's **local Language Server** directly: scan the process table for the native binary, extract its CSRF token, find the listening port via `lsof`, then query the `GetUserStatus` Connect RPC. Raw JS, no `npx` — **~40ms**.
- **Force Sync with Auto-Probe.** `/usage` reads local JSONL session logs (`~/.claude/projects/**/*.jsonl`) and computes usage locally. Output explicitly states *"does not include other devices or claude.ai"*. If the remote host has no active local session in the current 5h window, `/usage` conceals the reset time. Our script solves this by automatically running a quick "Probe Session" (a tiny dummy Haiku session with prompt "respond with ok") in a temporary directory to generate local logs, natively forcing the CLI to return the exact resets_at time.
- **The `.git/` mtime trap.** `git status` rewrites `.git/index`, bumping the `.git/` directory mtime, which made rsync think there was always something to push — button permanently lit. We filter directory-only entries from the dry-run count, so PUSH lights up for real changes, not git housekeeping.

→ Deep dives: [Claude Code quota](docs/arch/usage-claudecode.md) · [Antigravity quota](docs/arch/usage-antigravity.md) · [Background refresh](docs/feat/background-refresh.md) · [104-agent quota-measurement research](docs/ref/deepresearch-claudecode-antigravity-quota-measurement.md)

## 📦 Install (macOS)

1. Download the latest `.dmg` from the [**Releases**](https://github.com/lacvietanh/aki-dev-sync/releases) page
   (`Aki-DevSync-vX.X.X-arm.dmg` for Apple Silicon, `-universal.dmg` for Intel + Apple Silicon).
2. Open the `.dmg` and drag the app to `Applications`.
3. The build is unsigned — on first launch macOS Gatekeeper will block it. **Right-click the app → Open**, then confirm. (Or run `xattr -dr com.apple.quarantine "/Applications/Aki Dev Sync.app"`.)

**Requirements:** `rsync` and `ssh` available on your `PATH` (preinstalled on macOS), plus an SSH host you can reach.

## 🛠 Tech Stack

- **Frontend:** Vue 3 + Vite, vanilla CSS
- **Backend:** Rust + Tauri v2
- **Core engine:** native `rsync` + `ssh`

## 🔨 Build from source

```bash
npm install
npm run tauri dev    # first run compiles Rust (~5–10 min)
npm run build:app    # production build + artifact rename
```

Full prerequisites (macOS & Linux), build conventions, and Tauri gotchas are in **[CONTRIBUTING.md](CONTRIBUTING.md)**.

## 📚 Documentation

- **[docs/index.md](docs/index.md)** — full documentation index
- [Sync flow](docs/feat/sync-flow.md) · [Open Popup](docs/feat/open-popup.md) · [Background refresh](docs/feat/background-refresh.md)
- Agent usage internals: [Claude Code](docs/arch/usage-claudecode.md) · [Antigravity](docs/arch/usage-antigravity.md)
- Research: [quota measurement methods](docs/ref/claudecode-antigravity-quota-measurement.md)

---

*Built for speed and the Lạc Việt Anh Workflow.*
