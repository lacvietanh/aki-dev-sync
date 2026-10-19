# Plan: SSH color wrapper — tint only when stdout is a terminal

Status: done — shipped in 1.33.1 (2026-10-20).

Finding this plan fixes: `docs/research/test-case-check-flow.md` § E16, 🟡 "Hàm `ssh()` override áp cho cả shell không tương tác" (line refs there predate the current `system.rs`).

## 1. Problem

`SSH_COLOR_SNIPPET` (`src-tauri/src/system.rs:415`), installed into `~/.zshrc` by **Enable SSH Terminal Color**, prints OSC 11 / OSC 111 to stdout unconditionally. Whenever `ssh` runs with stdout piped or redirected, the escape bytes land in the data stream:

| Caller | Result |
|---|---|
| Owner typing `ssh host` in Terminal/iTerm2 | fine — the intended tint |
| `ssh host 'tar c …' \| tar x` from any zsh that read `.zshrc` | `Unrecognized archive format`: OSC bytes prefixed to the tar stream |
| `out=$(ssh host cmd)`, `ssh host cmd > file` | output starts with `\033]11;#1a0f0f\a`, ends with `\033]111\a` |
| AI agents on the Mac (Claude Code's Bash tool snapshots the user's zsh functions) | every `ssh … \|` pipe they build is corrupted |
| `rsync`, scripts calling the `ssh` binary directly | unaffected — they never see the shell function |

## 2. Fix

Guard each `printf` on stdout being a terminal; `command ssh` stays unconditional:

```zsh
ssh() {
  [[ -t 1 ]] && printf '\033]11;#1a0f0f\007'
  command ssh "$@"
  [[ -t 1 ]] && printf '\033]111\007'
}
```

- `[[ -t 1 ]]` is false for pipes, redirects and `$(…)`, true for an interactive terminal — exactly the tint/no-tint split wanted.
- The function's exit status must still be `ssh`'s: with the guard, a non-TTY call would return the failed `[[ -t 1 ]]` (1). Keep the status explicitly:

```zsh
ssh() {
  [[ -t 1 ]] && printf '\033]11;#1a0f0f\007'
  command ssh "$@"
  local rc=$?
  [[ -t 1 ]] && printf '\033]111\007'
  return $rc
}
```

The unguarded original has the same latent bug in reverse (it returns `printf`'s 0, masking ssh failures), so the `rc` line fixes both.

## 3. Steps

1. [x] Replace `SSH_COLOR_SNIPPET` with the §2 block. Markers unchanged, so a re-install strips the old block and writes the new one (existing idempotent path, `install_ssh_terminal_color`). `cargo check` passes.
2. [x] No unit test: an assertion that the constant contains `[[ -t 1 ]]` only restates the constant (`test.A1`, `test.C1`), and the marker-strip path is unchanged. The behavior is checked by step 3 instead.
3. [x] Verified on the Mac, with the snippet extracted from `system.rs` and sourced into zsh:
   - `ssh bien 'printf x' | od -c` → only `x`, no `033`.
   - `ssh bien true` / `ssh bien false` → exit `0` / `1` (status preserved).
   - Inside a pty (`script -q`) → `033]11;#1a0f0f\a` before the output, `033]111\a` after (tint kept for a real terminal).
4. [x] Owner's live `~/.zshrc` block updated by hand, markers kept, backup `~/.zshrc.aki-bak-<unix_ts>`; a fresh `zsh -i` gives clean piped output and `rc=1` for `ssh bien false`. A future **Enable SSH Terminal Color** click writes the same block.
5. [x] `## Amendments` entry added to the research doc. Remaining: ship in the next release, then move this plan to `done/`.

## 4. Out of scope

The sibling 🟡 in the same finding (only `~/.zshrc` is patched; bash/fish users get a success toast with no effect) is a separate change.
