# Backlog — jul27 batch (terminal ownership · usage OOP · IME · hygiene)

Master index for a seven-item request delivered through `/akiflow tier=2`. This doc is the **tracker**: every workstream below owns one plan doc, and status changes land here first so no stage drifts out of sight. Individual plan docs hold the design; this file holds only scope, owner, status and the dependency order.

**Version state**: `package.json` is `1.21.0`; everything here accumulates in `CHANGELOG.md`'s `[Unreleased]` buffer. No version is minted until the release event (`release.A`) — do not name new files after a version number that does not exist yet.

**Baseline warning**: the working tree was already half-finished when this batch opened (notes-JSON SSoT work, IME guard v2, terminal cap work, external-terminal modal — all uncommitted). Nothing here may `git add`/`stash`/`checkout`/`reset`. Any workstream that needs a clean baseline says so in its own doc.

## Sizing gate

`[akiflow] tier=2 (forced by user)` — signals present: new user-facing flow (terminal chrome settings menu), architecture rewrite across ≥3 modules (external-terminal ownership; `agent_usage.rs` split), and an unresolved native-layer bug. **Market counsel is deliberately NOT invited** (`akiflow` step 2, law 3): this is an internal developer tool with no pricing, positioning or audience decision in scope. UX-Psych *is* invited — WS-A and WS-B both turn on defaults and on what a badge is allowed to claim.

## Workstreams

| ID | Scope | Plan doc | Status |
|---|---|---|---|
| WS-A | External-terminal ownership model: spawn-origin registry in RAM, per-project vs global counting, one shared terminal-button class | `terminal-ownership-model.md` | **planned** |
| WS-B | Terminal chrome visibility: 3-dot settings menu on the stack header, per-control checkboxes, host-dependent defaults (browser = all shown, app = all hidden) | `terminal-chrome-settings.md` | **planned** |
| WS-C | Usage probe OOP: `get-antigravity-usage.js` → shell, delete `Interpreter::Node`, split `agent_usage.rs` (892 lines) into domain modules behind a shared `UsageProbe` trait | `usage-probe-oop.md` | **planned** (the trait was argued and rejected — see that doc §5) |
| WS-F | A corner indicator on PUSH and PULL showing whether that direction runs with `--delete` | §"WS-F" below | **implemented** |
| WS-D | Vietnamese IME residual bug: restore-after-backspace loses a character in the in-app terminal only | `docs/research/terminal-vietnamese-ime-root-cause-3.md` (research first) | **capture kit ready, patch drafted-but-gated — still awaiting Mac evidence** |
| WS-E | Hygiene batch: companion WebSocket reconnect noise, project-icon 404 logged as an error despite a working Rust fallback, hard-wrap sweep across recently touched files | `hygiene-jul27.md` | **planned** |

## Dependency order

- WS-D is **blocked on runtime evidence from the Mac** (`__akiIme.dump()`), not on any other workstream. It proceeds as research in parallel and only becomes code once the evidence lands.
- WS-B depends on WS-A's control inventory: the settings menu can only toggle controls that exist, and WS-A may add or remove some.
- WS-C and WS-E are independent of everything else and can land in any order.
- The hard-wrap sweep in WS-E must run **last** among the code workstreams, or it will collide line-for-line with every other diff.

## Item → workstream trace

Every item in the original request maps to exactly one workstream, so nothing silently drops:

1. Companion WebSocket `ws://127.0.0.1:1421/ws?role=host` — "network connection was lost" → **WS-E**
2. Project-icon 404 logged as an error although the Rust side already falls back → **WS-E**
3. Auto-wrapped lines violating `agent.C`'s no-auto-wrap rule → **WS-E**
4. Usage refactor: AG probe to shell, `Interpreter::Node` removal, `agent_usage.rs` decomposition → **WS-C**
5. Vietnamese typing still broken (`e x x i t` → loses the restored `x`), VS Code's terminal as the reference implementation → **WS-D**
6. Terminal chrome show/hide as an option with a settings menu → **WS-B**
7. External terminal tracking by spawn origin rather than by `cwd` scan → **WS-A**

## WS-F — the `--delete` corner indicator on PUSH and PULL

Added mid-batch, at the user's request: *"tôi muốn flag DELETE có hiện 1 icon nhỏ ở góc của nút pull|push"*. Small enough that it gets a section here rather than a plan doc of its own (`docs.C1` — two docs for one question is the failure that bound exists to prevent).

**Why it is worth having, beyond being asked for.** `delete_on_pull` defaults to **`true`** and `delete_on_push` to **`false`** (`src/composables/useProjectConfig.js`, and `src-tauri/src/projects.rs`'s field comment). So the more destructive of the two directions is the one that is armed by default, and today nothing on the button says so — the setting is two clicks away behind the gear. `src/components/ProjectTable.vue:373` already records the reasoning that a mistaken `--delete` mirror is the outcome most feared here. This puts the arming state where the decision is actually made.

**Shape — no new element in the flow, per CLAUDE.md's Extreme Narrow rule.** Both buttons are already wrapped in `CountBadgeWrap.vue`, which supplies the `position: relative` anchor and renders the red pending-count badge at `.cell-badge-top` (top-right, `main.css:833`). The indicator therefore goes in the **bottom-right** corner, where `main.css:837`'s `.cell-badge-bottom` already exists and is unused by these two buttons — so it costs one absolutely-positioned overlay, one existing class, and zero layout.

**Spec:**

- Rendered only when that direction's flag is on: the PUSH button's indicator follows `delete_on_push`, the PULL button's follows `delete_on_pull`. Off means no element at all, not a dimmed one — an unarmed direction should look exactly as it does today.
- Icon `fa-solid fa-trash` or `fa-scissors` at the badge's 9px scale, in the danger colour the app already uses for destructive state (`#ef4444`, the same value `CountBadgeWrap`'s count badge uses locally — reuse the token, do not introduce a second red).
- `title` states the consequence, not the flag name, per `RULE-content-write.md` B1: on PULL, "Mirror: files here that the remote does not have will be deleted." On PUSH, the same sentence with the sides swapped.
- It must not collide with the STOP state (`ProjectTable.vue:1222` — the same button turns red while syncing). Decide one of: hide the indicator while that direction is running, or keep it. Recommendation: **hide it** — during a run the button's whole meaning is "stop", and a destructive-mode marker on a STOP button reads as though stopping is what deletes something.
- Also check the narrow breakpoint (`ProjectTable.vue:1315`), where the DRY toggle is already squeezed so the top-right count badges have room. A second badge on the opposite corner needs the same check, and this is the one part that can only be judged on screen.

**Acceptance criteria:**

- With `delete_on_pull: true` (the default) a fresh project shows the indicator on PULL and not on PUSH.
- Toggling either flag in the gear updates the corresponding button's indicator with no reload.
- No new DOM element exists when both flags are off; no button changes width in any state (Extreme Narrow).
- No new colour value and no new `z-index` enter the diff.
- `README.md` and `IntroModal.vue` checked in the same task (CLAUDE.md, "Feature changed?"), plus a `CHANGELOG.md` `[Unreleased]` entry.

**Sequencing**: independent of every other workstream, but it touches `ProjectTable.vue`, which WS-A's step S5 also touches. Land it either before WS-A/S5 or after — not concurrently.

## Decisions taken by the user, 2026-07-27

Recorded here rather than only in the workstream docs, because each one overrides something a plan doc argued and a later reader needs to know the argument was heard and settled.

- **The compose input stays shown by default in the Mac app**, against the user's original "app hides everything" rule and following `terminal-chrome-settings.md` §6.2. The other six controls hide as originally stated. Consequence for WS-B: the app-side default map has exactly one `true` entry, and §6.3's one-time armed tint is no longer needed for the compose case — keep it only if the menu still tests as undiscoverable.
- **A terminal opened over SSH from a project's popup counts on that project's badge**, resolving `terminal-ownership-model.md` §11 question 1 in favour of the spawn-origin rule. This is a deliberate change in what the number claims — from "windows standing in this folder" to "windows I started from here" — and the CHANGELOG entry must say so plainly, since today such a window is counted nowhere at all.
- **Execution order: WS-D, WS-E and WS-F first**, then the larger workstreams, with WS-E's hard-wrap sweep still last of everything.

## Status log

- **2026-07-27** — backlog opened, tier declared, five workstreams dispatched to their planning stages.
- **2026-07-27** — WS-F added mid-batch at the user's request and specced in the section above; scope is one overlay per sync button, no new element in the flow.
- **2026-07-27** — WS-B planned (`terminal-chrome-settings.md`). Two findings outrank the menu itself. **The tab strip must be locked on for a companion**: `⌘T`/`⌘W`/`⌘⇧[`/`⌘⇧]` fully replace it on the Mac, but a phone has none of them, so a hidden strip leaves that screen unable to open, close or switch a tab at all. And **the text-size buttons live physically inside the key row's markup**, so one checkbox covering both would take a phone's only zoom control away with it — they need separating before either becomes toggleable. The doc also argues against one of the user's stated defaults (the compose input) and against moving either existing header button into the menu; both arguments are in its §6 and §7 for the user to settle.
- **2026-07-27** — WS-C planned (`usage-probe-oop.md`). **The `UsageProbe` trait was argued and rejected**, with the reasoning recorded in that doc's §5 rather than left implicit: there are two implementors but only one polymorphic call site, everything the two probes genuinely share is already extracted as shared types and functions, and what they do not share is structural (Claude Code reads a hook-written file where mtime is meaningful and there is one account by design; Antigravity queries a live RPC with N accounts and no mtime). The module split proceeds regardless and is what would make a later trait mechanical if a third agent ever arrives. Second decision worth surfacing: **the ported shell script parses no JSON at all** — neither `jq` nor `python3` can be assumed present on either target, and this repo already has an incident for each — so the script emits raw RPC bodies inside a delimiter frame and `serde_json` shapes them in Rust, which is the pattern the Claude Code probe already uses. One item blocks the port's central assumption and must be checked before any code is written: whether `curl` exists on the Ubuntu remote.
- **2026-07-27** — WS-A planned (`terminal-ownership-model.md`). Its own §3 reconciliation rule is the answer to the registry-versus-scan trap the backlog flagged at dispatch: the scan stays the sole authority on which sessions exist, the registry only ever says who launched one, so a registry entry can change attribution but can never change a total — which is what keeps the derived-count design from regressing into the counter that only grew. Two findings feed back into other workstreams: (1) WS-B's stated dependency on WS-A is weaker than assumed here, since WS-A adds no control to the terminal stack header and its UI change lands in the project table's TERMINAL header instead — WS-B may proceed in parallel and reconcile only against WS-A's step S5; (2) an unrelated defect surfaced en route — `open_remote_subprocess` in `src-tauri/src/system.rs` is a plain synchronous `pub fn` that escapes the never-block-the-UI rule today only because it never waits on its child, and it must become `async fn` + `spawn_blocking` in the same step that makes it read the child's output.
- **2026-07-27** — WS-D research written (`docs/research/terminal-vietnamese-ime-root-cause-3.md`); `-2.md` carries its `Status: superseded by` line. The chain now has a mechanism traced line-to-symptom rather than hypothesised, and it is in the guard rather than in xterm: `useWkImeGuard.js`'s `classify()` excludes any multi-character `key` whose payload is all ASCII letters, on the stated assumption that an OpenKey carrier "always contains a non-ASCII Vietnamese char" — true of the *correction* path the guard was designed against, false of the *auto-restore* path, whose carrier is the original raw ASCII. Verified independently at `src/composables/useWkImeGuard.js:196`. **Still a candidate, not a confirmed cause**: it holds only if the restore carrier arrives untagged by keyCode 229, and only the Mac dump can settle that. Note also that round 2's claim "Chromium never tags 229, which is why VS Code is fine" is now refuted as a blanket statement — xterm.js #5887 reproduces in Chrome per its own issue — so no VS Code technique exists to copy, and any fix must be ours.
- **2026-07-27** — WS-E planned (`hygiene-jul27.md`). Two of its three items changed shape under investigation and the backlog's own item descriptions above are now the stale version: neither the WebSocket message nor the icon 404 is emitted by app code at all — both are WebKit's own network-layer logs, which JavaScript cannot suppress. Item 1 is therefore expected reconnect churn rather than a defect. Item 2 remains fixable, but by a different route than "stop logging it": `src/utils/projectIcon.js`'s host branch requests an icon URL for every project, ignoring `projectStore.projectIcons` — a map that is already complete and null-aware on the host as well as the companion (verified: `App.vue` fills it via `refreshProjectIcons`, and `projectStore.js` documents every project id as present with an explicit `null` when there is no icon). Consulting it first means never requesting a URL already known to 404. **Open design point for implementation**: the map can lag a freshly added icon, so an id *absent* from the map must fall through to the request rather than be treated as icon-less — only an explicit `null` may suppress it.
- **2026-07-28** — WS-F implemented. `CountBadgeWrap.vue` gained `deleteArmed`/`deleteTitle` props rendering a `fa-solid fa-trash` icon on `.cell-badge-bottom` (already unused by these two buttons), styled with the same `#ef4444` the count badge already uses locally — no new colour, no new z-index. `ProjectTable.vue` wires `p.delete_on_push`/`p.delete_on_pull` (flat fields on the project record, confirmed in `src/composables/useProjectConfig.js` and `src-tauri/src/projects.rs`) through `!isStop(p, 'push'|'pull')` so the marker disappears during that direction's STOP state, per the recommendation above. Titles state the consequence, not the flag name. **Unverified**: whether the new bottom-corner icon visually collides with anything at the narrow breakpoint (`ProjectTable.vue`'s `.dry-group` squeeze) — this needs a screen, not a diff, and was left as reported rather than guessed at.
- **2026-07-28** — WS-D evidence kit and gated patch prepared (addendum appended to `terminal-vietnamese-ime-root-cause-3.md`, doc body above it left untouched per the research-doc immutability rule). `scripts/capture-ime-evidence.sh` is the runnable capture procedure (Mac-only, prints the numbered recipe including the `Main.html`-vs-`localhost` inspector trap, stages the console confirm+clear snippet via `pbcopy`, then starts `npm run tauri dev`). The `classify()` replacement is written in full in the addendum, gated on Candidate 1 being confirmed by the Mac dump — **not applied**, `src/composables/useWkImeGuard.js` is unmodified. Design: swaps the old "is the payload all ASCII letters" test for "is `ev.key` a named DOM key value" (`NAMED_KEYS` set), which is what the exclusion was always meant to test; per-shape claimant table confirms exactly one shape's claimant changes (the ASCII restore carrier, `null` → `multiCarrier` — the fix) plus one incidental latent-bug fix (function keys, previously mis-routed into `multiCarrier` by the old regex). Explicitly does not address Candidate 2 (229-tagged carrier with no claimable `beforeinput`) or Candidate 3 (decomposed-Unicode backspace-count mismatch) — either of those being confirmed instead means this patch must not be applied.
