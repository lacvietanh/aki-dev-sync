# Backlog — jul28 batch (pinned-task sweep: terminal mobile UX, sync badge)

Master index for the four pinned tasks in `.akidevsync/notes.json` plus a doc-hygiene request, delivered through `/akiflow`. This doc is the tracker: scope, status, and dependency order live here; the one workstream needing genuine design work owns its own plan doc.

**Sizing gate**: `[akiflow] tier=1 — trigger: batch spans terminal (3 pinned tasks) + sync badge (1, separate module); one item (mobile keyboard) requires UX research, so `METHOD-ux-psych.md` was consulted for that workstream specifically. Market counsel not invited — no pricing/positioning/audience decision in scope.`

**Baseline warning**: the working tree was already large and uncommitted when this batch opened (the jul27 batch's hard-wrap sweep and several other workstreams appear already applied on disk — see `git status`). Nothing in this batch ran `git add`/`stash`/`checkout`/`reset`.

## Workstreams

| ID | Scope | Status | Notes |
|---|---|---|---|
| WS-G | Terminal mobile: virtual-keyboard-appears layout research + design | **implemented** | `docs/plan/terminal-mobile-keyboard-viewport.md` §3 — new `src/composables/useVisualViewportHeight.js`, `main.css`/`useDockLayout.js` `vh`→`--vvh` rewrite, `index.html` `interactive-widget=resizes-content`. On-device verification (§4/§5 of that doc) still outstanding. |
| WS-H | Terminal mobile: key row wraps to a second line on narrow width | **done** | `src/components/TerminalView.vue` — `.pty-key-row` switched `flex-wrap: wrap` → `nowrap` + horizontal scroll, padding/gap tightened (4px→2px row gap, 4px 8px→3px 5px per key) |
| WS-I | Sync badge: PUSH/PULL `--delete` indicator too prominent | **done** | `src/components/CountBadgeWrap.vue` — dropped the pill/background/box-shadow to a plain small glyph; added `deleteSide` prop; `src/components/ProjectTable.vue` passes `delete-side="left"` for PUSH (PULL keeps the existing right-side default) so the two icons sit at opposite bottom corners instead of both at bottom-right; `src/assets/main.css` gained the `.cell-badge-left` modifier |
| WS-J | Terminal tab strip too narrow | **done** | `src/components/TerminalTabStrip.vue` — chip widened from an icon-only square (~24px) to a flexible 84–160px chip showing a truncated title, with an always-visible close-x (previously hover-only on the active chip only) |
| WS-K | Consolidate Vietnamese/WebKit IME content scattered across plan docs into one file | **audited, no action taken** | See "WS-K finding" below — the premise did not hold on inspection |
| WS-L | Terminal tab titles: auto-follow the shell (like an external window's titlebar) + manual rename | **done** | `TerminalView.vue`'s `term.onTitleChange`, `terminalTabsStore.js`'s new `renameTerminalTab` (`titleLocked` guard), `TerminalTabStrip.vue`'s right-click-to-edit chip |
| WS-M | Global terminal header button showed neither badge (tab count, external count) | **done (MVP floor)** | New `count_external_terminals_global` (Rust) + `externalTermGlobalCount` (store) + inline computed block in `ProjectTable.vue`, reusing `.btn-cell-trigger` + `TerminalCountBadges`. Diverges from `terminal-ownership-model.md` §10's exact S1/S2/S5 shape — see that doc's 2026-07-28 note. Introduced and fixed within this session: an interim version put the badge on an icon-sized box and visually covered the icon — final version reuses the exact `.btn-cell-trigger` class every per-project button already uses. |
| WS-N | Global terminal group's forced one-tab minimum removed | **done** | Root-caused as the actual mechanism behind phantom "Shell" tabs piling up in the global group across dev-server HMR reloads, not just a code-cleanliness complaint. Removed from `terminalTabsStore.js`'s `closeTerminalTab`, `useTerminalTabs.js`'s `closeTab`/`initTerminalTabs`. Global is now symmetric with every project scope: opens on demand, closeable to zero. `docs/arch/terminal-stack.md` and `docs/feat/in-app-terminal.md` updated (the `MAX_TABS` derivation note in particular — the constant itself is unchanged, only the story behind it). |

## WS-K finding — the content is not actually scattered

The request: "if multiple plan files mention the Safari/WebKit Vietnamese-typing bug, split all of it into one standalone file so it's easy to pick up in a separate session."

Grepped every plan/research/feat doc for Vietnamese/IME/WebKit/Safari content. Result: the substantive investigation is **already** consolidated into exactly one place, and everything else is a correctly-scoped pointer, not a duplicate:

- `docs/research/terminal-vietnamese-ime-root-cause-{jul27,2,3}.md` — the actual investigation, already an immutable, chained research record (`-jul27.md` → `-2.md` → `-3.md`, each carrying a `Status: superseded by …` line at the top of the superseded file). `-3.md` is the current head and is already fully self-contained: start time, purpose, strategy, checklist, ranked candidates, a copy-pasteable Mac evidence-capture recipe, a decision, and a 2026-07-28 addendum with a gated (not-yet-applied) patch. A future session picking this up needs exactly this one file plus `scripts/capture-ime-evidence.sh` — nothing to extract, it is already the standalone artifact the request asked for.
- `docs/feat/in-app-terminal.md` §"Vietnamese input" — the shipped-behavior summary. Correct location per `RULE-docs` A2 (a feat doc owns "what currently ships"); this is not the same content as the research chain (it summarizes the shipped guard, not the open investigation) and must not be merged into it.
- `docs/plan/backlog-jul27.md` (WS-D row + status-log entries) — a tracker pointing at the research doc, which is exactly what a tracker should do.
- `docs/plan/terminal-chrome-settings.md` §S2/§6.2 — **not** IME investigation content. It is WS-B's own argument for why the compose-input default should be `true`, which cites the open IME bug as its motivating context. Extracting it would break that feature's own default-decision away from its justification, for no benefit — a reader of the IME research doesn't need WS-B's settings-menu argument, and a reader of WS-B needs exactly that citation inline.

No file was moved or created for WS-K. If the user still wants a physical merge despite the above (e.g., because the *research chain itself*, spanning 3 files, still requires opening multiple files to get the full history), the correct unit to fold is `-jul27.md` + `-2.md` into `-3.md`'s own history section — flag this explicitly rather than doing it silently, since research docs are immutable event records per `RULE-docs.md` §B2 and collapsing them loses the individually-dated event trail.

## Verification

- `npm run build` (vite) and `cargo check` (src-tauri) both passed clean as of the last build run in this session, which predates WS-L/M/N. **Not re-run after WS-L/M/N** at the user's explicit instruction (unprompted builds cost tokens they did not ask to spend) — those three are **unverified by any tool**, not just by a live screen, and should be built before being treated as shippable.
- Visual/touch confirmation of every UI change in this batch (badge placement, tab width, key-row single-line-ness at phone width, the global button's badge layout, tab rename) is a runtime check not done here — flagging as **unverified, needs a live screen** per `coding.B3`.
- WS-G's real-device keyboard behavior is separately flagged unverified in `docs/plan/terminal-mobile-keyboard-viewport.md` §4/§5.

## Status log

- **2026-07-28** — batch opened; WS-H, WS-I, WS-J implemented directly (each a small, reversible, single-component CSS/prop change — no plan doc needed per the tier-1 gate's own judgment that a doc-writing pass would outweigh the change); WS-K audited and closed with no code/doc change; WS-G dispatched to a research agent, pending.
- **2026-07-28 (cont.)** — WS-G's research landed and was implemented directly (`useVisualViewportHeight.js` + the `vh`→`--vvh` rewrite). Three more requests arrived mid-session and were sized and implemented directly (no new plan docs — each stayed a small, reversible, multi-file-but-single-concern change): WS-L (tab title auto-follow + rename), WS-M (global button's missing badges, MVP floor per `terminal-ownership-model.md` §5), WS-N (removed the global scope's forced one-tab minimum, root-caused as the actual HMR-duplicate-shell bug mechanism, not merely a style complaint). `docs/arch/terminal-stack.md`, `docs/feat/in-app-terminal.md`, `README.md`, `IntroModal.vue`, `CHANGELOG.md`, and `terminal-ownership-model.md` (status + a divergence note) were all updated in the same pass to avoid drift. Per explicit user instruction, no build was run after this round — WS-L/M/N are implemented but tool-unverified.
