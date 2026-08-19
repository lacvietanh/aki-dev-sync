# Plan: make terminal copy actually work (`⌘C`, and a selection that survives)

Root cause record: `docs/research/terminal-copy-selection-root-cause.md`. Supersedes backlog B10 (`docs/plan/backlog.md`), whose "not fixable in this app's code" verdict is overturned by finding F1.

## 1. What is broken, in one line each

| # | Defect | Root cause (evidence in the research doc) |
|---|---|---|
| 1 | `⌘C` never copies, with or without a mouse-mode TUI — the clipboard is not touched | xterm's only copy route is a native `copy` DOM event; it mirrors the selection into its textarea for **Linux primary-selection only**, so in WKWebView the event never has anything to fire on (F1) |
| 2 | An Option-drag selection vanishes on mouse release under `claude` over SSH | xterm's `set activeProtocol` fires `onProtocolChange` on **every** assignment, including a redundant re-arm of the mode already active, and the handler answers it with `selectionService.disable()` → `clearSelection()` (F2) |
| 3 | `⌘⇧[` / `⌘⇧]` cycle tabs once, then stop | `TerminalStack.vue:150` gates on a `focusin`/`focusout` flag; the incoming tab's `term.focus()` runs at `pre` flush, before `v-show` reveals it, so focus is dropped and the flag stays `false` (F4) |

Item 1 is the one the owner actually needs. Item 2 is what makes the app *look* broken while item 1 is fixed. Item 3 is unrelated, reported the same day, and lives in the same two files.

## 2. Item 1 — an app-owned copy path

New composable `src/composables/useTerminalCopy.js`, attached per `TerminalView` instance after `term.open()`.

- **Stash the selection as it is made.** `term.onSelectionChange(...)` → `const s = term.getSelection(); if (s) stashed = s`. Never overwrite with an empty string: the whole point is that the selection is wiped moments after it exists, and the wipe arrives as an empty-selection change event.
- **Claim `⌘C`.** Capture-phase `keydown` on `term.element`: `ev.metaKey && !ev.ctrlKey && !ev.altKey && ev.key === 'c'` → copy `term.getSelection() || stashed`, then `preventDefault()` + `stopPropagation()`. If both are empty, do nothing and let the event through — behaviour with no selection is unchanged (today it does nothing; it must not start sending anything to the PTY).
- **Write through `copyText()`** (`src/utils/clipboard.js`) — the same path every COPY button in this app already uses successfully in this same webview. Do not re-implement a clipboard write (`pattern.A1`).
- **The stash is per tab and is not cleared.** Rule the user can hold: *`⌘C` copies the last text you selected in this terminal tab.* Clearing it on the next `mousedown` would mean a selection that xterm wiped is copyable only until the user clicks anywhere — which is the exact failure being fixed.
- Colliding claims checked: xterm does not cancel `⌘C` (`_keyDown` returns without `cancel()` for `⌘`+letter on macOS), `useTerminalTextDrain.js`'s `customKeyEventHandler` only vetoes `keypress` and the bare F-keys, and `TerminalStack.vue`'s window listener ignores `c`. No existing handler loses an event.

## 3. Item 2 — stop the redundant re-arm from clearing the selection

Same composable (one owner for "selection → clipboard"):

- Wrap the instance's `coreMouseService.activeProtocol` setter so that assigning the protocol **already active** is swallowed; a genuine change still goes through untouched. This is deliberately the narrowest possible intervention: the damage from a re-arm is only `clearSelection()`, and `macOptionClickForcesSelection` already lets a forced selection start while the service is disabled.
- This reaches `term._core.coreMouseService`, a private field of a vendored library. Feature-detect it (getter + setter present on the prototype), no-op if the shape differs, and record that state — an xterm upgrade must degrade to today's behaviour, never throw.
- **It may not be enough, and that must be measurable.** If `claude` disarms and re-arms (`DECRST` then `DECSET`) rather than re-arming in place, the change is genuine and the selection still dies — item 1 covers the user either way. So expose a pull-based diagnostic (precedent: `docs/research/terminal-vietnamese-ime-root-cause-2.md`): `window.__akiTermCopy.status()` returning at least `{ instances, available, protocolChanges, rearmSuppressed, stashLength }`, plus `help()`. Recorded always, read on demand — no flag to arm ahead of time.

## 4. Item 3 — the focus gate

- `src/components/dock/TerminalStack.vue`: add a template ref to `.terminal-mount-wrap`, delete the `hasTerminalFocus` ref and the `@focusin`/`@focusout` handlers, and replace both readers (`:137` collapse-blur, `:150` shortcut gate) with a live check — `mountWrapEl.value?.contains(document.activeElement)`. Asking the DOM at the moment of the keypress cannot go stale, which a flag maintained by two events with an ordering dependency can (`pattern.A8` — reshape the flow instead of guarding a weak one).
- `src/components/TerminalView.vue:311`: the `props.active` watcher focuses via `nextTick(() => term?.focus())` so the call lands after `v-show` has revealed the element. `nextTick`, not `flush: 'post'` — post-flush watchers and directive hooks share one queue, and this must be unambiguously after the DOM is painted.

## 5. Out of scope

- The mouse-reporting lock toggle proposed in B10. It fixes only the selection half, breaks scroll and click inside the TUI while on, and needs new UI in an app under the Extreme-Narrow rule. Closed, not deferred.
- `⌘C` sending `SIGINT` when there is no selection (raised in `docs/plan/done/terminal-ime-input-layer-separation.md` §247). Never implemented, not part of this fix, no behaviour change either way.
- Copy-on-select (auto-copy the moment a selection exists). It would also work, and it silently overwrites the clipboard on every stray drag. Rejected in favour of the stash, which keeps `⌘C` explicit.

## 6. Verify

Static — settled by reading the diff, no runtime needed (`coding.B3`):
- [ ] No existing key handler loses an event: the `⌘C` claim is capture-phase on `term.element`, and every other claimant is checked in §2.
- [ ] `copyText()` is reused, not reimplemented; no second clipboard code path.
- [ ] The protocol wrapper is feature-detected and no-ops on shape mismatch; nothing throws when `_core` changes.
- [ ] `hasTerminalFocus` has no remaining readers; both call sites use the live DOM check.
- [ ] `npm run typecheck` — only if `package.json` actually defines it; this is JS/Vue with no type surface added, so its absence is not a gap.

Runtime — one batch, on the owner's Mac, after the build:
- [ ] In an SSH terminal running `claude`: select with `⌥`-drag, release, press `⌘C` → the text is in the clipboard, **whether or not the highlight survived**. This is the one that closes the owner's report.
- [ ] Same, without Option and without a TUI (plain shell output): select, `⌘C` → clipboard has it.
- [ ] Does the highlight now survive the release? Then run `__akiTermCopy.status()` in the Safari inspector attached to the app (target `localhost`, **not** `Main.html` — that is the inspector's own UI) and record `rearmSuppressed` / `protocolChanges`. A non-zero `rearmSuppressed` confirms F2's first mechanism; zero with the wipe still happening means it is one of the other two, recorded as a follow-up rather than guessed at.
- [ ] `⌘⇧]` pressed three times in a row cycles three tabs, without clicking the terminal in between.
