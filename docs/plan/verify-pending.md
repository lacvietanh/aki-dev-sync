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

## UI

### U1 — Projects table column-flex reshape
**Changed:** `src/components/ProjectTable.vue:756-757` (wide) and `:1274` (narrow) — `--grid-cols` changed from a single-column `1fr` (all extra width absorbed by `.col-sync`) to `minmax(...)` on both `.col-project-info` and `.col-sync`, splitting the flex weight. Landed per `ui-sweep-misses.md` #5.
**Steps:** Open the app at the minimum supported width (~420px). Confirm `.col-sync`'s PUSH/DRY/PULL + LOG + gear buttons are not clipped or wrapped; widen the `minmax` floor (`6rem`/`4.5rem`) if they are.

### U2 — GlobalNoteModal narrow-mode padding repeat
**Changed:** `src/components/modals/GlobalNoteModal.vue` — added the local `@media (max-width: 700px)` repeat for `.note-body`/`.note-footer` padding that four sibling modals already carry (`ui-sweep-misses.md` #11a).
**Steps:** Open Global Note at a window width ≤700px. Confirm body/footer padding shrinks to `10px` / `8px 10px` like ChangelogModal/ClaudeProfileModal/SshConfigModal/UpdateModal already do.

### U3 — GlobalNoteModal textarea min-height re-tune
**Changed:** `src/components/modals/GlobalNoteModal.vue:67-81` — derived `min-height: 42px` (13px font × 1.6 line-height × 2 rows) plus `field-sizing: fixed`, replacing the earlier over-tall guessed value (`ui-sweep-misses.md` #11b).
**Steps:** Open Global Note and confirm the textarea shows ~2 rows by default with no dead vertical space, and the resize-drag handle still works in the Mac's WKWebView.

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
