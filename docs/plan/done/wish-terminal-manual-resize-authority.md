# wish — Manual, explicit resize-authority handoff (Direction C, executed)

> **Status:** Design settled 2026-08-11. Steps 1-7 landed in code and shipped in 1.24.0
> (2026-08-12). Step 8, the on-device runtime test (a companion phone taps "Fit to my screen" in
> a plain shell and in a full TUI, then taps the Mac's reclaim pill, confirming authority handoff
> works both ways with no hijack), was confirmed verified on a real Mac+phone on 2026-08-15 —
> tracked as row T6 in `docs/plan/done/verify-pending.md`. All 8 steps of the V1 plan are now
> fully shipped and verified. Supersedes Direction C's sketch in
> `docs/plan/done/restore-terminal-mobile-ux.md` (that doc's Directions A/B are now rejected/closed —
> see its own status update). Companion goes back to mounting `TerminalView` (real xterm.js),
> `SimpleView` becomes unused code, kept in the tree pending a follow-up deletion decision (not
> deleted by this change — see "SimpleView disposition" below).

## Provenance — how this design was reached

This went through two rounds of deep-think (`agy --model gemini-3.1-pro-high`) and one full
adversarial-attack pass (a clean-context reviewer with no knowledge of the reasoning that produced
the first draft), per the owner's explicit instruction for this task. Recorded because the
rejected first draft is instructive, not because process is normally logged here.

**Round 1 (rejected): "alt-buffer gated resize authority."** Idea: let a companion automatically
request the shared PTY's size whenever both its own xterm and the host's xterm observe
`buffer.active.type === 'alternate'` (i.e., a full-screen TUI is running) — normal-buffer content
(shell prompts, logs) stays Mac-authoritative and the companion soft-wraps it locally at its own
width. **Killed by adversarial review** on three independent, code-verified grounds:
1. **Critical — no consent signal.** Alt-buffer state is a property of the one shared PTY; an
   idly-connected, backgrounded phone would auto-fire a resize request the instant the *Mac's own
   local user* opened `vim`/`less`/`git log`, hijacking the Mac's live session to phone width with
   no visible trigger. This is the exact 1.20.0 incident T-4 exists to prevent, reopened under a
   new name.
2. **High — buffer-state tracking doesn't survive a remount.** `xterm.js`'s `buffer.active.type`
   is derived only from bytes that specific instance has observed. `TerminalStack.vue` unmounts
   the host's own xterm on dock-collapse; scrollback replay on remount only replays the last
   128KiB (`SCROLLBACK_CAP`, `pty.rs:61`), which for any long TUI session will not contain the
   original `1049h` entry sequence — a routine dock collapse/expand mid-session silently and
   permanently desyncs the gate.
3. **High — the "companion renders at its own, different local width" premise doesn't hold for
   live content at all**, not just full-screen TUIs. A shell's own line editor (zsh ZLE / readline)
   computes cursor-redraw escape sequences against the PTY's *real* `ioctl` winsize, not against
   whatever width the receiving terminal locally declares — ordinary interactive editing
   (backspace across a wrapped command, tab-completion menus, powerline/starship prompts) would
   misrender the moment the two widths differ. This is a **new** failure class: pre-1.23.0 the
   companion's xterm always mirrored the PTY's real size exactly, so it never hit this; SimpleView
   avoided it by not being a live grid at all. `tmux` — already cited as prior art in
   `docs/plan/done/terminal-resize-authority.md:18-20` — does not independently reflow per
   attached client either; it uses "window-size smallest/manual": every attached client sees the
   *same* single grid at any moment.

**Round 2 (this doc): explicit, manual, single-owner handoff — the tmux `window-size manual`
shape**, applied to this codebase's actual architecture rather than invented generically. Verified
against real code before being written down (not trusted from either the deep-think session or the
first design pass, both of which produced Vue-repo-inapplicable or fictional file paths at various
points — corrected here against the real tree).

## Goal

Give a companion a genuinely usable, full-featured phone terminal — full-screen TUI support
(`vim`, `htop`, `claude`/`agy`'s own UI) included — without ever letting a companion's resize touch
the shared PTY *automatically* or *invisibly*, which is what caused the 1.20.0 incident T-4 exists
to prevent. Also directly fixes the *original* 1.23.0 regression complaint (a wide Mac viewport
permanently force-fitting its grid onto a narrow phone with zero recourse) for the normal-buffer
case, which the rejected Round-1 design never actually addressed.

## Architecture — one shared grid, ownership is an explicit, revocable claim

Nothing about the shared-PTY-per-tab model changes (`src-tauri/src/pty.rs` — **zero Rust changes**,
see "Why zero Rust changes" below). What changes is *who is currently allowed to drive its size*:

- **Default:** the Mac (`resizeOwner: 'host'`) — exactly today's behavior, unchanged. The Mac's
  own window/dock-resize `ResizeObserver` → `doFit()` → `hostResize()` path keeps working exactly
  as it does today.
- **A companion may take over, but only via one deliberate tap** — never a background/automatic
  trigger of any kind. Tapping "Fit to my screen" in the companion's key row sends its own measured
  `{cols, rows}` to the host in the same frame that claims ownership. The host honors it
  unconditionally (no negotiation) and the shared grid becomes that size.
- **The Mac always has a one-tap reclaim.** A small overlay pill appears on the Mac's own terminal
  *only* while `resizeOwner !== 'host'`, and tapping it hands ownership straight back and
  live-remeasures the Mac's own container (not a cached pre-hijack value — the exact gap Round 1's
  Finding 4 flagged).

This defeats every Round-1 finding by removing the thing that caused them — there is no more
automatic, buffer-state-derived, or background-triggered path to a resize at all. Every resize
that ever reaches the real PTY is the direct, immediate result of one tap on one screen, exactly
as T-4's original spirit intended ("the host is the sole *automatic* resize authority") — this
doc narrows T-4's *literal* wording (see "T-4 docs update" below) without reopening what it was
actually built to prevent.

### Why zero Rust changes

- `pty_resize(tab_id, cols, rows)` (`pty.rs:790-806`) already accepts arbitrary caller-supplied
  `cols`/`rows` for any `tab_id` with no caller-identity enforcement — its own doc comment already
  states enforcement is "a frontend-side discipline," not a backend one. This design keeps that
  exactly true; it only changes the frontend-side policy for *when* to call it.
- The WS relay is verified fully generic for companion→host frames: `handle_companion_socket`
  (`web_server.rs:1019-1052`) calls `state.forward_to_host(stamp_from(msg, &conn_key))` for *any*
  `Message::Text`/`Message::Binary`, with no frame-type filtering anywhere in that path. A new
  frame tag needs zero relay changes to be forwarded, and it already arrives at the host
  pre-stamped with `from` = the sending connection's opaque key (`web_server.rs:353-361`) — the
  host does not need anything new to know *which* companion sent a claim.

### Why one new frame is a narrow, justified exception to `protocol.js`'s §13 freeze

`protocol.js`'s header says "do not diverge, do not add per-feature frame types" — aimed at
preventing ad hoc frames when the generic `FRAME_DELTA`/`FRAME_INTENT` mirror already covers a
need. `resizeOwner` itself (see below) *does* go through that generic path, honoring the freeze.
But the actual "here is my measured size, apply it now" payload cannot: it is direction-specific,
tab-scoped, and — per the file's own existing rationale for why the four PTY frames are separate
from the mirror in the first place ("raw terminal bytes are a firehose and do not fit a
JSON-diffed state model") — this is the natural fifth member of that *already-exempted* PTY frame
family, not a new category. Reusing the existing `FRAME_PTY_RESIZE` tag bidirectionally was
considered and rejected: every frame in this file is documented with one fixed direction, and
overloading one tag with direction-dependent semantics breaks that reader model for no real
savings over one clearly-new, clearly-documented tag.

```js
// companion -> host: "I am claiming resize authority for this tab, apply this size now." Honored
// unconditionally (last tap wins, see Arbitration) — no negotiation, no permission check beyond
// "this arrived from a companion." NOT part of the FRAME_DELTA mirror: this carries an imperative
// action + a size, not shared state (the resulting *ownership* IS shared state — see
// terminalTabsStore.js's `resizeOwner` field, which rides the existing mirror instead).
export const FRAME_PTY_RESIZE_REQUEST = 'pty_resize_request'
```

## Shared state — `resizeOwner` rides the existing mirror, not a new sync mechanism

`protocol.js` already states the governing principle for this exact class of fact: *"THE TAB LIST
ITSELF IS NOT A FRAME. It is ordinary shared state and rides the normal mirror... Only per-tab
BYTES and LIVENESS travel on the raw frames."* Who owns resize authority is infrequently-changing,
small, and exactly as shared as the tab list itself — so it lives as a new field on each tab object
in `src/store/terminalTabsStore.js`'s `terminalTabs` list, which `services/mirror.js` already
auto-discovers and mirrors (SSOT-1):

```js
/** [{ id, title, projectId, cwd, titleLocked?, resizeOwner }]
 *  resizeOwner: 'host' | <opaque companion connection id from `frame.from`>. Default 'host' —
 *  today's behavior, unchanged. Never anything but 'host' or a value that arrived as `frame.from`
 *  on a FRAME_PTY_RESIZE_REQUEST — never constructed or guessed client-side (same "opaque, echo
 *  don't parse" discipline `protocol.js` already states for `from`/`to`). */
```

`reclaimResizeAuthority(tabId)` — a new `action()`-wrapped function alongside the existing tab-list
actions, callable locally on the Mac (the reclaim pill) — sets `resizeOwner` back to `'host'`.
No separate "claim" action is needed in the store: a claim is never issued from the mirror/intent
path at all — it arrives bundled with the resize-request frame (see below), because bundling them
into one explicit tap is what keeps this safe (Round 1's core defect was ever separating "may I"
from "here is my size" into anything that could fire without a human present at the moment of the
tap).

## Host-side handling (`usePtyTerminal.js`)

- **`ownsPtySize` becomes conditional, not a static `isHost` alias**: `computed(() => isHost &&
  resizeOwnerFor(tabId) === 'host')`. This is the one behavior change to today's default path —
  without it, the Mac's own `ResizeObserver` → `doFit()` would silently steal authority back the
  next time its window/dock is merely touched, defeating the whole point of a companion's claim
  lasting until an explicit reclaim.
- **New host-only frame listener**, filtered by the same `isForThisTab` pattern every other PTY
  frame already uses (`usePtyTerminal.js:387-389`): on `FRAME_PTY_RESIZE_REQUEST`, unconditionally
  (a) call `reclaimResizeAuthority`'s sibling to set `resizeOwner = frame.from`, (b) call the
  existing `hostResize(cols, rows)` (already callable regardless of current `ownsPtySize` — it only
  gates on `isHost`, per `usePtyTerminal.js:585`), (c) if this tab's own instance is mounted on the
  host, also call `term.resize(cols, rows)` locally so the Mac's own render reflects the new grid
  immediately (mirroring exactly what a companion already does on receiving `FRAME_PTY_RESIZE`) —
  this listener lives inside the per-tab composable specifically so this step has direct access to
  that tab's own `term` ref, which a global module like `ptyBridge.js` does not have.
- **Reclaim** calls the store action, then re-runs the *live* `doFit()` measurement — not a cached
  value — resolving Round 1's Finding 4 exactly as specified there.

## Companion-side handling (`usePtyTerminal.js` + `TerminalView.vue`)

- `useTerminalViewType.js` reverts to always mounting `TerminalView` — the `isHost ? TerminalView :
  SimpleView` branch is removed; see "SimpleView disposition" for why the files themselves are not
  deleted in this change.
- **Default (no claim yet):** unchanged from pre-1.23.0 — the companion's xterm is purely passive,
  resized only by the echoed `FRAME_PTY_RESIZE`, exactly as `usePtyTerminal.js:432-436` already
  documents. This is what makes the "does this also fix the original regression" question answer
  yes: nothing about the default path changed, so today's `FRAME_PTY_RESIZE_REQUEST` escape hatch
  is additive, not a replacement of working behavior.
- **New**: a companion-only function (shape mirrors `hostResize`, sends over the WS relay instead
  of invoking Tauri) that measures the companion's own container via the same `fitAddon`
  measurement `doFit()` already computes, and sends `{ t: FRAME_PTY_RESIZE_REQUEST, tab_id, cols,
  rows }`.
- **New key-row button**, gated on the same `showKeyRow` capability that already scopes the
  font-zoom buttons to non-host screens (`TerminalView.vue`'s existing `pty-key-row`,
  `:29-66`/`:51-64`) — no new capability, no new DOM row (Extreme Narrow): "Fit to my screen."

## Mac-side affordance

A small, absolute-positioned overlay pill on `.pty-terminal`, `v-if`-gated on
`resizeOwner !== 'host'` for the active tab only — invisible in the default/common case, matching
Extreme Narrow exactly ("if a state change can be communicated via... an overlay... do not add a
new DOM element in the flow"). Reads roughly "Sized for a connected phone — tap to reclaim."
Companions render no analogous "someone else has it" state at all in V1 (see "Not in this pass").

## Arbitration

**One sentence, the whole rule:** the most recent explicit "fit to my screen" tap always wins
immediately and unconditionally — there is no negotiation, lock, or warning to whichever screen
previously held authority, which is the same behavior a shared single-PTY architecture has always
had for the Mac's own resize events (a second Mac-side resize today already just wins over the
first with no negotiation either — this is not a new class of risk, only a new class of sender).

## Session lifecycle, walked through

1. **Tab open** — `resizeOwner` defaults `'host'`. Identical to today.
2. **Phone joins an open tab** — receives the current tab list (incl. `resizeOwner`) via the
   normal `init`/mirror sync; its xterm passively mirrors the current grid via the existing
   scrollback-hydrate + `FRAME_PTY_RESIZE`-echo path. No special-casing needed.
3. **Phone taps "Fit to my screen"** — sends `FRAME_PTY_RESIZE_REQUEST`. Host sets
   `resizeOwner = frame.from`, resizes the PTY, broadcasts `FRAME_PTY_RESIZE` to all (including
   back to the requester — a harmless no-op re-apply of the size it already has).
4. **Mac reclaims** — tap → `resizeOwner = 'host'`, live re-`doFit()`, broadcast the Mac's own
   current size.
5. **Phone holding authority disconnects without reclaiming** — **explicitly out of V1** (see "Not
   in this pass"). `resizeOwner` stays pointed at a now-dead connection id; this is harmless
   (nothing auto-drives off a stale value — the mechanism only ever *changes* state on an explicit
   tap), and the Mac's one-tap reclaim always works regardless of whether the prior holder is still
   connected.
6. **Shell exits while a companion holds authority** — `pty_restart`'s existing JS-side handler
   additionally resets `resizeOwner = 'host'` as part of the same reset it already performs, so a
   fresh shell starts under the normal default rather than inheriting a stale claim.
7. **Second companion joins while phone A holds authority** — sees the tab list with `resizeOwner`
   already non-host; V1 renders no special state for this on any companion (see "Not in this
   pass"). If phone B also taps "Fit to my screen," it steals authority immediately per the
   Arbitration rule, with no warning to phone A — accepted, stated plainly, not hidden.

## `T-4` doc update (required by this change, `docs.B3`)

The live docs' current wording ("the host is the sole resize authority... A companion never sends
or acts on anything BUT [the echoed size]") is no longer literally true and must be corrected in
`docs/arch/terminal-stack.md` and `docs/feat/in-app-terminal.md` (the archived
`docs/plan/done/1.20.0-terminal-and-remote-sync.md:144` copy stays as the historical record and is
not rewritten, per `docs.B2` — only the *live* docs are current-state and must sync). Replacement
wording: *"The Mac is the DEFAULT and automatic resize authority — its own window/dock resize keeps
driving the shared grid unprompted, exactly as before. A companion may take explicit, temporary
authority only via one deliberate tap, never automatically or in the background; the Mac can always
reclaim it with one tap of its own. What T-4 actually prevents — a companion silently or
automatically resizing the shared PTY — still holds without exception."*

## SimpleView disposition

`SimpleView.vue`, `usePtyStream.js`, `ansiStrip.js`, `ptyCodec.js`, `scripts/lint-simpleview-boundary.js`,
`scripts/test-ansistrip.mjs` are not deleted by this change — they become unused once
`useTerminalViewType.js` stops mounting `SimpleView`. Kept, not removed, because: (a) this is
already a large change and bundling a separate deletion decision into it raises the risk of this
diff for no gain toward the actual goal; (b) the seam could plausibly be revived later as an
opt-in low-bandwidth text-only mode. Flagged here explicitly as a deliberate decision, not an
oversight — a candidate for a follow-up subtraction pass, not a silent accumulation of dead code.

## Not in this pass

- Auto-reclaim when the authority-holding companion disconnects (lifecycle item 5) — would need a
  new relay-originated `companion-disconnected` frame (a real Rust/relay change, unlike everything
  else in this doc), for a convenience the one-tap Mac-side reclaim already covers adequately.
- Any "someone else currently has it" UI state on a companion screen (lifecycle item 7) — avoided
  because a companion is architecturally never told its own opaque connection id
  (`protocol.js`: "never supplied by the companion and never shown to one"), so it cannot
  distinguish "I hold it" from "another phone holds it" without a new mechanism this V1 doesn't
  need to justify.
- Any negotiation, locking, or steal-warning between two companions racing for authority.

## Smallest correct shippable V1

```
Step 1  PATCH  src/constants/protocol.js               add FRAME_PTY_RESIZE_REQUEST + doc comment
Step 2  PATCH  src/store/terminalTabsStore.js           add resizeOwner field (default 'host'),
                                                         reclaimResizeAuthority(tabId) action,
                                                         reset resizeOwner on tab restart/respawn
Step 3  PATCH  src/composables/usePtyTerminal.js        ownsPtySize -> computed on resizeOwner;
                                                         host-side FRAME_PTY_RESIZE_REQUEST listener;
                                                         companion-side requestResize(cols, rows)
Step 4  PATCH  src/composables/useTerminalViewType.js   always TerminalView (drop the SimpleView branch)
Step 5  PATCH  src/components/TerminalView.vue          companion key-row "Fit to my screen" button;
                                                         Mac-side reclaim pill (v-if on resizeOwner)
Step 6  PATCH  docs/arch/terminal-stack.md,
               docs/feat/in-app-terminal.md              T-4 wording update
Step 7  PATCH  docs/plan/done/restore-terminal-mobile-ux.md,
               docs/index.md, CHANGELOG.md                status + release notes
Step 8  TEST   Mac: window/dock resize still auto-drives when resizeOwner is 'host' (unchanged
               default). Phone: tap "Fit to my screen" during a plain shell prompt (normal-buffer
               regression case) and during vim/htop (the TUI case) — both should render correctly
               at phone width. Mac: reclaim pill appears, tap restores Mac's live size.
```

## Cross-references

- `docs/plan/done/restore-terminal-mobile-ux.md` — the research/decision doc this executes; its Directions A/B are now closed in favor of this one.
- `docs/research/simpleview-mobile-regression.md` — original regression report this traces back to.
- `docs/plan/wish-terminal-split-simpleview.md`, `docs/research/audit-terminal-split-wish.md` —
  SimpleView's own design/review; this doc supersedes SimpleView as the companion's default mount
  without deleting it.
- `docs/plan/done/terminal-resize-authority.md` — the tmux `window-size manual` prior art this
  design follows.
- `docs/plan/done/1.20.0-terminal-and-remote-sync.md:144` — T-4's original wording and the incident
  it was built to prevent (archived; not rewritten, only the live docs are).
