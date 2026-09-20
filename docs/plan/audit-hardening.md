# Plan: Audit hardening after terminal-stack review

This plan records only findings that survived adversarial re-evaluation against the current working tree. Each candidate was challenged through six `/akithink` lenses: fact/reachability, strongest opposing case, trigger proof, impact, regression scope, and keep/downgrade/drop verdict.

The goal is not to preserve the audit's original severity labels. It is to leave the smallest execution plan that protects real user outcomes without turning intended product choices or theoretical edges into ship blockers.

## 1. Final triage

| ID | Candidate | Verdict | Priority | Why |
| :-- | :-- | :-- | :-- | :-- |
| A1 | `resolve_remote_path` accepts an SSH option as host | **KEEP** | P0 | `validate_remote_host` exists and rejects leading `-`, but this call path does not invoke it before passing `host` to `ssh` (`src-tauri/src/system.rs:36,647`). Reachable malformed project/companion data can therefore alter the local SSH invocation. |
| A2 | PTY close can resurrect a ghost tab from late output | **KEEP, severity reduced** | P2 | `kill_session` advances the generation floor, then `drop_tab_state` removes that floor and scrollback (`src-tauri/src/pty.rs:186,601`). A dying reader can append after the deletion and recreate a key consumed by `pty_list_tabs`. The race is real, but its impact is a recoverable ghost row plus bounded stray scrollback, so it is not a ship-blocking High/P0 issue. |
| A3 | Statusline install continues after a failed `jq` patch | **KEEP** | P0 | The generated shell uses `jq ... && mv ...` without an explicit failure branch (`src-tauri/src/statusline.rs:303-304,324-325`). Shell `errexit` semantics inside an AND-list do not provide the atomic “settings first, otherwise stop” guarantee documented by the module. |
| A4 | OS-entropy failure produces predictable remote-control secrets | **KEEP, severity reduced** | P1 | The fallback is explicit in `src-tauri/src/web_server.rs:623`. Entropy failure is rare, so this is not an ordinary exploitable High; however, minting authentication secrets from predictable state is the wrong failure mode. Fail closed. |
| A5 | Disabling remote control races with companion insertion | **KEEP pending narrow code test** | P1 | Authentication and insertion are separate phases around the enabled check (`src-tauri/src/web_server.rs:948` and connection registration path). The correct invariant is stronger than “checked once”: no companion may remain registered after disable returns. Add a deterministic race test before changing architecture. |
| A6 | Claude profile caches are scoped inconsistently | **KEEP, broadened** | P1 | The usage poller fixes rate-limit, credential and auth-cache paths under `$HOME/.claude`, but reads account metadata from `${CLAUDE_CONFIG_DIR:-$HOME/.claude}` (`scripts/get-claudecode-usage.sh:3-4,18,37`). A failed live auth call can therefore combine another profile's cached identity/quota state. Cleanup has the same split for auth/rate-limit caches (`src-tauri/src/claude_cleanup.rs:54-71`). |
| A7 | Extreme reset timestamps can overflow debug logging | **DROP as implementation task** | — | Direct `i64` subtraction exists only in diagnostic summarization (`src-tauri/src/agent_usage/claudecode.rs:281,291`). An extreme local cache value can panic debug builds or wrap in release, but cannot corrupt the returned payload; this does not justify a release-hardening batch. Reopen if the arithmetic enters user-visible scheduling or a regression test demonstrates a production failure. |
| A8 | `kill_session` holds the session map while waiting for process death | **KEEP, reduced to Low performance** | P2 | The maximum grace period is short, so this is not a correctness blocker. Still, one tab's process termination should not serialize unrelated tab operations. Remove the session under lock, then kill outside it. |
| A9 | Unlimited terminal tabs have no resource safety policy | **DOWNGRADE — accepted residual, no task** | — | Removing caps is intentional and frontend/backend are consistent. The removed ceiling was not a reliable backend safety boundary, so this is neither a regression nor a ship blocker. Reopen only on measured resource pressure or demonstrated automated companion abuse; do not restore the old small UX cap speculatively. |
| A10 | Statusline settings temp file may cross filesystems | **KEEP** | P2 | Script-file writes already use same-directory `mktemp`, but settings patches still use bare `mktemp` (`src-tauri/src/statusline.rs:303,324`). Place settings temp files beside their target before `mv`. |
| A11 | Timestamp-only backup names collide within one second | **DOWNGRADE — accepted residual, no task** | — | Two manual Applies to the same host within one second can overwrite a redundant backup, but never the live file. The current scheme already fixes the materially worse backup-once behavior; add uniqueness opportunistically if this code is touched. |
| A12 | Remote path lookup can wait indefinitely | **KEEP, merge with A1** | P1 | The same `resolve_remote_path` SSH boundary uses `output()` with no bounded connect/exec deadline. Fix validation and timeout together so this command matches the rest of the remote execution policy. |
| A13 | Statusline dumps every raw payload to a fixed shared `/tmp` path | **KEEP** | P0 | `statusline-unified.sh:24-26` unconditionally writes account email, cwd and usage data to `/tmp/statusline_stdin_dump.json`. The predictable shared path leaks data across local users/processes and follows a pre-planted symlink, allowing truncation of another user-writable file. This is shipping debug residue, not product behavior. |

## 2. Dropped or explicitly non-blocking candidates

These do not become implementation tasks unless new evidence appears.

| Candidate | Decision | Reason / reopen trigger |
| :-- | :-- | :-- |
| `project_id` / `projectId` re-adoption mismatch | **DROP** | `adoptTabs` is the explicit normalization boundary (`src/store/terminalTabsStore.js:129`), and the backend metadata contract is intentionally rehydrated there. Reopen only if a direct read proves the mapping absent or a reload places a project tab in global scope. |
| Disabled project must lose terminal access | **DROP as bug; document semantics** | The project toggle currently controls sync/git participation, not project existence or terminal permission. Automatically killing or blocking active terminals would be a separate product decision with destructive side effects. Reopen only if product docs define “disabled” as a global project lock. |
| Unlimited xterm views alone | **MERGED into A9** | This is the expected consequence of unlimited durable tabs, not a separate defect. The mitigation belongs at the resource boundary, not as CSS/component guards. |
| CSS-only popup blocking | **DROP** | Current popup rows are non-focusable `div`s and CSS pointer blocking is effective for their present interaction shape. Reopen when they become keyboard controls or gain another invocation path. |
| Null copy path | **DROP** | The relevant UI affordance is only meaningful where a path exists; no concrete reachable success-toast-on-null sequence was established. Add a cheap guard opportunistically if that code is touched. |
| OPEN popup creates new tab / normal button reuses | **CONFIRMED SOUND** | The two entry points deliberately pass `reuse:false` and `reuse:true` respectively. No task. |
| Scope fallback and reconnect | **CONFIRMED SOUND** | Existing reconciliation and idempotent spawn paths converge. No task. |
| Active tab not restored across full webview reload | **ACCEPTED limitation** | No persisted “last active tab” contract exists. This is UX enhancement work, unrelated to the audited regressions. |
| PTY input queue is unbounded | **DEFER** | Human input and normal paste traffic do not justify a new lossy/backpressured input protocol yet. Reopen on measured queue growth or automated companion input. |
| Scrollback/alive snapshot is non-atomic | **DROP** | The transient skew self-corrects through lifecycle events and has no durable state impact. |
| Public-origin parser permissiveness | **DEFER to `remote-ingress-rework.md`** | This belongs to the active ingress plan, not this terminal hardening batch. Require strict URL parsing before public ingress ships. |
| `read_text_file` TOCTOU | **DEFER / threat-model dependent** | Requires an attacker able to mutate a trusted project tree during the check/read window. Reopen if project roots become writable by untrusted remote users or this command is exposed beyond the paired-owner model. |
| Unrestricted git args | **DROP from this batch** | The command boundary is intentionally generic for trusted app flows; no untrusted caller chain was established. Any allowlist would be a separate API redesign. |
| Remote Bash requirement | **DOCUMENT, not defect** | The app's supported remote environment may require Bash. Change only if portability requirements expand. |
| Identity fields rendered in the statusline | **DROP as security finding** | Owner-visible identity is functional output. This does not cover the separate fixed-`/tmp` raw payload dump in A13, which crosses a local-user boundary and must be removed. |
| UI architecture scanner comment/string false positives | **KEEP in scanner's own test backlog, not ship blocker** | Scanner heuristics may be noisy, but this is developer tooling and independent from runtime correctness. Refine only with concrete false-positive fixtures. |
| UI architecture line-ratio bypass | **DROP as vulnerability** | The ratio is a directional hygiene signal, not a security boundary. Document that it is advisory. |

## 3. Execution plan

### P0-1 — Seal the SSH boundary used by remote-path resolution

**Files:** `src-tauri/src/system.rs`, focused tests in its existing `mod tests`.

1. Call `validate_remote_host(&host)` before constructing the SSH command.
2. Use the same SSH safety/options builder as other remote calls where practical; at minimum include a bounded `ConnectTimeout` and a local execution deadline.
3. Keep the remote path shell-quoted with the existing helper; host validation does not replace path quoting.
4. Do not rely on `ssh -- host`: OpenSSH option parsing and portability are better handled by rejecting option-shaped hosts at the shared validator.

**Acceptance:**
- `-oProxyCommand=...`, `-lroot`, whitespace and control characters fail before a subprocess is spawned.
- Valid `host`, `user@host`, dotted host and IP forms still pass.
- An unreachable host returns within the declared deadline.
- Existing remote path expansion behavior remains unchanged.

### P0-2 — Remove the unsafe raw statusline payload dump

**Files:** `src-tauri/src/statusline-unified.sh` and focused script/generator tests.

1. Delete the unconditional write to `/tmp/statusline_stdin_dump.json`.
2. Do not replace it with another default-on dump. If diagnostic capture is retained, require an explicit debug flag, create the file under the selected private config directory with `umask 077` and collision-safe creation, and document its sensitive contents.
3. Keep both Claude Code and Antigravity execution paths free of shared predictable temp files.

**Acceptance:**
- A payload containing a sentinel email/path creates no `/tmp/statusline_stdin_dump.json` and no other default diagnostic copy.
- A pre-planted symlink at the legacy path is not followed or modified.
- If opt-in debug capture remains, the file is unique, mode `0600`, and located under `${CLAUDE_CONFIG_DIR:-$HOME/.claude}` rather than shared `/tmp`.

### P0-3 — Make statusline installation transactional

**Files:** `src-tauri/src/statusline.rs` generated installers and behavioral tests.

1. Replace `jq ... && mv ...` with an explicit checked branch that removes the temp file and exits non-zero on any `jq` or `mv` failure.
2. Create the settings temp file in the settings file's directory, not system `/tmp`.
3. Write/backup the executable statusline only after the settings patch has landed.
4. Keep Claude Code and Antigravity installers behaviorally symmetric.

**Acceptance:**
- Malformed settings JSON leaves both settings and script unchanged and reports failure.
- A simulated failed settings rename leaves the prior files intact.
- Temp files are same-directory and cleaned on failure.
- Successful install still registers and renders both targets.

### P1-1 — Fail closed when secure randomness is unavailable

**Files:** `src-tauri/src/web_server.rs`.

1. Change secret generation to return `Result`; no authentication or pairing secret is produced from time/PID state.
2. Propagate failure to server enable/pairing operations with a clear local error.
3. Do not silently retain an old pairing code when the user requested rotation unless that behavior is explicitly surfaced.

**Acceptance:**
- An injected entropy failure produces no token/code and no partially enabled state.
- Normal tokens retain their full length and existing serialization contract.

### P1-2 — Close the disable/register WebSocket race

**Files:** `src-tauri/src/web_server.rs` and state-level concurrency tests.

1. Define one atomic invariant: after disable completes, the companion connection registry is empty and no in-flight handshake can insert.
2. Prefer an epoch/generation or registration-under-state-lock design over repeated boolean checks.
3. Existing paired devices may retain credentials, but must reconnect only after remote control is enabled again.

**Acceptance:**
- A deterministic barrier test pauses a valid companion after auth, disables the server, then resumes registration; insertion is rejected and the registry remains empty.
- Already registered sockets are closed on disable.
- Enabling again permits a fresh authenticated connection.

### P1-3 — Scope Claude caches and cleanup to the selected profile

**Files:** `scripts/get-claudecode-usage.sh`, `src-tauri/src/claude_cleanup.rs`, fixture/behavior tests.

1. Derive rate-limit, credentials and auth-cache paths from one `${CLAUDE_CONFIG_DIR:-$HOME/.claude}` base, consistently with `.claude.json`.
2. Keep the live `claude auth status` result authoritative; auth cache remains failure-only fallback.
3. Make account cleanup resolve the same selected-profile base for auth/rate-limit artifacts instead of silently targeting only `$HOME/.claude`.
4. Never combine quota, credentials or identity from different config directories.

**Acceptance:**
- Two isolated config dirs with different identities cannot cross-contaminate when one live auth call fails.
- Cleanup scans/removes the auth and rate-limit cache in the selected config directory.
- Default installs still read/write and clean `$HOME/.claude/*`.

### P2 — Robustness and performance cleanup

These are independently revertible and need not block the P0/P1 fixes.


### P2-1 — Make closed PTY tabs impossible to resurrect

**Files:** `src-tauri/src/pty.rs` and its unit tests.

1. Preserve a permanent close fence until all old-generation writers/readers can no longer append; do not remove the acceptance floor while a late read is possible.
2. Ensure `drop_tab_state` cannot be followed by `append_scrollback` recreating a closed tab's durable key.
3. Keep restart behavior distinct: restart accepts the new generation, while close accepts none.
4. Preserve per-tab scope: closing one tab must not touch another tab's session, metadata, generation floor or scrollback.

**Acceptance:**
- A deterministic test pauses an old reader, closes the tab, releases late bytes, and proves the tab is absent from sessions, metadata, scrollback and `pty_list_tabs`.
- Closing one of two tabs leaves the other untouched.
- Restart still rejects old-generation bytes and accepts new-generation bytes.
- Reopening/reloading cannot adopt a closed placeholder tab.

### P2-2 — Remaining bounded cleanup

1. **Release PTY lock before process wait:** remove the session under lock, preserve generation identity, then perform the kill grace loop outside the global map lock.
2. **Config-dir cleanup coverage:** if the cleanup-side part of P1-3 cannot share the same batch safely, land it as a separate bounded follow-up with its own selected-profile path tests.

**Acceptance:** targeted unit tests settle each item; no broad UI redesign is bundled here.

## 4. Order and batch boundaries

1. **Batch A — security/correctness boundary:** P0-1, P0-2 and P0-3.
2. **Batch B — remote-control/auth correctness:** P1-1, P1-2, P1-3.
3. **Batch C — bounded PTY/robustness cleanup:** P2-1 and P2-2 items, each separately reviewable.
4. Run targeted tests after each item, then the existing Rust suite and frontend production build once at the end.
5. Update the relevant `arch/feat` docs only for behavior that actually changes; do not turn dropped audit hypotheses into documentation claims.

## 5. Completion criteria

This plan is complete when:

- every P0 and P1 acceptance test exists and passes;
- no closed PTY can reappear after late output and reload;
- remote-path SSH validates the host and is time-bounded;
- the statusline never writes raw payloads to a fixed shared temp path;
- a statusline settings failure changes neither settings nor executable script;
- entropy failure cannot mint a secret;
- disable completion guarantees zero companion registrations;
- Claude profile identity fallback cannot cross config directories;
- dropped findings remain dropped unless their stated reopen trigger is demonstrated.
