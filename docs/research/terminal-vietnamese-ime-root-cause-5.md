# Vietnamese typing in the in-app terminal — the double-space blocker and the narrow-veto premise's real cost

Chain: follows `terminal-vietnamese-ime-root-cause-4.md` (§7 decided architecture: keypress veto narrowed to the multi-character carrier signature, backed by an "exclusivity by construction" table whose first row claimed `_keyPress` sends and cancels the physical-printable path). This doc corrects that row against xterm 5.5.0 source and hardware evidence, and records the fix that closes `docs/plan/terminal-input-jul31.md` §2.1. It edits nothing in `-4.md` beyond the single `Status: superseded by` line `docs.B2` allows.

## Start time

2026-07-31 (agent council session `2026.07.31-0232-double-space`: roster `red-team-drain` + `regression-surface` as mutual challengers, sonnet5 lead; every file read delegated to a haiku subagent with a narrow line range per the session's own REQ-8).

## Initial purpose

`docs/research/terminal-input-jul31.md` §5.2 recorded a blocker no hypothesis in this chain predicted: every space typed into the in-app terminal on Chrome arrived doubled, letters did not, and it happened even with no input method active. §5.4 gave a discriminating constraint any explanation would have to satisfy: the stretch right after OpenKey's auto-restore in the same typing session was **not** doubled. `-4.md`'s own §7 exclusivity table asserted the "physical printable" path was already exclusive (`_keyPress` sends it and cancels — line 162 of that doc), so on that account the blocker should not have been possible at all: either the table's premise or the code built from it was wrong.

Constraints inherited from the plan's requirement ledger (§2.1, and the council's own REQ-1 through REQ-8):

- Must explain why only space is affected, not every character.
- Must explain §5.4 without contradiction.
- Must not regress anything `docs/plan/terminal-input-jul31.md` §1 already confirmed on hardware (arrows in `vim`/`less`, Ctrl+C, Ctrl+D, Option+word-motion, F-keys, bracketed paste, Shift+Enter in the compose box, the sticky Ctrl/Shift latch, compose-row gating).
- Must keep exactly one input path — no new flag, no revived escape hatch.
- The Android/Gboard defect (plan §2.2, research §5.5) stays a separate item unless proven to share the same root cause.

## Strategy

Two owner/challenger items tracing the mechanism to file:line evidence with a hardware-testable prediction stated before the fix, plus a third item — roles reversed — re-deriving every already-confirmed surface item against the proposed fix before it shipped. Full ledger, requirement coverage and closing rationale: `/Users/aki/.aki/agent-council/aki-dev-sync/2026.07.31-0232-double-space/checklist.md`.

## Checklist

- [x] ITEM 1 (owner `red-team-drain`) — trace why space, and only space, reaches xterm's `_keyPress` without a forced cancel; state a hardware-testable prediction before the owner runs anything
- [x] ITEM 2 (owner `red-team-drain`) — decide whether the reported mid-syllable space insertion (`bá o`) shares ITEM 1's mechanism
- [x] ITEM 3 (owner `regression-surface`) — re-derive every plan §1 confirmed item against "veto all keypresses" before it ships
- [x] Owner runs the ITEM 1 prediction and, after the fix, the full retest across macOS host, remote Chrome, and remote Android/Gboard

## Result

### The premise `-4.md` §7's table stated does not hold for space

`-4.md` §7's exclusivity table (its line 162) reads: "physical printable | `_keyPress` sends it and calls `cancel(ev)` → textarea never mutates". Traced against xterm 5.5.0's `Terminal.ts` and `Keyboard.ts`, that only holds for keys `_keyDown` force-cancels — and space is not one of them:

- `Keyboard.ts:381` assigns `result.key = ev.key` only when `ev.keyCode >= 48`; space is keyCode 32, fails that test, so `_keyDown` (`Terminal.ts:1046-1048`) returns `true` without a forced `cancel`.
- `_keyPress` (`Terminal.ts:1155`) still fires and sends the character, then calls `this.cancel(ev)` **without** `force` (`Terminal.ts:1133`).
- `cancel(ev, force)` (`Terminal.ts:1308`) is a no-op unless `cancelEvents` is on; it defaults to `false` (`OptionsService.ts:56`), and `TerminalView.vue` never passes that option.
- So `preventDefault` never runs, the browser still inserts the space into xterm's own textarea, and `useTerminalTextDrain.js`'s capture-phase `input` listener reads and sends it a second time — the observed double.
- **Corollary, not previously measured:** uppercase A-Z travel the same unforced-cancel path via a separate early exit, the caps-lock HACK at `Terminal.ts:1052-1056`.

xterm's own anti-double-send guard, `_keyPressHandled` (`Terminal.ts:1177`), cannot rescue this: it lives inside `_inputEvent`, bound directly to xterm's own textarea (`Terminal.ts:384`), while the drain's `input` listener sits in the capture phase on `term.element` — an ancestor — and calls `stopPropagation()`, so `_inputEvent` never runs at all. The exclusion is disabled structurally, not by any flag value the app could have set differently.

### §5.4's discriminating constraint is satisfied, not contradicted

The stretch after OpenKey's auto-restore was not doubled because OpenKey delivers the corrected syllable — including its trailing space — as one multi-character carrier event, and Chrome/WKWebView tag a `>1`-character `event.key` with `keyCode 229`. That keydown routes through `CompositionHelper` and never reaches the space-specific gap above: the carrier is claimed once, by the drain, before xterm's own textarea diff can run. This part of `-4.md`'s design was already correct and is untouched by this doc.

### `bá o` (a space landing mid-syllable) does not share this mechanism

No code path from the space-keyCode gap above produces a space landing inside an already-committed syllable rather than after it. The candidate explanation considered — the drain skipping a textarea mutation on a composition/paste branch (`useTerminalTextDrain.js:172-186`) — requires a real `compositionstart`, which contradicts the observation that the defect reproduced with no input method active at all. Kept as a separate, still-open question; on retry during this session's hardware pass it did not reproduce at all (see Verification).

### The fix

`useTerminalTextDrain.js`'s `customKeyEventHandler` (`src/composables/useTerminalTextDrain.js:280`) now returns `false` for every `keypress`, not only the multi-character carrier signature it checked before (`ev.key.length > 1 && ev.charCode === ev.key.codePointAt(0)`). This narrows the guard's *condition* — one branch removed, not one added — so the invariant becomes "no keypress may ever both proceed through xterm and leave the textarea mutation standing", true by DOM construction (`preventDefault` suppresses the textarea mutation) rather than by classifying which keys need it. The file's own header comment was rewritten with this file:line evidence (`src/composables/useTerminalTextDrain.js:21-38`).

### ITEM 3 — the broadened veto does not regress anything plan §1 confirmed

Every item on the confirmed-working list resolves through a mechanism other than `keypress`, so widening the veto's scope does not touch any of it: arrows/DECCKM and Ctrl+C/Ctrl+D are force-cancelled at `_keyDown` before any keypress can fire (`Keyboard.ts:113-168`, `Terminal.ts:1078-1080`); Option+word-motion and F-keys are likewise keydown-encoded (`Keyboard.ts:118-124,136-142`, `:244-324`); bracketed paste never reaches `customKeyEventHandler` at all (`Terminal.ts:1005,1129` wire it only into `_keyDown`/`_keyPress`) and the drain already excludes `insertFromPaste`; Shift+Enter in the compose box uses its own textarea outside `term.element`; the sticky Ctrl/Shift latch is one funnel (`usePtyTerminal.js:501-561`) indifferent to which path a character's bytes took to reach it; the compose row's visibility is a plain `v-if` unrelated to key events. The only class of key that ever reached `_keyPress` without an `_keyDown` force-cancel was exactly `{space, uppercase A-Z}` — the set this fix corrects, and nothing wider.

### Verification

**Verified on real hardware, by the owner, 2026-07-31:**

- Prediction stated from source alone, before the fix: typing `TEST` should produce `TTEÉTT`. Measured: exactly `TTEÉTT` — confirms both the space mechanism and the uppercase corollary in one shot.
- After the fix, macOS host with OpenKey: `tét` / `TÉT` / `báo cáo` clean (`test` → `tét` is Telex reading `s` as a tone mark — expected engine behaviour, not a defect).
- After the fix, remote companion over Chrome: `tét TÉT báo cáo` clean.
- After the fix, remote companion on Android/Gboard: `teét TEÉT baoáo caoáo` — no double space; the plan §2.2 defect shape is unchanged, which is itself evidence this fix does not touch that defect.
- `vim`: arrow keys and Ctrl+C behave normally after the fix.
- `bá o` (mid-syllable space, research §5.2): retried, did not reproduce. Recorded as **not reproduced**, not as fixed — no mechanism was identified for it, so no claim is made about why it did not recur this time.

**Not verified, stated as such:**

- Option+arrow-key combinations were deliberately not exercised this round (the owner's own call) — not reported as PASS.
- Whether `keydown.preventDefault()` reliably suppresses the following synthetic `keypress` is a UI Events spec assumption underlying xterm's entire baseline design, not something specific to this fix; it was not independently re-verified in this session.

### Corroborating links

- `docs/research/terminal-input-jul31.md` §5.2, §5.4 — the original observation and the discriminating constraint this doc resolves.
- `docs/plan/terminal-input-jul31.md` §2.1 — the blocker entry this doc closes, and §1 — the confirmed-surface list ITEM 3 re-derived.
- `/Users/aki/.aki/agent-council/aki-dev-sync/2026.07.31-0232-double-space/checklist.md` — full requirement ledger, ITEM 1-3 rationale with file:line citations.
- `src/composables/useTerminalTextDrain.js:21-38` (corrected exclusivity table), `:280-292` (`customKeyEventHandler`) — the fix as it stands in the working tree at the time of this doc.
- xterm.js 5.5.0 `src/browser/Terminal.ts:1046-1048, 1052-1056, 1133, 1155, 1177, 1308, 384`, `src/common/input/Keyboard.ts:381`, `src/common/services/OptionsService.ts:56` — source citations for the mechanism.

## Decision

**Action** — `src/composables/useTerminalTextDrain.js` (working tree at the time of this doc, not yet committed), broadening `customKeyEventHandler`'s veto to every keypress; `docs/plan/terminal-input-jul31.md` §2.1 marked closed.

**Rejected/closed** — `-4.md` §7 point 1's "narrow" keypress veto (multi-character carrier signature only) and the first row of its exclusivity table (`physical printable | _keyPress sends it and cancels`) are dropped, superseded by the broadened veto and the corrected table above. Every other part of `-4.md`'s architecture — public-API-only text drain, composition stand-down, the 229 irreducible keydown claim, no vendoring or patching, no bundled xterm 6.x upgrade — stands unchanged and is not reopened by this doc.

**No action** — the Android/Gboard defect (plan §2.2, research §5.5); ITEM 2 found no mechanism connecting it to this fix, so it stays a separate, still-open item.

**Follow-up research** — none opened by this doc. `bá o`'s non-reproduction and the untested Option+arrow surface are both recorded as open/unmeasured rather than closed; either could seed a future round if either recurs with a reproducible trigger.

**Cross-references** — `docs/research/terminal-vietnamese-ime-root-cause-4.md` (gets the `Status: superseded by` line this round adds), `docs/index.md` (chain head moves to this doc), `docs/arch/terminal-stack.md` and `docs/feat/in-app-terminal.md` (synced separately per `docs/plan/terminal-input-jul31.md` §5).
