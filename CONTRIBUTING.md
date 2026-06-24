# Contributing to Aki Dev Sync

This is a Vue 3 (Vite) + Tauri v2 + Rust desktop app. This guide covers local setup, build conventions, and the Tauri gotchas worth knowing before you touch the code.

## Prerequisites

### macOS

Install the Xcode Command Line Tools and Rust:

```bash
xcode-select --install
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

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

> `build-essential`, `libssl-dev`, and `pkg-config` are usually already present on a dev machine — kept here for fresh installs.

## Run & build

```bash
npm install
npm run tauri dev    # dev (first run compiles Rust, ~5–10 min)
npm run build:app    # production build + post-build artifact rename
```

Use `npm run build:app`, **not** raw `tauri build` — see "Post-build artifact rename" below.

## Tauri gotchas & conventions

Lessons from building a macOS-first Tauri v2 app. Recorded to avoid re-discovery.

### Titlebar sacred boundary

`"decorations": false` + `"transparent": true` removes the native titlebar entirely. Every fixed or absolute-positioned element that spans the full window **must** start at `top: var(--titlebar-h)` (42px), never `top: 0`. Covering the drag region makes the window un-movable.

→ Full rule + rationale: [`docs/ref/titlebar-sacred-boundary.md`](docs/ref/titlebar-sacred-boundary.md)

### Version SSOT — `package.json` only

`tauri.conf.json` sets `"version": "../package.json"`. Do **not** hardcode a version there or sync `Cargo.toml`'s version to track the app release — they are separate concerns. Bump only `package.json`.

### Post-build artifact rename

Raw `npm run tauri build` outputs filenames with spaces (e.g. `Aki Dev Sync_1.2.0_aarch64.dmg`). Use `npm run build:app` instead — it chains `tauri build` and `node scripts/post-build.js` to produce `Aki-DevSync-v1.2.0-arm.dmg` (or `-universal.dmg`).

### IPC capability: silent failures

Every Tauri command — including `@tauri-apps/api/window` calls — must be **granted** in `src-tauri/capabilities/default.json`. A missing entry causes a **silent no-op**: the JS call resolves without error and nothing happens. This was the root cause of window drag/minimize/close not responding (fixed by adding `core:window:allow-start-dragging`, `core:window:allow-minimize`, `core:window:allow-close`).

### async IPC + blocking subprocess

Tauri runs `async fn` commands on an async executor. Calling `std::process::Command` (blocking) directly inside an `async fn` starves the thread pool — the UI appears frozen until the command returns. Use `tauri::async_runtime::spawn_blocking` for any blocking work inside an `async fn` command.

> History: `run_sync` was temporarily changed to a sync `fn` as a workaround, introducing a different UI freeze. Reverted in v1.1.1 with proper `spawn_blocking`.

### CSP

Never leave `"csp": null` in `tauri.conf.json`. Minimum safe policy:

```json
"csp": "default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self'"
```

### Serde struct fields and old JSON

Adding a field to a Rust struct deserialized from persisted JSON (e.g. `projects.json`) will **silently drop** records missing the new key — unless the field is annotated `#[serde(default)]`. Always add `#[serde(default)]` to new optional fields on persistent structs.

### `#[cfg(target_os = "macos")]` variable scoping

Variables declared **outside** a `#[cfg(target_os = "macos")]` block but only used inside it produce unused-variable warnings on Linux/Windows builds. Declare them **inside** the cfg block.

## Project conventions

- Follow **SRP, SOLID, DRY**.
- Shared engineering rules live under `~/.aki/claudedoc/` (see [`CLAUDE.md`](CLAUDE.md)).
- Documentation lives under `docs/` — start at [`docs/index.md`](docs/index.md).
