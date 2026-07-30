# Pre-allow AGY Commands

Menu action (`AppHeader.vue` → "Pre-allow AGY Commands") that seeds a checked-in, recommended set of
dev commands into `permissions.allow` in `~/.gemini/antigravity-cli/settings.json` on one or more
hosts, so a new machine or a new agy account stops hitting a permission prompt for every routine
command (`git status`, `curl`, `jq`, `agy` itself, ...).

Code: `src-tauri/src/gemini_allowlist.rs` (backend), `src/components/modals/GeminiAllowlistModal.vue`
(UI), `share/gemini_allowlist_unified.json` (the seed list, checked in as SSOT).

## What it touches, what it doesn't

Only `permissions.allow` is read-modified-written, as a **union merge** (`jq ... | unique`) against
whatever is already there. Every other key in `settings.json` (`agentMode`, `model`, `statusLine`,
...) passes through untouched. A one-time timestamped backup (`settings.json.aki-bak-<epoch>`) is
taken on every apply, same shape as the statusline installer in `statusline.rs`.

Re-running it is harmless: the seed list is deduplicated against the union, so a second apply is a
no-op on `permissions.allow`'s contents (verified below).

## Rollout

`apply_gemini_allowlist` is `async fn`, and the actual work runs inside
`tauri::async_runtime::spawn_blocking` (one thread per host) — it must never block the IPC dispatch
thread, since it shells out to `run_remote_script_bounded()` (local) or SSH (remote). Every host
string is validated with `system::validate_remote_host()` before any thread is spawned, rejecting a
host starting with `-` (which `ssh` would otherwise read as an option) or containing characters
outside `[A-Za-z0-9.@_-]`.

## Failure surfacing

`command -v jq` is checked before anything else; a host without `jq` fails cleanly with nothing
changed. After the seed is written to a temp file via a heredoc, a second guard
(`jq -e '.permissions.allow | length > 0'`) validates the seed actually parsed as non-empty JSON
before the merge runs — if the heredoc ever fails to capture correctly (a shell heredoc whose closing
marker is not on its own line silently swallows the remainder of the script as literal content,
skipping the `jq`/`mv` entirely while still exiting 0), the script now aborts with a clear stderr
message instead of reporting a false success. Every host's result carries `{host, ok, message}` and
renders in the modal; a stderr-carrying failure surfaces there rather than a silent no-op.

## Verified 2026-07-30

Tested against a fake `settings.json` (with `permissions.allow` plus unrelated `agentMode`, `model`,
`statusLine` keys) by running the exact shell script `build_installer_script()` produces, with
`SETTINGS` pointed at the fake file:

- Merge is a true union: 82 seed commands + 2 pre-existing entries already present in the seed →
  82 total, no duplicates.
- All three unrelated keys (`agentMode`, `model`, `statusLine`) survive byte-identical.
- Running the same script twice is idempotent: the second run leaves `permissions.allow` at 82
  entries.
- `cargo check` passes on `src-tauri/`.

Not tested on a real remote host over SSH in this pass — only the local script logic (which is the
same script `run_remote_script_bounded` executes either locally or over SSH).
