# The three pinned notes — terminal tab identity, entry points, and strip width

**Start time:** 2026-08-22

## Initial purpose

The project's own task file (`.akidevsync/notes.json`, the per-project notes this app writes into every repo it manages) holds 47 tasks, 8 open, **3 pinned**. All three are terminal items, filed 2026-08-21:

1. **UX — one button, two intents.** A project's terminal button should *reuse* that project's tab when it already has one, but the OPEN popup's "In-App Terminal" item should always open a **new** tab. Owner's constraint, verbatim: "không chắp vá code, không redudant guard, không quá nhiều overlap/redudant check/if…".
2. **Tab strip max-width.** A deliberately renamed tab gets truncated by a fixed narrow width; and once tabs outnumber the strip there must be horizontal overflow scrolling, "như cách mà vscode làm".
3. **CRITICAL — reload flow is wrong.** An accidental right-click → Reload left every shell counted as the *general* (global) group instead of its project, the pin icon gone, and the terminal-button badges wrong. Owner's framing: this is not a UI mismatch but a *"lệnh biểu hiện"* — a wrong symptom proving a wrong root, so the origin of the detect-and-store flow must be examined, not the symptom patched.

This doc answers "what is actually broken and where", not "which patch to write". No code was changed.

## Strategy

Read the live source as the source of truth (`coding.A3`) along the whole tab lifecycle rather than at the three symptom sites: where a tab record is created, where it is stored, what the Rust side knows about it, and what survives a webview reload. Item 3 is treated as the root question and items 1–2 as its neighbours, because all three touch the same record.

## Checklist

1. Read `src/store/terminalTabsStore.js` — the tab record's shape and every mutation of it.
2. Read `src-tauri/src/pty.rs` `PtyTabInfo` / `pty_list_tabs` — what the backend knows about a tab.
3. Trace host boot: `initTerminalTabs()` → `pty_list_tabs` → `adoptTabs`.
4. Trace both entry points into a project's terminal (`TERM` cell, OPEN popup) down to the shared function.
5. Read the strip's CSS chain: `.tab` → `.tab-group` → `.terminal-title` → `.terminal-header`.

## Result

### F1 — reload wipes tab identity, because identity is stored in the wrong process (item 3)

`adoptTabs` (`src/store/terminalTabsStore.js:140`) is the whole of it:

```js
terminalTabs.value = list.map((t) => ({ id: t.id, title: `Shell ${t.id}`, projectId: null, cwd: null }))
```

Every surviving shell comes back as an unnamed, unpinned, project-less tab. `projectId: null` **is** the global group (`scopeOf` = `tab.projectId || GLOBAL_SCOPE`), so the observed "mọi shell bị tính thành cái shell general" is not a display bug — the record genuinely says global now. `pinned`, `titleLocked`, `runKind` and `cwd` are dropped in the same line, which is exactly the pin icon and the badge counts the owner saw go wrong (the `TERM` cell's cyan badge counts tabs whose `projectId` matches that project — after adoption, none do).

`adoptTabs` cannot do better with what it is handed. `pty_list_tabs` returns `PtyTabInfo { id, alive }` (`src-tauri/src/pty.rs:553-558` (the doc comment plus the two fields)) and nothing else, because the backend never knew a tab had a project: `pty_spawn` takes only `tab_id` and `cwd`, and `cwd` is honoured once, at first spawn, then forgotten.

**Root:** the tab record lives in the webview's memory, while the thing it describes — the PTY, its scrollback ring, its threads — lives in the Rust process. Those two have different lifetimes, and every reload (⌘R, the WebView context menu's Reload, a devtools reload, a renderer crash) destroys the shorter one while the longer one survives. Nothing else in this file is at fault; `terminalTabs` is mirrored host→companion, so a paired phone is degraded by the same event rather than being a second copy that could restore it.

**The shape that removes the class:** state whose subject is the PTY belongs where the PTY lives. If the record (`projectId`, `title`, `titleLocked`, `cwd`, `runKind`, `pinned`) is held in `PtyState` alongside the session and returned by `pty_list_tabs`, then `adoptTabs` restores it verbatim and there is no second source to keep in sync. Lifetimes then match by construction: kill the tab and the record dies with it; quit the app and both are gone, so no disk file and no migration are needed. That is a different fix from persisting tab metadata to `~/.aki/devsync/` — persisting to disk would *outlive* the shells and reintroduce reconciliation (a stored record for a PTY that no longer exists), which is the very coupling being removed.

Blocking Reload in the context menu is a mitigation, not this fix: it narrows one trigger and leaves a renderer crash producing the identical loss.

### F2 — the two entry points are already one function with the switch it needs (item 1)

- `TERM` cell → `TerminalScopeButton.vue:42` → `openProjectTerminal(project)`
- OPEN popup "In-App Terminal" → `ProjectTable.vue:178` → `openProjectTerminal(p)` — the same call

`openProjectTerminal` (`useTerminalTabs.js:272`) delegates to `openScopeTerminal(scope, { title, cwd, expandStack: true })`, whose signature (`useTerminalTabs.js:154`) already carries `reuse = true`, and whose reuse branch is a single `if`. `newTab()` (⌘T / `+`) already passes `reuse: false` for exactly this reason.

So item 1 needs no new guard, no branch inside the shared path, and no duplicated function body — only a second, honestly-named entry that passes `reuse: false`, called from the popup. The existing reuse path (switch group, re-focus the remembered tab, never `cd`) stays untouched, which is the "như cũ" half of the request. One consequence worth stating rather than discovering later: with `reuse: false` the popup click is subject to `capReached` (5 tabs per project), so at the cap it toasts and opens nothing — correct, and the same behaviour ⌘T already has.

### F3 — the strip cannot scroll, and it cannot scroll for a reason two levels above it (item 2)

Chip sizing is `flex: 1 1 84px; min-width: 84px; max-width: 160px` (`TerminalTabStrip.vue:141-143`), with `.tab-title` ellipsising (`:209-213`). `160px` is the truncation the owner hit on a renamed tab.

The overflow half is not in that file. `.tab-group` (`:120-125`) has `min-width: 0` but no `overflow-x`, and above it `.terminal-title` (`src/assets/main.css:809`) is a flex item with **no `min-width: 0`**, i.e. default `min-width: auto`, so it refuses to shrink below its content. Inside `.terminal-header`'s `justify-content: space-between`, an over-long strip therefore pushes rather than scrolls, and `.dock-stack { overflow: hidden }` clips whatever leaves the box. Adding `overflow-x: auto` to `.tab-group` alone would not produce a scrollbar — the ancestor never gives it a constrained width to overflow within.

The minimal chain is therefore three properties, not one: `min-width: 0` (and a `flex` basis) on `.terminal-title`, `overflow-x` on `.tab-group`, and a wider `max-width` on the chip. Only the last needs a judgment call, and the data to make it conditional already exists on the record: `titleLocked` is set exactly when the user renamed the tab by hand (`renameTerminalTab`, `auto: false`), so "wider only for the ones that were deliberately named" is expressible as a class on the chip rather than a blanket widening that costs every unnamed tab its narrowness. A visible scrollbar in a `--control-h - 4px` row would violate Extreme Narrow (`CLAUDE.md`); hiding it while keeping wheel/trackpad scroll is the VS Code behaviour being asked for.

## Verification

Static reading of the current tree at `73a0b71` (`coding.B3` — the properties above are fully determined by visible code and CSS cascade; no runtime tier was needed to establish them). `notes.json` counts were read mechanically from the file, not from the UI. Nothing here was measured on a running app: what a fix *feels* like — scroll ergonomics in F3, and confirming F1's restoration after a real ⌘R — is runtime judgment and belongs to whoever implements it, on the Mac.

Corroborating links:

- `docs/arch/terminal-stack.md` — the scope/group architecture these three items sit inside.
- `docs/feat/in-app-terminal.md` § Groups — the documented contract for pinning, per-group caps and the two entry points, which F2 and F1 are measured against.
- `docs/plan/done/wish-terminal-manual-resize-authority.md` — precedent for a per-tab field (`resizeOwner`) riding the same record F1 says should move.

## Decision

**Follow-up work, in this order** — F1 first: it is the only one that loses user state, and it is also the one whose fix moves the record F2 and F3 both read.

1. **F1 (critical, root fix).** Move the tab record into `PtyState`, return it from `pty_list_tabs`, restore verbatim in `adoptTabs`. Rust + JS, one plan doc; touches the multi-entity guard (`CLAUDE.md`) so the ≥2-entity check applies — verify with two projects' groups plus a pinned tab, reload, and confirm each group keeps its own tabs.
2. **F2 (small).** Give the OPEN popup a `reuse: false` entry point; leave `openProjectTerminal` as the reuse path.
3. **F3 (small, CSS only).** `min-width: 0` on `.terminal-title`, `overflow-x` on `.tab-group` with the scrollbar hidden, wider `max-width` for renamed (`titleLocked`) chips only.

**No action** on blocking the WebView Reload menu item — F1 makes it unnecessary, and on its own it would hide the defect rather than remove it.

Cross-references: `docs/plan/backlog.md` (not yet amended — these three are tracked in `.akidevsync/notes.json` as pinned tasks); `docs/arch/terminal-stack.md` (must be updated by F1, since it documents where tab state lives).
