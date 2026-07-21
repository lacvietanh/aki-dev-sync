# Plan - Narrow mode + UX batch (target 1.14.0)

Status: done (shipped in 1.14.0)
Owner: parallel subagents (A/B/C/D), coordinated by the main agent.

## Scope

Ten user-requested changes, grouped by **file ownership** so four agents can run in parallel with
zero file overlap. No agent may touch a file it does not own - if a change seems to need one,
report it back instead of editing.

| Agent | Owns (exclusive) |
|---|---|
| A | `src-tauri/src/system.rs`, `src-tauri/capabilities/default.json`, `src/composables/useSync.js`, `src/components/modals/ProjectTasksModal.vue` |
| B | `src/components/AgentUsageSection.vue`, `src/components/AgentUsageSlot.vue`, `src/components/AgentUsage.vue` |
| C | `src/components/ProjectTable.vue` |
| D | `src/components/AppHeader.vue`, `src/components/AppConsole.vue`, `src/assets/main.css` |

## Shared contract - the narrow breakpoint (READ FIRST, all agents)

There is exactly **one** narrow breakpoint for this whole feature:

```css
@media (max-width: 700px) { … }
```

Agent D adds these **global** helper classes to `src/assets/main.css` (unscoped, so any component
can apply them regardless of scoped styles). Agents B/C/D apply them by name; do not redefine them
locally, and do not invent a second breakpoint value.

```css
/* Narrow mode (<=700px): hide the text half of an icon+label pair, keep the icon. */
@media (max-width: 700px) {
  .u-narrow-hide { display: none !important; }
}
```

Rules (RULE-ui-pattern A1/A3):
- Prefer marking the *label span* with `.u-narrow-hide` over writing a new media query per component.
- A component-scoped `@media (max-width: 700px)` block is allowed only for **layout** changes
  (column widths, gaps, font-size) that a utility class cannot express. Use the exact same 700px value.
- Any element that currently shows a bare text label with no icon and must survive narrow mode needs
  an icon added first - never leave a control unlabelled and iconless.
- Window `minWidth` in `src-tauri/tauri.conf.json` stays as-is (600) - it is already below 700, so
  narrow mode is reachable. Do not change it. (Verified: current value is 600.)

---

## Agent A - backend commands + sync dialogs + task row order

### A1. REPORT.html must stop prompting (user item 3)

`REPORT.html` is an artifact of this app's own flow (produced by the `akihtmlreport` skill, opened
via the OPEN popup's REPORT button). It should never make the user answer a scary dialog about
routine syncing. Both dialogs in `src/composables/useSync.js` are in scope.

Introduce one shared constant at module top (SSoT - do not inline the string twice):

```js
// Artifacts this app itself produces/manages - routine sync churn on them is expected and must
// not raise a confirm dialog. See docs/plan/done/narrow-mode-and-ux-1.14.0.md §A1.
const FLOW_APP_ARTIFACTS = ['REPORT.html']
```

**(a) `--delete` confirm (`startSync`, currently ~line 102).**
Filter `FLOW_APP_ARTIFACTS` out of `deleteList` before the `deleteList.length > 0` check, using the
same shape as the existing R3 push-only auto-approve just above it (reuse that pattern - log the
auto-approval with `appendLog`, do not silently drop). Applies to **both** directions, not just
push: the source side simply does not keep the report, so its deletion at the destination is
expected churn, and there is no mtime to compare against.

**(b) SELECT-push overwrite confirm (`openSelectDialog`, currently ~line 260).**
A conflict row for a flow-app artifact only deserves a prompt when the **destination copy is newer
than the source** - i.e. pushing would clobber a report that was regenerated on the remote. Filter
out rows where `FLOW_APP_ARTIFACTS.includes(basename(f.rel_path))` **and** `f.remote_mtime <=
f.local_mtime`. Everything else keeps prompting exactly as today.

Check `get_file_conflict_info`'s returned struct in `src-tauri/src/git.rs` first - confirm the raw
numeric `local_mtime`/`remote_mtime` fields are actually serialized to the frontend (the formatted
`*_mtime_fmt` fields are the ones the table renders). If only the formatted strings cross the IPC
boundary, add the numeric fields to the serialized struct rather than parsing the formatted string.

### A2. DEV button opens the browser for web projects (user item 4)

Today `runProjectCommand` (ProjectTable.vue) → `run_project_command` (system.rs:546) just opens
Terminal with `cd <path> && <cmd>`. Add the browser-open half **in Rust**, as a new command; Agent C
only changes which command the DEV button invokes.

**New command signature (Agent C codes against exactly this):**

```rust
#[tauri::command]
pub async fn run_project_dev(local_path: String, cmd: String) -> Result<(), String>
```

Behaviour:
1. Start the dev command in Terminal exactly as `run_project_command` does today (reuse
   `open_terminal_with_command`, do not duplicate its logic - extract a private helper if needed).
2. Resolve a dev URL for the project (below). If none resolves, stop here - Terminal-only, same as
   today, no error toast.
3. Poll `TcpStream::connect_timeout("127.0.0.1:<port>")` every 500ms for up to 60s. On first
   success, open `http://localhost:<port>` with `open -g <url>`.
   - `-g` opens **in the background without activating the browser**, which is what keeps macOS
     from switching Spaces/workspaces (user's explicit requirement). Do not drop this flag.
   - Timeout with no connection → return `Ok(())` silently. A dev server that never came up is not
     an app error and must not raise a toast.

**ABSOLUTE (RULE-stack-tauri A1):** this command waits on a subprocess *and* does a 60-second poll
loop. It **must** be `async fn` with the whole blocking body inside
`tauri::async_runtime::spawn_blocking(move || { … }).await.map_err(|e| format!("spawn_blocking panicked: {}", e))?`.
A poll-and-sleep loop on the IPC thread would freeze the window for a full minute. This is the exact
bug class that has already shipped twice in this app - do not repeat it.

**Port resolution - in this order, first hit wins** (verified against `/home/guest/aki/web/*`):

1. `package.json` → `scripts.dev`: scan for an explicit `--port <n>` or `-p <n>` flag.
   (Real scripts chain other commands first, e.g. `"npm run killport && nuxt dev"` - match the flag
   anywhere in the string, not only at the end.)
2. `nuxt.config.ts` / `nuxt.config.js` → `devServer: { port: <n> }`. This is how every real project
   in `/home/guest/aki/web/*` pins its port (3000-3006). A tolerant regex over the file text is
   correct here - do not add a JS/TS parser dependency.
3. `vite.config.{ts,js,mjs}` → `server: { port: <n> }`.
4. Framework default: Nuxt (nuxt.config present) → `3000`; Vite (vite.config present) → `5173`.

**Only web projects get a browser.** Resolve a URL only when the project is Nuxt or Vite **and not
Tauri** - `check_project_stack` (system.rs:498) already computes `is_nuxt`/`is_tauri`; extend it if a
`is_vite` signal is needed rather than re-statting files in a second place (SSoT). A Tauri project's
`dev` command opens its own native window; opening a browser at 1420 would be wrong.

Register the new command in `src-tauri/src/lib.rs`'s `invoke_handler`. Tauri commands need no
capability entry (only plugin/window APIs do) - but re-read `capabilities/default.json` and confirm
nothing else is needed before closing out (RULE-stack-tauri B2: a missing grant fails **silently**).

Keep `run_project_command` as-is: BUILD still uses it.

### A3. Task row - Mark Done moves after the title (user item 2)

`src/components/modals/ProjectTasksModal.vue`. Today `.task-states-left` is `[Done | Pin | Wish]`, so
Done leads the row. Move the **Mark Done** button out of `.task-states-left` to sit immediately
**before** `.task-copy-btn` (~line 136), leaving Pin first in the row. Move its CSS with it; adjust
`.task-states-left` spacing so the two remaining buttons don't leave a gap. Behaviour, title
attributes, and the `@keyup.enter` shortcut are unchanged - this is purely positional.

---

## Agent B - Usage area: fixed height, scroll, narrow labels

### B1. Fixed usage viewport (user item 1)

The usage area currently grows/shrinks with content, so the whole UI jumps when a source loads,
errors, or switches account. Freeze it.

- Add a token in the section's own scope: the measured target is **161px from the bottom edge of the
  titlebar to the bottom of the usage area**. Since `.agent-usage-section` starts directly below the
  42px titlebar, that means `height: 161px` on `.agent-usage-section`. Re-derive this against the
  real content (`AgentUsage` card at its tallest normal state) and adjust to the nearest sensible
  value if 161 visibly clips a normal (non-error) state - state the final number in the CHANGELOG.
- Apply `height` (not `min-height`) + `overflow-y: auto` + `overflow-x: hidden` on
  `.agent-usage-section`, and `box-sizing: border-box` so the existing `padding: 6px 12px` counts
  inside the fixed height.
- Scrollbar must be nearly invisible: the global `::-webkit-scrollbar` rule in `main.css` is already
  6px - narrow it further **for this element only** via a scoped
  `.agent-usage-section::-webkit-scrollbar { width: 4px; }` plus a low-contrast thumb. Do not change
  the global scrollbar rule (Agent D owns main.css).
- Do not add any wrapper element or visible border (project rule: Extreme Narrow - no new DOM
  elements to communicate state).

### B2. Narrow mode - hide usage labels (user item 5, first two bullets)

At `<=700px`:
- **Agent name text** (`AgentUsage.vue`, `.agent-name` - renders "Claude Code" / "Antigravity" in
  both the claudecode header and the antigravity header): hide the text, keep the icon
  (`.agent-icon-wrapper` img). Apply the shared `.u-narrow-hide` class to the text node - wrap the
  `{{ agentName }}` interpolation in its own `<span>` where it is currently bare (antigravity branch,
  ~line 38) so the plan badge next to it survives. Do **not** hide the plan badge or the account
  email row; only the agent-name words go.
- **Tab labels** (`AgentUsageSlot.vue`): the `LOCAL` / `REMOTE` words in `.tab-group` (~lines 6-17)
  and the `AG` / `CC` label span (~line 39). Each already has an icon (`fa-laptop-code`, `fa-cloud`,
  and the per-source `<img class="src-icon">`), so hiding the text leaves a complete control. Wrap
  the bare `LOCAL`/`REMOTE` text in `<span class="u-narrow-hide">` and add the class to the existing
  `<span>{{ src.label }}</span>`.
- Tighten `.tab` horizontal padding at the breakpoint so the icon-only tabs don't keep the old
  label-sized footprint.

Tooltips (`title`) must keep the full words in every case - icon-only is the display, not the
information.

---

## Agent C - ProjectTable.vue (sole owner)

### C1. DEV button wires to the new command (user item 4, frontend half)

`runProjectCommand(p.local_path, getDevCmd(p))` currently drives both DEV and BUILD. Split:
- DEV (~line 174) → `invoke('run_project_dev', { localPath, cmd })`.
- BUILD (~line 177) → unchanged `run_project_command`.

Extract the shared try/catch/Toast wrapper so the two call sites differ only by command name (do not
copy-paste the handler). Keep the existing success toast wording for BUILD; for DEV the toast should
say the dev server started and the browser will open when it is ready. Errors keep the existing
`Toast.fire({ icon: 'error', … })` path.

The browser-open decision (web vs not, which port, when) lives entirely in Rust - the frontend must
not duplicate any stack detection.

### C2. OPEN popup opens centered (user item "chỉnh chung", first bullet)

`onOpenEnter` (~line 299) currently pins the popup's `left` to the trigger's `left`, so a wide popup
gets cropped by the right edge of the window. Change to horizontally center the popup on the window
(or clamp to the viewport with a small margin) while keeping the existing `bottom` anchoring above
the button. Set `transformOrigin: 'bottom center'` to match, so the scale-in animation still reads as
growing out of the trigger. The popup is `position: fixed`, so viewport coordinates are the right
frame of reference.

### C3. Narrow mode - table (user item 5, bullets 4-7)

All at `<=700px`, inside a single scoped `@media (max-width: 700px)` block:
- **Project name column** `--grid-cols` first track: `12rem` → about **40% of that** (~`4.8rem`).
  Text truncates with the existing ellipsis; the project icon stays fully visible. Verify the icon
  (28px handle) plus its 12px gap still fits.
- **Column gap** `--grid-gap` (`0.5rem`) reduces, and the gap between the icon and the name block
  (the inline `gap: 12px` at ~line 72) tightens too - move that inline value into a class first so it
  is reachable from CSS (RULE-ui-pattern: no styling logic stranded in inline attributes).
- **PUSH / PULL labels**: the `<span class="btn-text">PUSH</span>` / `PULL` (~lines 225, 249) get
  `.u-narrow-hide`; the cloud-arrow icons already carry the meaning. Note the existing
  `@media (max-width: 800px)` block at the bottom of the file already hides `.btn-text` for OPEN and
  the secondary buttons - fold the new rules into a coherent set rather than stacking a contradicting
  second breakpoint, and keep 800px behaviour for the buttons it already covers.
- **Header cell**: `PROJECTS (n)` + `NEW` button → `PJ (n)` + a `+`-only button. Hide the word `NEW`
  with `.u-narrow-hide`, and render the heading text so only "PJ" shows at narrow (e.g. a
  `<span class="u-narrow-hide">PROJECTS</span><span class="u-wide-hide">PJ</span>` pair, or the same
  effect via one element and a CSS-driven swap - pick one and keep it consistent). If a `.u-wide-hide`
  counterpart is needed, tell Agent D so it lands in `main.css` next to `.u-narrow-hide`; do not
  define it locally.

Count badges stay `position: absolute` overlays (project rule - never inline, never widening).

---

## Agent D - header, console, global CSS

### D1. Global narrow utilities in `main.css`

Add the `@media (max-width: 700px) { .u-narrow-hide { display: none !important } }` block described
in the shared contract, plus `.u-wide-hide` (hidden **above** 700px) if Agent C reports needing it.
Put them next to the existing spacing/text utilities, with a one-line comment naming the breakpoint
as the single narrow-mode value for the app. This is the SSoT for narrow mode - no other file
defines a different breakpoint.

### D2. Version/build moves into the dropdown menu (user item "chỉnh chung", second bullet)

`AppHeader.vue`. Today `{{ appVersion }} {{ buildTime }}` renders in the titlebar
(`.app-version` / `.build-time`, ~line 51-58) and clicking it opens the Changelog modal.

- Add a **first item** to `.icon-dropdown` (above "GitHub Repository", followed by an
  `.icon-dropdown-separator`) that displays the version and build time with the **exact same format
  as the titlebar shows today** - `{{ appVersion }} {{ buildTime }}`, same separator, same casing.
- On hover, the item's text swaps to **"Read Changelog"** (CSS-only swap is fine: two spans, one
  shown on hover, one hidden - keep both in the DOM so the item width does not jump).
- The item is clickable and opens the Changelog modal - the same `showChangelogModal = true` action
  the titlebar version currently triggers. Do not duplicate the handler; both entry points call the
  same thing.
- The "Update available" badge (`.update-badge`) stays where it is in the titlebar - it is a
  notification, not a version display. Only the version/build text is affected by D3 below.

### D3. Narrow mode - titlebar (user item 5, bullet 3)

At `<=700px`:
- Hide the titlebar version/build text (`.app-version`, or just `.build-time` + version - hide the
  whole `.app-version` span **except** keep the update badge visible if a new version is available,
  since that is the only signal the user has). With D2 in place the version is still reachable from
  the dropdown, so nothing is lost.
- Shrink `.logo-section h1` font-size a step (13px → ~11px) so "Aki Dev Sync" still fits.
- The existing `@media (max-width: 850px)` block in `AppHeader.vue` (hides `.btn-text` in
  `.header-actions`) stays - it is a wider, complementary breakpoint, not a conflict. Leave it.

Titlebar height stays exactly `var(--titlebar-h)` = 42px (project rule: sacred boundary).

### D4. Narrow mode - Global Event Log actions (user item 5, last bullet)

`AppConsole.vue`, the three `.terminal-actions` buttons (~lines 19-28). At `<=700px` hide the words
`COLLAPSE`/`EXPAND`, `COPY`/`COPIED`, `CLEAR`, keeping the chevron / copy / trash icons.

- The COPY button's label currently *is* the feedback (`COPIED`). With the text gone, the copied
  state must still be visible - swap the icon to `fa-check` (or tint it green) while `copied` is
  true, rather than adding any new element. Same for the expand/collapse chevron, which already
  flips direction.
- Wrap each bare label in `<span class="u-narrow-hide">`. The inline `style="padding: 4px 8px; …"`
  attributes on those buttons should move into a class in the same edit so the narrow rules can
  reach them.

### D5. Log panel default height ≈ 2 lines (user item "chỉnh chung", third bullet)

`main.css`, `.dashboard-bottom.is-collapsed { height: 120px }` (~line 113). The panel's header is
~29px and `.console-output` has `padding: 12px 16px` with `font-size: 12px; line-height: 1.5`
(= 18px per line), so 120px currently shows ~3.7 lines. For **2 lines**: `29 + 24 + 36 ≈ 89px` →
set it to **92px** (small buffer). Re-measure against the real header height before committing to the
number; the goal is "about two log lines visible", not the literal constant. The expanded height
(`40vh`) is unchanged.

---

## Definition of done (all agents)

- No agent edited a file outside its ownership row.
- Exactly one narrow breakpoint value (700px) exists across the whole change.
- Every hidden label still has an icon **and** a `title` carrying the full text.
- No new DOM row/banner/divider was added to communicate state (project Extreme Narrow rule).
- Any new/changed `#[tauri::command]` that touches a subprocess or the network is `async fn` +
  `spawn_blocking` (RULE-stack-tauri A1) - re-run
  `grep -n "#\[tauri::command\]" -A2 src-tauri/src/*.rs` and check every hit before reporting done.
- Reported back to the coordinator: what changed, any file you needed but did not own, and the final
  measured numbers (usage height, log height, narrow column widths).

## Release (coordinator, after all agents land)

Pre-bump state confirmed: `package.json` 1.13.1 == CHANGELOG top 1.13.1. New features, backward
compatible → **1.14.0**.

- Bump `package.json` **and** `src-tauri/Cargo.toml` to `1.14.0` in the same change (bare semver, no
  `v` prefix - RULE-release A3).
- New `### [1.14.0]` CHANGELOG section, English, Keep-a-Changelog sections.
- No `releases.json` in this project (Tauri, not web) - nothing to sync there.
- Output a copy-ready GitHub Release block (title `v1.14.0 - …`).
- Move this plan to `docs/plan/done/`.
- Do **not** commit, tag, or push - leave it in the working tree.
