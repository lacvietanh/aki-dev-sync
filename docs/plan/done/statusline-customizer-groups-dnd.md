# Statusline Customizer - quota/cache groups + drag-and-drop reorder

Owner file: `src/components/modals/ClaudeSettingModal.vue`.
Backend it feeds: `src-tauri/src/statusline.rs` (persisted config contract:
`{ fields: [{key, enabled, color}], thresholds }`, localStorage key `aki-statusline-config`).

## Current structure (before this change)

- `cfg.fields` is a flat array rendered by one `v-for`, in generation/display order.
- One special case, `isSubField(key)`: `rate_reset` is flagged as a sub-field of `rate_limits`  - 
  rendered indented, dashed border, no color/reorder controls of its own. This was the only
  "grouping" concept, hardcoded to one pair of keys.
- `rate_limits` was a single toggle covering **both** the 5h and 7d windows; `rate_reset` was a
  single toggle covering the ETA suffix for **both** windows too - no way to show only one
  window's reset ETA.
- `cache_pct` and `cache_tokens` were two fully independent standalone rows (own toggle, own
  color-or-locked slot, own reorder buttons) with no visual relationship to each other.
- Reordering was two chevron buttons (`move(idx, dir)`) doing an array swap - no drag, no
  animation.
- Rust catalog (`statusline.rs::default_config()` + `render_field()` match arms) mirrors the same
  flat key list: `identity, cwd, model, context, rate_limits, rate_reset, cache_pct, cache_tokens,
  session, git_branch`.

## Problems being fixed

1. `rate_limits` (one row) + `rate_reset` (an indented sub-row under it) reads as unrelated
   elements, and there is no way to show the reset ETA for only 5h or only 7d.
2. `cache_pct` / `cache_tokens` have no visual grouping despite being conceptually one "cache"
   concern.
3. Reordering has no drag interaction and no landing-position feedback.
4. The `isSubField` special case is exactly the kind of hardcoded-per-group-type branch
   `RULE-design-core` (Law 3/4/5) flags - extending it to a second group (cache) would mean a
   second special case, not a reusable shape.

## Group model (declarative, one shape for every row)

Single new concept: a **row** is either a bare field or a **group**. Both are described by the
same data shape so the template never forks per group identity - only group *data* differs.

```
Row
 └─ line[]              (a group renders 1..N lines; a bare field is a group of exactly 1 line)
     └─ control[]        one of:
         { type:'master', keys:[...], label }   - synthetic checkbox, no own cfg.fields entry,
                                                   get/set proxies the enabled flags of `keys`
         { type:'toggle', key, label }           - cfg.fields[key].enabled
         { type:'color',  key }                  - cfg.fields[key].color (only rendered if the
                                                    key is in FIELD_COLOR_EDITABLE)
```

- **Quota group** (`id: 'quota'`, keys `rate_limits_5h, rate_reset_5h, rate_limits_7d,
  rate_reset_7d`): 2 lines, no master. Each line = `[toggle main] [toggle reset]`.
- **Cache group** (`id: 'cache'`, keys `cache_pct, cache_tokens`): 1 line = `[master "Cache"]
  [toggle "%"] [toggle "token"] [color]`. The master is a computed proxy (checked when either
  member is enabled; setting it flips both members together) - it has no backing `cfg.fields`
  entry of its own, so it costs zero schema surface.
- **Bare fields** (identity, cwd, model, context, session, git_branch): modeled as a 1-line,
  1-control group `{ toggle: key } + optional color`, rendered through the exact same
  `<GroupRow>`-shaped markup - this is what makes it "one structure," not two.

`ROWS` is derived from `cfg.fields` order at render time: walk the flat array, and whenever the
next N keys exactly match a known group's key set (in any relative sub-order), collapse them into
one group row; everything else is a bare-field row. This keeps `cfg.fields` (the thing persisted
and sent to Rust) as the single flat source of truth - the grouped/bare distinction is a pure
read-time projection for the template, never a second stored structure.

## Change 1 - Quota group, independent 5h/7d Reset ETA

**Schema change (additive):** `rate_limits` → split into `rate_limits_5h` + `rate_limits_7d`.
`rate_reset` → split into `rate_reset_5h` + `rate_reset_7d`. No struct field changes in
`StatuslineField` (still just `{key, enabled, color}}`) - only the catalog of known key strings
changes, in both `statusline.rs::default_config()`/`render_field()` and the Vue `CATALOG` /
`defaultLocalConfig()`.

**Backward compatibility:** `loadCfg()` migrates an old saved config on load - if `rate_limits` or
`rate_reset` keys are present, their `enabled`/`color` state is copied onto the new
`rate_limits_5h`/`rate_limits_7d` (resp. `rate_reset_5h`/`rate_reset_7d`) entries and the old keys
are dropped before the config is used or re-saved. A user's existing toggle/color choices survive
exactly (same state applied to both new windows, since the old config couldn't distinguish them).

**Rust rendering:** `render_field()` gets two match arms, `rate_limits_5h` and `rate_limits_7d`,
each replacing the old single `rate_limits` arm - same `rate_block` helper, each now checks only
its own sibling reset flag (`rate_reset_5h` / `rate_reset_7d`) instead of one shared flag. The two
blocks are independent `$g_rate_limits_5h` / `$g_rate_limits_7d` script variables and join into the
final statusline through the existing generic " | " separator loop, same as every other top-level
field - the previous double-space-no-pipe join between 5h/7d was an artifact of them being one
field; independent control requires independent field identity, so they now join like any other
two adjacent fields. This is a minor, intentional visual change, called out here rather than
hidden.

**Label wording:** `.color-locked` "locked" text → recessed/dimmed "Dynamic color" label (same
tooltip content, just reworded per `RULE-content-write` - state the fact plainly, no jargon).
Visually: lower contrast color + no uppercase/letter-spacing shout, consistent with extreme-narrow
(communicates via styling, not a new element).

## Change 2 - Cache group

No Rust/schema change - `cache_pct` and `cache_tokens` keep their existing keys, `enabled`, and
`color` semantics untouched. Purely a Vue-side visual regrouping via the group model above:
`[ ] Cache  [ ] %  [ ] token  [Color]` in one row, no separator between the three controls, one
shared color select bound to `cache_tokens` (the only color-editable member).

## Change 3 - Drag-and-drop reorder

- Native HTML5 DnD (`draggable`, `dragstart`/`dragover`/`drop`/`dragend`) - no new dependency.
- Draggable unit = **row** (bare field or whole group), never an individual group member - a group
  row carries all of its member keys and moves them as a contiguous block within `cfg.fields`.
- `<TransitionGroup>` wraps the row list (`name="row"`, keyed by row id) so the reorder animates;
  CSS-only transform/opacity transition, no JS animation library.
- Visual affordance within extreme-narrow: a small grip icon (`fa-grip-vertical`) as the drag
  handle, `.dragging` class (reduced opacity + slight scale) on the row being dragged, and a thin
  accent-colored insertion-line pseudo-element on the row currently under the pointer (`.drop-before`
  / `.drop-after`) - no new row, no extra text, communicated entirely via existing-element styling.
- On `drop`, the flat `cfg.fields` array is rebuilt in the new row order (each row expands back to
  its 1 or N keys) - same array identity `cfg` stays reactive, so the existing
  `watch(cfg, ..., {deep:true})` → `localStorage.setItem` persistence and the `previewHtml`
  computed both update automatically; no new watcher needed.
- Old chevron up/down buttons are removed (replaced by drag) since the row-level DnD fully
  supersedes them at equal or better information density.

## Verification

- `npm run build` - production build passes.
- `cd src-tauri && cargo check` - Rust compiles, no warnings introduced.
- Manual/logical check: default config renders identical statusline output to before the change
  (5h+7d both enabled, no reset ETA) - confirmed by reading through the generated bash for the
  default catalog.
- Migration check: an old-shape saved config (`rate_limits`/`rate_reset` keys) loads without
  throwing and produces the equivalent new-shape config.

## Shipped (post-implementation notes)

Implemented as planned, no deviations of substance:

- `src-tauri/src/statusline.rs`: `default_config()` now emits `rate_limits_5h`, `rate_reset_5h`,
  `rate_limits_7d`, `rate_reset_7d` in place of the old `rate_limits`/`rate_reset` pair (defaults:
  both windows enabled/white, both resets off/grey - same effective default statusline as before).
  `render_field()`'s single `"rate_limits"` match arm was replaced by two arms, `rate_limits_5h`
  and `rate_limits_7d`, each independently checking its own `rate_reset_5h`/`rate_reset_7d` sibling
  flag. No `StatuslineField`/`StatuslineConfig` struct changes - only catalog key strings changed,
  so no serde/`#[serde(default)]` concern (unknown/removed keys just fall through the existing
  `_ => {}` no-op arm, exactly like `rate_reset` always did).
- `src/components/modals/ClaudeSettingModal.vue`:
  - `GROUPS` descriptor (quota, cache) + `rows` computed projecting `cfg.fields` into
    group-or-bare-field rows, per the group model above - implemented exactly as designed.
  - `migrateOldKeys()` added to `loadCfg()`: an old-shape saved config's `rate_limits`/
    `rate_reset` enabled+color state is copied onto both new per-window keys, then the old keys
    are dropped, before the "append missing keys" step runs.
  - `.color-locked` → `.color-dynamic` ("Dynamic color" label), same tooltip text, recessed via
    lower contrast + no uppercase/letter-spacing shout (was `color:#4b5563` uppercase; now
    `color:#475569` normal-case, smaller).
  - Cache group's `Cache` master toggle is a pure UI convenience (checked = all members enabled,
    click sets both members to the same value) - no new `cfg.fields` entry, no schema surface.
  - Chevron up/down `move()` removed entirely; native HTML5 DnD (`draggable`, `dragstart`/
    `dragover`/`drop`/`dragend`) replaces it, row-granularity only (`onDragStart`/`onDragOver`/
    `onDrop`/`applyRowOrder`). `<TransitionGroup name="row">` wraps the row list for the reorder
    animation (`.row-move` transform transition).
  - Visual affordance: `fa-grip-vertical` drag handle, `.dragging` (opacity 0.4) on the source row,
    `.drop-before`/`.drop-after` (inset box-shadow edge line, accent color) on the row under the
    pointer - no new row/label, per extreme-narrow.
  - `previewHtml`/`renderField()` updated in lockstep with the Rust rendering: `rate_limits_5h`/
    `rate_limits_7d` are now two independent preview segments (each checks its own reset flag);
    joined by the same generic `" | "` separator as every other field - the old double-space,
    no-pipe join between 5h/7d (an artifact of them being one field) is gone by construction now
    that they're independent fields, matching the intentional Rust-side change noted above.

**Verification actually run:**
- `npm run build` - passed (Vite production build, no new warnings beyond pre-existing chunk-size
  notice).
- `cd src-tauri && cargo check` - passed, no warnings.
- Read through the generated match arms for the default catalog by hand to confirm 5h/7d output
  is byte-for-byte equivalent in content to the old combined arm (only the join separator between
  them changed, as documented above).

---

## Round 2 - group cohesion, model/effort split, print-shape hints

Feedback after round 1 landed, addressed in the same plan rather than a new doc since it is the
same feature.

### 1. A group must not be split by ` | ` - the real fix

Round 1 left 5h and 7d joined by the generic ` | ` separator, reasoning that they were now
independent fields. That was backwards: they are independent *toggles* inside one *visual group*,
and the separator is what broke the group apart on screen. The same applied to `cache_pct` /
`cache_tokens`.

The fix is structural rather than per-field, so it cannot drift again:

- `GROUPS` in `src-tauri/src/statusline.rs` is the backend's list of "these keys render as one
  block". A `join_sp` shell helper joins a group's enabled members with a single space (skipping
  members that produced no output), into one `g_group_<id>` var; only whole blocks get `SEP`.
- The group contributes its block **once**, at the position of its first enabled member, so drag
  ordering still works and a group can never be emitted twice.
- The Vue `previewHtml` computes the identical two-level join, so the preview and the generated
  script cannot disagree.
- `GROUPS` now exists on both sides (Rust + Vue) and each carries a comment pointing at the other.
  This duplication is deliberate and small: the Vue list also drives the UI layout, which the Rust
  side has no concept of. Adding a group means editing both.

Groups: `model` (model + effort), `quota` (5h + 7d), `cache` (% + tokens). `rate_reset_*` are
deliberately absent - they modify another field's block rather than producing one.

### 2. Model and effort are two fields, not one

`model` used to print the model name and the effort abbreviation together, so effort could not be
turned off. They are now separate keys in one `model` group: `model` carries the color picker,
`effort` is locked grey (absent from `field_color_editable`) because it qualifies the model rather
than being its own signal.

`effort` is a new key, and `dependsOn: 'model'` in the UI makes its checkbox inert while `model`
is off. `setFieldEnabled()` also forces dependants off when their parent goes off, so the stored
config can never hold a state the greyed-out checkbox contradicts (the script generator reads
`enabled` directly and would otherwise disagree with the UI).

Migration: `migrateOldKeys()` inserts `effort` directly after `model`, seeded from `model`'s
enabled state, so an existing statusline looks identical until the user changes it. Without this
it would have been appended to the bottom of the field list by `loadCfg()`'s generic
unknown-key step.

### 3. The reset ETA icon is gone

`⟳` carried no information the adjacent time value did not already carry. `rate_block` now emits
a plain space + the ETA, both grey. The catalog `desc` strings and the JS preview dropped it too.

### 4. Rows show what they print (`parts` control)

New non-interactive control type: `parts` renders the pieces a field prints, in print order, as
tiny monospace chips. It exists because a label alone ("Context window") does not say what the
row will put on screen or how the number is derived:

- `context` → `ctx` · `NN%` · `in+out` · `/max`. The third chip is the point: the printed number
  is `ctx_input + ctx_output`, not a single reported figure.
- `cache` → `read/all%` · `read/all`, where `all` = `cache_read + cache_creation + cache_input`.
- `model` → `grey, fixed`, stating why effort has no color picker.

Both quota lines also gained the "Dynamic color" note that round 1 only put on some rows - the
5h/7d percentage color comes from the threshold ladder, and the row now says so.

### 5. Regression guard added

`src-tauri/src/statusline.rs` gained two tests, because the generated script is assembled from
string fragments that no compiler checks:

- `generated_script_is_valid_shell` - pipes the generated script through `bash -n`.
- `groups_join_without_separator` - asserts the group vars exist and that a grouped key never
  appears standalone in the final join line.

**Verification run:** `npm run build` passed; `cargo test` passed, 49 tests, including the two new
ones.

---

## Round 3 - identity colors, 5-tier ladder, cost scaling, preview wrapping

Round 2's group machinery turned out to be the right shape for all of this: every item below is a
new entry in an existing structure rather than a new special case.

### 1. Identity is two fields with their own colors

`identity` printed `user@host` with the two halves hardcoded cyan and bold green and no user
control, while the customizer told the user the color was "dynamic" - which was simply untrue, and
the reason this came up.

Split into `identity_user` + `identity_host`, both in `field_color_editable`, forming an `identity`
group. Their separator is a literal `@` with no padding, which is why `Group` gained a `sep` field
and `join_sp` became `join_with` - every other group passes `" "`. The old hardcoded
`COLOR_identity_user`/`COLOR_identity_host` emissions are gone; both now come from the generic
per-field color loop. Migration seeds the two new colors with the previously hardcoded cyan/green
so nothing changes on screen until the user picks.

### 2. Five tiers, unevenly spaced

The ladder gained `blue` below `green`: blue < 25, green 25-55, yellow 55-75, orange 75-88, red
>= 88. The bands narrow as the value gets urgent - the top of the range is where the color actually
has to carry information, the bottom is where nothing is at stake.

`StatuslineThresholds.green` carries `#[serde(default)]` and the Vue `migrateOldKeys()` fills any
missing threshold key from the defaults, so a config saved under the 4-tier ladder neither fails to
deserialize nor renders an empty number input.

### 3. Cost is scaled, not tiered

Session cost has no percentage of its own, so `COST_FULL_USD = 30` turns dollars into a percentage
the shared ladder can color; anything at or above that is red. It is a denominator rather than a
tier, so it is deliberately not exposed as a threshold in the customizer.

### 4. "Dynamic color" now shows the ladder

The words said something happens without saying what. The control is now `auto` plus the five tier
dots in order, with the current thresholds spelled out in the tooltip, and the threshold editor
itself renders from `TIER_KEYS` with the implied blue floor shown as a dot and a `<`. Adding a tier
is one array entry, not four hand-written rows.

### 5. The preview wraps by block, not by character

`.preview-line` was `word-break: break-all`, so a narrow modal split values away from their labels
mid-word. Each block plus its trailing separator is now one `inline-block` `nowrap` span
(`.pv-block`), so the preview wraps between blocks exactly the way the real statusline does in a
narrow terminal.

### 6. Third test: it actually renders

`renders_a_line` runs the generated script against a realistic Claude Code payload and asserts it
produces output; `cargo test -- --nocapture` prints the line with its ANSI codes. This is what
confirmed the round-3 changes end to end: `aki@Aki-M` glued by `@`, `sonnet 5 med` and
`5h:42% 7d:92%` with no separator inside the group, and `$8.40` green (8.4/30 = 28%, just over the
green threshold).

**Verification run:** `npm run build` passed; `cargo test` passed, 50 tests.

**Feature doc:** the living description of the customizer now lives at
`docs/feat/statusline-customizer.md` - this plan doc is the record of how it got there.
