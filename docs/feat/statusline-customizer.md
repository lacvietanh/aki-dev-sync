# Statusline Customizer

Builds `~/.claude/statusline-command.sh` from a visual field editor and pushes it — plus the
`statusLine` key in `settings.json` — to the local machine and any configured remote host.

Code: `src/components/modals/ClaudeSettingModal.vue` (UI + live preview),
`src-tauri/src/statusline.rs` (script generation + rollout).
Related: `docs/arch/usage-claudecode.md` (the `aki-rlcache` block the generated script always
emits, which this app's quota monitoring depends on and the user cannot switch off).

## Config shape

Stored in `localStorage` under `aki-statusline-config`, sent to Rust verbatim on Apply:

```
{ fields: [{ key, enabled, color }], thresholds: { green, yellow, orange, red } }
```

`fields` is a flat, ordered list — the order is the print order. `color` is honored only for keys
in `field_color_editable()` (Rust) / `COLOR_EDITABLE` (Vue); every other field's color is computed
from its value.

## Two-level layout: fields, groups, blocks

A **field** is one key that prints one piece of text. A **group** is a set of fields that read as
one unit: its members are joined by the group's own separator, and only whole blocks are separated
by ` | `. Groups exist twice and must be edited together:

| Where | What it drives |
|---|---|
| `GROUPS` in `statusline.rs` | the actual join in the generated script (`join_with`) |
| `GROUPS` in `ClaudeSettingModal.vue` | the customizer's row layout **and** the live preview's identical two-level join |

Current groups:

| Group | Members | Separator | Label |
|---|---|---|---|
| `identity` | `identity_user`, `identity_host` | `@` (no padding) | — |
| `model` | `model`, `effort` | space | — |
| `quota` | `rate_limits_5h`, `rate_limits_7d` | space | — |
| `cache` | `cache_pct`, `cache_tokens` | space | `cache` |

A group contributes its block **once**, at the position of its first enabled member, so drag
reordering moves the group as a unit and a group can never be emitted twice. Members that produce
no output (disabled, or no data this turn) leave no stray separator behind.

`rate_reset_5h` / `rate_reset_7d` are deliberately not group members: they are flags that modify
another field's block rather than printing one of their own.

**Group label** (`Group.label` in Rust, `groupLabel` in Vue): a fixed white word prepended once,
only if the group produced non-empty output. `cache` needs this because neither `cache_pct` nor
`cache_tokens` prints the word "cache" itself (see their match arms) — without a group-level label
it would disappear whenever the currently-enabled member happened to be the wrong one. Rust builds
it through an intermediate variable (`_lbl="$(colored "$WHITE" "cache")"` then `g_group_cache="$_lbl
$g_group_cache"`) rather than nesting the command substitution inside the group's own double-quoted
assignment, purely to keep the quoting simple to read and generate.

## The dynamic-color ladder

Five tiers. `blue` is everything below the `green` threshold — it has no threshold of its own.
Defaults are uneven on purpose: wide calm bands at the bottom, narrow urgent ones at the top, so
the range that matters gets the resolution.

| Tier | Default | Meaning |
|---|---|---|
| blue | < 25% | plenty left |
| green | 25–51% | comfortable |
| yellow | 51–75% | worth noticing |
| orange | 75–90% | getting tight |
| red | ≥ 90% | act now |

`color_for_pct()` in the generated script and `tierHex()` in the Vue preview implement the same
ladder against the same stored thresholds. `sanitized_thresholds()` clamps and sorts, so typing
the tiers out of order still yields a monotonic ladder instead of an unreachable tier.

Two values are not percentages and are mapped onto the ladder before coloring:

- **cache hit %** — inverted (`color_for_pct_inv`), because a high hit rate is good.
- **session cost** — scaled against `COST_FULL_USD` (30), mirrored by `COST_FULL_USD` in the Vue
  preview. It is a denominator, not a tier, so it is not exposed in the customizer.

Wherever a field's color is computed rather than chosen, the row shows `auto` plus the five tier
dots instead of a color picker, with the current thresholds spelled out in the tooltip.

## Color doctrine

Default colors follow a visual hierarchy, not a personal preference:

| Role | Color | Fields |
|---|---|---|
| Labels / punctuation | white | the `ctx`, `5h:`, `7d:`, `ss` prefixes, the `cache` group label — structural, not a value |
| Ordinary information | cyan | `model` |
| Where am I / which machine | yellow / magenta | `cwd` (yellow), `identity_host` (magenta) — the two the eye hunts for first with several terminals open |
| Supporting detail | grey | `identity_user`, `effort`, `session` duration, `cache_tokens` read count, token counts in context |
| Value that needs attention | dynamic | `context`, `rate_limits_5h/7d`, `cache_pct`, `session` cost and its `+/-` line counts — color follows the value via the ladder |
| Git / metadata | magenta | `git_branch` |

**Rule**: white = label, cyan = info, grey = qualifier/detail, dynamic = must-notice, standout hues for "which machine" and "where am I". Fields with fixed color (like `effort`, `session`, `cache_tokens`) show no color control at all — `field_color_editable()` (Rust) / `COLOR_EDITABLE` (Vue) list only `identity_user`, `identity_host`, `cwd`, `model`, `git_branch`; everything else's color is either computed (the ladder) or hardcoded.

## What each field prints

Rows carry `parts` chips naming the pieces a field prints, in print order, because a label alone
does not say what will appear or how a number is derived:

| Field | Prints | Note |
|---|---|---|
| `context` | `ctx` · `NN%` · `in+out` · `/Max Token` | the number is `total_input_tokens + total_output_tokens`, not a single reported figure |
| `cache_pct` | `hit/max Token` | `cache_read / all`, where `all` = read + creation + input; the `cache` word comes from the group label, not this field |
| `cache_tokens` | `cache_read`, formatted (`Token`) | grey, no `/total` — just the read count |
| `session` | duration · `+added/-removed` · cost | cost is ladder-colored; `+`/`-` line counts are bold green/red (fixed, not user-colorable) so the glyph reads at the font's small size |

Cache's master checkbox ("Cache", in front of the `%`/`read` toggles) is a **one-way control**: it
force-sets both children on or off, but its own checked state is never derived from them — toggling
`%` or `read` individually leaves the master's checked box exactly as it was. This is deliberate
(`isMasterChecked()`/`toggleMaster()` in the Vue component, seeded once per row-render from
`keys.every(fieldEnabled)` and mutated only by clicking the master itself) so the master reads as
"which button decided this" rather than "are both currently on" — the same control also backs
identity's `id` master.

## Migration

`migrateOldKeys()` in the Vue component moves an already-saved config onto the current key set
before `loadCfg()`'s generic append-unknown-keys step, so a migrated key lands in its right
position rather than at the bottom of the list:

| Old | New | Seeded from |
|---|---|---|
| `identity` | `identity_user`, `identity_host` | old enabled state; colors seeded to the previously hardcoded cyan/green |
| `rate_limits` | `rate_limits_5h`, `rate_limits_7d` | old enabled state + color |
| `rate_reset` | `rate_reset_5h`, `rate_reset_7d` | old enabled state + color |
| (absent) | `effort` | the `model` field's enabled state, inserted directly after it |
| (absent) | `thresholds.green` | the default, 25 |

The Rust side additionally carries `#[serde(default)]` on `thresholds.green`, so a config that
reaches it without that key still deserializes.

## Verification

`src-tauri/src/statusline.rs` carries three tests, because the script is assembled from string
fragments that no compiler checks:

- `generated_script_is_valid_shell` — pipes the generated script through `bash -n`.
- `groups_join_without_separator` — asserts each group's join expression, including identity's
  non-space separator, and that a grouped key never appears standalone in the final join line.
- `renders_a_line` — runs the generated script against a realistic Claude Code payload and asserts
  it produces output. Run with `cargo test -- --nocapture` to eyeball the rendered line and its
  ANSI codes.

Requires `bash` and `jq` on the machine running the tests.
