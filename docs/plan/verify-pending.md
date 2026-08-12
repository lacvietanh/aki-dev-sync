# Verify-pending — runtime confirmation backlog

Every item below is **code-complete and committed**; nothing here is missing work. Each is blocked only on a human sitting at the Mac (or a phone against it) and observing the result — static reading cannot settle any of them. Walk top to bottom. When an item passes, delete its row and, if that was the last row for a source plan, follow the pointer back to that plan and close it per `docs.B1`.

Consolidated 2026-07-31 from `hygiene-jul27.md`, `ui-sweep-misses.md`, `terminal-input-jul31.md`, `terminal-input-surface.md` — each of those docs now carries a one-line pointer to this file instead of its own unverified-status prose.

## Terminal

### T1 — Ctrl/Shift armed-button display + tab alive-status
**Changed:** `src/components/TerminalView.vue` — `ptyApi` changed from `ref(null)` to `shallowRef(null)`; a deep `ref` was silently unwrapping `pendingModifiers`/`alive` inside the `reactive()`-assigned object, so every `.pendingModifiers.value` read `undefined`. Landed 2026-07-31, commit `4851f40`.
**Source:** `terminal-input-jul31.md` §2.3.
**Steps:** On a companion (phone), tap the Ctrl button — it must turn solid cyan (`is-armed`) and clear when a keystroke consumes it or it's tapped again. Same for Shift. Kill the PTY (e.g. `exit` the shell) and confirm the tab strip shows the dead state instead of the permanently stuck `'unknown'`.

### T2 — Option+arrow with the double-space fix in place
**Changed:** `src/composables/useTerminalTextDrain.js` — `customKeyEventHandler` now vetoes every keypress (not just multi-char carriers). Landed 2026-07-31, commit `b99502a`.
**Source:** `terminal-input-jul31.md` §2.1 (owner deliberately deferred this one measurement this round — "chưa đo, không tính là PASS").
**Steps:** In the in-app terminal, use Option+Left/Right (word jump) and Option+Backspace on both Mac keyboard and OpenKey. Confirm no regression from the keypress-veto change.

### T3 — Compose-textarea auto-grow vs PTY resize oscillation
**Changed:** none — this is a pre-existing mechanism (`field-sizing: content` on `.pty-compose-input`, `src/components/TerminalView.vue:614`), not a fix, but it was never confirmed not to fight the resize chain.
**Source:** `terminal-input-surface.md` §6 row 6.
**Steps:** On a companion, type a Shift+Enter multi-line compose message so the textarea grows. Watch whether `.pty-terminal-mount`'s height changes trigger `ResizeObserver → scheduleFit → fitAddon.fit() → hostResize → pty_resize` (`TerminalView.vue:261-298`) repeatedly (oscillation) rather than settling once. Fallback if it oscillates: fix the textarea to 2 rows instead of auto-growing.

### T4 — Stuck `:hover` false-positive on iOS Safari key-row buttons
**Changed:** `src/components/TerminalView.vue:546-551` — `.pty-key:hover` is scoped inside `@media (hover: hover)` so it never applies on touch, guarding against iOS Safari's known behavior of sticking a `:hover` state on the last-tapped element until something else is tapped.
**Source:** `terminal-input-surface.md` §6 row 1 (folded into T1's scope — same key-row buttons).
**Steps:** On a real iPhone, tap a key-row button that is not Ctrl/Shift (e.g. Esc), then look at it without tapping anything else — confirm it does not show a cyan border that could be mistaken for an armed-latch state.

### T5 — Terminal resize propagation reaching both the app and the companion
**Changed:** none — static analysis across two rounds found no defect at any candidate origin (`FRAME_PTY_RESIZE` wired both ways, host is sole resize authority, resize frames are never-dropped, geometry is pushed unprompted on every new/reconnected socket). The honest close is "no defect found statically, one runtime check outstanding," not "no bug" — see `docs/plan/done/backlog-jul27.md` T-9.
**Source:** `backlog-jul27.md` T-9 (#1).
**Steps:** `scripts/verify-pty-resize.sh` prints the full walkthrough. Four parts, on a real Mac with a real phone paired: (1) drag the terminal stack's splitter, collapse/expand the stack, switch tabs, apply a window-size preset — confirm the companion's grid matches the Mac's after each; (2) resize on the Mac before pairing a phone (geometry should arrive correct, unprompted), then background the phone until the socket drops and let it reconnect (geometry should re-arrive without a resize event); (3) **the one genuinely unsettled item** — zoom with the companion's own key-row `+`/`−` and watch whether xterm actually re-measures/repaints on a bare `fontSize` write with no accompanying `fit()`/`resize()`; (4) confirm whether the original symptom report predates the font-zoom commit `8cc2669` and the dock-splitter work — if so, parts 1–3 passing means it's already gone. If all four pass, this closes verified-with-no-change and gets **no** CHANGELOG entry (a verification is not a fix).

## UI

### U1 — Projects table column-flex reshape
**Changed:** `src/components/ProjectTable.vue:756-757` (wide) and `:1274` (narrow) — `--grid-cols` changed from a single-column `1fr` (all extra width absorbed by `.col-sync`) to `minmax(...)` on both `.col-project-info` and `.col-sync`, splitting the flex weight. Landed per `ui-sweep-misses.md` #5.
**Steps:** Open the app at the minimum supported width (~420px). Confirm `.col-sync`'s PUSH/DRY/PULL + LOG + gear buttons are not clipped or wrapped; widen the `minmax` floor (`6rem`/`4.5rem`) if they are.

### U2 — GlobalNoteModal narrow-mode padding repeat
**Changed:** `src/components/modals/GlobalNoteModal.vue` — added the local `@media (max-width: 700px)` repeat for `.note-body`/`.note-footer` padding that four sibling modals already carry (`ui-sweep-misses.md` #11a).
**Steps:** Open Global Note at a window width ≤700px. Confirm body/footer padding shrinks to `10px` / `8px 10px` like ChangelogModal/ClaudeProfileModal/SshConfigModal/UpdateModal already do.

### U4 — Bottom dock: per-stack height and per-stack splitter
**Changed:** `src/composables/useDockLayout.js`, `src/components/DockStack.vue`, `src/components/AppConsole.vue`, `src/assets/main.css` — the dock's stored total height replaced by a per-stack length map summed into `dockHeightCss`; each expanded stack renders its own splitter. Landed 2026-08-12, **uncommitted** at the time of writing.
**Source:** `docs/plan/dock-stack-independent-height.md` §4 (full check list, seven items).
**Steps:** Both panels open, collapse the terminal — the log must not move; same in reverse. Terminal collapsed, log open, expand the terminal — the log must not move; collapse the log instead and only the log's own height must disappear. Drag each splitter: it tracks the pointer with no lag, the terminal re-fits live while its own is dragged, the sibling is frozen, and double-click resets only that panel. MAXIMIZE/restore behaves as before and collapsing both panels leaves maximised mode. Relaunch and confirm both heights persist — **the first launch resets to defaults**, since the old `aki-dock-height-pct` value is deliberately not migrated.

*(`ui-sweep-misses.md` #10 — `.u-select-text` on the delete-confirm phrase — is explicitly marked "none of consequence" by its own doc; a pure CSS-inheritance fix on an already-escaped string. Not listed here as a verify item.)*

## Infra

### I1 — Companion WebSocket reconnect convergence on real sleep/wake
**Changed:** none — `src/services/bridge.js`'s reconnect loop (`scheduleReconnect()`, exponential backoff 1s→10s cap) was re-traced and confirmed structurally sound; no code change was made or needed.
**Source:** `hygiene-jul27.md` item 1.
**Steps:** On a real Mac, (a) trigger a dev rebuild (`cargo`/`tauri dev` hot-restart) and (b) a real sleep/wake cycle. Confirm `connectionState` returns to `'open'` within one backoff cycle (≤10s after the relay is reachable again) in both cases, with no `console.error` from `bridge.js` during the transition (the browser's own uncontrollable "network connection was lost" log is expected and out of scope).

### I2 — Project-icon 404 avoidance
**Changed:** `src/utils/projectIcon.js` — the host branch of `projectIconSrc()` now consults `projectStore.projectIcons` (populated by `refreshProjectIcons()`) and returns `''` when the entry is explicitly `null`, instead of unconditionally building the `aki-devsync-icon://` URL and letting it 404.
**Source:** `hygiene-jul27.md` item 2.
**Steps:** Open a project with no icon. Confirm devtools' Network tab shows **no request at all** for that icon (not a 404, an absent request). Open a project that does have an icon and confirm it still resolves via `aki-devsync-icon://` unchanged.
