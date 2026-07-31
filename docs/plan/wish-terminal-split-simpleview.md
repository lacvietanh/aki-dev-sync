# wish — Terminal Split: TerminalView (Mac) + SimpleView (Phone PWA)

> **Status:** unbuilt — wishlist plan, not yet scheduled into a release. All four open design
> questions resolved 2026-08-01 (see Decisions below); nothing left blocking implementation except
> someone picking it up.
> Written 2026-08-01. Not a blocker for any current release.

## Goal

Refactor the in-app terminal into two clearly separated views:

1. **TerminalView (Mac)** — keep the existing xterm.js full PTY grid. Extend with a Right-Dock option (vertical split on the right side of the window) and auto-expand the Mac window when activated.
2. **SimpleView (Phone PWA)** — do not mount the xterm.js grid at all. Render as a clean Chat/Text Stream (parsed ANSI/plain text) + a simple compose box for input, completely independent of the Mac's PTY `cols/rows`.

The core motivation: the Mac is the sole PTY resize authority (`T-4`, `docs/feat/in-app-terminal.md`). `FRAME_PTY_RESIZE` carries the Mac's `cols/rows` to every companion, and the companion's xterm calls `term.resize()` against those values — forcing the phone to render a grid shaped by the Mac's viewport. This causes word-wrap breakage, buffer misalignment, and IME preedit conflicts (WebKit's `disableStdin: false` interaction with a PTY-sized canvas).

---

## Root cause

`FRAME_PTY_RESIZE` is the single pain point. The Rust PTY has one `(cols, rows)` — that is always the Mac's viewport. The phone receives this frame and calls `term.resize(cols, rows)`, forcing xterm on the phone to render a grid constrained by the Mac. SimpleView removes this coupling entirely: it consumes the byte stream without caring about grid dimensions.

---

## Architecture

### Data flow — SimpleView

```
[Phone WebSocket] ← FRAME_PTY_OUTPUT (base64, tab_id, reset, alive)
  → usePtyStream.js
      → decodeBase64 → Uint8Array → TextDecoder (UTF-8)
      → stripAnsi(str)
      → lines.value.push(...) or lines.value = [] on reset
  → SimpleView.vue renders: <div v-for="line in lines">

FRAME_PTY_RESIZE  → ignored entirely (no grid, no term.resize call)
FRAME_PTY_EXIT    → alive.value = false (same tri-state as usePtyTerminal)
FRAME_PTY_INPUT   → sendRaw() → send({ t: FRAME_PTY_INPUT, tab_id, data })
```

### What does NOT change

- `src-tauri/src/pty.rs` — **zero Rust changes**. `PtyOutputPayload` format unchanged.
- `src/services/ptyBridge.js` — relay is byte-blind, does not need to know SimpleView exists.
- Wire protocol frames — no new frame types added.
- `terminalTabsStore.js`, `useTerminalTabs.js` — tab state unchanged.
- `TerminalView.vue` — untouched; remains the Mac-only xterm.js mount.
- `useTerminalTextDrain.js` — Mac-only drain, untouched.

### File map — what changes

| File | Change |
|---|---|
| `src/utils/ptyCodec.js` | **New** — extract `decodeBase64ToBytes` / `encodeBytesToBase64` from `usePtyTerminal.js` |
| `src/utils/ansiStrip.js` | **New** — pure `stripAnsi(str)` function |
| `src/composables/usePtyStream.js` | **New** — companion-only stream composable (no xterm import) |
| `src/components/SimpleView.vue` | **New** — companion terminal view |
| `src/composables/useTerminalViewType.js` | **New** — ENV-1 boundary module; the only place this feature imports `isHost`, exposes which component to mount |
| `src/composables/usePtyTerminal.js` | **Minor** — import codec from `ptyCodec.js` instead of inline |
| `src/components/dock/TerminalStack.vue` | **Minor** — `<component :is="...">` switch host↔companion, driven by `useTerminalViewType.js` |

---

## usePtyStream API

```js
// src/composables/usePtyStream.js
export function usePtyStream(tabId = 0) {
  const lines = ref([])        // stripped text lines
  const alive = ref('unknown') // 'unknown' | true | false (same tri-state as usePtyTerminal)

  function sendRaw(str) { /* send FRAME_PTY_INPUT */ }

  async function start() {
    // 1. subscribe FRAME_PTY_OUTPUT + FRAME_PTY_EXIT via onFrame()
    // 2. call invoke('pty_get_scrollback', { tabId }) for replay
    // 3. decode + strip + populate lines
  }

  onBeforeUnmount(() => { /* unsubscribe */ })

  return { lines, alive, sendRaw, start }
}
```

**Does not:**
- Import `@xterm/xterm`
- Call `hostResize` / `pty_resize`
- Read `isHost` (companion-only by design)
- Subscribe Tauri native events (companion has no Tauri bridge)

---

## ANSI stripping

```js
// src/utils/ansiStrip.js
const ANSI_RE = /\x1b(\[[0-9;?]*[A-Za-z]|[()][AB012]|\][^\x07\x1b]*(\x07|\x1b\\)|[=>])/g

export function stripAnsi(str) {
  return str.replace(ANSI_RE, '').replace(/\r/g, '')
}
```

V1: strip everything (plain text). V2 option: preserve SGR color codes and render as `<span>` elements — deferred.

---

## SimpleView.vue interface

Props match `TerminalView.vue` exactly so `TerminalStack.vue`'s `v-for` can use either with no change to loop logic:
- `tabId: Number` (default 0)
- `active: Boolean` (default true)

Expose matches `TerminalView.vue` for liveness aggregation:
- `alive` — tri-state ref

Key row and compose row: copy-adapted from `TerminalView.vue`, not extracted into a shared component (Decision #4 below).

---

## TerminalStack.vue change

Per Decision #3 below, `TerminalStack.vue` never imports `isHost` directly — it asks a capability
composable which component to mount, the same pattern `useTerminalChrome.js` already uses for
chrome visibility (`docs/plan/done/terminal-chrome-settings.md` §5).

```diff
+ import SimpleView from '../SimpleView.vue'
+ import { useTerminalViewType } from '../../composables/useTerminalViewType'
```

```diff
+ const { ViewComponent } = useTerminalViewType() // TerminalView on host, SimpleView on companion
```

```diff
  <template v-for="t in tabs" :key="t.id">
-   <TerminalView
+   <component
+     :is="ViewComponent"
      v-if="activatedTabs.has(t.id)"
      v-show="t.id === activeTabId"
      :tab-id="t.id"
      :cwd="t.cwd"
      :active="t.id === activeTabId && !collapsed"
    />
  </template>
```

`useTerminalViewType.js` is the one new boundary module allowed to import `isHost` for this
feature — everywhere else (including `TerminalStack.vue` itself) stays role-agnostic.

---

## Regression protection

| Feature | Protection mechanism |
|---|---|
| PTY Mac — xterm grid | `TerminalView` unchanged; only hidden when `!isHost` |
| Scrollback replay | `usePtyStream.hydrateScrollback` uses same `pty_get_scrollback` IPC |
| Tab liveness | Module-scope `tabLiveness` (usePtyTerminal.js) still covers all tabs; `SimpleView` exposes `alive` ref in same shape |
| Congestion recovery | `ptyBridge.pushAllScrollbacks` broadcast reset frame → `usePtyStream` rebuilds `lines` |
| Keystroke / compose | `sendRaw` in `usePtyStream` uses `FRAME_PTY_INPUT` — same path as companion in `usePtyTerminal.js` |
| `FRAME_PTY_RESIZE` | `usePtyStream` ignores it entirely |
| Vietnamese IME | Compose row in SimpleView is native `<textarea>`, no xterm IME trap |
| Tab lifecycle | `TerminalStack`'s `activatedTabs` set unchanged — SimpleView mounts/unmounts on same logic |

---

## Implementation order

```
Step 1  CREATE  src/utils/ptyCodec.js             zero-risk extract
Step 2  CREATE  src/utils/ansiStrip.js             pure util, zero-risk
Step 3  CREATE  src/composables/usePtyStream.js    companion-only composable
Step 4  CREATE  src/components/SimpleView.vue      companion view
Step 5  PATCH   src/composables/usePtyTerminal.js  import codec from ptyCodec.js (API unchanged)
Step 6  PATCH   src/components/dock/TerminalStack.vue  <component :is="..."> switch
Step 7  TEST    Mac — xterm mounts, resizes, scrolls, Vietnamese input unchanged
Step 8  TEST    Phone — SimpleView renders, sends input, scrollback replay, liveness badge
```

---

## Right-Dock Mac (separate feature, same wish bucket)

Extend `TerminalStack` with a right-dock mode:
- `useDockLayout.js`: add `dockSide: 'bottom' | 'right'` reactive ref
- CSS layout: when `right`, `.dashboard-bottom` switches to a horizontal split (row flex with fixed right column)
- `AppConsole.vue` / root layout: support flex direction change
- Auto-expand Mac window: call Tauri window API when switching to right dock

This is independent of SimpleView and not a prerequisite. Scope separately.

---

## Decisions (resolved 2026-08-01)

All four were architecture/scope calls answerable from this repo's own precedent and stated
principles — none needed a device or the owner's input, so they were settled directly rather than
left open.

1. **ANSI rendering depth: V1 plain text (strip all).** MVP-first / YAGNI (`coding.A2`) — the doc's
   own draft already defaulted here and only deferred SGR-color preservation as "V2 option"; nothing
   about build-log output has been shown to need it yet. Revisit only if a real SimpleView user hits
   unreadable stripped output.
2. **Right-Dock Mac: separate milestone.** The doc's own "Regression protection" and "Implementation
   order" sections already scope it out ("independent of SimpleView and not a prerequisite. Scope
   separately") — this just makes that already-implied answer explicit instead of leaving it phrased
   as an open question.
3. **ENV-1: no exception — add `useTerminalViewType.js`.** `docs/plan/done/remote-control.md`'s ENV-1
   invariant is stated as absolute and grep-checkable ("must never appear in `src/components/**`...
   than the boundary modules named"), and this project already has the exact precedent for this
   situation: `useTerminalChrome.js` is the boundary module that imports `isHost` so
   `TerminalStack.vue`/`TerminalView.vue` never have to (`terminal-chrome-settings.md` AC-3).
   "Layout-routing, not terminal logic" doesn't earn an exception ENV-1 doesn't grant to anything
   else — the whole point of the rule is that the seam decides, not the call site. See the updated
   `TerminalStack.vue change` section above.
4. **Compose row: copy-adapt, do not extract a shared component yet.** Rule of Three (`design.A2`) —
   this is the second consumer, not the third, and the two are not drop-in identical: `SimpleView`'s
   compose row has no PTY-resize coupling and no xterm IME trap to work around, which is exactly the
   part of `TerminalView.vue`'s version that carries the most complexity. Forcing a shared
   `PtyComposeRow.vue` now would abstract over that difference before it's clear which parts are
   truly common. Revisit extraction if a third view type (or a real duplicated-bugfix pain point
   between the two) ever shows up.
