# wish — Terminal Split: TerminalView (Mac) + SimpleView (Phone PWA)

> **Status:** SimpleView — ✅ built, shipping in 1.23.0 (see `CHANGELOG.md`; SV-1..SV-7 below).
> Right-Dock Mac — still unbuilt, gated behind its own preconditions (see "Right-Dock Mac" section)
> and not yet started. This doc stays in `docs/plan/` (deliberately not moved to `done/`) while its
> Right-Dock milestone remains live wishlist content. Decisions are settled as of
> `docs/research/audit-terminal-split-wish.md` (2026-08-01), which corrected one broken SimpleView
> decision and four factual errors in an earlier draft of this doc — see SV-1..SV-7 and RD-1..RD-4
> below. Do not start SimpleView and
> Right-Dock as one PR: they remain independent milestones, and neither is a prerequisite for the
> other.
> Written 2026-08-01, Right-Dock section expanded 2026-08-01, corrected against audit 2026-08-01.
> Not a blocker for any current release.

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
- `TerminalView.vue` — zero diff required by this change. FACT (`docs/research/audit-terminal-split-wish.md`
  Finding 3): it is NOT Mac-only today — `TerminalStack.vue` mounts it with no host/companion branch,
  and `usePtyTerminal.js:590`'s `showKeyRow = !isHost` feeds a companion key row (`TerminalView.vue:29-66`)
  and a companion compose row (`TerminalView.vue:78-~100`). This change makes it Mac-only as a side
  effect: once `<component :is>` stops mounting `TerminalView` on companion, those branches become
  unreachable dead code, owned by a later cleanup — along with `usePtyTerminal.js:590`, and
  `usePtyTerminal.js:228`/`:404`/`:472` (flagged, not individually confirmed). Nothing breaks in the
  meantime: `tabLiveness` is module-scope (`usePtyTerminal.js:25-92`), independent of any `TerminalView`
  mount; `activatedTabs` reconciliation (`useTerminalTabs.js:260-267`) runs off the shared store; and
  `ptyBridge.js:140`'s relay is `isHost`-gated and per-tab.
- `useTerminalTextDrain.js` — Mac-only drain, untouched.

### File map — what changes

| File | Change |
|---|---|
| `src/utils/ptyCodec.js` | **New** — extract `decodeBase64ToBytes` / `encodeBytesToBase64` from `usePtyTerminal.js` |
| `src/utils/ansiStrip.js` | **New** — incremental stateful parser carrying a pending escape/`\r` tail across calls (SV-1), not a pure function |
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
  const lines = ref([])        // committed lines, capped at 2000 entries
  const buffer = ref('')       // in-progress (uncommitted) line — not yet pushed to `lines`
  const alive = ref('unknown') // 'unknown' | true | false (same tri-state as usePtyTerminal)

  function sendRaw(str) { /* send FRAME_PTY_INPUT */ }

  async function start() {
    // 1. subscribe FRAME_PTY_OUTPUT + FRAME_PTY_EXIT via onFrame() FIRST, so no live frame is lost
    //    while step 2 is in flight — but queue (do not feed to the parser) any live frame that
    //    arrives during the await, since the parser is stateful and order-sensitive
    // 2. call invoke('pty_get_scrollback', { tabId }) for replay
    // 3. decode + feed the scrollback replay through the buffer/control-behaviour processor first
    //    (see ANSI stripping), populating lines
    // 4. THEN drain the queue from step 1 through the same processor, in arrival order
  }

  onBeforeUnmount(() => { /* unsubscribe */ })

  return { lines, alive, sendRaw, start }
}
```

FACT (LEAD ruling #1, `2026.08.01-1834-review-terminal-split-wish/chat.md` turn #44): this composable
now holds a small amount of buffer/line state — a step up from a pure stripper — to make redraw-in-place
output legible instead of garbled. See "ANSI stripping" below for the exact rule and its hard boundary.

**Replay-vs-live ordering (serious defect, fixed 2026-08-01):** subscribing before awaiting the
scrollback fetch is correct — it avoids a lost-frame gap — but since the parser is stateful and
order-sensitive, a live frame arriving during that await must not be fed to the parser immediately: it
would commit before the chronologically-earlier scrollback and produce visibly out-of-order output.
Live frames arriving during the await are queued and flushed, in arrival order, only after the
scrollback replay has been fed to the parser.

**Does not:**
- Import `@xterm/xterm`
- Call `hostResize` / `pty_resize`
- Read `isHost` (companion-only by design)
- Subscribe Tauri native events (companion has no Tauri bridge)
- Implement columns, cursor addressing (`\x1b[<r>;<c>H`), scroll regions, or any grid geometry — see
  the hard boundary in "ANSI stripping"

---

## ANSI stripping — supersedes the original "strip everything" spec

FACT (`docs/research/audit-terminal-split-wish.md` Finding 1, LEAD ruling #44 DECISION 1): the
original stripper — delete all ANSI codes, delete `\r`, push straight to an append-only `lines`
array — was implemented verbatim and measured against real `script -q`-captured PTY bytes. It failed
both ways a terminal actually redraws in place: a 20-frame `\r`-only spinner (curl/npm-style)
rendered as **one entry with all 20 frames concatenated** into an unreadable string; a 4-line status
box redrawn 7 times via `\x1b[5A` cursor-up + `\x1b[2K` erase rendered as **37 entries — 7 permanent
duplicate copies** of the box. The regex removed the codes but not their effect: redraw-in-place
output depends on grid state an append-only array doesn't have. The original decision was argued
only as a V1/V2 staging question about deferring SGR colour — legibility of redraw output was never
assessed, by either council that touched it.

`ansiStrip.js` is now an **incremental stateful parser**, not a per-chunk pure function — it carries a
pending tail across calls, not just a one-line write `buffer`. RATIONALE (adversarial review,
2026-08-01): `FRAME_PTY_OUTPUT` chunks arrive over a WebSocket with no alignment to escape-sequence
boundaries, so `\x1b[2K` can arrive as `\x1b[2` in one chunk and `K` in the next. Under a per-chunk
spec neither half matches any rule, the catch-all strip mangles or leaks them as literal text, and
the buffer is never cleared — reproducing exactly the garble this decision exists to eliminate. Rule:
if a chunk ends inside an incomplete escape sequence (an `\x1b` whose terminating letter has not
arrived yet), hold that fragment and prepend it to the next chunk rather than stripping or emitting
it. The same mechanism covers a trailing bare `\r`: defer the bare-`\r`-vs-`\r\n` decision until the
next chunk reveals whether `\n` follows it — otherwise a `\r` that ends a chunk gets the clear-buffer
rule applied prematurely and then commits a phantom blank line when `\n` arrives, inserting blank
entries at every chunk boundary and eroding the 2000-line cap faster than real output would. This is
a distinct, additional layer to the `TextDecoder` byte-level statefulness already in place at
`usePtyTerminal.js:6-12` — that handles split UTF-8 bytes, this handles split escape sequences.

**Pending-tail bound (serious defect, fixed 2026-08-01):** the pending tail cannot be held
unboundedly. A lone `\x1b` that never gets a terminating letter — binary output piped through `cat`,
a process killed mid-sequence, a malformed emitter — would otherwise make the parser hold forever and
append every subsequent byte to a never-resolving sequence, so nothing is ever committed and the
phone renders permanently stale with no visible error. Rule: the pending tail is capped at 64 bytes
(real CSI sequences are far shorter); if it exceeds 64 bytes with no terminator, flush it as literal
text and reset the parser state. That truncation must occur at code-point boundaries and must never
split a UTF-16 surrogate pair, or emoji and astral characters corrupt at the cut point.

`usePtyStream.js` keeps a one-line write `buffer` alongside the committed `lines` array (see
"usePtyStream API" above) and honours four control behaviours instead of deleting them:

| Input | Behaviour |
|---|---|
| `\n` | commits `buffer` to `lines`, clears `buffer` |
| bare `\r` (not part of `\r\n`) | clears `buffer` — the next write overwrites the current line instead of concatenating onto it |
| `\x1b[<n>A` (cursor-up) | drops `min(n, lines.length)` entries from `lines`, so a redraw replaces rather than appends |
| `\x1b[2K` / `\x1b[K` (erase line) | clears `buffer` |

Every other CSI sequence and all SGR colour codes are still stripped — V1 remains plain text, colour
is still deferred to V2 exactly as originally planned. `lines` is capped at 2000 entries to bound
memory.

**Cursor-up clamp (never underflow, corrected 2026-08-01 — the prior wording was self-contradictory):**
`\x1b[<n>A` drops `min(n, lines.length)` entries, never below zero. If `n` meets or exceeds the
current count, `lines` empties — and that is correct, not a defect: a redraw asking to move above
everything currently retained has nothing left to preserve, and inventing a survivor to avoid an
empty array would fabricate output that was never actually emitted. There is no "clamp to the oldest
surviving line" behaviour — `min(n, lines.length)` is the one and only rule.

**Clear-list (what must reset the pending tail):** because the parser is now stateful across calls,
anything that resets `lines` must also clear the pending escape/`\r` tail and the write `buffer`, or
a stale fragment survives the reset and corrupts the first line rendered after it.
- Congestion recovery (`ptyBridge.pushAllScrollbacks` broadcast reset frame).
- `FRAME_PTY_EXIT`. FACT (verified against code): `src/services/ptyBridge.js:170-180` shows
  `FRAME_PTY_EXIT` is a bare notice with no `data` and no `reset` flag, and while the explicit-restart
  path (`src-tauri/src/pty.rs:681`, `pty_restart`) does emit a `reset` frame, the common respawn path
  — typing to restart, via the idempotent `pty_spawn` — emits no reset frame at all. A shell dying
  mid-escape-sequence would otherwise leave a stale tail that corrupts the next shell's first bytes,
  so `FRAME_PTY_EXIT` must independently clear both the pending tail and the write buffer, not rely on
  a reset frame that may never arrive.

See "Regression protection" below.

**Hard boundary (explicit part of this decision, not an implementation detail):** the phone honours
cursor-up, line-erase, and carriage-return ONLY. It never implements columns, cursor addressing
(`\x1b[<r>;<c>H`), scroll regions, or any grid geometry, and it never learns `cols`/`rows` — that is
the whole point of the split from the Mac's PTY grid. If a future need calls for cursor addressing,
that is the signal SimpleView is the wrong shape for that use case, not a licence to grow it into a
second terminal emulator.

**Enforcement mechanism (not CI-grade — this repo has no `.github/workflows` and no commit hook):**
`src/composables/usePtyStream.js` and `src/utils/ansiStrip.js` must contain none of `cols`, `rows`,
`\x1b[H`, any other cursor-addressing pattern, or an `@xterm/xterm` import. Ship this as an npm
script modelled on `scripts/lint-remote-scripts.js` — including its comment-stripping precaution
(`stripCommentLines`, which blanks full-line comments before the regex scan so prose describing the
forbidden pattern, e.g. this doc's own "never learns `cols`/`rows`", cannot self-trip a naive grep).
Run manually (`npm run lint:scripts`-style), the same way `lint-remote-scripts.js` is manual today —
there is no CI in this repo to host either check.

**Accepted scope limitation, not a future fix:** SimpleView targets **line-oriented CLI output** —
spinners, progress bars, log streams, and the cursor-up status boxes actually measured in Finding 1.
Full-screen TUI applications (`vim`, `htop`, `less`) that redraw via absolute cursor addressing
(`\x1b[H`, `\x1b[<n>B/C/D`, save/restore cursor) are explicitly NOT supported and will render
garbled. This ties to the hard boundary above: if someone needs a full-screen TUI on the phone, that
is the signal SimpleView is the wrong shape for that use case, not a licence to grow it.

V2 option (unchanged from the original plan): preserve SGR color codes and render as `<span>`
elements — still deferred, no new information changes this.

---

## SimpleView.vue interface

Props match `TerminalView.vue` exactly so `TerminalStack.vue`'s `v-for` can use either with no change to loop logic. FACT (`docs/research/audit-terminal-split-wish.md` Finding 2, F2.2): `TerminalView.vue:123-127` declares three props, not two — `cwd` was missing from this list, and `TerminalStack.vue` already passes `:cwd` unconditionally:
- `cwd: String` (default `null`)
- `tabId: Number` (default 0)
- `active: Boolean` (default true)

Expose matches `TerminalView.vue` for liveness aggregation:
- `alive` — tri-state ref

Key row and compose row: copy-adapted from `TerminalView.vue`, not extracted into a shared component (SV-4 below).

---

## TerminalStack.vue change

Per SV-3 below, `TerminalStack.vue` never imports `isHost` directly — it asks a capability
composable which component to mount. FACT (`docs/research/audit-terminal-split-wish.md` Finding 2,
F2.1): `useTerminalChrome.js` does NOT exist anywhere in this repo — it was specified in
`docs/plan/done/terminal-chrome-settings.md` §5/AC-3 but never built, despite that doc being filed
under `done/`. The real precedent for this pattern is `usePtyTerminal.js` importing `isHost`
(line 20) and publishing derived capability flags `ownsPtySize` / `showKeyRow`, which
`TerminalView.vue` consumes instead of reading `isHost` itself — described at
`docs/arch/terminal-stack.md:39-41`.

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
| Congestion recovery | `ptyBridge.pushAllScrollbacks` broadcast reset frame → `usePtyStream` rebuilds `lines` AND clears the pending escape/`\r` tail (SV-1) — a reset that only clears `lines` leaves a stale fragment that corrupts the first line rendered after it |
| `FRAME_PTY_EXIT` | Bare notice, no `data`/`reset` flag (`ptyBridge.js:170-180`); the common respawn path via idempotent `pty_spawn` emits no reset frame at all → `usePtyStream` must independently clear the pending escape/`\r` tail and the write `buffer` on EXIT, or a stale tail survives a shell death and corrupts the next shell's first bytes |
| Keystroke / compose | `sendRaw` in `usePtyStream` uses `FRAME_PTY_INPUT` — same path as companion in `usePtyTerminal.js` |
| `FRAME_PTY_RESIZE` | `usePtyStream` ignores it entirely |
| Vietnamese IME | Compose row in SimpleView is native `<textarea>`, no xterm IME trap |
| Tab lifecycle | `TerminalStack`'s `activatedTabs` set unchanged — SimpleView mounts/unmounts on same logic |

---

## Implementation order

```
Step 1  CREATE  src/utils/ptyCodec.js             zero-risk extract
Step 2  CREATE  src/utils/ansiStrip.js             stateful transformer per SV-1 (buffer + control behaviours), not a pure regex
Step 3  CREATE  src/composables/usePtyStream.js    companion-only composable, holds buffer/lines state per SV-1
Step 4  CREATE  src/components/SimpleView.vue      companion view
Step 5  PATCH   src/composables/usePtyTerminal.js  import codec from ptyCodec.js (API unchanged)
Step 6  PATCH   src/components/dock/TerminalStack.vue  <component :is="..."> switch
Step 7  TEST    Mac — xterm mounts, resizes, scrolls, Vietnamese input unchanged
Step 8  TEST    Phone — SimpleView renders, sends input, scrollback replay, liveness badge
```

---

## Right-Dock Mac (separate feature, same wish bucket)

Independent of SimpleView and not a prerequisite — still true, and still the right way to scope it
(SimpleView can ship without this, and this can ship without SimpleView).

**Expanded 2026-08-01 — this is no longer just "move the terminal dock to the right side."** The
owner's actual ask is a full app-shell reflow on Mac:

- **Main Stack** (new name, owner-specified) — the existing top area (`AgentUsageSection` +
  `ProjectTable`) becomes a full-height left column instead of a fixed-height header block above the
  bottom dock. Width is fluid/responsive with `max-width: 900px` as a ceiling (owner-specified) — not
  a fixed narrow rail, and not the 400px this plan's own first draft guessed before being corrected.
- **Terminal** takes the entire remaining width on the right, full viewport height (not a bottom
  panel sharing space with LogStack anymore).
- **Vertical tabs** for switching between projects' terminal groups — a different axis than the
  existing horizontal `TerminalTabStrip.vue` (which switches tabs *within* one group); this is
  switching *between* groups/scopes, visually.

This is a bigger lever than the original one-line sketch above (`dockSide: 'bottom' | 'right'` on
`useDockLayout.js`) — it reflows the whole shell (`AppConsole.vue`'s `.dashboard-top` +
`.dashboard-bottom` split), not just where the terminal dock panel sits within the existing bottom
strip. The four bullets in the original sketch (dockSide ref, CSS row-split, AppConsole flex-direction
support, auto-expand window) are superseded by this, not additive to it.

### Decisions RD-1..RD-4 (resolved 2026-08-01)

These decisions resolve the four Right-Dock questions without changing the already-settled
SimpleView contract. They describe the target interaction model; implementation still needs its own
separate milestone and verification pass.

RD-1. **Use a nonmodal, on-demand LogInspector.** Global Activity (renamed "Activity" — see SV-6) is a keyboard-operable item in the existing app-icon dropdown; per-project LOG opens the same inspector in project context. It has no backdrop/focus trap and preserves terminal mounts, active tab, and live output. Exit path: SV-5.
RD-2. **Companion navigation moves from project list → focused SimpleView detail → back.** The phone never mounts TerminalStack/xterm; project actions retain confirmation/state; global activity is an in-stream selector, not desktop inspector/modal. SimpleView and Right-Dock remain independent.
RD-3. **A fixed-width vertically scrollable rail sits between Main Stack and terminal content.** It lists Global then non-empty project groups, featuring active/count/exited overlays, accessible name/tooltips, and Up/Down/Home/End/Enter/Space roving focus. Selection restores remembered tab with no spawn. `TerminalTabStrip` stays intra-scope. `TERM`, global TERM, and OPEN remain create/direct-jump routes, and a project appears after its first tab exists.
RD-4. **Expand window on activation, no OS minimum enforcement.** Do not add `core:window:allow-set-min-size`. `useAppWindow.js` already has `setSize()`/`setPosition()` working correctly via Cmd+1/Cmd+2. Triggering right-dock simply expands the window (e.g. max 1920px, min auto-calculated from content) without OS-level capability gates.

### Preconditions before Right-Dock starts (LEAD ruling #44, DECISION 5 — sequencing)

FACT (`docs/research/audit-terminal-split-wish.md`, "Recommended sequencing"): SimpleView ships
first and alone — it fixes a bug (`FRAME_PTY_RESIZE` coupling) and is cheaply falsifiable. Right-Dock
is ergonomic, not a bugfix, and far less reversible (window min-size enforcement, `AppHeader`
accessibility, a breakpoint rewrite) — it does not start until these are met:

1. The LogInspector trigger (RD-1) given a keyboard-reachable path — `AppHeader.vue`'s
   `.icon-dropdown` currently opens on `:hover` only (`:688`), which is not a keyboard path.
   *(Note: The previous preconditions regarding RIGHT_DOCK_MIN_WIDTH and core:window:allow-set-min-size have been removed. Activating right-dock will just expand the window directly, with no toggle-disable logic if the screen isn't wide enough).*

---

## Decisions SV-1..SV-7 (resolved 2026-08-01, SV-1 corrected 2026-08-01)

SV-1 was originally settled, then broken by measurement and superseded. SV-5..SV-7 were silent
assumptions in the original draft, made explicit as named decisions per LEAD ruling
(`2026.08.01-1834-review-terminal-split-wish/chat.md` turn #44). Source for all corrections:
`docs/research/audit-terminal-split-wish.md`.

SV-1. **ANSI rendering: redraw-aware buffer, not a pure stripper (supersedes the original "V1 = strip
   everything" call).** FACT (measured against real captured PTY bytes): the original stripper turned
   a 20-frame `\r`-only spinner into one unreadable concatenated entry, and a 7-tick redrawn status
   box into 37 duplicate entries. See "ANSI stripping" above for the full rule (one-line write buffer,
   four honoured control behaviours, 2000-entry cap) and its hard boundary (cursor-up/line-erase/`\r`
   only — no columns, no cursor addressing, no grid geometry, never learns `cols`/`rows`). Colour
   (SGR) is still deferred to V2 exactly as originally planned — that half of the original MVP/YAGNI
   call was never wrong, only the legibility half was unassessed.
SV-2. **Right-Dock Mac: separate milestone, gated behind its own preconditions.** The doc's own
   "Regression protection" and "Implementation order" sections already scoped it out; LEAD ruling #44
   DECISION 5 sharpens this into an explicit sequencing call: SimpleView ships first and alone, because
   it fixes a bug and is cheaply falsifiable, while Right-Dock is ergonomic and far less reversible.
   See "Preconditions before Right-Dock starts" above.
SV-3. **ENV-1: no exception — add `useTerminalViewType.js`.** The ENV-1 invariant text lives only at
   `docs/plan/done/remote-control.md:484` (verified by grep — `docs/feat/remote-control.md` does not
   contain the string "ENV-1" anywhere, despite being the live feature doc for the companion). It is
   stated as absolute and grep-checkable ("must never appear in `src/components/**`... than the
   boundary modules named"). FACT-corrected precedent (`useTerminalChrome.js` does not exist — see "TerminalStack.vue
   change" above): the real precedent is `usePtyTerminal.js` importing `isHost` (line 20) and
   publishing derived flags `ownsPtySize` / `showKeyRow`, which `TerminalView.vue` consumes instead of
   reading `isHost` itself (`docs/arch/terminal-stack.md:39-41`). "Layout-routing, not terminal logic"
   doesn't earn an exception ENV-1 doesn't grant to anything else — the whole point of the rule is that
   the seam decides, not the call site. See the updated `TerminalStack.vue change` section above.
SV-4. **Compose row: copy-adapt, do not extract a shared component yet.** Rule of Three (`design.A2`) —
   this is the second consumer, not the third, and the two are not drop-in identical: `SimpleView`'s
   compose row has no PTY-resize coupling and no xterm IME trap to work around, which is exactly the
   part of `TerminalView.vue`'s version that carries the most complexity. Forcing a shared
   `PtyComposeRow.vue` now would abstract over that difference before it's clear which parts are
   truly common. Revisit extraction if a third view type (or a real duplicated-bugfix pain point
   between the two) ever shows up.
SV-5. **LogInspector exit (closes a gap RD-1 left open).** Three ways out, all cheap: Esc closes it
   while it holds focus; a click outside closes it; re-selecting the same dropdown item toggles it
   shut. No backdrop and no focus trap, so RD-1's nonmodal promise is intact — a nonmodal panel needs
   an exit that does not steal focus, and this adds no new DOM chrome (Extreme Narrow unaffected).
SV-6. **Naming: "Global" vs "Activity".** The RD-3 rail keeps "Global" — it names a terminal scope
   (tabs with no project). The app-icon dropdown item (RD-1) becomes "Activity" — it names an event
   feed. Per `RULE-design-core.md` A7 (name by role), these are different domains and must not share a
   word; only the shared label was wrong, not either surface.
SV-7. **Phone tab scope: V1 is one tab per project (named limitation, not an implied default).** V1 is
   explicitly one tab per project on the phone — SimpleView shows that project's active tab.
   Multi-tab switching on the companion is deferred to V2. This was previously only implied by `tabId`
   defaulting to `0` with no phone-side switcher; the silence was the defect, not the choice — it is
   now written down as a named limitation instead of left to be discovered.
