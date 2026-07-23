# Statusline Customizer

Builds a statusline script from a visual field editor and pushes it - plus the CLI's own settings
key - to the local machine and any configured remote host.

**One physical script serves both CLIs.** It is installed byte-identically at two paths and works
out at run time which CLI is running it, from its own invocation path (`$0`):

| CLI | Installed at | Also patched |
|---|---|---|
| Claude Code | `~/.claude/statusline-command.sh` | `statusLine.type`/`.command` in `~/.claude/settings.json` |
| AGY CLI | `~/.gemini/antigravity-cli/statusline.sh` | `statusLine.type`/`.command`/`.enabled` in `~/.gemini/antigravity-cli/settings.json` |

Both halves are mandatory: a CLI runs nothing until its settings name the file. AGY ships
`statusLine: {type: "", command: "", enabled: true}` - willing, but pointed at nothing - so writing
only the script produced a statusline that never appeared. AGY gets an absolute path rather than a
`~/…` one, since nothing documents whether it expands a tilde.

```sh
case "$0" in
  */.gemini/*) CLI="AG" ;;
  *)           CLI="CC" ;;
esac
```

`CLI` decides exactly three things: whether the `aki-rlcache` block runs, which on-disk file the
account falls back to, and the two letters on the tag. Everything else is identical, so there is no
"CC dialect" and "AGY dialect" to keep in sync. Earlier attempts guessed the CLI from payload
fields; both CLIs turned out to send `.model` as an object, so no payload gate could tell them
apart. The install path can't be wrong.

Which targets an Apply writes is the "Apply to" checkbox pair in the modal header; both are ticked
by default and the choice persists (`aki-statusline-targets`). An empty selection is rejected by
the backend rather than silently falling back to one target.

Code: `src/components/modals/ClaudeSettingModal.vue` (UI + live preview),
`src-tauri/src/statusline-unified.sh` (the script), `src-tauri/src/statusline.rs` (patching + rollout).
Related: `docs/ref/statusline-unified-spec.md` (the full spec, §8.3b is the generator contract),
`docs/arch/usage-claudecode.md` (the `aki-rlcache` block, which this app's quota monitoring depends
on and the user cannot switch off).

## The script is the template

`src-tauri/src/statusline-unified.sh` is not a copy of what the generator emits - it *is* what the
generator emits. `statusline.rs` pulls it in with `include_str!` and rewrites exactly one region:

```
# >>> AKI-GENERATED-CONFIG >>>
   ... only variable assignments live here ...
# <<< AKI-GENERATED-CONFIG <<<
```

Every byte outside that region ships untouched. There is no second copy of the shell logic anywhere
in the codebase, which is the point: the file people read, run by hand and debug is the same file
Apply writes. The test `generated_defaults_match_template` generates from the UI's defaults and
compares byte-for-byte against the checked-in file, so a drift of one character fails the build.

**Edit the body in that .sh file.** Editing generated shell inside Rust string literals is what
produced three divergent scripts in 1.18.0.

The region holds six groups of variables and no logic at all:

| Group | Variables | Comes from |
|---|---|---|
| Enable | `EN_<key>` (18) | each field's checkbox, **dependency-resolved** |
| Color | `COLOR_<key>` (6) | the six real color pickers |
| Ladder | `THRESH_GREEN/YELLOW/ORANGE/RED` | the four threshold inputs, sorted ascending |
| Truncate | `TRUNC_ACCOUNT/USER/HOST/CWD/BRANCH` | the per-row scissors inputs |
| Zebra | `BG_ZEBRA_A/B`, `SEPARATE_BLOCKS` | the two shade swatches + the "separate" checkbox |
| Order | `BLOCK_ORDER` | the drag order of the rows, collapsed to block names |

## SSOT: the Vue component

`defaultLocalConfig()` in `ClaudeSettingModal.vue` is the single source of truth for every default.
Rust defines none: there is no `default_config()` and no `get_default_statusline_config` command -
both were deleted in Phase 2.2, along with the Vue call that used to overwrite the local defaults
with them (which is how the customizer ended up showing colors the running script never used).

The defaults are duplicated exactly once, in the checked-in .sh, so the file stays runnable on its
own - and `generated_defaults_match_template` exists to make that duplicate provably identical.

## Config shape

Stored in `localStorage` under `aki-statusline-config`, sent to Rust verbatim on Apply:

```
{
  version, separate,
  fields:     [{ key, enabled, color }],   // flat and ordered - the order IS the print order
  thresholds: { green, yellow, orange, red },
  trunc:      { account, user, host, cwd, branch },
  zebra:      { a, b },
}
```

`color` is honored only for keys in `COLOR_KEYS` (Rust) / `COLOR_EDITABLE` (Vue); every other
field's color is computed from its value or is a fixed label color.

**No migration, ever.** A saved config whose `version` is not the current `CONFIG_VERSION` is
discarded wholesale and the defaults take over. Translating old shapes forward is how a second,
invisible source of truth gets in.

## Fields, groups, blocks

A **field** is one key that prints one piece of text. A **group** is a set of fields that read as
one unit and move together as one draggable row. A **block** is what the script actually assembles
and the zebra alternates over - one `g_<block>` shell variable each.

| Group / block | Members | Glued with |
|---|---|---|
| `identity` | `identity_user`, `identity_host` | `@` |
| `model` | `model`, `effort` | nothing - they read as one token (`Opus4.8med`) |
| `context` | `context` | - |
| `cache` | `cache`, `cache_pct`, `cache_tokens` | space |
| `quota` | `rate_limits_5h`, `rate_reset_5h`, `rate_limits_7d`, `rate_reset_7d` | space |
| `cwd`, `session`, `git_branch`, `ram` | one field each | - |

`block_of()` in Rust maps a field key to its block; `block_order()` walks the user's field order and
emits each block once, at its first member's position. So a group can never be printed twice and
dragging moves it as a unit. A block whose fields are all off simply renders empty and is skipped by
the assembly loop - `BLOCK_ORDER` lists it either way, because *whether* something prints is the
`EN_` flags' job, not the order list's.

`cache` sits directly after `context` on purpose: the two are read together ("how many tokens, and
how much of that came from cache").

## Gated fieldset (non-destructive master switch)

`cache` gates `cache_pct` / `cache_tokens`; `rate_limits_5h` gates `rate_reset_5h`; `model` gates
`effort`. The rule, which matches HTML's `<fieldset disabled>`:

**Turning a gate off must not write `false` into its children.** It only makes them unreachable.
Turning it back on restores exactly what the user had. Writing into the children destroys their
choices, and that is the bug this pattern exists to prevent.

Declared once, in `DEPENDS`:

- Vue reads it in `fieldActive()` (what the preview and the checkboxes ask - never raw `enabled`)
  and `controlBlocked()` (what greys a control out).
- Rust mirrors the same table and resolves it into the generated flags: a child whose parent is off
  arrives as `EN_child=0`. No shell gate re-checks a parent, and the stored config is never mutated.

`rate_reset_5h` / `rate_reset_7d` are modifiers, not blocks: they add the ETA to another field's
output and never print anything of their own.

## The pinned `cli_tag` cluster

`cli_tag` prints a two-letter badge on a light plate - `CC` in Claude orange (`1;38;5;208`), `AG` in
royal blue (`1;38;5;33`) on background `48;5;255` - immediately followed by the account name on a
slightly darker plate (`48;5;252`). One glance says which CLI and which account produced the line
when both write to the same terminal.

It is pinned first and has no drag handle. The pin lives in the Vue layer (`rows` excludes it from
the draggable projection, `applyRowOrder()` re-prepends it, `loadCfg()` forces it and `account` back
to the front); the script simply prints it before the loop. A badge that can drift into the middle
of the line defeats its own purpose.

The cluster is never padded even when "separate" is on - the account name has to stay glued to its
badge - and it is not part of the zebra alternation, since it paints its own background.

## Zebra background, not a separator

Blocks are separated by an alternating background rather than a `|` glyph: two shades of the 256-
color **greyscale** ramp, `BG_ZEBRA_A` and `BG_ZEBRA_B`, applied per block. Grey has no hue, so it
can never clash in hue with a color the user picked for the text; only brightness has to be judged,
which keeps the check one-dimensional instead of "every foreground × every background". The picker
is therefore restricted to `16` and `232..255`, and Rust clamps into that range.

`SEPARATE_BLOCKS` ("separate", default on) pads each block with one space on each side so the shade
bands read as separate plates. Off, blocks butt directly together. In the live preview the padding
is a **non-breaking** space: an ordinary leading/trailing space inside an `inline-block` is collapsed
away by HTML layout, which once made the checkbox look like it did nothing.

## The dynamic-color ladder

Five tiers. `blue` is everything below the `green` threshold - it has no threshold of its own.
Defaults are uneven on purpose: a wide calm band at the bottom, narrower urgent ones at the top, so
the range that matters gets the resolution.

| Tier | Default | Meaning |
|---|---|---|
| blue | < 20% | plenty left |
| green | 20-50% | comfortable |
| yellow | 51-74% | worth noticing |
| orange | 75-89% | getting tight |
| red | ≥ 90% | act now |

`color_for_pct()` in the script and `tierHex()` in the Vue preview implement the same ladder against
the same stored thresholds. `sanitized_thresholds()` clamps and sorts before emitting, so typing the
tiers out of order still yields a monotonic ladder instead of an unreachable tier.

Two values are not percentages and are mapped onto the ladder before coloring:

- **context tokens** - scaled against a fixed **200k**, not the model's window size. Quality starts
  degrading around that mark whatever the advertised window is, so that is the number worth warning
  about. The percentage itself is not printed; the token count is simply colored.
- **session cost** - scaled against `COST_FULL_USD` (30), mirrored in the Vue preview. It is a
  denominator, not a tier, so it is not exposed in the customizer.

Two readings are deliberately **off** the ladder and stay static grey: the cache hit rate (a high
value is good news and must not shout in red) and RAM (whole-machine context, not something to
escalate about mid-session).

Wherever a field's color is computed rather than chosen, the row shows the "Dynamic color" note
instead of a picker.

## Color doctrine

| Role | Color | Fields |
|---|---|---|
| Labels / punctuation | white | the `ctx`, `5h:`, `7d:`, `ss`, `↬`, `⚅` prefixes - structural, not a value |
| Ordinary information | cyan | `model` |
| Where am I / which machine | white / magenta | `identity_user`, `identity_host` (white), `cwd` (magenta) |
| Supporting detail | grey | `account`, `effort`, ETAs, session duration, cache and RAM readings |
| Value that needs attention | dynamic | `context` tokens, `rate_limits_5h/7d`, session cost - color follows the value |
| Git / metadata | magenta | `git_branch` |

**Rule**: white = label, cyan = info, grey = qualifier/detail, dynamic = must-notice.

Only six fields have a real picker - `COLOR_KEYS` (Rust) / `COLOR_EDITABLE` (Vue) / the `COLOR_*`
block in the template must name the same six: `identity_user`, `identity_host`, `cwd`, `model`,
`git_branch`, `account`. A Rust test walks the template for every `COLOR_`/`EN_` it reads and fails
if any is not generated, and vice versa. That check exists because `COLOR_cwd` once shipped
declared-but-never-read: the picker was on screen, changed the generated script, and changed
nothing on the terminal.

## What each field prints

Rows carry `parts` chips naming the pieces a field prints, in print order, because a label alone
does not say what will appear or how a number is derived:

| Field | Prints | Note |
|---|---|---|
| `context` | `ctx` · tokens · `/max` | tokens = `total_input_tokens + total_output_tokens`; no `%` - see the ladder section |
| `cache_pct` | `↬` · `NN%` | `cache_read / (read + creation + input)`, static grey |
| `cache_tokens` | read count, `fmt_k`-formatted | grey, no `/total`; off by default |
| `rate_limits_5h/7d` | `5h:`/`7d:` · `NN%` · ETA | ETA only if the matching `rate_reset_*` is on |
| `session` | duration · `+added/-removed` · cost | cost is ladder-colored; the `+`/`-` counts are bold green/red (fixed) so they read at small sizes |
| `ram` | `⚅` · `NN%` | `memory_pressure` on macOS, `free` on Linux |

`fmt_k` rounds to whole units - `126k`, `250k`, `1M`, never `125.2k` or `1.0M`.

The `Cache` and `id` **master checkboxes** are one-way controls: clicking one force-sets its members,
but its own checked state is never derived from them, so toggling a member individually leaves the
master exactly as it was (`isMasterChecked()`/`toggleMaster()`). It reads as "which button decided
this", not "are both currently on". Note that `Cache` is a real stored gate field as well; `id` is
UI-only.

## Truncate widths

Per-field, because the widths are not interchangeable: a directory or branch name needs more room
before it stops being recognisable than a user name does.

| Field | Variable | Default | Range |
|---|---|---|---|
| Account | `TRUNC_ACCOUNT` | 4 | 3..12 |
| User | `TRUNC_USER` | 5 | 3..12 |
| Host | `TRUNC_HOST` | 6 | 3..12 |
| CWD | `TRUNC_CWD` | 12 | 3..**15** |
| Git branch | `TRUNC_BRANCH` | 10 | 3..**15** |

The script clamps these itself, so a bad generated value degrades instead of rendering garbage; the
generator does not duplicate the clamp. **Account strips the domain first** (`${email%%@*}`), then
truncates - AGY sends a full address, so a raw 4-character cut of `lva@akitao.com` would print the
useless `lva@`.

## Account resolution

Neither CLI puts an email in its statusline payload reliably, so both fall back to disk when the
`account` field is on. The two branches are **mutually exclusive, not sequential**: on a machine
with both files, trying them in order would show the other CLI's account.

| CLI | Payload keys tried | Disk fallback |
|---|---|---|
| CC | `.account.email`, `.user.email`, `.email` | `~/.claude.json` → `.oauthAccount.emailAddress` |
| AGY | same | `~/.gemini/google_accounts.json` → `.active` (an object, not an array) |

In practice AGY does send an email in the payload, so its disk file is only a backstop; Claude Code
genuinely depends on the fallback.

## Quota cache (`aki-rlcache`, CC only) - DESIGN LOCK

Claude Code sends `rate_limits` only on some turns, so the script persists it to
`~/.claude/rate-limits-cache.json` and merges it back on turns that omit it. AGY carries its quota
in `.quota` every turn and has nothing to persist, so the whole block is skipped when `CLI=AG` - an
AGY run must neither read nor write Claude Code's cache file.

Two rules are **not optional** and are marked with a `DESIGN LOCK` comment in the `# aki-rlcache v4`
block:

1. **Expiry** - an entry whose `resets_at` has already passed is dropped, not displayed.
   `resets_at: 0` means "unknown", not "expired", and is kept.
2. **Account scope** - the cache file records the account that wrote it; a cache belonging to a
   different account is discarded rather than displayed. Nothing surviving both gates → the file is
   removed.

Without these, a field that landed in the cache once lived there forever, because the merge only
adds/overwrites keys present in the live payload and never drops absent ones. That shipped in every
release from 1.10.0 through 1.17.0 and produced a phantom `7d 45%` for an account with no weekly
limit at all. Background: `docs/plan/1.18.0-statusline-apply-correctness.md` §P0-5.

The block also validates the cache with `jq -e` before reading (a corrupt file used to blank the
whole line), merges instead of replacing (a 5h-only payload no longer erases 7d), and writes via
tmp+rename.

`scripts/get-claudecode-usage.sh` (the reader, polled ~30s per host) mirrors the same two gates at
read time, independent of the writer above - it protects the app even when a host is still running
an older statusline hook that never got the v4 write-side fix. A foreign-account cache or an
already-past `resets_at` entry is dropped before the JSON reaches stdout; a legacy cache with no
`account` field is kept, not dropped (a host pending re-apply must not go blank). The reader never
writes to or deletes the cache file - see `docs/arch/usage-claudecode.md` §3.

## Quota routing (AGY)

AGY splits its quota into a Gemini pool and a 3P pool (Claude/GPT models running inside AGY). The
script picks the pool matching the running model - `gemini-5h`/`gemini-weekly` if the model name
contains `gemini`, otherwise `3p-5h`/`3p-weekly` - so there is nothing to configure, only a note on
the quota row. Dropping the `3p-*` branch silently blanks the quota for anyone running Claude inside
AGY.

The two CLIs also report reset time on **different scales**: Claude Code sends an absolute epoch
(`.rate_limits.*.resets_at`), AGY a relative countdown (`.quota[*].reset_in_seconds`). The script
normalises CC onto AGY's scale before formatting.

## Rollout to hosts

`check_statusline_status` (probe) and `apply_statusline_config` are both `async` commands whose work
runs under `spawn_blocking`, with one thread per host - hosts do not wait on each other. Everything
goes through `run_remote_script_bounded()` in `agent_usage.rs`, which adds two guarantees on top of
the shared execution funnel:

- **5s local cap / 4s remote self-bound.** The remote shell wraps itself in `timeout`/`gtimeout`/a
  `perl -e alarm` fallback, so killing the local SSH client cannot leave an unbounded orphan process
  on the remote host.
- **One lock per host** (`host_lock()`), taken inside `run_interpreter_timeout()` - so *every*
  feature that talks to a host (statusline probe, Apply, usage polling, git info) serializes against
  the others on that host, while different hosts stay fully parallel.

Each target's installer keeps a one-time `.aki-bak` of the script it replaces. `settings.json` is not
backed up: it is patched key-by-key with `jq`, so everything else in the file survives untouched.

The probe answers **per CLI** (`cc_present`/`cc_configured`, `ag_present`/`ag_configured`), and
"configured" requires both halves - the script on disk *and* the settings key naming it. A host chip
draws one tag per CLI it actually has: filled green = renders a line, hollow amber = the CLI is there
with nothing wired up. A CLI the host does not have draws nothing, which is both the honest reading
and the cheapest one in width.

Auto-repair stays Claude Code-only: a host with Claude Code but no statusline gets one written
(the quota reading depends on that hook existing), while AGY is never written without being asked.

## Verification

`src-tauri/src/statusline.rs` carries its tests as executable checks - the script is shell, which no
compiler checks. Requires `bash` and `jq`. Add `-- --nocapture` to eyeball the rendered lines and
their ANSI codes.

| Test | Asserts |
|---|---|
| `generated_defaults_match_template` | generating from the UI defaults reproduces the checked-in .sh byte-for-byte |
| `every_gate_the_template_reads_is_generated` | no `EN_`/`COLOR_` is read-but-never-written or written-but-never-read |
| `generated_script_is_valid_shell` | `bash -n` on the defaults **and** on an all-off config |
| `a_disabled_parent_switches_its_children_off` | the gated-fieldset rule, including that the stored child state is not mutated |
| `block_order_follows_the_field_order` | dragging a row changes `BLOCK_ORDER` |
| `out_of_range_values_are_clamped_not_defaulted` | zebra shades clamp into the neutral ramp; an inverted ladder is sorted |
| `renders_a_line`, `agy_renders_a_line` | a realistic payload renders; `$0` decides CC vs AG; AGY's fractions are not negative |
| `the_vendor_word_leaves_no_stray_punctuation_behind` | dropping "claude"/"gemini" out of a raw id leaves no orphaned separator (`gemini-2.5-flash` → `2.5-flash`) |
| `agy_never_touches_the_claude_rate_limit_cache` | the `CLI=AG` skip, verified against the file on disk |
| `cc_account_falls_back_to_claude_json`, `agy_account_falls_back_to_google_accounts_object` | the disk fallbacks above, including domain-stripping |
| `cc_rate_limits_survive_a_payload_that_omits_them`, `cc_rate_limits_merge_instead_of_overwrite` | the cache restores and merges rather than replaces |
| `cc_drops_a_cached_quota_whose_reset_has_passed`, `cc_ignores_a_cache_written_by_another_account` | the two DESIGN LOCK rules |
| `cc_survives_a_corrupt_rate_limits_cache` | a corrupt cache does not blank the line |
| `agy_reset_eta_includes_minutes`, `a_disabled_reset_hides_only_the_eta` | `5400s → 1h30m`, and switching the ETA off leaves the reading itself |
| `no_target_selected_is_an_error_not_a_silent_agy_write`, `both_targets_receive_the_same_body` | empty "Apply to" is an error; the two paths get one identical script |
| `the_ui_payload_reproduces_the_template` | the customizer's **actual JSON** (not a Rust fixture) deserializes and rebuilds the checked-in script byte-for-byte |
| `a_field_the_backend_does_not_know_is_ignored_not_fatal`, `a_missing_section_is_rejected_rather_than_silently_defaulted` | a newer UI's extra keys are harmless; a half-written payload fails the Apply instead of being silently completed |
| `every_toggle_flips_its_own_output_and_nothing_else` | all 18 gates, flipped one at a time against one payload: the text each owns appears exactly while its switch is on, and flipping it changes the line. Off-by-default rows are driven the other way round, and a guard fails the test if a gate is added to the template without a row here |
| `the_numeric_and_color_settings_reach_the_rendered_line`, `dragging_a_row_reorders_the_rendered_line` | truncate widths, color pickers, zebra shades, the separate toggle, the ladder thresholds and the drag order all land in the escape codes |

### Running the tests

```bash
npm run test:statusline              # all of the above
npm run test:statusline -- toggle    # filter by test name
```

Plain `cargo test` on the real crate — no harness, no extra files; the whole suite is `#[cfg(test)]`
inside `statusline.rs`. It is the automated replacement for ticking every switch in the modal,
pressing Apply and reading the written file by hand — the loop that let the dead CWD color picker
ship. Needs a machine that can build the crate, i.e. the Mac.

Full write-up, including what the suite cannot prove:
`docs/research/statusline-generator-test-suite.md`.

Worth the setup: it is the layer that catches what a formatter cannot. A lifetime-elision error and
a jq expression that aborted on a string `.model` — blanking the entire statusline — both survived a
`rustfmt` pass and were caught the first time these tests ran.
