# Plan — DEV/BUILD popup buttons: disabled, never hidden

Status: **Verified on macOS by the owner, 2026-07-30.** `ProjectTable.vue` always renders both buttons, disables each on `resolved === ''`, and carries the two tooltip strings below verbatim. One thing this doc did not anticipate: `.popup-item.popup-disabled` sets `pointer-events: none`, which also suppresses the native `title` — so the disabled-state tooltip, the entire affordance this plan rests on, would never have appeared. Disabled DEV/BUILD keep hit-testing (`pointer-events: auto`, one scoped rule) with the click guarded in the handler instead. Confirmed at runtime: the popup renders DEV/BUILD correctly, including the disabled + tooltip state. Council finding (`cmd-surface`, akiflow session
`2026.07.30-0213-terminal-usage-ui-backlog`), Phase B doc only. No code changed by this doc.

## The defect (source-read, not owner-reported guess)

`src/components/ProjectTable.vue:195` — `<div v-if="getDevCmd(p) || getBuildCmd(p)" class="popup-run-row">`
wraps the entire DEV/BUILD row. `:196`/`:199` additionally gate each button individually on its
own resolved command (`v-if="getDevCmd(p)"` / `v-if="getBuildCmd(p)"`). When both resolve empty,
the row — both buttons — is absent from the DOM entirely, not merely styled off.

The resolution functions themselves are **already correct** and need no change:

```js
// ProjectTable.vue:735-741
function getDevCmd(p) {
  return p.dev_cmd_override || projectRuntime.value[p.id]?.stack_info?.dev_cmd || ''
}
function getBuildCmd(p) {
  return p.build_cmd_override || projectRuntime.value[p.id]?.stack_info?.build_cmd || ''
}
```

`dev_cmd_override` / `build_cmd_override` are `Option<String>` on the persisted project record
(`src-tauri/src/projects.rs:94-96`, default `None`). Override wins when present; the detected
stack's default is the fallback. **The defect is entirely in the existence-gate, not the
resolution order** — owner's report ("nó bị dính theo stack") and this source read agree.

## Why the severity is larger than the owner's report states

`check_project_stack_blocking` (`src-tauri/src/system.rs:783-838`) only produces a non-empty
`dev_cmd`/`build_cmd` for three detected stacks — Tauri, Nuxt, and plain Node. Every other project
type (Python, Go, static sites, anything without `package.json`, `src-tauri`, or a Nuxt config)
gets `("", "")` from stack detection. For such a project that has never had an override set, the
row-level `v-if` removes the DEV/BUILD row from the popup **entirely** — there is no visible
control that would prompt the user toward Project Settings' RUN COMMANDS section to add one. This
is not a cosmetic gap on projects the stack already handles; it is a silent, total loss of the
DEV/BUILD affordance for every project outside those three detected stacks, discoverable only by
already knowing the feature exists and going to look for it in Project Settings unprompted.

## The precedence rule (single enforceable sentence)

**For each of DEV and BUILD independently:
`resolved = (override ?? '').trim() || stackDefault || ''` — the button is always rendered; it is
`disabled` exactly when `resolved === ''`; setting a non-empty override in Project Settings is
what makes it live, regardless of what the detected stack does or does not provide.**

Implementation shape (for whoever executes this doc): remove the outer `v-if` at `:195`; change
each button's individual `v-if` to a `:disabled`/`:class="{ 'popup-disabled': resolved === '' }"`
guard (mirroring the pattern already applied for `localBlocked(p)` at the same lines), and no-op
the click handler when `resolved === ''`.

## Disabled-state affordance

No new element (UI Extreme Narrow, `CLAUDE.md`). Reuse the shipped `popup-disabled` class + `title`
tooltip pattern — the same mechanism already used for `local_path_missing`
(`docs/feat/open-popup.md` §2: "every LOCAL item ... is dimmed with a 'Local folder missing on
disk' tooltip"). Two reasons can make DEV/BUILD disabled; they are mutually exclusive in practice
(a missing local folder blocks every LOCAL item regardless of command state) and resolved by
priority:

1. **`localBlocked(p)` is true** → keep the existing copy unchanged: `"Local folder missing on
   disk"`. DEV/BUILD must not diverge from every other LOCAL item's established wording for this
   state.
2. **Else, `resolved === ''`** → new copy, stating the problem then the exact next action
   (`RULE-content-write` B1):
   - DEV: `"No dev command detected — set one in Project Settings"`
   - BUILD: `"No build command detected — set one in Project Settings"`

When enabled, the tooltip is unchanged from today: `localTitle(p) || getDevCmd(p)` /
`getBuildCmd(p)` — shows the exact command that will run. That path is already correct.

## Empty vs. unset — decided: ONE state (explicit non-goal recorded)

A plain `<input type="text">` bound via `v-model` (`ProjectConfigModal.vue:51`, `:60`) cannot
express `null` once touched — typing then deleting always produces `Some("")`, which
`resolved = (override ?? '').trim() || stackDefault` already treats identically to `None` (both
fall through to the stack default). This is not a bug to fix; it matches the owner's own framing
("không có thì giá trị mặc định rỗng" — no override ⇒ default applies) and is the simplest correct
reading of "override, else default."

A genuinely distinct third state — "force off despite a detected stack default" (e.g. a
misdetected Tauri project whose `tauri dev` script is meaningless for this project) — is a real
hypothetical but requires a **new affordance** the current text field cannot carry: a tri-state
toggle or a distinct sentinel value. Nothing in the owner's wording or in the reported defect asks
for this, and Extreme Narrow forbids adding a new control without concrete justification that
isn't present here.

**Non-goal, recorded explicitly so it is not silently designed in and not later assumed to be an
oversight:** this plan does not add a way to force-suppress a resolved default. If that need
surfaces later, it is a separate, standalone request with its own UI-cost discussion — not a
follow-on to this fix.

## Fallback vs. prefill — decided: keep fallback (placeholder-only), do not prefill

`ProjectConfigModal.vue:48-64` shows the resolved stack default as a `<label>` hint and an
`:placeholder`, and leaves the `<input v-model="editingProject.dev_cmd_override">` itself empty
when there is no override — it does not copy the stack default into the field's actual value.

**Decision: keep this.** Prefilling the input with the stack default at render time (or on first
save) would convert `None` into `Some(stackDefault)` as a side effect of merely opening the
config modal, which:

- Permanently freezes that project to whatever the stack default was at the moment the modal was
  opened, even if stack detection later changes (e.g. the project's lockfile changes from
  `package-lock.json` to `pnpm-lock.yaml`, which changes the `{pm}` prefix `check_project_stack_blocking`
  computes) — the override would silently stop tracking the live default.
- Defeats the empty-vs-unset decision above: a prefilled field is never "unset" again, so a truly
  untouched project would immediately and irreversibly acquire an override the first time anyone
  opens its config modal, even if they change nothing else and close it.
- Contradicts `METHOD-ux-psych` A4 (good default + escape hatch): the default should stay live and
  dynamic until the user deliberately opts out of it by typing something of their own, not get
  baked in as a side effect of viewing the form.

Placeholder-only is the correct shape and needs no change.

## Doc-sync obligations — done

- `docs/feat/open-popup.md` §2 ("DEV + BUILD") states buttons are always shown and disabled (with
  tooltip) when no command resolves.
- `docs/arch/terminal-stack.md` — not directly touched by this defect (it owns the in-app PTY
  layer, not the popup); cross-references checked against the in-app-launch doc, no drift found.
- `README.md` and `src/components/modals/IntroModal.vue` both describe the always-shown,
  disabled-when-empty behavior.

## Cross-refs

- `docs/feat/open-popup.md` §2 — current DEV/BUILD feature description.
- `docs/plan/done/dev-build-in-app-launch.md` — the companion defect (#7) on the same two buttons;
  deliberately a separate doc (different definition of "done," different verification method,
  different blast radius — see that doc's own framing).
- `src/components/ProjectTable.vue:203-208,747-753` — the fix and the (already-correct)
  resolution functions.
- `src/components/modals/ProjectConfigModal.vue:47-65` — the RUN COMMANDS override inputs.
- `src-tauri/src/system.rs:783-838` — `check_project_stack_blocking`, the stack-detection source
  of the severity-driving `("","")` case.
- `src-tauri/src/projects.rs:94-96` — the persisted `Option<String>` override fields.
