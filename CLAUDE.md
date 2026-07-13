# CLAUDE.md

## Aki Rules

Shared rules live at `~/.aki/claudedoc/`. Read `~/.aki/claudedoc/index.md` for the full rule index and loading policy.
Claude Code loads these automatically via the `akirule` skill. Gemini reads them directly from that path.

---

## GLOBAL TAURI STACK

Generic Tauri v2 + Rust lessons, not specific to this project — copy this whole section verbatim
into any new Tauri project's `CLAUDE.md`.

- **Titlebar sacred boundary**: `"decorations": false` + `"transparent": true` → no native titlebar. All `position: fixed/absolute` elements **must** start at `top: var(--titlebar-h)` (or your app's titlebar height), never `top: 0`. Window controls (drag/minimize/close) via JS `@tauri-apps/api/window`.
- **Version SSOT**: `package.json` only. `tauri.conf.json` → `"version": "../package.json"`. Never hardcode version in `tauri.conf.json`. `Cargo.toml` has its own crate version (separate concern).
- **IPC capability silent fail**: Every Tauri command AND window API call must be granted in `src-tauri/capabilities/default.json`. Missing → **silent no-op**, no error, no log. Window needs: `core:window:allow-minimize`, `core:window:allow-close`, `core:window:allow-start-dragging`.
- **async fn + blocking subprocess**: Blocking subprocess directly in `async fn` starves the executor → UI freeze. Use `tauri::async_runtime::spawn_blocking`.
- **Serde fields + old JSON**: New fields on structs deserialized from persisted JSON need `#[serde(default)]` or old records silently drop.
- **`#[cfg(target_os = "macos")]` scoping**: Declare variables **inside** the cfg block. Declared outside but used only inside → unused-variable warning on non-macOS.
- **Subprocess PATH-resolution race at cold start (ABSOLUTE — apply to every spawned CLI binary)**:
  Any Rust code that spawns a shell to invoke a user-installed CLI (`Command::new("sh"/"zsh"/"bash")`,
  or over `ssh host sh`) and relies on `zsh -lc`/`bash -lc` login-shell PATH resolution to find that
  binary is racing the user's shell rc/profile (nvm, path_helper, zinit, etc.) — which may not have
  finished sourcing yet if the subprocess is spawned right at/near app cold-start. Symptom: intermittent
  `exit=127 command not found: <bin>` that self-heals within minutes and is NOT reproducible when
  testing the identical command manually a bit later — easy to misdiagnose as a CLI-version or
  auth problem instead of a timing race.
  **Fix pattern**: resolve the binary via static, well-known install-directory candidates FIRST
  (a `[ -x "$path" ]` file-existence test has zero dependency on rc-sourcing timing), falling back
  to `command -v` / login-shell PATH lookup only if none match — do this in ONE shared preamble
  injected at the single funnel where scripts are dispatched (e.g. wherever `Command::new(...).stdin`
  is written to), not patched ad hoc at each call site. NOTE: seed the candidate path list for
  **macOS only** first (e.g. `~/.local/bin`, `~/.claude/local`, `/opt/homebrew/bin`, `/usr/local/bin`
  for Claude Code specifically) if the app currently ships Mac-only — extend the list when a
  Linux/Windows build ships rather than guessing those paths upfront.

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

### This-project Tauri specifics

- **Titlebar height**: 42px (`var(--titlebar-h)`). Ref: `docs/ref/titlebar-sacred-boundary.md`
- **Post-build rename**: `npm run build:app` (= `tauri build` + `scripts/post-build.js`), not raw `tauri build`. Output: `Aki-DevSync-vX.X.X-arch.dmg`.
- **async fn + blocking subprocess history**: `run_sync` v1.1.1 hit the UI-freeze pitfall (see GLOBAL TAURI STACK) — already fixed, kept here as the concrete precedent.

### Runtime log location

- Usage/debug log: `~/Library/Application Support/aki.devsync/usage.log` (macOS) / `~/.local/share/aki.devsync/usage.log` (Linux). Ref: `docs/arch/logger.md`, `docs/arch/usage-claudecode.md`.
