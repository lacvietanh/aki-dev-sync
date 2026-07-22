# CLAUDE.md

## Aki Rules

Shared rules live at `~/.aki/claudedoc/`. Read `~/.aki/claudedoc/index.md` for the full rule index and loading policy.
Claude Code loads these automatically via the `akirule` skill. Gemini reads them directly from that path.

---

## GLOBAL TAURI STACK

Generic Tauri v2 + Rust lessons, not specific to this project - copy this whole section verbatim
into any new Tauri project's `CLAUDE.md`.

- **Titlebar sacred boundary**: `"decorations": false` + `"transparent": true` → no native titlebar. All `position: fixed/absolute` elements **must** start at `top: var(--titlebar-h)` (or your app's titlebar height), never `top: 0`. Window controls (drag/minimize/close) via JS `@tauri-apps/api/window`.
- **Version SSOT**: `package.json` only. `tauri.conf.json` → `"version": "../package.json"`. Never hardcode version in `tauri.conf.json`. `Cargo.toml` has its own crate version (separate concern) and **must always be bumped to the same number in the same commit** - a mismatch between `package.json` and `Cargo.toml` is the same class of bug as a bad tag.
- **Version string format (ABSOLUTE - never violate)**: The version *attribute itself* - `package.json`'s `"version"`, `Cargo.toml`'s `version`, and every git tag - is **always bare semver, never prefixed with `v`** (`1.10.1`, not `v1.10.1`). This is not cosmetic: a tag/field is data other tooling parses and compares (`hasUpdate()`-style semver comparisons, release scripts, `git describe`) - an inconsistent prefix across tags silently breaks those comparisons or produces doubled-up displays (e.g. a UI that does `` `v${version}` `` against a value that already contains `v` renders `vv1.10.1`). Concretely:
  - `git tag 1.10.1` - **never** `git tag v1.10.1`. Check `git tag -l | sort -V | tail -5` before cutting a release to confirm the existing convention wasn't drifted from (this project's history briefly, incorrectly, switched to `v`-prefixed tags between `v1.9.8` and `v1.10.0` - treat those two as the one-off exception being corrected, not the precedent to follow).
  - Any UI that shows a friendly version to the user (e.g. "Update Available - v1.10.1") **may** prepend `v` at render time - that's a display concern and is unrelated to and does not violate this rule. The forbidden thing is a `v` baked into the stored/compared value itself.
- **IPC capability silent fail**: Every Tauri command AND window API call must be granted in `src-tauri/capabilities/default.json`. Missing → **silent no-op**, no error, no log. Window needs: `core:window:allow-minimize`, `core:window:allow-close`, `core:window:allow-start-dragging`.
- **NEVER BLOCK THE UI (ABSOLUTE - zero exceptions, no case-by-case judgment calls)**: This exact bug class has recurred repeatedly across this app's history (statusline-customizer auto-install freezing the whole window on modal-open; `check_for_updates`' blocking `curl` call running on every app launch) - each time because a `#[tauri::command]` ran a **blocking subprocess wait** (`Command::output()`, `Command::wait_with_output()`, an SSH/poll loop like `run_remote_script`) or a **blocking network call** synchronously on the thread that dispatches the IPC call. Tauri does not put plain `fn` commands on a separate thread for you - a slow subprocess or a dead network directly freezes window repaint and all input for however long that call takes, with no partial-progress warning to the user.
  - **The rule, no exceptions**: any `#[tauri::command]` whose body runs a subprocess (`Command::new(...).output()`/`.wait()`/`.wait_with_output()`, an SSH round-trip, a poll-and-sleep loop) or a blocking network call **must** be `async fn`, and the blocking call **must** be wrapped in `tauri::async_runtime::spawn_blocking(move || { … }).await.map_err(...)`. Never call the blocking function directly inside the `async fn` body even once "just this one time because it's quick" - network and remote-host calls have no fast-path guarantee (a bad connection is exactly the case that must not freeze the app).
  - **Before adding or reviewing ANY new `#[tauri::command]`**, ask: does this call a subprocess, SSH, or the network? If yes, it is not optional whether to wrap it - `spawn_blocking` goes in from the first draft, not as a follow-up fix. Grep `grep -n "#\[tauri::command\]" -A2 src-tauri/src/*.rs` before closing out any Tauri-touching task and check every new/changed command against this rule; `git.rs`'s `get_git_info`/`run_git_command`/`get_file_conflict_info` and `agent_usage.rs`'s `get_agent_usage`/`logout_antigravity` are the canonical examples of the correct pattern to copy.
  - Plain, fast, synchronous local file I/O (reading a small JSON/config file, a single `Path::exists()` check) is not this bug class and does not need `spawn_blocking` - the line is "does this call wait on a subprocess or the network," not "is this technically a syscall."
- **Serde fields + old JSON**: New fields on structs deserialized from persisted JSON need `#[serde(default)]` or old records silently drop.
- **`#[cfg(target_os = "macos")]` scoping**: Declare variables **inside** the cfg block. Declared outside but used only inside → unused-variable warning on non-macOS.
- **Subprocess PATH-resolution race at cold start (ABSOLUTE - apply to every spawned CLI binary)**:
  Any Rust code that spawns a shell to invoke a user-installed CLI (`Command::new("sh"/"zsh"/"bash")`,
  or over `ssh host sh`) and relies on `zsh -lc`/`bash -lc` login-shell PATH resolution to find that
  binary is racing the user's shell rc/profile (nvm, path_helper, zinit, etc.) - which may not have
  finished sourcing yet if the subprocess is spawned right at/near app cold-start. Symptom: intermittent
  `exit=127 command not found: <bin>` that self-heals within minutes and is NOT reproducible when
  testing the identical command manually a bit later - easy to misdiagnose as a CLI-version or
  auth problem instead of a timing race.
  **Fix pattern**: resolve the binary via static, well-known install-directory candidates FIRST
  (a `[ -x "$path" ]` file-existence test has zero dependency on rc-sourcing timing), falling back
  to `command -v` / login-shell PATH lookup only if none match - do this in ONE shared preamble
  injected at the single funnel where scripts are dispatched (e.g. wherever `Command::new(...).stdin`
  is written to), not patched ad hoc at each call site. NOTE: seed the candidate path list for
  **macOS only** first (e.g. `~/.local/bin`, `~/.claude/local`, `/opt/homebrew/bin`, `/usr/local/bin`
  for Claude Code specifically) if the app currently ships Mac-only - extend the list when a
  Linux/Windows build ships rather than guessing those paths upfront.

---

## THIS PROJECT

**Stack**: Vue 3 (Vite), Tauri v2, Rust. Dark mode desktop tool for rsync-based deploy workflows.

### UI Principle - Extreme Narrow (ABSOLUTE, Never Violate)

This app optimizes space with extreme aggression. Every pixel counts.

- **Never add extra rows, banners, or labels** to communicate state - use existing elements (button color, outline, badge overlay, tooltip).
- **Count badges must be `position: absolute` overlays** on the button - never inline text that widens the button or adds a new element in the flow.
- **No decorative separators, dividers, or status bars** beyond what already exists.
- **When in doubt: less is more.** If a state change can be communicated via color/outline/tooltip alone, do not add a new DOM element.

### Regression Guard - Multi-entity State (ABSOLUTE, Never Violate)

Root cause of the 1.9.3 regression that deleted the entire Antigravity multi-account history on
every logout: a "fix" called a `clear*Store()` that wiped the WHOLE store when only one pointer
field needed resetting. Rule, to make this class of bug structurally harder to reintroduce:

- Any store that holds a **list/map of entities** (accounts, projects, hosts, caches keyed by id)  - 
  a function that clears/resets/wipes it must be scoped to the ONE entity the bug is actually about.
  Wiping the whole store is only correct when the user explicitly asked for "clear everything."
- **Name the function by its actual scope.** `clearLastActiveAg()` (clears one pointer) must never
  be named `clearAgStore()` (implies clearing everything) - a vague name is what let the wrong
  blast radius pass review last time, including AI review.
- Before shipping any change that touches a multi-entity store, manually verify with **≥2 entities
  present**: perform the action the fix targets, then confirm every *other* entity's data survived
  untouched. Testing only the single active/happy-path entity is not enough - that's exactly the
  gap that shipped 1.9.3's bug.
- CHANGELOG entries for such fixes must state explicitly what was preserved, not just what was
  fixed (e.g. "the per-account map survives a logout intact") - this is a claim a future audit can
  check against the diff.

### This-project Tauri specifics

- **Titlebar height**: 42px (`var(--titlebar-h)`). Ref: `docs/ref/titlebar-sacred-boundary.md`
- **Post-build rename**: `npm run build:app` (= `tauri build` + `scripts/post-build.js`), not raw `tauri build`. Output: `Aki-DevSync-vX.X.X-arch.dmg`.
- **Release build & GitHub release command**: when a release is requested on Mac, default to `npm run build:rmud` (universal dmg), then attach DMG to GitHub release via `cat << 'EOF' | gh release create X.Y.Z <dmg_path> --title "X.Y.Z" --notes-file -`.
- **async fn + blocking subprocess history**: `run_sync` v1.1.1 hit the UI-freeze pitfall (see GLOBAL TAURI STACK) - already fixed, kept here as the concrete precedent.

### Runtime log location

- Usage/debug log: `~/Library/Application Support/aki.devsync/usage.log` (macOS) / `~/.local/share/aki.devsync/usage.log` (Linux). Ref: `docs/arch/logger.md`, `docs/arch/usage-claudecode.md`.
