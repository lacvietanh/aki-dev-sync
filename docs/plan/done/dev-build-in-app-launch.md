# Plan — DEV/BUILD launch into the in-app terminal

Status: **Verified on macOS by the owner, 2026-07-30.** Council finding (`cmd-surface`, akiflow
session `2026.07.30-0213-terminal-usage-ui-backlog`), implemented same day, then real-Mac tested
same day. Confirmed at runtime: DEV/BUILD open or focus a tab inside the in-app terminal (never an
external `Terminal.app` window); pressing the same button again while that tab is still alive only
focuses it and does not re-send the command; a tab whose shell has exited respawns and the command
is re-sent automatically; and the popup renders as designed.
`src/store/terminalTabsStore.js` (`runKind`/`pendingCmd` on the tab record, `setTabPendingCmd`/
`consumeTabPendingCmd`), `src/composables/useTerminalTabs.js` (`openRunCommand`),
`src/composables/usePtyTerminal.js` (`sendPendingCmdIfAny`, called deterministically once
`hydrateScrollback()` has taken its snapshot for a fresh tab's first spawn, and reactively off the
`alive` watcher — gated by `readyForPendingCmd` — for a dead tab's respawn; see that file's own doc
comment for why a single reactive trigger for both cases sent the command's echo twice),
`src/components/ProjectTable.vue`
(`runProjectCommand`/`runProjectDev` now call `openRunCommand`), `src-tauri/src/pty.rs` (R-1's PATH
seed, applied to every tab spawn, not only DEV/BUILD). `run_project_command`/`run_project_dev`/
`run_in_project_terminal` (`src-tauri/src/system.rs`), the old external-`Terminal.app` path this
replaced, had no remaining call site and are now removed, along with their `hostInvoke.js`
allowlist entries and `lib.rs` registrations (see "Follow-up: orphaned commands" below).
**Not specifically re-confirmed by the 2026-07-30 session:** the R-1 cold-start settling
observation (a stone-cold app launch, `nvm`-managed Node, immediate DEV press) — general DEV/BUILD
functioning was tested and works, but that one pathological timing case was not called out as part
of the test, so it stays an open item rather than a claimed pass.

## The defect / gap

DEV and BUILD currently open an **external `Terminal.app` window**, never the in-app PTY terminal:
`run_project_command` (BUILD) and `run_project_dev` (DEV) — both `async fn` + `spawn_blocking`
already, so today's implementation is not a never-block-UI violation — call the shared funnel
`run_in_project_terminal(local_path, cmd)` (`src-tauri/src/system.rs:840-882`), which refuses
before opening a window if the volume is unmounted (`ensure_local_dir`) and otherwise shells out
to `open_terminal_with_command(cd <path> && <cmd>)`, an `osascript`-driven external window that
reads nothing back.

This makes DEV/BUILD **invisible from a paired phone** — exactly the gap
`docs/feat/in-app-terminal.md`'s own "Why it exists" section names: *"The app already opens
Terminal.app windows for DEV, BUILD, SSH and the AkiClaudeDoc installer — and a phone on the other
end of Remote Control can see none of them."* That same doc's "Not in this version" section
currently excludes redirecting DEV/BUILD into the in-app terminal, for two reasons — one of which
(the terminal "has no notion of which project") predates Terminal v2's scoped per-project groups
(`docs/arch/terminal-stack.md`) and no longer holds. See "Doc-sync obligations" below: that
exclusion must be rewritten, not appended to, once this ships.

## The PTY entry point this reuses

`openScopeTerminal(scope, { title, cwd, reuse, expandStack })`
(`src/composables/useTerminalTabs.js:199-221`) — the same function `openProjectTerminal` (the
`TERM` cell and the popup's existing **In-App Terminal** item) already calls. Writing the initial
command uses the existing `pty_write(tab_id, data)` (`src-tauri/src/pty.rs:758`) exactly as a
keystroke would.

**This adds zero new Tauri commands.** `pty_spawn` is already `async fn` wrapping
`spawn_blocking` (compliant with the never-block-the-UI rule); `pty_write` is deliberately
synchronous by the codebase's own design — it decodes base64 and enqueues onto the writer thread,
it does not touch a subprocess or the network, so `RULE-stack-tauri` A1 does not apply to it at
all. Stated explicitly because it is the first question any reviewer should ask of a new execution
path: **this cannot freeze the window, because it introduces no new command surface — it drives
the existing PTY layer the same way a human typing does.**

The one addition is **frontend-only, no Rust change, no wire-format change**: a pending-command
field on the tab record, set when the tab is created for a DEV/BUILD press and consumed exactly
once by `usePtyTerminal.js`, then cleared. For a fresh tab this happens after that tab's own
`pty_spawn` invoke resolves **and** its scrollback hydrate completes — not on the spawn's own
`alive` transition, which races the ring buffer's own copy of the same bytes (see
`usePtyTerminal.js`'s `sendPendingCmdIfAny` doc comment). This is where R-1 below applies.

## Second-press behavior — decided

Tag each tab with `runKind: 'dev' | 'build' | null` (frontend-only field on the tab record,
mirrored like the rest of `terminalTabsStore.js` — the backend stays scope-blind, same as
`projectId` today).

1. **A live tab with matching `runKind` already exists in that project's scope** → focus/expand it;
   do not spawn a new tab; do not re-send the command. A build that already finished with output on
   screen must not be silently re-triggered by a stray extra click.
2. **A matching tab exists but has exited** (`tabAlive === false`) → focus that same tab, let
   `pty_spawn` respawn it (idempotent-per-tab, `pty.rs:444`), and re-send the command through the
   same pending-command mechanism — equivalent to "RESTART, then re-type," done automatically.
3. **No matching tab exists** → `openScopeTerminal(project.id, { title: 'DEV'|'BUILD', cwd:
   local_path, reuse: false, expandStack: true })`, forcing a **new** tab rather than reusing
   whatever the user's last-active shell in that scope happens to be.

**Reason for (3), stated because it is the one place this design deviates from the existing
In-App Terminal popup item's `reuse: true` default:** reusing the user's active shell would
interleave build/dev output with whatever they are mid-typing or mid-running in that shell — the
exact risk `docs/feat/in-app-terminal.md`'s "Not in this version" section flagged under its
first (still-valid) reason for not merging DEV/BUILD into the terminal naively. DEV and BUILD also
never share a tab with each other (`runKind` distinguishes them): pressing BUILD while DEV is
running opens/reuses a separate BUILD tab.

**Cap interaction:** a dev-server tab is an ordinary tab from the cap's point of view — it occupies
one of 5 per-project slots for as long as it runs (`docs/arch/terminal-stack.md` §"Scope as the
aggregate root"). The `runKind` dedup above is not cosmetic; without it, repeated DEV presses would
each eat a slot toward that cap. With it, DEV/BUILD behave like any other tab a user has to
close when actually done with it — no special-cased cap logic needed.

## R-1 — subprocess PATH-resolution cold-start race (binding constraint, named per council ruling)

**This is the single most important correctness constraint on this feature and is not a
follow-up.** `RULE-stack-tauri.md` A2 (also restated in this project's `CLAUDE.md`) names the
failure mode directly: a command that races a user's shell rc/profile sourcing (nvm, `path_helper`,
zinit, etc.) produces an intermittent `exit=127 command not found` that self-heals within minutes
and is **not reproducible** running the identical command by hand a bit later — a timing bug that
is trivially misdiagnosed as a CLI-version or auth problem.

**The design above is the trigger condition, not a mitigation, as written.** "Write the command
once `pty_spawn`'s invoke resolves" only tells us the shell **process has been forked** — that
promise says nothing about whether `$SHELL -l`'s own `.zprofile`/`.bash_profile` sourcing (which
is what actually puts `nvm`'s `node`/`npm` on `PATH`) has finished by the time the write lands.
Firing the DEV/BUILD write at that exact moment is precisely the near-cold-start race A2 describes
— arguably worse here, since a fresh tab spawned by a DEV/BUILD press is more likely to be a
cold-start event (first terminal use since app launch) than an already-open shell a user has been
typing in for a while.

**Required resolution pattern, per A2, applied at the single funnel that dispatches a command into
a PTY (the pending-command consumer in `usePtyTerminal.js`) — never patched ad hoc per call site:**

1. Prefer a fix that does not depend on rc-sourcing timing at all: seed the **PTY-spawned shell's
   own environment** with the well-known macOS install-directory candidates this project's A2 list
   already names (`~/.local/bin`, `/opt/homebrew/bin`, `/usr/local/bin`, `~/.claude/local`) —
   prepended to `PATH` in the `CommandBuilder` env passed to `pty_spawn` (`pty.rs`), so those
   directories are on `PATH` **before** the login shell's own rc sourcing even starts, not
   contingent on it finishing before the write arrives.
2. Fall back to `command -v` / login-shell PATH lookup only for anything not covered by the static
   list — same two-tier order A2 specifies elsewhere in this codebase.
3. One shared preamble, applied where DEV/BUILD commands are dispatched into a PTY — not
   duplicated per call site.

**The honest limit of this pattern, stated rather than glossed over (this is what the lead asked
me to address directly, not assume away):** A2's pattern was written for resolving *a single known
binary's path* (e.g. finding `claude` before spawning it). Here the thing racing PATH-resolution
is **the user's own arbitrary command text** typed into Project Settings (`npm run dev`,
`pnpm dev`, a custom script) — this plan cannot statically resolve *that command*, only improve the
odds that whatever `PATH` the shell has at write-time already contains common package-manager
install locations. **This does not fully close the gap for `nvm`-managed Node installs**, whose
`node`/`npm` live under a version-numbered path (`~/.nvm/versions/node/vX.Y.Z/bin`) that nvm's own
rc logic resolves dynamically — no static candidate list can predict that path without either
reading `.nvmrc`/`nvm`'s own state (out of scope for this plan) or genuinely waiting for rc
sourcing to complete.

**Left honestly unverified, with the Mac observation that settles it:** whether the static-PATH
seeding above is sufficient in practice, or whether a genuine "wait for shell-ready" signal is
also required (e.g. a short fixed delay before the write, or watching the PTY's own output stream
for a first-prompt heuristic before firing `pty_write`). **Settling observation:** on a Mac using
`nvm` for Node, cold-launch the app, immediately press DEV on a Node project, and check whether
`npm: command not found` (`exit=127`) appears in the new tab — repeat immediately after (warm) to
confirm the self-healing signature A2 describes. If it reproduces cold and not warm, the static
seeding alone is insufficient and a shell-ready gate must be added before this ships; if it never
reproduces, the seeding was sufficient. This must be run and recorded before Phase C implementation
is considered done for this feature — it is not optional polish.

## WS-A consequence (pointer only — not an edit performed here)

`docs/plan/terminal-ownership-model.md` (WS-A) §5's attribution table has a row
*"DEV / BUILD window | Owned by that project | Same funnel, same rule"*, and §2 counts DEV/BUILD
(`run_project_command`, `run_project_dev`) among 4 launch paths reaching
`open_terminal_with_command`. Now that this has shipped and those two Rust commands are deleted,
DEV/BUILD no longer reach that funnel at all — that attribution row is vacuous (it describes a path
that no longer exists) and WS-A's real launch-path count drops from 4 to 2 (`open_local_terminal`,
`open_remote_subprocess("terminal")`). WS-A is narrowed, not eliminated — the Terminal item and SSH
still open real external windows and still need ownership tagging.

**This plan does not edit `terminal-ownership-model.md` or `backlog-jul27.md`** — per council
assignment, `plan-docs` owns that amendment. This section exists so the conflict is on record and
findable from this side too.

## Follow-up: orphaned commands — done (2026-07-30)

`run_project_command`, `run_project_dev` and their shared helper `run_in_project_terminal`
(`src-tauri/src/system.rs`) had no remaining call site once `ProjectTable.vue`'s
`runProjectCommand`/`runProjectDev` wrappers switched to `openRunCommand` — confirmed by a
repo-wide grep before removal. All three are deleted, along with their `lib.rs` handler
registrations and `hostInvoke.js` allowlist entries. There was no capability grant to remove:
`src-tauri/capabilities/default.json` never listed either command by name. `cargo check` is clean
after the removal.

## Doc-sync obligations — done

- `docs/feat/open-popup.md` §2 ("DEV + BUILD") describes the in-app-terminal launch and `runKind`
  tagging.
- `docs/arch/terminal-stack.md` names DEV/BUILD as the third entry point into `openScopeTerminal`
  (`openRunCommand`, dedup on scope + `runKind`).
- `docs/feat/in-app-terminal.md`'s "Not in this version" section's old *"Redirecting DEV / BUILD /
  REPORT into it"* line was rewritten in place — REPORT stays a static-file browser open (never a
  terminal-redirect candidate), and DEV/BUILD's own line now states the shipped design instead of
  the old excluded-scope reasoning.
- `README.md` and `src/components/modals/IntroModal.vue` both describe the in-app-terminal
  behavior.

## Cross-refs

- `docs/feat/in-app-terminal.md` — in-app terminal behavior, groups, caps.
- `docs/arch/terminal-stack.md` — scope/group architecture, cap derivation, mount semantics.
- `docs/feat/open-popup.md` §2 — current DEV/BUILD description.
- `docs/plan/done/dev-build-visibility.md` — the companion defect (#6) on the same two buttons;
  deliberately a separate doc.
- `docs/plan/terminal-ownership-model.md` (WS-A) — the attribution-table conflict above; amendment
  owned by `plan-docs`, not this doc.
- `src-tauri/src/pty.rs:446,758` — `pty_spawn`, `pty_write`.
- `src/composables/useTerminalTabs.js:199-221,341-343` — `openScopeTerminal`, `openProjectTerminal`.
- `RULE-stack-tauri.md` A2 — the PATH-resolution cold-start race rule this plan's R-1 section
  applies.
