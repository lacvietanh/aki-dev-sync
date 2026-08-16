# Android/Gboard double-insert — why the pre-correction base characters and the corrected replacement both reach the PTY

Separate item from the `terminal-vietnamese-ime-root-cause` chain — `terminal-input-jul31.md` §2.2 keeps it apart from the double-space blocker (`-5.md`) deliberately, and that blocker's fix was hardware-confirmed on 2026-07-31 to leave this defect's shape unchanged. This doc does not supersede `-5.md`; it opens a new, narrower question the chain never carried (§5.5 recorded the observation but did not trace it).

## 1. Start time

2026-07-31.

## 2. Initial purpose

Diagnose why the in-app terminal, driven by `src/composables/useTerminalTextDrain.js` against xterm 5.5.0, doubles text specifically on Android + Gboard: `tét` → `teét`, `TÉT` → `TEÉT`, `báo cáo` → `baoáo caoáo`, `ăn gì` → `aăn giì`. FACT: the same drain is clean on macOS host + OpenKey and on desktop Chrome after the 1.22.0 double-space fix (`docs/plan/done/terminal-input-jul31.md` §2.1, `docs/research/terminal-vietnamese-ime-root-cause-5.md` Verification). FACT: the original hardware observation (`docs/research/terminal-input-jul31.md` §5.5) already characterized the shape as "original form and corrected form both appear, adjacent; i.e. the original is sent first, the correction sent next, and the delete operation in between never reaches the PTY" — this doc's job is to grind that observation down to file:line code evidence, not to re-discover it. CONSTRAINT: diagnosis only, no code change — this is a read-only research task per the council brief (`agent.B5`).

## 3. Strategy

Read `useTerminalTextDrain.js` in full (306 lines) and trace, event-by-event, what its `onInputCapture`/`onKeydownCapture`/`onCompositionStart`/`onCompositionEnd` handlers do with each DOM `inputType` a mobile IME can emit, then reconstruct the minimum event sequence that reproduces the exact observed string shape from `báo cáo` → `baoáo caoáo` and `ăn gì` → `aăn giì`. Reject any candidate mechanism that would predict a different shape (e.g. uniform per-character doubling, which is the already-closed, unrelated §2.1 mechanism). Name the single measurement — the `__akiTermInput` ring — that would confirm or kill the surviving hypothesis on real hardware, since no Android device is available in this environment.

## 4. Checklist

- [x] Read `useTerminalTextDrain.js` in full, cite every `inputType` branch.
- [x] Read `docs/research/terminal-vietnamese-ime-root-cause-5.md` (current chain head) to confirm the §2.1 mechanism does not also explain this shape.
- [x] Read `docs/research/terminal-input-jul31.md` §5.4/§5.5 for the exact observed strings and the discriminating detail already on record.
- [x] Reconstruct the event sequence per test word (`bao`→`báo`) against the code, character by character.
- [x] Check whether `deleteContentBackward` is excluded by the drain's inputType filter (it is not) and whether that matters given the drain's own resting-state assumption.
- [x] Identify the two surviving candidate mechanisms and the one hardware measurement that separates them.

## 5. Result

### 5.1 What the drain does with each `inputType`

`onInputCapture` (`useTerminalTextDrain.js:186-226`) is gated first by `if (composing || ev.isComposing) return` (`:188`) — while a composition is open, the drain touches nothing and defers entirely to xterm's own composition path. Once not composing, it explicitly passes through (does nothing, returns) exactly four `inputType` shapes (`:192-200`): anything starting with `insertComposition` (i.e. `insertCompositionText`), `insertFromComposition`, `insertFromPaste`, `insertFromDrop`. FACT: `deleteContentBackward`, `insertText`, and `insertReplacementText` are **not** in that exclusion list — grepped directly (`grep -n "insertComposition\|insertFromComposition\|insertFromPaste\|insertFromDrop\|deleteContentBackward\|insertText\|insertReplacementText" useTerminalTextDrain.js` returns matches only at `:193-196`, the four excluded types). Every other `inputType`, including both of those, falls through to the drain's universal body (`:202-225`): read `textarea.value` into `raw`; **if `raw` is falsy, return silently, sending nothing** (`:203`); otherwise empty the textarea, strip OpenKey's sentinel characters, and call `term.input(text, true)` (`:225`).

### 5.2 The drain's core assumption and where it breaks

The file's own comment states the model precisely (`:183-185`): "A no-local-echo terminal's textarea has exactly one correct resting value: empty. So the entire algorithm is 'read all of it and empty it'." FACT: this assumption holds for a human typing character-by-character, because each keystroke's `input` event is drained and the textarea returns to `''` before the next keystroke fires. ASSUMPTION (settled only by hardware capture, see §5.5 below): it does **not** hold for an IME that needs to look back at characters it itself already caused to be typed, because the drain has already erased that history out from under it by the time the IME tries to act on it — the textarea is a relay, not a buffer the IME can inspect after the fact.

### 5.3 Reconstructing `báo cáo` → `baoáo caoáo` — why it is a run, not per-character doubling

FACT, from `docs/research/terminal-input-jul31.md` §5.5: the same defect, observed directly on hardware, is characterized as "the original form and the corrected form both arrive, adjacent" and "the delete operation in between does not reach the PTY" — this doc's shape (`bao`+`áo`) is the same mechanism at word granularity rather than single-character granularity, and the task brief's framing ("not each char twice, but the whole pre-composition run followed by the composed run") is the correct read of it. Walking `bao` → `báo` through §5.1's rules: (1) user types `b`, `a`, `o` — each, if delivered as a plain (non-composing) `insertText` `input` event, is read by `onInputCapture`, drained, and sent via `term.input`, leaving the textarea at `''` after each keystroke — PTY has received `bao`. (2) Gboard's autocorrect/diacritic engine then needs to replace the tail of what it thinks is still in the field (`ao`) with `áo`. If that replacement fires as ordinary (non-composition) DOM events — `deleteContentBackward` for the 2 characters, then `insertText`/`insertReplacementText` of `áo` — the delete event lands on a textarea that the drain already emptied in step 1: there is nothing left to delete, so either no `input` event fires at all for a no-op delete, or it fires with `raw === ''` and is silently dropped by `:203`. The following `insertText` of `áo` then goes through normally and is appended: PTY now has `bao` + `áo` = `baoáo`. This reproduces the observed string exactly, and it discards, by construction, any hypothesis predicting uniform per-character doubling (e.g. reusing the §2.1 space/uppercase mechanism) — that mechanism doubles individual keypresses one-for-one via an uncancelled `_keyPress`, which would produce `bbaaoo`-shaped output, not a base run followed by a correction run; `docs/plan/done/terminal-input-jul31.md` §2.1 already records the §2.1 fix landing on real hardware with the Gboard shape unchanged, corroborating that these are different mechanisms.

### 5.4 `ăn gì` → `aăn giì` — same mechanism, earlier correction boundary

FACT: this string's correction boundary falls mid-word (`a`→`ă` corrected before `n` is even typed) rather than at word-end as in `báo`. ASSUMPTION, not verifiable from this repo's code (Gboard's correction-timing heuristic is closed-source, platform behavior): Gboard's dictionary/diacritic engine does not wait for a word boundary before correcting — it can correct a single already-committed character as soon as it recognizes the pattern, then continue accepting new keystrokes. Under §5.1's rules this still produces the same shape per corrected span: base char(s) drained and sent, correction's delete silently dropped against an already-empty textarea, correction's insert appended — `a` sent, then `ă` appended (`aă`), then `n` typed and sent normally (`aăn`); `g`,`i` sent (`gi`), then `ì` appended (`giì`). The only thing that varies between `báo` and `ăn` is *when* Gboard decides to correct, not *how* the drain mishandles the correction — the mechanism in §5.3 is unchanged.

### 5.5 Is the drain "supposed to" handle composition here — and is the delete genuinely dropped?

FACT: the drain's composition stand-down (`:188`, `:192-193`) is designed for a **real** DOM composition session — `compositionstart` → `compositionupdate`* → `compositionend`, with `ev.isComposing` true throughout and the final commit tagged `insertFromComposition` or `insertCompositionText`. In that shape the drain does nothing at all and defers to xterm's own composition handling (header comment `:39`, `"composition | xterm owns it -> the drain stands down"`). ASSUMPTION requiring hardware confirmation: whether Gboard's Vietnamese autocorrect on Chrome/Android actually wraps its corrections in real composition events, or fires plain `deleteContentBackward` + `insertText`/`insertReplacementText` outside any composition session. This is the fork the whole diagnosis pivots on, and it cannot be settled by reading this repo's code — it is a fact about Chromium-on-Android's IME bridge for Gboard specifically, which is why §6 names a concrete measurement rather than asserting an answer. **If plain delete+insert (no composition wrapper):** §5.3's mechanism applies exactly as traced — confirmed against the code, not refuted. **If real composition wrapper:** the drain correctly stands down per its own design, and the doubling would instead originate inside xterm's own composition-completion path (`Terminal.ts`'s composition helper, outside this file entirely) failing to reconcile Gboard's delete+replace against what it already sent — a different bug, in different code, that this file's drain cannot be blamed for or fixed to prevent. Both branches predict the same *observed string shape* (base run, then correction run, with the delete lost) because in both cases something along the pipeline drops the delete and only forwards the two inserts — so the string evidence alone cannot distinguish them; only the recorded `inputType`/`compositionstart` sequence in the ring can.

### 5.6 Why macOS/OpenKey and desktop Chrome are clean

FACT (established in `-5.md` §"the premise... does not hold for space" and its explanation of §5.4): OpenKey on macOS delivers its corrected syllable — the whole thing, diacritics included — as **one** synthetic keydown whose `event.key` is a multi-character string, which WebKit tags `keyCode 229`; that carrier is claimed once, either by the drain's `onInputCapture` (single `input` event, single `raw` read, single send) or by the drain's own `onKeydownCapture` 229-claim for Backspace/Enter (`:234-259`) — there is no separate "delete the old, insert the new" round-trip visible to the DOM at all, because OpenKey retypes the whole syllable as one unit rather than issuing an incremental correction against already-committed text. ASSUMPTION (plausible, not independently verified against a live desktop IME session in this task): a desktop system IME (e.g. a Vietnamese Telex/VNI IME on Windows/Linux Chrome, not exercised in the jul31 hardware run) would use genuine `compositionstart`/`compositionupdate`/`compositionend` for the whole syllable and commit once via `insertFromComposition`, which the drain explicitly passes through — again no incremental delete-then-reinsert against text the drain has already erased. FACT: Gboard is the one input path in this comparison set that is known (from the observed doubling itself) to perform an incremental **correction of already-committed text** rather than a single atomic commit of a whole unit — that behavioral difference, not a platform/browser difference per se, is what the evidence points to as the actual discriminator; whether it additionally uses composition events (§5.5's fork) changes *where* the bug lives but not *that* an incremental post-commit correction is the trigger.

## 6. The one measurement that settles §5.5's fork

The app exposes a pull-based debug ring for exactly this purpose (`useTerminalTextDrain.js:48-52,79-125`), reachable from any page running the drain — including the remote companion browser session on the Android device itself. On the Android/Gboard device, open the remote terminal page, then in Chrome's own DevTools console (remote-debug the Android Chrome tab from a desktop via `chrome://inspect`, or use an on-device console if available) run:

```js
__akiTermInput.clear()
// then type "báo" into the terminal on the device
__akiTermInput.dump()
```

What confirms the plain-delete-outside-composition branch (§5.3/§5.5 first case): the dumped ring shows **no** `compositionstart`/`compositionend` entries around the `báo` keystrokes, and shows `drained` entries whose `inputType` includes `deleteContentBackward` interleaved with `insertText`/`insertReplacementText` — or, if the delete truly produces no `input` event at all on an empty textarea, the ring simply shows three `drained` entries for `b`,`a`,`o` (or one merged `bao`) followed directly by a `drained` entry for `áo` with no delete entry between them at all (the silent-drop case, `:203`, never even reaches `record()` since the function returns before recording anything for a falsy `raw` — so its absence from the ring is itself the confirming signal, not a gap in the ring). What confirms the composition-wrapper branch (§5.5 second case) instead: the ring shows `compositionstart` immediately before the correction, `input-passed` entries (`:198`, recorded for the four excluded `inputType`s) around it, and a `compositionend` — in which case the doubling is not this file's doing and the next step is tracing xterm's own `_finalizeComposition`/composition-helper path against the same event capture, which is out of this doc's scope. Either outcome is a strict confirm/refute of §5.5's fork with no further inference needed — `__akiTermInput.status().counts` also gives a fast sanity check (`compositions` incrementing or not) before reading the full dump.

### Verification

Not verified on hardware — no Android device is available in this task's environment; this is a code-level trace against a hardware observation already on record (`docs/research/terminal-input-jul31.md` §5.5), not an independent repro. The §5.3/§5.4 shape reconstruction is verified by matching character-for-character against the exact strings already recorded on real hardware (`báo cáo`→`baoáo caoáo`, `ăn gì`→`aăn giì`, `tét`→`teét`, `TÉT`→`TEÉT`), which is the strongest verification available without a device, but it is a fit to a recorded string, not a live capture of the `inputType` sequence — §5.5's fork (drain-level plain-delete vs. xterm-level composition mishandling) is explicitly **not resolved** by this doc; §6 names the measurement that would resolve it.

### Corroborating links

- `docs/research/terminal-input-jul31.md` §5.5 — the original hardware observation this doc traces to code, and §5.4 — the OpenKey discriminating case reused in §5.6.
- `docs/research/terminal-vietnamese-ime-root-cause-5.md` — confirms the §2.1 double-space/uppercase mechanism is unrelated (different shape, already hardware-confirmed the Gboard defect is untouched by that fix).
- `docs/plan/done/terminal-input-jul31.md` §2.2 — the plan entry this doc is written to close out diagnostically (see Decision).
- `src/composables/useTerminalTextDrain.js:183-226` (the drain's universal body and its inputType exclusion list), `:39` (header table's composition row), `:48-52,79-125` (`__akiTermInput` debug surface).

## 7. Decision

**Action** — none in code (diagnosis only, per task scope). `docs/plan/done/terminal-input-jul31.md` §2.2 updated to point at this doc and to record status as diagnosed-not-fixed, not closed.

**No action (yet)** — no fix is proposed or implemented here; §8 below states fix-direction tradeoffs for the owner's decision without picking one, since picking one requires §6's measurement first.

**Follow-up research** — a hardware capture per §6 is required before this can move from "candidate mechanism" to "confirmed root cause"; if the Android trace shows the composition-wrapper branch instead, follow-up research shifts scope to xterm's own composition-completion path, outside this file.

**Cross-references** — `docs/index.md` (this doc added under the research list), `docs/plan/done/terminal-input-jul31.md` §2.2 (updated by this same task).

## 8. Fix direction and its cost (for the owner, not implemented here)

Two shapes were considered, neither implemented:

**(a) Discriminate `deleteContentBackward` outside composition and translate it to real backspace bytes sent to the PTY** (mirroring the existing 229 Backspace claim at `:237-243`, but for the ordinary, non-229 case) instead of silently dropping it when the textarea has nothing left to remove. Cost: this is the "one branch removed, not one added" invariant's opposite — `-5.md`'s fix explicitly celebrated removing a classification branch (`docs/plan/done/terminal-input-jul31.md` §2.1: "bớt một nhánh phân loại, không thêm guard"); this direction reintroduces `inputType`-based classification, which the file's own header (`:22`, "EXCLUSIVITY IS BY CONSTRUCTION, NOT BY CLASSIFICATION") argues against on principle. It would only be correct in the plain-delete branch of §5.5's fork — if the composition-wrapper branch is confirmed instead, this fix does nothing.

**(b) Track a shadow buffer of "already-sent-but-still-theoretically-editable" characters** so a late delete can be reconciled against PTY-side state rather than DOM-side state. Cost: this reintroduces exactly the app-owned text-state tracking the 1.22.0 architecture deliberately deleted (`useTerminalInput.js`, the overlay textarea) — the header comment (`:1-6`) frames the whole file's existence as replacing that approach because it kept getting details wrong; reviving any form of shadow state is a direct regression against that decision and should not be taken lightly.

**If confirmed as the composition-wrapper branch instead**, the fix is not in this file at all — it would mean xterm's own composition-completion handling needs correction or a narrower stand-down condition, which is out of this doc's ownership and would need its own investigation against `Terminal.ts`.

**Bottom line for the owner**: no fix direction here can both (a) preserve the "exactly one input path, no branch-by-classification" invariant `-5.md` established and (b) correctly reconcile a delete that targets text the drain has already forwarded and erased — those two goals are in tension by construction, and which cost is worth paying should wait for §6's measurement rather than be decided from the code trace alone.
