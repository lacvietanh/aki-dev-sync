# Terminal chrome settings — WS-B spec

Workstream WS-B of `docs/plan/backlog-jul27.md`. A 3-dot settings menu on the terminal stack's title bar, drop-up, one checkbox per toggleable piece of terminal chrome, host-dependent defaults. Feature doc to sync on implementation: `docs/feat/in-app-terminal.md`; architecture: `docs/arch/terminal-stack.md`.

**Contingency marker.** Every row and paragraph tagged `[WS-A]` is contingent on WS-A's external-terminal ownership model, which is being designed in parallel and may add, remove, or rename a header control. Nothing else in this doc depends on it.

## 1. Findings, severity-weighted (METHOD-ux-psych §C1)

### SEVERE — S1. Hiding the tab strip on a companion is a dead end
On the Mac, `⌘T` / `⌘W` / `⌘⇧[` / `⌘⇧]` (`TerminalStack.vue`'s keydown handler) fully replace the strip's `+`, `✕` and chip-click. On a phone none of those exist. A companion whose tab strip is hidden cannot open a tab, close a tab, or switch tabs, and the only route back is a menu the user has to remember they used. **The tab strip must not be user-hideable on a companion** — see §3, where it is rendered checked-and-disabled with a stated reason rather than omitted.

### SEVERE — S2. Default-hiding the compose input on the Mac withdraws the only working Vietnamese path, while the bug it works around is still open
`TerminalView.vue`'s own comment and `docs/research/terminal-vietnamese-ime-root-cause-2.md` record that for true composing IMEs (macOS's built-in Vietnamese) composing in a plain `<input>` "remains the supported path" — which is exactly why the current release stopped making the row phone-only. WS-D (the residual restore-after-backspace bug) is still open as research. Hiding the compose input by default on the Mac means a first-run user whose primary language is Vietnamese opens the terminal, types, gets wrong characters, and the fix is behind a button they have never pressed. Argued in full in §6; this is the one place I recommend deviating from the user's stated default.

### MATERIAL — S3. Text-size buttons are nested inside the key row and would disappear with it
The zoom trio (`−` / `%` / `+`) lives *inside* `.pty-key-row` in `TerminalView.vue`, gated by the same `v-if="ptyApi?.showKeyRow"`. On a companion those three buttons are the **only** way to change text size (there is no `⌘+`). One checkbox covering both would let a phone user who dislikes the Esc/Ctrl row silently lose all zoom control. Fix: two independent checkboxes, and the sub-groups get their own `v-if` inside a `.pty-key-row` container that renders when *either* is visible (the `.pty-key-sep` hairline renders only when both are).

### MATERIAL — S4. First-run discoverability of the 3-dot button under an all-hidden default
See §6's friction ledger. Mitigated without changing any default by the one-time armed tint in §6.3.

### MATERIAL — S5. The settings button and the CLOSE/EXPAND chevron must never be members of their own list
If either can be hidden, the state is unrecoverable from the UI. Both are excluded from the inventory by construction, not by a default. Acceptance criterion AC-12.

### MATERIAL — S6. Do not copy `.popup-item`'s markup — it is not keyboard-reachable
`ProjectTable.vue`'s popup rows are `<div>` + `@click`. Reusing that shape for checkboxes would produce a menu no keyboard can operate. This menu's rows are `<label>` + real `<input type="checkbox">`; the visual classes are reused, the markup is not.

### TRIVIAL — S7. Do not open the menu on hover
`ProjectTable.vue` opens its popup on hover because a whole column of triggers is scanned that way. Here the trigger sits immediately beside the maximize button, so hover-open would fire every time the cursor travels to maximize (A5). Click/tap only — and therefore `RULE-ui-pattern.md` B5's hover-bridge requirement does not apply. Stated explicitly so its absence does not read as an oversight.

### TRIVIAL — S8. The menu gets no title row
`ProjectTable.vue`'s popup has a `.popup-header` because it must name *which project*. This menu has no ambiguity, and a header row is exactly the "extra label" CLAUDE.md's Extreme Narrow section forbids.

## 2. Conflict check against the deliberate removals

`docs/feat/in-app-terminal.md`'s migration table records CLEAR / RESTART / KILL / OPEN as deliberately removed, with named replacements (chip `✕`, `✕` then `+`, the OPEN popup's Terminal item, and "dropped from the UI entirely" for CLEAR). `TerminalStack.vue`'s `#actions` comment restates the rule: the slot holds only icon buttons that act on the **panel**, never on a shell.

**No conflict.** The user's request ("ẩn hiện các input, nút bấm" — show/hide the inputs and buttons) is about controls that exist today. Nothing in it asks for a removed control back.

**The boundary must still be written down now**, because a settings menu is a magnet for exactly that request. The menu's contract: it toggles the **visibility of controls that already render**. It is never a home for a control a previous release removed. Re-introducing CLEAR/RESTART/KILL/OPEN through a checkbox would be worse than re-adding them plainly — they would arrive default-off, undocumented, and would silently reverse a decision the migration table records with reasons. If any of them is ever wanted back, that is a separate decision argued against the migration table, not a new row in this list.

## 3. Control inventory

Everything the terminal stack renders, exhaustive, from the code as it stands today.

| Control | Lives in | Available on | Current render condition | Toggleable | Default: companion | Default: app (user's rule) | Default: app (recommended) |
|---|---|---|---|---|---|---|---|
| Compose input + send button | `TerminalView.vue` `.pty-compose-row` | host + companion | unconditional | yes | on | off | **on** (see §6) |
| Key row (Esc/Tab/Shift/Ctrl/arrows/Enter) | `TerminalView.vue` `.pty-key-row` | companion only | `v-if="ptyApi?.showKeyRow"` (= `!isHost`) | yes, companion only | on | not offered | not offered |
| Text size buttons (`−` / `%` / `+`) | `TerminalView.vue`, nested inside `.pty-key-row` | companion only | same `showKeyRow` gate as the key row | yes, companion only (needs the S3 split) | on | not offered | not offered |
| Tab strip (`TerminalTabStrip`) | `TerminalStack.vue` `#title` | host + companion | unconditional | **host only** — locked on for a companion (S1) | on, locked | off | off |
| Group name chip (icon + 4-char name) | `TerminalStack.vue` `#title` `.term-scope-id` | host + companion | unconditional | yes | on | off | **on** (weak, see §6) |
| External terminals button | `TerminalStack.vue` `#actions` | host only | `v-if="externalTerminalsSupported"` (= `isHost`) | yes, host only `[WS-A]` | not offered | off | off `[WS-A]` |
| Maximize / restore button | `TerminalStack.vue` `#actions` | host + companion | `v-if="!dockAllCollapsed"` | yes | on | off | off |
| Settings button (this feature) | `TerminalStack.vue` `#actions` | host + companion | see §8.1 | **never** (S5) | — | — | — |
| CLOSE / EXPAND chevron | `DockStack.vue` | host + companion | unconditional | **never** (S5) | — | — | — |
| xterm mount (`.pty-terminal-mount`) | `TerminalView.vue` | host + companion | unconditional | never — it is the feature | — | — | — |
| `.pty-key-sep` hairline | `TerminalView.vue` | companion only | inside the key row | no — it follows its two neighbours (renders only when both groups are visible) | — | — | — |

Deliberately **out of scope**: the `TERM` cell's two count badges and `TerminalCountBadges.vue` (they live in the project table, not the terminal stack), and the OPEN popup's In-App Terminal / Terminal items (same, and the migration table owns them).

## 4. Two-condition composition — capability × preference

Effective visibility of any control is:

`visible = capability(control, thisScreen) && preference[control]`

with one absolute rule: **preference never widens capability.** A control the screen cannot support is not rendered, and its checkbox is not offered as checkable — a phone must never be able to tick "External terminals", whose `list_external_terminals` command is deliberately absent from `services/hostInvoke.js`'s companion allowlist and could only ever open a modal showing an error.

Mechanically this follows the pattern `docs/arch/terminal-stack.md` § "The capability pattern" already establishes: the composable publishes named capabilities (`showKeyRow`, `ownsPtySize`, `externalTerminalsSupported`) and the component asks what it can *do*, never who it is. `useTerminalChrome.js` (§5) follows suit — it may import `isHost` from `services/bridge` (as `useExternalTerminals.js` and `usePtyTerminal.js` already do), and publishes a `chromeVisible` map plus a `chromeMenuRows` list; `TerminalStack.vue` and `TerminalView.vue` read the map and never gain an `isHost` import (ENV-1).

**Unavailable-on-this-device: absent, not disabled-with-reason.** Justification: `ProjectTable.vue`'s `.popup-disabled` is the right pattern where the user can *change* the blocking condition (install VSCode, set a local path). Here the condition is "which device you are holding", which no action inside the menu can alter, so a permanently greyed "Key row (phone only)" on the Mac teaches nothing and spends menu height forever (A1 + Extreme Narrow). Omit it.

**One exception, and it is the opposite case.** The tab strip on a companion is *visible* and *not hideable* — a rule, not a device fact. Omitting its row would read as an oversight next to a strip the user can plainly see. It renders **checked and disabled**, reusing `.popup-disabled`, with `title="The tab strip is the only way to open, close and switch tabs on this screen."` (problem first, per `RULE-content-write.md` B1).

## 5. Preference model

**Scope: per-device.** Not per-project, not mirrored. This is verbatim the argument `useTerminalFont.js`'s file header already makes and wins: which chrome a screen shows is a fact about the screen you are looking at, not about the project, and mirroring it would make a phone and a 27" Mac fight over each other's preference. It is therefore *not* a `useProjectConfig.js` concern either — that module is disk-backed per-project config, and "hide the maximize button" has no per-project meaning.

**Placement: `src/composables/useTerminalChrome.js`, never `src/store/`.** `services/mirror.js` auto-discovers every `isRef` export under `src/store/` (SSOT-1). Living in a composable is the same mechanism `terminalStackCollapsed`, `dockHeightPct` and `terminalFontScale` already use to stay off the wire.

**Storage: `localStorage`, key `aki-terminal-chrome`, a JSON object.** Follows `refreshStore.js`'s merge shape (`{ ...DEFAULTS, ...JSON.parse(raw || '{}') }`) and `useTerminalFont.js` / `useDockLayout.js`'s try/catch around `setItem` (a private-mode or quota failure must leave the preference applying for the session, never throw).

**Can a companion store anything at all? Yes, confirmed from the code.** A companion is the same bundle served by axum to a phone browser; it already writes `DEVICE_TOKEN_STORAGE_KEY` (`services/bridge.js`), `aki-terminal-font-scale`, `aki-dock-height-pct` and `aki-selected-ssh-host` to its own `localStorage`. Nothing new is needed.

**Sparse storage, lazy defaults.** Only explicitly-changed controls are written; the stored object is merged *over* the role defaults at read time, and defaults are recomputed from `isHost` on every launch. Two consequences, both wanted: a control added in a later release picks up its role default instead of `undefined` with no migration, and a device's role can never be baked into a stored value.

**First run, and a companion that has never connected before.** No key present → the merge yields the pure role defaults → the Mac window gets the app defaults and the phone gets all-shown. There is no boot-order hazard: `isHost` is set by `src/boot/roleStamp.js`, the first import in `main.js`, long before any composable evaluates.

**Escape hatch: a "Show all" row** at the foot of the menu, rendered only when at least one available control is hidden. It writes every available control to `true` and leaves the locked ones alone (a multi-entity guard in the CLAUDE.md sense: it touches exactly the controls this device offers, never a blanket wipe of the stored object). Verb-first per `RULE-content-write.md` B1.

## 6. Argued position on the defaults — separated from the spec

Raised so the orchestrator can put it to the user. **If the user restates "app hides everything", implement it verbatim** — §6.3 is a mitigation that changes no default and is therefore not a countermand.

### 6.1 What is right about the user's rule

It is the same reasoning this codebase already applies to itself. `TerminalView.vue`'s comment on the zoom buttons says it outright: they are browser-only "by construction rather than by a second condition", because on the Mac the same three actions are `⌘+` / `⌘−` / `⌘0`, "so the app window spends no pixels on a control its keyboard already has — which is the whole Extreme Narrow trade." Extending that to the maximize button (the splitter drag and its double-click reset already cover it), to the external-terminals button `[WS-A]`, and to the tab strip (`⌘T` / `⌘W` / `⌘⇧[` / `⌘⇧]` cover it fully) costs a Mac user nothing they cannot reach another way, and hands the header's width back to what remains. Six of the seven rows: agreed.

### 6.2 What is wrong about it — the compose input (S2)

The compose input is the one control with **no host-side equivalent**. It is not a convenience duplicated by a keyboard shortcut; it is the documented workaround for a bug that is still open (WS-D). Default-hiding a workaround while the defect it works around is unfixed is the single move that makes the app look broken on first launch, to the user group least able to work around it.

Friction ledger, first run, task "type a Vietnamese command", app default = all hidden:

| Step | Cost |
|---|---|
| Type; wrong characters appear | 1 confusion, 0 recoveries offered |
| Notice the 3-dot button is the only unexplained thing in the header | 1 scan of a ~28px target in a header the user has not learned yet |
| Open it | 1 click |
| Read 7 rows and map "Compose input" to an object never seen | 1 seven-way decision, and it is a **recall** task, not recognition (A2) — the label names something the user has no memory of |
| Check it | 1 click |
| Dismiss, click into the input | 1–2 clicks |

Four to five interactions plus one seven-way decision, against **zero** today and **one** on the phone. The `title` attribute mitigates but tooltips are hover-only and mostly unread.

I therefore recommend `compose: true` as the app default. I hold a second, weaker recommendation for the **group name chip**: it is `flex-shrink: 0` at a 10px font (a few dozen pixels), and it is the only thing on screen answering "which project's shell am I looking at" (A2). It does have a fallback — every tab chip's `title` carries the tab title — so this one is genuinely arguable and I will not press it.

Net: the deviation from the user's stated rule is **one row** (two if the group name is accepted).

### 6.3 Mitigation that changes no default

On the first launch after this ships, when every available control is hidden **and** the menu has never been opened on this device, tint the 3-dot button with the cyan armed state the codebase already owns (`.pty-key.is-armed`, `.src-power.is-on`), until it is opened once. No new element, no banner, no row, no count text — Extreme Narrow-compliant, and it converts §6.2's recall problem into a recognition one. Stored as `aki-terminal-chrome-seen` alongside the preference; delete-safe (losing it re-tints once, which is harmless).

## 7. Consolidation recommendation for the existing header buttons

The brief asks whether maximize and external-terminals should move *into* the menu. Split answer.

**External terminals — do not decide here.** A checkbox list is a visibility control; this button is an *action*, so a menu *item* would be a different kind of row and would make the menu two things at once. The consistent destination, if it moves, is the OPEN popup — which is precisely where `docs/feat/in-app-terminal.md`'s migration table already routed its predecessor ("The old external-terminal button on the `TERM` cell → Same OPEN popup item"). **`[WS-A]`** WS-A owns the external-terminal ownership model and "one shared terminal-button class" and may be adding a header control here. WS-B therefore does exactly one thing with it: expose it as a checkbox, default off on the Mac, and defer its placement entirely to WS-A. If WS-A removes the header button, the row disappears with it and no other part of this spec changes.

**Maximize — keep it a button; do not move it into the menu.** It is a *momentary mode* toggled repeatedly within a session — `useDockLayout.js` refuses to persist it for exactly that reason ("a preference … maximise is a momentary mode"). Burying a high-frequency reversible toggle two clicks deep is the A5 mistake this consolidation was meant to avoid. The checkbox achieves the width saving anyway, by defaulting it off on the Mac where the splitter drag and its double-click reset already cover the need, while keeping it one click away for anyone who turns it on.

**Net width, which is the actual Extreme Narrow question.** On the Mac at default: header goes from two icon buttons + chevron to **one** icon button + chevron — one button *narrower* than today. On a companion: from one (maximize; external is host-only) + chevron to two + chevron, i.e. +1 button ≈ 28px, on a header that also carries the tab strip. Accept it — the phone is where the menu is most useful (a phone paired to a hardware keyboard wants the key row gone), and if width ever becomes critical the honest fix is `min-width: 0` on the title row so the strip can shrink, not dropping the menu.

## 8. Drop-up spec

### 8.1 Trigger

A third `#actions` button in `TerminalStack.vue`, before the external-terminals button so the two panel-action buttons stay adjacent:

- Classes: `btn-tech btn-tech-secondary btn-terminal-action` — the exact trio already on both neighbours and on `DockStack.vue`'s chevron. Zero new button styling, no arbitrary values, no new token.
- Icon: `fa-solid fa-ellipsis-vertical`.
- `title="Terminal controls"` — noun-based setting name per `RULE-content-write.md` B1.
- `aria-expanded` bound to the open state; `aria-haspopup="true"`.
- Render condition: `v-if="!collapsed"` on this stack's own collapse ref. A control acting on the body must not be offered when the body is hidden, and the drop-up would otherwise rise out of a header with nothing behind it. Not `!dockAllCollapsed` (the maximize button's condition) — that one is about *dock height*, a different fact.
- Never hideable by its own list (S5).

### 8.2 Placement and classes

Reuse `ProjectTable.vue`'s `.open-popup` model, which is already a genuine drop-up (`bottom: window.innerHeight - rect.top`, `transform-origin: bottom …`, opened by state rather than `:hover` so a phone can reach it). Do **not** reuse `AppHeader.vue`'s `.icon-dropdown`: it is hover-only and drops downward, both disqualifying here.

- `position: fixed`, measured on open (the element is `visibility: hidden`, not `display: none`, so its real width is measurable before it becomes visible — the same trick `positionPopup` uses).
- `bottom: window.innerHeight - triggerRect.top + 4`, `right: window.innerWidth - triggerRect.right`, each clamped to an 8px viewport margin. Right-aligned to the trigger, not window-centred: this menu has one anchor, unlike the OPEN column's many.
- `transform-origin: bottom right`.
- `z-index`: the value `.open-popup` already carries (80). Do not invent a second number — modals start at 1000 and this must sit below them, which 80 already satisfies.

**Where the CSS lives — one decision for the orchestrator.** `.open-popup` / `.popup-item` / `.popup-section-label` / `.popup-disabled` are currently *scoped* inside `ProjectTable.vue`. This is their second call site and the third menu in the app, so Rule of Three (`RULE-ui-pattern.md` A1/Law 2) has fired and the extraction is evidenced, not speculative. Recommended: **promote them unchanged into `src/assets/main.css`** and point both call sites at them. Precedent is in that file already — `.terminal-header` / `.terminal-actions` / `.btn-terminal-action` were moved there for exactly this reason, with the comment "all three now use these classes, so they need to be global rather than duplicated per-component". This does mean WS-B touches `ProjectTable.vue`; if the orchestrator wants WS-B's diff confined to the terminal files, the fallback is to duplicate ~30 lines into the new component and file the extraction as a follow-up — which is a knowingly-taken Law 1 debt and must be recorded as such, not left silent. Either way, per `RULE-ui-pattern.md` B4, the promoted classes get a line in the pattern library.

**New component: `src/components/terminal/TerminalChromeMenu.vue`** (next to the existing `TerminalCountBadges.vue`), owning the trigger, the menu, the positioning and the dismiss handlers. `TerminalStack.vue` gains one line in `#actions`. Do **not** extract the positioning into a shared composable — at n=2 with two different anchor rules (window-centred vs trigger-right-aligned) that is a pre-extracted abstraction, which A1 forbids.

### 8.3 Rows

Real form controls, not `.popup-item` divs (S6):

`<label class="popup-item"><input type="checkbox" v-model="…" /><span>Label</span></label>`

reusing `.popup-item` for the row and `main.css`'s existing `.checkbox-group input` sizing (`14px`, `accent-color: var(--accent-cyan)`) for the box. No new colours, no arbitrary values. A locked row (companion tab strip) adds `.popup-disabled` and `disabled` on the input — note `.popup-disabled` sets `pointer-events: none`, so the explanatory `title` must sit on a wrapper element to remain hoverable, or the reason is unreadable.

Row order — availability-stable, so a control appearing or disappearing never reshuffles the rest: Compose input · Key row · Text size buttons · Tab strip · Group name · External terminals `[WS-A]` · Maximize button. Then the `Show all` row, separated by the `.popup-run-row`-style 1px top rule that already exists (a rule inside a row that exists, not a separator element — Extreme Narrow).

No header row (S8). No "N hidden" text anywhere — CLAUDE.md forbids inline count text, and the only compliant form (an absolutely-positioned badge overlay via `CountBadgeWrap` / `.cell-badge`) is **not** recommended here: under the app's all-hidden default it would be a permanent badge on every Mac, which is noise, not information.

### 8.4 Labels (`RULE-content-write.md` A3, B1)

Nouns, naming what the user sees, in words the UI already uses. Each label is canonical: the same string must be used in the doc, the code comment and any future copy.

| Label | Why this wording | `title` |
|---|---|---|
| Compose input | The doc calls it the compose row; "input" is what the user sees. Canonical from here on — nothing else may call it "message box" or "type-a-line field". | The text box under the terminal. Type a whole line, including Vietnamese, then press Enter. |
| Key row | Matches `showKeyRow` and `docs/feat/in-app-terminal.md`. | Esc, Tab, Shift, Ctrl, arrows and Enter for a screen with no physical keyboard. |
| Text size buttons | The buttons' own titles are "Smaller text" / "Larger text" / "Reset text size", so "text size" is the on-screen word — never "zoom", never "font". | Smaller, reset and larger text, next to the key row. |
| Tab strip | The doc's term. | The row of terminal tabs and the + button. |
| Group name | The header already says "Terminal group: …"; the doc says "Group identity (icon + name)". | The project icon and name at the left of this header. |
| External terminals `[WS-A]` | The button's title starts "External Terminal.app sessions". **If WS-A renames this control, this label follows WS-A's noun** — A3 requires one term, and two workstreams inventing two is exactly the drift it forbids. | Which external Terminal.app sessions are running. |
| Maximize button | Its titles are "Maximize panel" / "Restore panel height". | Fill the window with the dock, and restore it. |
| Show all | Verb-first action row per B1. | — |

### 8.5 Dismiss and keyboard

- Opens on click/tap only. Never on hover (S7) — and consequently the `::before` hover bridge is deliberately omitted.
- Stays open across toggles. Closing after each check would cost seven open/close cycles to configure seven controls; multi-toggle is the entire point.
- Closes on: document `pointerdown` outside the wrapper (registered only while open, exactly as `onDocPointerDown` does, so the opening gesture cannot close it); `Escape`; the stack collapsing; the window losing the dock.
- `Escape` must `@keydown.esc.stop` so it never reaches xterm, and must return focus to the trigger.
- Rows are native checkboxes, so Tab and Space work with no JS. Container carries `aria-label="Terminal controls"`. Do **not** use `role="menu"` — checkboxes are not menuitems, and the native controls are already announced correctly.
- No conflict with `TerminalStack.vue`'s capture-phase keydown listener: it returns early unless `hasTerminalFocus && e.metaKey`, and the menu is outside `.terminal-mount-wrap`'s focus scope.

## 9. Acceptance criteria

- **AC-1** — A `fa-ellipsis-vertical` button exists in `TerminalStack.vue`'s `#actions`, with classes exactly `btn-tech btn-tech-secondary btn-terminal-action`, and it does not render while the stack is collapsed.
- **AC-2** — The diff introduces **zero** arbitrary Tailwind-style values, zero new hex colours, and zero new `z-index` numbers. Grep per `RULE-ui-pattern.md` C1 returns nothing new.
- **AC-3** — `src/composables/useTerminalChrome.js` exists, imports `isHost` only from `services/bridge`, and publishes a capability-composed visibility map. Neither `TerminalStack.vue` nor `TerminalView.vue` gains an `isHost` import (ENV-1).
- **AC-4** — The composable lives in `src/composables/`, not `src/store/`; `services/mirror.js` therefore never discovers it. A `grep` for the new storage key in `src/store/` returns nothing.
- **AC-5** — `localStorage.setItem` for the new key is wrapped in try/catch, matching `useTerminalFont.js`.
- **AC-6** — With no stored key: a host screen resolves the app defaults from §3, a companion screen resolves every available control to shown.
- **AC-7** — On a companion, the tab strip's row renders checked and disabled with a stated reason, and no code path can set its preference to `false`.
- **AC-8** — On a host, the Key row and Text size rows are **absent** from the menu, not disabled.
- **AC-9** — On a companion, unchecking "Key row" leaves the three text-size buttons rendered, and vice versa; `.pty-key-sep` renders only when both groups are visible.
- **AC-10** — The menu closes on outside `pointerdown`, on `Escape` (which does not reach xterm and returns focus to the trigger), and on stack collapse; it does **not** close when a checkbox is toggled.
- **AC-11** — Every row is a `<label>` wrapping a real `<input type="checkbox">`; no row is a bare `<div>` + `@click`. Tab and Space operate every enabled row.
- **AC-12** — Neither the settings button nor `DockStack.vue`'s CLOSE/EXPAND chevron appears in the checkbox list, and no preference key exists for either.
- **AC-13** — With every togglable control unchecked on a host, the terminal still accepts input and `⌘T` / `⌘W` / `⌘⇧[` / `⌘⇧]` / `⌘+` / `⌘−` / `⌘0` all still work; the settings button and chevron are still on screen; "Show all" restores everything in one click.
- **AC-14** — No new row, banner, label or separator **element** is added to the terminal header or body, and no inline count text appears anywhere (CLAUDE.md Extreme Narrow).
- **AC-15** — Labels in the diff match §8.4 verbatim; no synonym for any of them appears anywhere in the diff (`RULE-content-write.md` A3).
- **AC-16** — No CLEAR / RESTART / KILL / OPEN control is added, in the menu or anywhere else (§2).
- **AC-17** — Doc sync: `docs/feat/in-app-terminal.md` gains the preference layer and the host-default split; `docs/arch/terminal-stack.md` § "The capability pattern" gains the capability × preference composition rule; this plan doc is listed in `docs/index.md`; `README.md` and `src/components/modals/IntroModal.vue` are checked in the same task (CLAUDE.md, "Feature changed?"). `CHANGELOG.md`'s `[Unreleased]` gains an entry; no version is minted.
- **AC-18 `[WS-A]`** — If WS-A removes or renames the external-terminals header button, the corresponding row and label follow it, and no other row changes.

## 10. Behavioural validation (METHOD-ux-psych §C4)

- Compose-input default: the signal is whether Mac users end up re-enabling it. If nearly every Mac device acquires `{"compose":true}` within a week, the default was wrong; if nearly no device ever writes the key at all, §6.2's discoverability concern was real and the menu is not being found.
- Menu discoverability: whether the one-time armed tint (§6.3) is ever cleared on a given device.
- **Stated honestly: this app has no telemetry, so neither signal is observable without asking the user directly.** They are questions to put to the user after a week of use, not metrics that will arrive on their own. Nothing else here has an observable signal, and I am not inventing one.
