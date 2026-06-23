# CLAUDE.md

## Aki Rules

Shared rules live at `~/.aki/claudedoc/`. Read `~/.aki/claudedoc/index.md` for the full rule index and loading policy.
Claude Code loads these automatically via the `akirule` skill. Gemini reads them directly from that path.

SRP, SOLID, DRY

---

## THIS PROJECT

**Stack**: Vue 3 (Vite), Tauri v2, Rust. Dark mode desktop tool for rsync-based deploy workflows.

### Tauri Pitfalls — Critical, Always Apply

- **Titlebar sacred boundary**: `"decorations": false` + `"transparent": true` → no native titlebar. All `position: fixed/absolute` elements **must** start at `top: var(--titlebar-h)` (42px), never `top: 0`. Window controls (drag/minimize/close) via JS `@tauri-apps/api/window`. Ref: `docs/ref/titlebar-sacred-boundary.md`
- **Version SSOT**: `package.json` only. `tauri.conf.json` → `"version": "../package.json"`. Never hardcode version in `tauri.conf.json`. `Cargo.toml` has its own crate version (separate concern).
- **Post-build rename**: `npm run build:app` (= `tauri build` + `scripts/rename-artifacts.js`), not raw `tauri build`. Output: `Aki-DevSync-vX.X.X-arch.dmg`.
- **IPC capability silent fail**: Every Tauri command AND window API call must be granted in `src-tauri/capabilities/default.json`. Missing → **silent no-op**, no error, no log. Window needs: `core:window:allow-minimize`, `core:window:allow-close`, `core:window:allow-start-dragging`.
- **async fn + blocking subprocess**: Blocking subprocess directly in `async fn` starves the executor → UI freeze. Use `tauri::async_runtime::spawn_blocking`. (History: `run_sync` v1.1.1.)
- **Serde fields + old JSON**: New fields on structs deserialized from persisted JSON (e.g. `projects.json`) need `#[serde(default)]` or old records silently drop.
- **`#[cfg(target_os = "macos")]` scoping**: Declare variables **inside** the cfg block. Declared outside but used only inside → unused-variable warning on non-macOS.
