# Audit - 1.11.0 → 1.15.0

Status: audit only. No source files were changed while producing this document.
Sources of truth used, in order: current source code (grep/read), `git log`/`git show`, then `CHANGELOG.md`.

## Scope

Reviewed every version from `1.11.0` (2026-07-17) to `1.15.0` (2026-07-21), five releases over five
days: `1.11.0`, `1.12.0`, `1.13.0`, `1.13.1` (same-day revert of `1.13.0`'s R2), `1.14.0`, `1.15.0`.
Cross-checked the CHANGELOG's own claims (removals, behavior changes) against the actual files named
in each entry - not just read the CHANGELOG prose.

## Breaking-change map

| Version | What changed | Verified against | Still references the old shape? |
|---|---|---|---|
| 1.11.0 | Window pinned across Spaces, proxy-mode locks native Claude monitor, project table narrowed, agent usage decluttered | not deep-dived (predates the flagged removals) | - |
| 1.12.0 | Usage monitor freeze self-heal, AG timeout fix, statusline hook v3, New Project button relocation | not deep-dived | - |
| 1.13.0 → 1.13.1 | Push-only paths: `sync_git` boolean replaced by exclude-list membership (`pull_excludes` minus `push_excludes`); `sync.rs`, `projects.rs`, `useProjectConfig.js` | `src-tauri/src/projects.rs:70` (`sync_git: Option<bool>` + `skip_serializing_if`), `src/composables/useProjectConfig.js:14-96` (`migratePushOnlyPaths`) | **No** - migration is idempotent and correctly implemented; `sync_git` is dead-but-harmless (explicitly documented for removal "once the migration has shipped for a full release cycle", `projects.rs:60-69`) |
| 1.14.0 | Narrow mode, window-size presets, OPEN popup centering | not deep-dived | - |
| 1.15.0 | (a) `aki-remote-mode-enabled` split into `syncCheckEnabled` + `aki-src-ccremote-enabled`; (b) one refresh controller (`useBackgroundRefresh.js`, `epoch`/`bumpEpoch`, per-check busy counters); (c) DEV button's auto-open-browser removed entirely (`run_project_dev`, `extract_port_flag`/`extract_port_field`/`resolve_dev_port` deleted) | `src/store/syncCheckStore.js:1-36`, `src/components/AgentUsageSection.vue:36-44` (both migrations correct - seed-once from legacy key, no double-consumption bug); `src-tauri/src/system.rs:582-593` (`run_project_dev` now just opens Terminal, matches CHANGELOG) | **Partially** - see Findings #1 and #2 below: the browser-open removal left a dead field in Rust, and a separate prior removal (the active Claude Code usage flow, `d900e66`, before this window but still live in the codebase) left dead frontend code and stale in-app copy that this window's docs never caught. |

The pre-1.11 removal of the entire active Claude Code usage flow (`d900e66`, "force-sync"/headless
probe) is outside the requested version window but its debris is still present today and is the
single highest-severity finding below, so it is included.

## Findings (ranked by severity)

### 1. HIGH - In-app help text and a fully orphaned component describe a feature that no longer exists

- `src/components/modals/IntroModal.vue:175-176` and `:209` still describe **"Force Sync Quota"** /
  **"Force Sync với Auto-Probe"**: user-facing copy explaining that clicking a refresh icon "tự động
  chạy Probe Session (Haiku ~100 tokens)" to refresh quota. This entire active-probe mechanism was
  deleted in `d900e66` ("remove(usage): delete the entire active Claude Code usage flow"). `docs/arch/usage-claudecode.md:147` correctly states *"App không tự chạy `claude` để lấy usage. Luồng active
  (force-sync/probe) đã bị xoá"* - so the architecture doc was fixed, but the in-app modal that users
  actually read was not.
- `src/components/UsageProgressBar.vue` is a **completely orphaned component** - `grep -rn
  "UsageProgressBar" src/` returns zero hits outside the file itself, confirming no parent imports or
  renders it. It still contains a live `force-sync` button (`:16`) and `defineEmits(['timeout',
  'force-sync'])` (`:44`) wired to nothing. Dead file, and if it were ever re-mounted by accident it
  would silently no-op on click (an emit with no listener), which is exactly the "silent no-op" bug
  class `RULE-stack-tauri` calls out for capability misses - same failure shape, different cause.

**Fix:** delete `UsageProgressBar.vue` (confirmed zero call sites). Rewrite `IntroModal.vue` lines
175-176 and 209 to describe the current statusLine-hook-only flow - mirror the language already
correct in `docs/arch/usage-claudecode.md`. (Per task scope, this doc records what the text should
say; the actual edit belongs to the agent already assigned to README/IntroModal.)

**RESOLVED (component half only - 2026-07-21):** Deleted `src/components/UsageProgressBar.vue`.
Verified before deletion: `grep -rn "UsageProgressBar" src/` and `grep -rn "force-sync\|forceSync"
src/` returned zero call sites outside the file itself (no import, no PascalCase/kebab-case template
tag, no listener bound to its `force-sync` emit). `npm run build` passes after removal. The
`IntroModal.vue` copy half of this finding is **out of scope for this pass** (assigned to a separate
agent per the note above, working on that file concurrently) and is left untouched - note the
in-app copy at `IntroModal.vue:209` observed during this pass already reads as corrected (references
the statusLine-hook-only flow, not "Force Sync"), so that half of the finding may already be resolved
by the other agent; not independently re-verified line-by-line here since it was out of scope.

### 2. MEDIUM-HIGH - Dead Rust field + falsified doc comment left by the DEV auto-open-browser removal

- `src-tauri/src/system.rs:501-505`: `is_vite_project()` carries the doc comment *"used to decide
  whether `run_project_dev` should try to open a browser tab for this project"* - but that behavior
  was removed in 1.15.0 (see the doc comment on `run_project_dev` itself, `system.rs:576-581`,
  which correctly explains the removal). The comment on `is_vite_project` was never updated to match.
- The field it feeds, `ProjectStackInfo.is_vite` (`system.rs:491`, set at `:515`, serialized at
  `:551`), has **zero frontend consumers**: `grep -rn "is_vite\|isVite" --include="*.vue"
  --include="*.js" src/` returns nothing. It is computed on every `check_project_stack` call and
  shipped over IPC to no one.

**Fix:** either delete `is_vite_project()` + the `is_vite` field entirely (it served exactly one
purpose, and that purpose is gone), or, if a future feature is planned to consume it, rewrite the
comment to state that plainly instead of describing a removed feature.

**RESOLVED (2026-07-21):** Deleted `is_vite_project()` (`system.rs`, including its stale doc
comment), the `is_vite` field on `ProjectStackInfo`, its assignment in `check_project_stack`, and its
value in the struct-literal return. Verified before removal: `grep -rn "is_vite" src-tauri/src/`
showed only the field declaration, the function, its call site, and the struct-literal use - all
now removed together; `grep -rn "is_vite\|isVite" --include="*.vue" --include="*.js" src/` returned
zero hits, confirming no frontend consumer of the serialized field. `cargo check` (src-tauri) passes
clean after removal - no new warnings, no unused-import fallout.

### 3. MEDIUM - Hardcoded color values sprawled across SFCs instead of CSS-variable tokens (RULE-ui-pattern A2/A3)

Quantified via the audit greps this project has no Tailwind, so the "token vs arbitrary value"
question collapses to "CSS var in `main.css` vs. hex/rgb inline in a `.vue` file":

```
grep -rnoE '#[0-9a-fA-F]{3,6}\b|rgb\([^)]+\)|rgba\([^)]+\)' --include="*.vue" src/ | wc -l
→ 416 occurrences, across 21 of 24 .vue files

grep -cE '^\s*--[a-z-]+:' src/assets/main.css
→ 14 CSS custom properties defined
```

Worst offenders by count: `ClaudeSettingModal.vue` (83), `IntroModal.vue` (48), `AppHeader.vue` (45),
`AgentUsage.vue` (40), `ProjectTable.vue` (34), `ClaudeProfileModal.vue` (31).

Most-repeated raw values - each one already blows past the Rule-of-Three threshold for token
extraction many times over, so this is not a scattering of one-offs, it is unextracted duplication:

```
18×  #e2e8f0
16×  rgba(255, 255, 255, 0.1)
16×  rgba(255, 255, 255, 0.08)
15×  #94a3b8
13×  rgba(255, 255, 255, 0.05)
10×  rgba(255, 255, 255, 0.07)
10×  #f87171
10×  #64748b
 9×  #22d3ee
 8×  rgba(255, 255, 255, 0.06)
```

**Fix:** add ~6-10 semantic tokens to `src/assets/main.css` (e.g. `--color-border-subtle: #e2e8f0`,
`--color-muted: #94a3b8`, `--overlay-white-10/08/05/07/06` for the repeated rgba whites) and sweep
call sites file-by-file, starting with `ClaudeSettingModal.vue`. This is real, quantified drift  - 
not a stylistic nitpick - but it long predates the 1.11-1.15 window; it is not something this
window's changes newly introduced, so it does not block anything, it is simply overdue.

### 4. MEDIUM - CHANGELOG entries have drifted into essay-length architecture narration (RULE-release B4/B5 content discipline)

`CHANGELOG.md`'s `[1.15.0]` section: 1084 words across 15 bullets (~72 words/bullet average); the
three longest bullets run 109, 142, and 158 words respectively, and read as retrospective design
postmortems - e.g. the `epoch`/cancellation bullet includes *"An earlier attempt put a `refreshing`
flag on a `refreshProject()` wrapper instead; the background timers didn't call that wrapper, which
is exactly why the per-project icons stayed inert..."* - narrative reasoning about a design iteration
that never shipped, not a description of what changed for `1.15.0` itself.

This is real information - it is a de facto architecture-decision log - but it is filed in the wrong
place per `RULE-docs` A1 (`docs/arch/`, `docs/plan/` exist precisely for this) and violates the
spirit of `RULE-release` B4 (GitHub Release bullets must be "one short sentence... no internal
jargon") since the CHANGELOG is the source B4 trims *from* - a CHANGELOG this dense makes that
trimming step lossy and effortful rather than mechanical. To the project's credit, this window
mostly does the right thing at the same time: `1.13.0`/`1.13.1` push the deep narrative into
`docs/plan/done/push-only-paths.md` §9 and `1.15.0` pushes its architecture into
`docs/arch/refresh-controller.md` - but the CHANGELOG bullets *themselves* still restate that full
narrative inline instead of a one-line pointer to the doc.

**Assessment:** genuinely a content-discipline miss, not merely stylistic - but low urgency (nothing
is broken, nothing is misleading). Recommend trimming future entries to symptom+fix+doc-pointer,
not retrofitting the last 5 versions.

### 5. LOW - Version SSOT and tag convention: clean in this window, legacy drift untouched (as instructed)

`package.json` (`"version": "1.15.0"`), `src-tauri/Cargo.toml` (`version = "1.15.0"`), and
`tauri.conf.json` (`"version": "../package.json"`) all agree. `git tag -l | sort -V | tail -3` shows
`1.13.1`, `1.14.0`, `1.15.0` - bare semver, correct. The only drift is the pre-existing
`v1.9.8`/`v1.10.0` tags, which `CLAUDE.md` already documents as a known one-off exception to leave
alone, not retag. No new drift was introduced across 1.11.0-1.15.0. No action needed.

## akirule compliance scorecard

| Rule | Status | Note |
|---|---|---|
| `release.A5` (version minted at release, not per work-session) | **Pass** | Each of 1.11.0-1.15.0 bundles a material, non-trivial accumulation (verified via CHANGELOG word counts and diff scope, e.g. `1.13.1`'s same-day revert is a genuine regression fix, not a filler bump). |
| `release.B4/B5` (content discipline, terse GitHub-Release-ready prose) | **Fail (medium)** | See Finding #4 - entries are essay-length; narrative belongs in `docs/arch`/`docs/plan` with the CHANGELOG holding a one-line pointer. |
| `docs.A1/A2` (topic folders, `docs/biz/` mandatory for business-dimension projects) | **Open question** | `docs/biz/` is absent. This project reads as an internal single-developer tool (per its own `CLAUDE.md`: "Dark mode desktop tool for rsync-based deploy workflows") with a small community-facing surface (`share/aki-statusLine/`, public GitHub releases) - arguably no business/monetization dimension exists to document. Not flagging as a hard violation without confirming intent; if the project ever gains a business angle (pricing, positioning, a public-facing pitch), `docs/biz/` becomes mandatory per `RULE-docs` A2. |
| `docs.B1/B2` (plan lifecycle, doc sync) | **Pass** | `docs/plan/done/` correctly used (not `archived/`); `docs/arch/usage-claudecode.md` and `docs/feat/background-refresh.md`/`docs/feat/open-popup.md` were all found already correctly updated for their respective removals - the miss is narrower than "docs are stale," it's specifically `IntroModal.vue` (in-app copy, not a `docs/` file) that was missed. |
| `ui-pattern.A2/A3` (design tokens, arbitrary-value policy) | **Fail (medium, pre-existing)** | See Finding #3 - 416 hardcoded color values vs. 14 tokens. Not introduced by 1.11-1.15, but not addressed by it either. |
| `stack-tauri.A1` (never block the UI) | **Pass** | Audited every `#[tauri::command]` in `src-tauri/src/*.rs` (`grep -n "#\[tauri::command\]" -A3`). Every command whose body waits on a subprocess or network call (`resolve_remote_path`, `check_for_updates`, `install_ssh_terminal_color`, `get_git_info`, `run_git_command`, `get_file_conflict_info`, `provision_agent_usage`, `get_agent_usage`, `logout_antigravity`, `run_sync`, `check_statusline_status`, `apply_statusline_config`) is `async fn` + `spawn_blocking`. Commands using only `.spawn()` (fire-and-forget, non-blocking: `macos_open`, `open_local_terminal`, `open_remote_subprocess`, `run_project_command`, `run_project_dev`, `install_akiclaudedoc`) are correctly left as plain `fn` - `.spawn()` does not wait, so this is not the bug class the rule targets. |
| `stack-tauri.B3` (serde default for persisted JSON) | **Pass** | `SyncProject` (`projects.rs:42-87`) - every field added after the original schema carries `#[serde(default...)]`; the deprecated `sync_git` correctly uses `Option<bool>` + `skip_serializing_if` per its own removal plan. `StatuslineConfig`/`StatuslineField` (`statusline.rs`) are not read back from a Rust-persisted file (config lives in frontend `localStorage`, passed in per-call), so `serde(default)` does not apply there the same way - the frontend's `loadCfg()` merge (per CHANGELOG 1.15.0) is the correct analogous protection and was verified present. |
| `coding.B2` (Chesterton's Fence before removing/changing) | **Pass** | The DEV auto-open-browser removal (`system.rs:576-581`) documents *why* two prior fix attempts failed before removing the feature outright - exactly the standard this rule asks for. |

## Recommended follow-ups

1. Delete `src/components/UsageProgressBar.vue` (Finding #1) - zero call sites, confirmed by grep.
2. Rewrite `IntroModal.vue` lines 175-176 and 209 to drop "Force Sync"/"Auto-Probe" language (Finding
   #1) - belongs to the agent already assigned to README/IntroModal per this task's instructions.
3. Remove `is_vite_project()` + `ProjectStackInfo.is_vite` from `system.rs`, or repurpose with an
   honest comment (Finding #2).
4. Schedule (not urgent) a token-extraction pass per Finding #3's top-10 repeated values, starting
   with `ClaudeSettingModal.vue` (83 occurrences) and `IntroModal.vue` (48).
5. Going forward, keep new CHANGELOG bullets to symptom + fix + doc-pointer; let `docs/arch/*.md` and
   `docs/plan/done/*.md` hold the design narrative (Finding #4). No retrofit of past entries needed.
6. Confirm with the user whether this project has (or will have) a business/monetization dimension;
   if yes, `docs/biz/` becomes mandatory per `RULE-docs` A2 and should be created.
