# Terminal tab ownership lost on webview reload — root cause

**Start time:** 2026-09-09

## Initial purpose

Owner hit this by accident: aiming for a terminal tab's rename menu, he right-clicked slightly off target and hit the native "Reload" instead. After the reload every terminal's state was wrong — every shell was attributed to the global scope instead of its own project, pinned icons were gone, and the per-project TERMINAL button badges showed wrong counts.

His instruction was explicit that this is not a refresh glitch to paper over: *"đây không chỉ là vấn đề lệch UI mà là 'lệnh biểu hiện' => cần suy xét nguồn gốc kỹ lưỡng của bản chất cốt lõi gốc rễ của flow detect và lưu trữ thông tin. biểu hiện sai chứng tỏ gốc rễ sai."* — wrong display proves the underlying model is wrong. So the question was not "how do we repaint correctly after a reload" but "where does the truth about which project owns a terminal actually live, and why is it not there after a reload".

Context at the time: 1.28.1 released, `[Unreleased]` open. The in-app terminal (`docs/feat/in-app-terminal.md`) runs PTYs in the Rust process; tabs are grouped per project in the frontend.

## Strategy

1. Find what the mis-clicked "Reload" actually is — an app menu item, a Tauri window reload, or the WKWebView's own context menu. This decides what survives it.
2. For every piece of tab state (id, title, pinned, owning project, cwd), determine where it is stored and whether it survives that reload. Build a table rather than reasoning case by case.
3. Trace the two reported symptoms (badges, pin icon) back to the field they read, to confirm they share one cause rather than being two bugs.
4. Only then choose a fix, preferring one that makes the state recoverable by construction over one that guards the symptom (`pattern.A8`).

## Checklist

- [x] Locate the reload trigger and confirm what it destroys.
- [x] Read the Rust PTY module's tab record and the command the frontend re-hydrates from.
- [x] Read the frontend's adopt path and every consumer of the ownership field.
- [x] Confirm both symptoms reduce to one cause.
- [x] Weigh three fix shapes, including the ones rejected.

## Result

### The mechanism

A webview reload destroys all JS state; the Rust process and its PTY sessions are untouched. On re-boot the frontend asks the backend which tabs exist — and the backend has never known anything about them beyond their id and whether the shell is alive.

`PtyTabInfo` (`src-tauri/src/pty.rs:553-558` pre-fix) carried exactly `{ id, alive }`. The module's own doc comment at `pty.rs:38` described the backend as deliberately **"scope-blind"**. So `adoptTabs` (`src/store/terminalTabsStore.js:139-143` pre-fix) had nothing to rebuild from and hardcoded the gap:

```js
terminalTabs.value = list.map((t) => ({ id: t.id, title: `Shell ${t.id}`, projectId: null, cwd: null }))
```

Every surviving shell therefore became a global, placeholder-titled, unpinned tab. Both reported symptoms fall out of that one line:

- **Badges** — `src/components/TerminalScopeButton.vue:32` counts tabs by `projectId`; with every `projectId` forced to `null`, each project reads 0 and the global scope absorbs everything.
- **Pin icon** — `src/components/TerminalTabStrip.vue:13-20` renders on `t.pinned`, a field the mapping above does not even emit, so it is `undefined` for every tab.

The owner's reading was correct and this is the load-bearing sentence: **the ownership link was never persisted anywhere that could survive a reload.** It only ever existed in the JS `terminalTabs` ref. There was no drift, no race, and nothing to repair at display time — the durable record simply did not exist.

### Verification

Read directly at both ends, so no runtime test was needed to distinguish candidate causes: the Rust struct definition (what the backend can possibly return) and the JS adopt mapping (what the frontend does with it). The two together are sufficient — if the backend never carried the field, no frontend logic could have restored it.

Not verified here, and not verifiable on this machine: whether the native WKWebView context menu is reachable in the **shipped release build** or only in a dev build. No suppression was found in `tauri.conf.json`, but the app cannot be built on the Linux dev box. This affects only how easy the bug is to *trigger*, not the mechanism — a reload from any source (`⌘R`, devtools, a relaunch) produces the identical loss.

### State table (pre-fix)

| State | Where it lived | Survived a webview reload? |
|---|---|---|
| tab `id` | Rust | Yes |
| shell alive | Rust | Yes |
| `projectId` (ownership) | JS only | **No** — reset to `null` |
| `title` | JS only | **No** — reset to `Shell {id}` |
| `pinned` | JS only | **No** — field not emitted at all |
| `cwd` | JS only | **No** |
| `titleLocked`, `runKind`, `pendingCmd`, `resizeOwner` | JS only | **No** |

## Decision

**Action — make the Rust side the durable record for the facts that must outlive a reload.** A `TabMeta { project_id, title, pinned }` map in `PtyState`, upserted with PATCH semantics (a `None` field says nothing rather than erasing), returned by `pty_list_tabs`, and rehydrated by `adoptTabs`. Landing in the global scope with a placeholder title is now the *degenerate* case for a genuinely unowned shell, not the blanket outcome for every tab. Shipped in the same session; see CHANGELOG `[Unreleased]`.

Two alternatives were considered and **rejected**:

- **Suppress the native context menu** so the mis-click cannot happen. Rejected as a fix (it may still be worth doing as a UX nicety): it hides one trigger while leaving the state amnesiac, and every other reload path — `⌘R`, devtools, a relaunch — reproduces the bug untouched. This is the "stack another guard on a weak path" shape `pattern.A8` exists to refuse.
- **Persist ownership in `localStorage`, reconciled against `pty_list_tabs` on boot.** Rejected on a concrete failure, not on taste: tab ids are reused by `nextTabId()`, so a stale entry can attach an old project to a brand-new unrelated shell. It also creates a second source of truth about ownership alongside the backend's live tab set (`pattern.A1`).

**Reopen trigger:** if a tab ever becomes movable between projects, `TabMeta.project_id`'s "set once at first spawn, never reassigned" comment stops being true and the upsert path must be revisited. Likewise if tab metadata ever needs to survive an app *restart* (not just a reload) — `TabMeta` is in-memory only, which is deliberate: a tab whose shell is gone with the process has nothing to own.

## Cross-refs

- `docs/feat/in-app-terminal.md` — the feature this bug lives in.
- `docs/plan/backlog.md` B16 — the backlog entry this closes.
- `src-tauri/src/pty.rs` — `TabMeta` and the lock-order comment it extends (it is a leaf lock, like `min_accepted`).
- `CLAUDE.md` "Regression Guard - Multi-entity State" — the rule that shaped the PATCH-not-replace upsert and the id-scoped rename/pin commands.
