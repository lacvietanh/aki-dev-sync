# App data dir moved to `~/.aki/devsync/`

Owner's note (`docs/plan/backlog.md` B1, `task-1786953511448`): standardize this app's data directory onto the shared `~/.aki/` ecosystem convention, with a one-way migration - "when the new location sees a file the old location has but the new one does not, move every needed file over, then delete it from the old location."

Shipped in this batch, alongside the idle-GPU fix, the F-key fix, and the titlebar changelog move (see `CHANGELOG.md` `[Unreleased]`).

## What changed

- New `src-tauri/src/app_paths.rs` is the single funnel: `app_data_dir()` returns `$HOME/.aki/devsync` (created if missing), replacing every direct `app.path().app_data_dir()` call. It also holds `migrate_legacy_app_data()`.
- `lib.rs`'s `setup()` runs `migrate_legacy_app_data(app.handle())` as its **first statement**, before `logger::init` - deliberate, since the migration itself moves `usage.log` and the logger has no file open yet to log through. The returned summary string is logged right after, under tag `MIGRATE`.
- `projects.rs` (`get_app_data_dir`), `logger.rs`, `global_note.rs`, `sync.rs` now call `app_paths::app_data_dir()` instead of resolving the Tauri app data dir themselves.
- `ssh.rs` and `web_server.rs` needed no change: both call `projects::get_app_data_dir(app)`, whose signature was kept intact and now internally forwards to the new funnel.
- Old default location (macOS): `~/Library/Application Support/aki.devsync/`. New location, every OS: `~/.aki/devsync/` (`dirs::home_dir()` + `.aki/devsync`, no per-OS branch).

## Migration semantics - exact behavior

Runs once per launch, unconditionally (no version flag/marker file gating it - idempotent by construction, see below). Walks **per file**, not per top-level artifact - this is a post-review revision; see "Defect found and fixed" below for why the first draft's directory-level shape was wrong.

1. Resolve the legacy dir via Tauri's own `app.path().app_data_dir()`. If that fails to resolve, or the dir does not exist, the migration is a no-op and says so in its summary - nothing to move.
2. For each of the 8 known top-level artifacts - `projects.json`, `usage.log`, `globalnote.json`, `ssh_undo_state.txt`, `ssh_redo_state.txt`, `baselines/` (directory), `companion-devices.json`, `companion-server.json` - missing entirely at the legacy path → skip, not counted in any list. Otherwise, `migrate_tree(src, dest, &mut tally)` walks it:
   - **A file**: destination already exists → tallied `skipped`, both copies left untouched. Destination absent → copy, then delete the source **only after the copy succeeds** → tallied `moved`. A failed copy (source untouched) or a copy that succeeded but whose source delete then failed (file now duplicated, not lost) both tally `failed`.
   - **A directory**: the destination directory is created first (idempotent - a no-op if it already exists, unlike the pre-review shape below), then every child is walked the same way, recursively. Only after every child has been processed, and the source directory turns out empty, is the now-empty source directory itself removed. A directory holding any un-migrated child survives untouched and is retried in full on the next launch - **an existing destination directory is never itself treated as "already done"**, only its individual children decide that, file by file.
3. Each top-level artifact's tally collapses to exactly one outcome in the summary: every file moved, nothing skipped or failed → `moved`; every file already present, nothing moved or failed → `skipped`; nothing existed to migrate at all (an empty tally) → omitted from the summary entirely; anything else - any failure at all, or even an all-succeeded mix of some moved and some skipped - → `failed`, formatted `<name> (partial: moved=X skipped=Y failed=Z)`, so a partially-migrated artifact can never read like a clean one in the log.
4. After all 8 artifacts, the legacy dir is removed only if it is now completely empty; otherwise it survives, holding whatever still needs a retry.
5. The function never panics and returns a one-line summary (`moved=[...] skipped=[...] failed=[...]`) for the caller to log; it does not log anything itself (see why above).

**Idempotence**: a second run after a successful migration finds no legacy dir (already removed) and is a pure no-op. A second run after a *partial* migration re-walks every remaining top-level artifact from scratch, but re-copies nothing that already succeeded: the destination-exists check happens per file, and for a directory the walk always re-enters it (directories are never skip-checked as a unit) so every still-missing child gets another attempt while every already-migrated child is left alone. Nothing here is a one-shot flag; the per-file exists-check on both ends is what makes re-running safe.

**Data loss window**: none by design - a source file is only deleted after its own copy call returns `Ok`, and this is now decided per file rather than per artifact. The narrowest exposure is a crash between one file's `fs::copy` and its own `fs::remove_file`, which leaves that one file duplicated (copy succeeded, source not yet removed) rather than lost; the next launch's per-file `skipped` branch then leaves the legacy copy in place - a duplicate, not data loss - until the legacy dir is otherwise empty and gets removed.

**Defect found and fixed (adversarial review, before this doc's first version shipped)**: the original draft copied `baselines/` (and any future directory artifact) as a single recursive unit - `fs::create_dir_all(dest)` ran once, unconditionally, *before* any child was copied, and the top-level `if dest.exists() { skip }` check only ever looked at that one directory path. A single unreadable/unwritable child inside `baselines/` left the destination directory sitting there half-populated, and because the directory itself now existed, every later launch's `dest.exists()` check read the whole artifact as "already migrated" and never retried the missing children - a permanently stranded partial copy with no path to completion short of manual intervention. The per-file walk above removes the failure mode structurally: a directory is never itself the unit that gets skip-checked, so a half-finished directory keeps being walked, file by file, until nothing is left to migrate.

## What was NOT touched

- No app data file changed its on-disk *shape* - only its parent directory. `projects.json`, `globalnote.json`, etc. are copied byte-for-byte.
- `docs/feat/project-task-list.md`'s per-project `.akidevsync/notes.json` (inside each synced repo, not app data) is unrelated and unaffected.
- The already-existing legacy-baseline migration (`~/.aki/devsync-baselines` → the old Tauri app data dir, from 1.7.1, documented in `docs/research/sync-button-semantic-analysis.md`) is a separate, older one-shot cleanup and was not touched or re-used as code - `app_paths.rs`'s migration is new, general-purpose code for this move specifically.

## Verify checklist

Static reading settles (done in this pass, no runtime needed):
- [x] `app_data_dir()` is the only path-construction site; every caller in the 4 changed Rust files goes through it or through `projects::get_app_data_dir` (`git diff` read directly - see file list above).
- [x] Migration order in `lib.rs`: migration runs strictly before `logger::init`, confirmed by reading the two adjacent lines in `setup()`.
- [x] Copy-then-delete ordering inside `migrate_tree`'s file base case: `fs::copy` is called and only its `Ok` arm calls `fs::remove_file` - read directly in `app_paths.rs`.
- [x] Per-file destination-exists guard (`else if dest.exists() { tally.skipped += 1 }`) precedes the copy attempt for every file, and a directory is never skip-checked as a unit (`fs::create_dir_all(dest)` always runs, then always recurses into children) - confirmed by reading `migrate_tree`.
- [x] `record()`'s match arms: a tally with any `failed` count, or a mix of nonzero `moved` and `skipped` with zero `failed`, both fall through to the `(partial: moved=X skipped=Y failed=Z)` branch under `failed` - read directly, not inferred.
- [x] `ssh.rs`/`web_server.rs` needed no edit: both call `projects::get_app_data_dir(app)` (grepped), whose new body is `crate::app_paths::app_data_dir()` - same return value, no signature change.

Verified on the owner's Mac 2026-08-19 (this box was headless Linux and could not compile or run the Tauri binary at all - `Cargo.toml`/Rust changes were unverifiable there per `coding.B3`):
- [x] Fresh launch with a legacy `~/Library/Application Support/aki.devsync/` directory present (real pre-existing data, backed up first to `~/aki.devsync-backup-20260819-0600` per the owner's mandatory-backup step): `~/.aki/devsync/` was created and received every known artifact (`companion-devices.json`, `companion-server.json`, `globalnote.json`, `projects.json`, `ssh_undo_state.txt`, `baselines/`); byte-diffed identical against the backup for globalnote.json/companion-devices.json/ssh_undo_state.txt, 25/25 baseline files, 24/24 projects by id. The legacy directory survived, holding only non-artifact leftovers (`.DS_Store`, three old `.bak`/`.pre-*` files) that were never part of the migration's known-artifact list - correct per spec, not a partial migration.
- [x] `usage.log`'s own `MIGRATE`-tagged line - owner confirmed closed; superseded by the byte-diff evidence above as accepted proof of a correct migration - `logger::info()` only writes under `--debug`/`AKI_DEBUG=1`, and re-running with that flag would require destroying the now-live migrated data, so the log line itself was not separately captured.
- [x] Second launch immediately after the first was a silent no-op: no file in `~/.aki/devsync/` changed mtime, legacy directory unchanged.
- [x] App behaves normally after migration: projects list, Global Note, SSH undo/redo state, and baselines all present and correct at the new location (confirmed via the byte-diff above, not just presence on disk).
- [x] Companion device reconnect after `companion-devices.json`/`companion-server.json` moved - owner confirmed closed (no companion device was paired during this pass, so nothing to actually reconnect against).
- [x] The specific unreadable-child-in-`baselines/` defect regression test - owner confirmed closed; covered by the static code reading in §"Defect found and fixed" (per-file skip-check confirmed in `migrate_tree`), not re-run dynamically.

## Cross-references

- `docs/plan/backlog.md` B1 - the backlog row this closes.
- `docs/research/sync-button-semantic-analysis.md` - the older, unrelated `~/.aki/devsync-baselines` legacy-baseline migration, kept for historical accuracy and not to be confused with this one.
- `src-tauri/src/app_paths.rs` - the two in-code comments (on `migrate_tree` and `migrate_legacy_app_data`) that point at this doc by path; this doc is what they defer to for the full semantics.
- `CLAUDE.md`, `docs/arch/logger.md`, `docs/arch/usage-claudecode.md`, `README.md`, `docs/feat/sync-flow.md`, `docs/feat/project-task-list.md`, `src/components/modals/IntroModal.vue` - all updated in this same batch to stop naming the old location.
