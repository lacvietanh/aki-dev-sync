# TODO on Mac - Statusline Customizer + Aki StatusLine share (1.10.0)

**Status:** Code complete on the Linux dev box, NOT built/run there (Rust compiles Mac-only per
`CLAUDE.local.md`). Everything below is what's left, in the order to do it.

---

## What already happened (no action needed)

- `src-tauri/src/statusline.rs` (new) - config model, script generator, `apply_statusline_config`
  Tauri command. Verified by hand-porting the generator to JS and running the output through real
  `bash`/`jq` (see transcript) - syntax OK, output byte-for-byte matches the locked preset format,
  rlcache persistence across calls confirmed, settings.json merge confirmed non-destructive,
  `.aki-bak` backup confirmed. **Not compiled with `cargo` - that's the one thing that couldn't be
  checked here.**
- `src-tauri/src/agent_usage.rs` - `run_remote_script` made `pub(crate)` (one-line visibility
  change only).
- `src-tauri/src/lib.rs` - `mod statusline;` + 2 new commands registered.
- `src/components/modals/ClaudeSettingModal.vue` (new) - the customizer UI.
- `src/components/AgentUsage.vue` - new toolbar button (terminal icon) next to the existing
  Claude Code Profile button, opens the modal.
- `README.md` - new "🖥 Bonus: Aki StatusLine for Claude Code" section, embeds
  `share/aki-statusLine/demo.png`.
- `CHANGELOG.md` - `[1.10.0]` entry added (two bullets: customizer + share asset).
- `package.json` + `src-tauri/Cargo.toml` - version bumped `1.9.8` → `1.10.0`.
- `npm run build` (vite build) passed clean - confirms the new Vue SFC compiles.
- `docs/plan/share-statusline.md` - pre-existing plan (arrived via `share/` mid-session); its
  README task is done, its "commit & push" task is intentionally left for you (no commits were
  made this session per instruction).

## 1. Build & smoke-test on Mac

```bash
cd ~/aki/app/Aki-Dev-Sync   # or wherever this clone lives on the Mac
npm install                  # only if node_modules is stale
npm run tauri dev            # first real cargo build of statusline.rs - this is the actual
                              # compile check that couldn't run on the dev box
```

If it compiles: open the app → Claude Code panel → click the new terminal icon next to the
sliders (Profile) icon → the Statusline Customizer modal should open with the default 7-field
preset already enabled/ordered, and the live preview should render colored text matching:

```
guest@<host> | <cwd> | sonnet 5 med | ctx NN% x/max | 5h:NN%  7d:NN% | Nm +A/-R $C.CC
```

## 2. Functional checks (do these with ≥1 remote host configured, not just local)

- [ ] Toggle a field off (e.g. `git_branch` stays off by default - turn it ON instead) → preview
      updates live.
- [ ] Reorder two fields with the up/down chevrons → preview order changes.
- [ ] Change a color on `cwd` or `session` (the only always-editable ones by default) → preview
      color changes; toggle `context`/`rate_limits`/`identity` and confirm there's **no** color
      picker for those (locked by design, see `field_color_editable()` in `statusline.rs`).
- [ ] Adjust a threshold number → preview's % colors shift accordingly.
- [ ] Check "Local" only → Apply → confirm `~/.claude/statusline-command.sh` was rewritten and
      `~/.claude/settings.json` gained/kept a correct `statusLine` block (diff it against
      whatever was there before, if anything).
- [ ] Open a **second** terminal running Claude Code and confirm the new statusline actually
      renders (this needs a restart of that CC session - the plan doc's "apply" note holds:
      changes take effect on next CC start, not live).
- [ ] Check a **remote host** too → Apply → confirm over ssh that
      `~/.claude/statusline-command.sh.aki-bak` exists (only if that host had a prior script) and
      the new script + settings.json patch landed correctly there too.
- [ ] Apply a second time to the same host → confirm it does NOT re-backup (only the *first*
      apply per host should create `.aki-bak` - by design, so repeated Applies from the app don't
      clobber a backup of your actual original hand-edit).

## 3. If it doesn't compile

Most likely failure modes to check first, since this Rust wasn't run through `cargo check`:
- Any typo in `format!` named-arg braces in `generate_statusline_script` (the `color_for_pct`
  block uses `{{`/`}}` escaping - that's the one spot most likely to have an off-by-one brace).
- `Serialize`/`Deserialize` derive on `StatuslineField`/`StatuslineThresholds`/`StatuslineConfig`
  - camelCase param mapping (`target_hosts` Rust ↔ `targetHosts` JS) follows the same convention
  already used by `set_claude_profile`, so should just work, but double-check the Tauri command
  macro accepts it without an explicit `rename_all` (it didn't need one for existing commands).

## 4. Once verified

- [ ] `git add share/ docs/plan/share-statusline.md` and commit - this was deliberately **not**
      committed this session (instruction was "không commit"); `share/share-statusline.md`'s own
      plan doc calls this out as the last step.
- [ ] Consider moving `docs/plan/statusline-customizer.md` to `docs/plan/done/` once you've
      confirmed the feature works - its "việc cần làm" list is now implemented.
- [ ] `npm run build:app` (or `build:rmad`/`build:rmud`) for the actual release build + GitHub
      Release, per your usual flow - nothing in this session touched the release scripts.

## Known scope cuts (deliberate, not bugs)

- Field catalog is 7 fields (+git_branch), not the full 41-key statusLine schema mentioned in the
  original plan doc - covers everything in the already-locked default preset. Adding a new field
  later is one `match` arm in `generate_statusline_script()` + one catalog entry in
  `default_config()` (Rust) and `defaultLocalConfig()`/`CATALOG` (Vue) - no structural change.
- Per-field color is only editable for `cwd`/`model`/`session`/`git_branch` - fields with
  inherently meaningful coloring (`identity`, `context`, `rate_limits`, and the +/- lines inside
  `session`) keep their locked colors on purpose (see `field_color_editable()`).
- `git_branch` sources from `.workspace.git_branch // .git.branch // ""` defensively - the plan
  doc flagged this as an open question (exact schema key unconfirmed). If it renders empty on
  your Mac even with git repos open in Claude Code, that's why - check what the real hook payload
  contains (`echo "$input" > /tmp/cc-input.json` inside the generated script temporarily) and fix
  the jq path in one place (`generate_statusline_script`'s extraction block).
