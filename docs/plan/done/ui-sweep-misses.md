# UI sweep misses — projects table + confirm dialog + GlobalNote modal

Four small, binary-correct UI fixes surfaced in the 2026.07.30 backlog triage (owner items #5, #10,
#11). Grouped here because each has one obviously-correct answer and lands entirely inside the
existing design system — no new pattern, no persistence, no owner judgment call. Contrast
`project-visibility-toggle.md` (#12), which is a new capability and does not belong in this doc.

No version number is introduced by this doc (release.A5). Status: **all four implemented (Phase B).**
Runtime confirmation for #5, #11a, #11b is tracked centrally in
[`verify-pending.md` §UI](verify-pending.md#ui) (U1/U2/U3) rather than here. #10 needs no runtime
check — its own section below already concludes "none of consequence". Doc-sync obligation closed
2026-07-31 (see note below) — only runtime verification remains, tracked in `verify-pending.md`.

## Doc-sync note (read before implementing)

**No `docs/feat/` doc owns the projects table at all** (plan-docs' drift finding D-6, this session).
It is the app's primary surface and is currently undocumented. #5 (this doc) and #12
(`project-visibility-toggle.md`) both change that surface and there is nowhere to sync the change to
today. Not fixed in this doc, and not owed by it either — named so the gap is visible, not
rediscovered next session; creating that doc is separate scope.

#11 (GlobalNote) synced 2026-07-31 to `docs/feat/project-task-list.md`, which already documents the
Global Note / task-list shared engine (`useTaskCollection`, `TaskListPanel.vue`, `NotesField.vue`) —
its "Global" bullet under "Reached by" now describes the `:deep()` fixed `min-height`/`resize:
vertical` sizing (#11b) and the local 700px narrow-mode padding repeat (#11a), read from
`src/components/modals/GlobalNoteModal.vue:66-119`.

---

## #5 — projects table: wrong column flexes

**Current mechanism.** `src/components/ProjectTable.vue:756-757`:
```css
/* project-info | tasks | git | last-sync | action (OPEN + select-push) | sync (PUSH/DRY/PULL + LOG + gear) */
--grid-cols: 12rem 2.5rem 2.5rem 2.5rem 7rem 1fr;
```
6-column CSS Grid (`grid-template-columns: var(--grid-cols)` on both `.grid-header` and `.grid-row`).
The trailing `1fr` belongs to `.col-sync` (last column) — that is what absorbs extra width when the
parent widens, not `.col-project-info` (fixed `12rem`). Narrow-mode override,
`ProjectTable.vue:1274`: `--grid-cols: 6.5rem 2.1rem 1.9rem 2.5rem 4.2rem 1fr` — same shape, project
column still fixed at `6.5rem`, sync still owns all the flex.

**Decided fix.** Split the flex weight instead of swapping which single column owns 100% of it:
```css
/* wide */   --grid-cols: minmax(12rem, 2fr) 2.5rem 2.5rem 2.5rem 7rem minmax(6rem, 1fr);
/* narrow */ --grid-cols: minmax(7.5rem, 2fr) 2.1rem 1.9rem 2.5rem 4.2rem minmax(4.5rem, 1fr);
```
`7.5rem` is the literal `+1rem` the owner asked for over the current `6.5rem` narrow value.

**Why not a blind swap (project `1fr` / sync fixed).** `.col-sync`'s own inline comment
(`ProjectTable.vue:756`) says it holds PUSH/DRY/PULL + LOG + gear — real button content, not empty
chrome. Its true minimum rendered width was never measured (headless dev box, macOS-only app) — a
fixed px guessed wrong risks clipping or wrapping those buttons. `minmax(Xrem, 1fr)` keeps sync
elastic as a safety margin while project-info (`2fr`) absorbs the majority of any extra width — gets
the owner's outcome without betting on an unverified number.

**Why this lands in the existing pattern.** `ProjectTable.vue:1268-1273` already documents a history
of tuning every one of these rem literals by hand with an inline comment explaining each ("widened
back up from an initial 4.8rem/40% guess..."). That satisfies `ui.A3`'s arbitrary-value exception
(no scale value fits a bespoke 6-column content-driven grid; each value used exactly once, in this
one custom property; already commented) — this is the file's own established practice, not a new
violation. No token extraction needed (not a repeating value, `design.A2` Rule-of-Three doesn't
apply).

**Unverified at runtime:** tracked as [`verify-pending.md` U1](verify-pending.md#u1--projects-table-column-flex-reshape).

---

## #10 — confirm-delete phrase is not selectable

**Current mechanism / root cause.** `src/assets/main.css:41-68`: app-wide default is
`user-select: none` on `body`; opt-in is `.u-select-text`, already documented for exactly this
content class ("paths, emails, pair codes"). The delete-confirmation dialog is SweetAlert2
(`src/components/DialogHost.vue`), which renders its DOM appended to `document.body` — outside this
app's Vue component tree/scoping — so it inherits `body`'s `user-select: none` by plain CSS
inheritance. The dialog's typed-confirmation `<input type="text">` is already selectable (global
`input`/`textarea` opt-in, `main.css:58-60`) — not the bug. The actual caller,
`src/composables/useSync.js:170-188`:
```js
html: body + `Type the project name <b>${safeName}</b> to confirm:`,
...
requireText: project.name,
```
Only the `<b>${safeName}</b>` span is unselectable; everything else in the dialog is already fine.

**Decided fix.** `<b class="u-select-text">${safeName}</b>` — one class, on the one span. Scope is
deliberately narrow: only the phrase, not the dialog's wrapper text, not the whole dialog body, and
not every dialog app-wide — matches `.u-select-text`'s documented per-element opt-in intent and
CLAUDE.md's "less is more" (no new class, no CSS change, no new DOM element).

**Injection check (pre-answered, this is the obvious objection to making delete-confirmation text
copyable).** `useSync.js:154-159` already HTML-escapes both the file list and the project name before
they reach `html:` — `const safeName = escHtml(project.name)` — with an inline comment explaining why
("Filenames are attacker-controllable content... rendered into the LAST confirmation before permanent
deletion... this SELECT dialog below already escapes; so does this one now"). Adding `.u-select-text`
to an already-escaped span changes nothing about what can be injected; it only changes whether the
resulting text node can be copied. Not a new attack surface.

**Safety-gate tension (resolved on the record, per mandate).** Does copyable text weaken
type-to-confirm? No: the gate's real property is "submit a matching string via a deliberate 5-step
action" (select → copy → focus input → paste → click confirm), not "type from memory with zero
assistive copy." An accidental drag-select elsewhere in the dialog still cannot trigger deletion —
only the phrase itself, already visible and already the thing the user is being asked to reproduce,
becomes copyable. Meanwhile the removed friction is real and asymmetric: project names are
free-form (dashes/digits/mixed case), so a mistype means a validation error and a retry with zero
safety benefit over paste (paste still requires hitting the right input and clicking confirm).
Net: copyable phrase removes error-retry friction without shortening the deliberate-action chain the
gate relies on.

**Unverified at runtime:** none of consequence — pure CSS inheritance fix on an already-escaped
string.

---

## #11a — GlobalNote modal missed the narrow-mode refactor

**Current mechanism.** Narrow-mode SSoT is one global `@media (max-width: 700px)` block in
`src/assets/main.css:1362-1419`. It documents (L1368-1371) that a modal which scope-overrides
`.modal-body`/`.modal-footer` under its own local class names needs a **local repeat** of the narrow
override, because Vue's `data-v-*` scoping attribute gives the scoped selector higher specificity
than the global unscoped rule regardless of source order. Four modals already carry that local
repeat: ChangelogModal, ClaudeProfileModal, SshConfigModal, UpdateModal (verified — each has its own
`@media (max-width: 700px)` block with a comment citing "SSoT 700px, main.css").

`src/components/modals/GlobalNoteModal.vue` is dual-classed the same way (`"modal-body note-body
scrollable"` at L6, `"modal-footer note-footer"` at L29) and its own `<style scoped>` sets
`.note-body { padding: 12px 16px }` (L49-51) and `.note-footer { ... padding: 8px 16px }` (L80-86) —
but has **zero** `@media`/`700px` anywhere in the file. On a narrow window it keeps full padding
while every sibling with the same collision shrinks. This is the 5th case of the pattern and the
only one missing its repeat.

**Sweep (checked, not expanded).** Every modal in `src/components/modals/` was checked for the same
override-collision shape. Only 5 files scope-override `.modal-body`/`.modal-footer`/`.modal-header`
at all: Changelog, ClaudeCleanup, ClaudeProfile, ClaudeSetting, GlobalNote. Of those, GlobalNote is
the only one without its local `@media 700px` repeat (ClaudeCleanup and ClaudeSetting each carry
their own). The remaining modals (ExternalTerminals, Git, ProjectTasks, RefreshSettings) never
scope-override those classes, so they already inherit the global narrow padding with no collision —
nothing to fix there.

**Decided fix.** Copy the 4-modal pattern verbatim:
```css
@media (max-width: 700px) {
  .note-body { padding: 10px; }
  .note-footer { padding: 8px 10px; }
}
```
**Why this lands in the existing pattern.** Zero new mechanism — the 5th application of an already
4x-precedented repeat.

**Unverified at runtime:** tracked as [`verify-pending.md` U2](verify-pending.md#u2--globalnotemodal-narrow-mode-padding-repeat).

---

## #11b — GlobalNote textarea min-height (filed as VERTICAL-SPACE, not narrow-mode)

**This is explicitly not a 700px-width issue.** Filing it in the narrow-mode bucket is how it gets
"fixed" at the wrong breakpoint — it is about vertical room on any window height, independent of the
narrow-mode width breakpoint above. Keep it a separate line item from #11a even though both live in
the same file.

**Current mechanism.** `src/components/tasks/NotesField.vue:77-91` (the base component) has no
`min-height` at all by default — `resize: none; overflow-y: hidden; field-sizing: content;` — and
accepts a `:rows` prop (default 2) as its sizing hint. `field-sizing: content` is a Chromium-only
property; WKWebView (this app's Tauri runtime is WebKit/Safari) does not implement it, so it is
inert there — but inert is not the same as absent, and a box relying on it for its only sizing
signal has no explicit CSS height for `resize` to use as a drag baseline in engines that ignore it.

**First fix attempt (reverted, see below).** Dropped `min-height` entirely and lowered `rows` from
8 to 2, relying on `field-sizing: content` + `rows="2"` alone to size the box. This broke the
resize-drag handle in practice (WKWebView) on top of not reliably landing at 2 rows either.

**Fix applied.** `src/components/modals/GlobalNoteModal.vue`'s `:deep()` override now sets a
*derived* `min-height: 42px` (13px font-size x 1.6 line-height x 2 rows = 41.6px, rounded up — not
a guessed round number like the old 320px/190px) and resets `field-sizing: fixed` for this box only,
so sizing is driven solely by `min-height`/`max-height: 60vh`/`resize: vertical` — the same
mechanism this file always used successfully; only the floor's px value, and the accidental
dependence on an unsupported auto-sizing property, were ever wrong.

**Unverified at runtime:** tracked as [`verify-pending.md` U3](verify-pending.md#u3--globalnotemodal-textarea-min-height-re-tune).

---

## Severity / effort / coupling summary

| Item | Severity | Effort | Pattern | Coupling |
| :-- | :-- | :-- | :-- | :-- |
| #5 grid-cols reshape | material | trivial | existing (`--grid-cols` literal-rem convention) | independent |
| #10 `.u-select-text` on confirm phrase | material | trivial | existing (`.u-select-text` utility) | independent |
| #11a narrow-mode repeat | material | trivial | existing (4 precedents) | independent |
| #11b min-height re-tune | trivial | trivial | existing mechanism, re-tuned number | independent |

All four are independent — shippable in any order or any subset, no real file collision (each lands
in a different file: `ProjectTable.vue` CSS / `useSync.js`+`DialogHost.vue` / `GlobalNoteModal.vue`).
Priority (impact × effort, all low-effort): #10 and #11a first (cheapest, closes a known pattern gap
and a daily friction point), #5 next, #11b last (trivial-severity cosmetic).
