# CLAUDE.md

## Aki Rules

Shared rules live at `~/.aki/claudedoc/`. Read `~/.aki/claudedoc/index.md` for the full rule index and loading policy.
Claude Code loads these automatically via the `akirule` skill. Gemini reads them directly from that path.

SRP, SOLID, DRY

---

## THIS PROJECT

**Stack**: Vue 3 (Vite), Tauri v2, Rust. Dark mode desktop tool for rsync-based deploy workflows.

### UI Principle — Extreme Narrow (ABSOLUTE, Never Violate)

This app optimizes space with extreme aggression. Every pixel counts.

- **Never add extra rows, banners, or labels** to communicate state — use existing elements (button color, outline, badge overlay, tooltip).
- **Count badges must be `position: absolute` overlays** on the button — never inline text that widens the button or adds a new element in the flow.
- **No decorative separators, dividers, or status bars** beyond what already exists.
- **When in doubt: less is more.** If a state change can be communicated via color/outline/tooltip alone, do not add a new DOM element.

### Regression Guard — Multi-entity State (ABSOLUTE, Never Violate)

Root cause of the 1.9.3 regression that deleted the entire Antigravity multi-account history on
every logout: a "fix" called a `clear*Store()` that wiped the WHOLE store when only one pointer
field needed resetting. Rule, to make this class of bug structurally harder to reintroduce:

- Any store that holds a **list/map of entities** (accounts, projects, hosts, caches keyed by id) —
  a function that clears/resets/wipes it must be scoped to the ONE entity the bug is actually about.
  Wiping the whole store is only correct when the user explicitly asked for "clear everything."
- **Name the function by its actual scope.** `clearLastActiveAg()` (clears one pointer) must never
  be named `clearAgStore()` (implies clearing everything) — a vague name is what let the wrong
  blast radius pass review last time, including AI review.
- Before shipping any change that touches a multi-entity store, manually verify with **≥2 entities
  present**: perform the action the fix targets, then confirm every *other* entity's data survived
  untouched. Testing only the single active/happy-path entity is not enough — that's exactly the
  gap that shipped 1.9.3's bug.
- CHANGELOG entries for such fixes must state explicitly what was preserved, not just what was
  fixed (e.g. "the per-account map survives a logout intact") — this is a claim a future audit can
  check against the diff.

### Tauri Pitfalls — Critical, Always Apply

- **Titlebar sacred boundary**: `"decorations": false` + `"transparent": true` → no native titlebar. All `position: fixed/absolute` elements **must** start at `top: var(--titlebar-h)` (42px), never `top: 0`. Window controls (drag/minimize/close) via JS `@tauri-apps/api/window`. Ref: `docs/ref/titlebar-sacred-boundary.md`
- **Version SSOT**: `package.json` only. `tauri.conf.json` → `"version": "../package.json"`. Never hardcode version in `tauri.conf.json`. `Cargo.toml` has its own crate version (separate concern).
- **Post-build rename**: `npm run build:app` (= `tauri build` + `scripts/post-build.js`), not raw `tauri build`. Output: `Aki-DevSync-vX.X.X-arch.dmg`.
- **IPC capability silent fail**: Every Tauri command AND window API call must be granted in `src-tauri/capabilities/default.json`. Missing → **silent no-op**, no error, no log. Window needs: `core:window:allow-minimize`, `core:window:allow-close`, `core:window:allow-start-dragging`.
- **async fn + blocking subprocess**: Blocking subprocess directly in `async fn` starves the executor → UI freeze. Use `tauri::async_runtime::spawn_blocking`. (History: `run_sync` v1.1.1.)
- **Serde fields + old JSON**: New fields on structs deserialized from persisted JSON (e.g. `projects.json`) need `#[serde(default)]` or old records silently drop.
- **`#[cfg(target_os = "macos")]` scoping**: Declare variables **inside** the cfg block. Declared outside but used only inside → unused-variable warning on non-macOS.
