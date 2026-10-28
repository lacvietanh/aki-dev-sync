# Contributing to Aki Dev Sync

This is a Vue 3 (Vite) + Tauri v2 + Rust desktop app. This guide covers what a contributor must have before starting, local setup, build conventions, and the Tauri gotchas worth knowing before you touch the code.

The app ships for macOS only today. Windows and Linux support is planned but not started: [`docs/plan/v2-uniplatform-windows-linux-mvp.md`](docs/plan/v2-uniplatform-windows-linux-mvp.md) is the plan, and the maintainer has no Windows machine, so Windows work depends on contributors.

## Required before you start

A pull request that does not meet both requirements below is closed without review.

### 1. akirule installed in your coding agent

Every change in this repo is written and reviewed against the shared rule corpus [akidevrule](https://github.com/lacvietanh/akidevrule) (akirule). Install it before your first session, and re-run the same command to update:

```bash
npx @akinet/akidevrule@latest
```

On Windows without npm: `git clone https://github.com/lacvietanh/akidevrule.git; cd akidevrule; .\install.ps1`.

- Work with an agent the installer supports (Claude Code, Antigravity/Gemini, Codex CLI, …). Its replies must open with a `[RULES] …` line; no such line means the rules are not loaded, and the work does not count as done under them.
- In the pull request, state the akidevrule version you used (`npx @akinet/akidevrule@latest --check` prints it).
- The rules in this repo's [`CLAUDE.md`](CLAUDE.md) bind you as well. Read it whole before the first edit.

### 2. Windows work: at least one Linux host to test against

The app's core is reading Claude Code and Antigravity quota on remote dev hosts over SSH. A Windows change is only verified once the Windows build has read a real host directly. You need:

- one Linux machine (a VM, a cloud box or a spare PC) that the Windows machine reaches with `ssh <host>` without a password prompt (key auth, entry in `~/.ssh/config`);
- on that Linux host, at least one of `claude` (Claude Code) or `agy` (Antigravity CLI) installed and logged in, and used at least once so it has a quota to report. Both are better: each has its own probe;
- from the Windows build of the app: add the host, open the Agent Usage panel and confirm the host's quota bars fill in. Attach a screenshot to the pull request, with account names blurred.

Changes that touch neither Windows nor the remote path (docs, macOS-only UI) need only requirement 1.

## Where to start

- **Windows / Linux:** Phase 1 of the v2 plan (compile blockers) is self-contained and gates everything else. Phase 0 (changing the product's stated platform scope) is the maintainer's decision, so open an issue before starting anything beyond Phase 1.
- **Anything else:** open an issue describing the change first, then a pull request that links it.
- Docs start at [`docs/index.md`](docs/index.md).

## Prerequisites

### macOS

Install the Xcode Command Line Tools and Rust:

```bash
xcode-select --install
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Windows 10 / 11

- Rust via [rustup](https://rustup.rs) with the default MSVC toolchain, plus the Microsoft C++ Build Tools ("Desktop development with C++");
- WebView2 Runtime (present on current Windows 10/11);
- Node.js LTS;
- the OpenSSH client (`ssh.exe`, built into Windows 10/11), needed for requirement 2.

Full list: [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/).

### Linux (Ubuntu 22.04 / 24.04)

**1. Install Rust:**

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

**2. Install Tauri v2 system dependencies:**

```bash
sudo apt install -y \
  libwebkit2gtk-4.1-dev \
  libjavascriptcoregtk-4.1-dev \
  libsoup-3.0-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  build-essential \
  libssl-dev \
  pkg-config
```

> `build-essential`, `libssl-dev`, and `pkg-config` are usually already present on a dev machine - kept here for fresh installs.

## Run, test & build

```bash
npm install
npm run tauri dev    # dev; also the compile check (first run compiles Rust, ~5-10 min)
cargo test --manifest-path src-tauri/Cargo.toml --lib
npm run build:app    # maintainer's Mac only: arm64 .app, signed with a stable identity, replaces /Applications
```

`npm run help` lists every script with its purpose. `npm run build:rmaa` builds the `.app` without installing. To hand the app to someone, use `npm run build:rmad:friend` or `npm run build:rmud:friend` (the `.dmg` plus a double-click installer and a readme - `docs/ref/install-desktop.md`). For artifacts, use the package scripts, **not** raw `tauri build` - see "Post-build artifact rename" below.

## Pull request checklist

- `cargo test --manifest-path src-tauri/Cargo.toml --lib` passes on every OS you touched; for Windows changes, paste the output from a real Windows machine.
- For Windows changes: the screenshot from requirement 2.
- A user-visible change adds a line under `## [Unreleased]` in [`CHANGELOG.md`](CHANGELOG.md) and updates `README.md` and `src/components/modals/IntroModal.vue` if they describe it.
- Do not bump versions or create tags: releases are cut by the maintainer.
- The akidevrule version you used (requirement 1).

## Tauri gotchas & conventions

Lessons from building a macOS-first Tauri v2 app. Recorded to avoid re-discovery.

### Titlebar sacred boundary

`"decorations": false` + `"transparent": true` removes the native titlebar entirely. Every fixed or absolute-positioned element that spans the full window **must** start at `top: var(--titlebar-h)` (42px), never `top: 0`. Covering the drag region makes the window un-movable.

→ Full rule + rationale: [`docs/ref/titlebar-sacred-boundary.md`](docs/ref/titlebar-sacred-boundary.md)

### Version SSOT - `package.json`

`tauri.conf.json` sets `"version": "../package.json"`; never hardcode a version there. `Cargo.toml`'s crate version is bumped to the same number in the same commit. Every version value and git tag is bare semver (`1.10.1`, never `v1.10.1`).

### Post-build artifact rename

Raw `npm run tauri build` outputs filenames with spaces (e.g. `Aki Dev Sync_1.2.0_aarch64.dmg`). Use `npm run build:rmad` / `build:rmud` instead - each chains `tauri build` and `node scripts/post-build.js` to produce `Aki-DevSync-vX.X.X.BUILD-arch.dmg` (e.g. `Aki-DevSync-v1.20.0.2346-uni.dmg`; `BUILD` is `HHMM` at build time, or `$BUILD_NUM` if set).

### IPC capability: silent failures

Every Tauri command - including `@tauri-apps/api/window` calls - must be **granted** in `src-tauri/capabilities/default.json`. A missing entry causes a **silent no-op**: the JS call resolves without error and nothing happens. This was the root cause of window drag/minimize/close not responding (fixed by adding `core:window:allow-start-dragging`, `core:window:allow-minimize`, `core:window:allow-close`).

### async IPC + blocking subprocess

Tauri runs `async fn` commands on an async executor. Calling `std::process::Command` (blocking) directly inside an `async fn` starves the thread pool - the UI appears frozen until the command returns. Any command that waits on a subprocess, SSH or the network is an `async fn` that wraps the call in `tauri::async_runtime::spawn_blocking`, from the first draft.

> History: `run_sync` was temporarily changed to a sync `fn` as a workaround, introducing a different UI freeze. Reverted in v1.1.1 with proper `spawn_blocking`.

### CSP

Never leave `"csp": null` in `tauri.conf.json`. Minimum safe policy:

```json
"csp": "default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self'"
```

### Serde struct fields and old JSON

Adding a field to a Rust struct deserialized from persisted JSON (e.g. `projects.json`) will **silently drop** records missing the new key - unless the field is annotated `#[serde(default)]`. Always add `#[serde(default)]` to new optional fields on persistent structs.

### `#[cfg(target_os = "macos")]` variable scoping

Variables declared **outside** a `#[cfg(target_os = "macos")]` block but only used inside it produce unused-variable warnings on Linux/Windows builds. Declare them **inside** the cfg block.

## Project conventions

- Follow **SRP, SOLID, DRY**.
- Shared engineering rules: akidevrule (requirement 1); project facts and limits: [`CLAUDE.md`](CLAUDE.md).
- Documentation lives under `docs/` - start at [`docs/index.md`](docs/index.md).
