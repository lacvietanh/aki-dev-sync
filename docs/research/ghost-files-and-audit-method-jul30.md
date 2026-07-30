# Ghost files — a working tree that lies to every reader, and what it cost an eight-agent audit

## Start time

2026-07-30 (during the `/akiflow` backlog-triage council, session `2026.07.30-0213-terminal-usage-ui-backlog`). Discovered at the close of Phase A, after the council had already produced seven plan docs written against the tree this doc is about.

## Initial purpose

The owner reported thirteen defects and stated as background that the usage-probe OOP refactor "chưa hề viết dù qua bao plan" — never written despite several plans. The council took that as given, and one specialist (`oop-architect`) was convened to design it. Its first act was to check the premise against git rather than against the files. The premise was false, and the reason it was false is the subject of this doc.

The question this doc answers is therefore not "what is broken in the code" but: **how did eight independent agents, each reading source directly and each required to tag every load-bearing statement FACT / ASSUMPTION, all converge on the same wrong picture of what is in this repo?**

## Strategy

1. Establish what is actually at `HEAD` for the disputed files, using `git cat-file -e HEAD:<path>` rather than reading the working tree.
2. Establish whether the refactor commit is reachable from `HEAD` (`git merge-base --is-ancestor`), not merely present in `git log --all`.
3. Establish the *index* state separately from both, by hashing it (`git write-tree`) and comparing against commit trees — `git status`'s letter codes are meaningless if the index itself is stale, which it turned out to be.
4. Determine statically whether the resulting tree compiles, without running cargo — this box cannot build Rust (`CLAUDE.local.md`) and a failed build would prove nothing about *why*.
5. Re-derive which of the council's conclusions were downstream of the false picture, and which survive.

## Checklist

- [x] Is `610fd93` an ancestor of `HEAD`?
- [x] For each disputed path: at `HEAD`, in the index, in the worktree — three separate questions
- [x] Does `lib.rs` declare a module that now has two candidate files?
- [x] Which council conclusions cited the disputed files as evidence?
- [x] Does the plan-doc lifecycle state agree with the commit?
- [x] Is the defect the council predicted would be *introduced* by the port already present in the shipped port?

## Result

### 1. The refactor shipped; the files it deleted were never removed from disk

`610fd93` — *"refactor(agent-usage): JS→sh probe, decompose agent_usage.rs into submodules"*, 2026-07-29 17:34:12 +0700, 18 files, +1542 / −1573 — **is an ancestor of `HEAD`** (`96232a3`, one commit later). It removed `src-tauri/src/agent_usage.rs` (892 lines) in favour of seven modules under `src-tauri/src/agent_usage/` (`mod`, `probe_result`, `probe_log`, `claudecode`, `antigravity`, `antigravity_payload`, `antigravity_logout`) plus a new top-level `remote_shell.rs`, and replaced `scripts/get-antigravity-usage.js` with a POSIX-shell probe.

**The removal was recorded in git but never applied to the working tree.** Both files are still on disk on this box, with their **original pre-refactor mtimes** — `agent_usage.rs` at 2026-07-27 05:51, `get-antigravity-usage.js` at 2026-07-22 — and `agent_usage.rs` is byte-identical to `610fd93^:src-tauri/src/agent_usage.rs` (`cmp` clean). An old mtime rules out a restore: a `git checkout` or `git reset --hard` would have stamped the file with the time of the restore. The only shapes that produce this are `git rm --cached` / `git update-index --remove`, or a commit assembled from an index that was edited without touching the worktree.

The same pattern holds for the plan doc: `HEAD` carries `docs/plan/done/usage-probe-oop.md` only, but `docs/plan/usage-probe-oop.md` — the pre-move copy — is still on disk. The lifecycle move was committed; the old file was never deleted.

### 2. A second, independent artefact: the index is stale, so `git status` is unreadable

`.git/index` was last written **2026-07-30 02:12:31** and its tree hashes to `c14e723` — which is exactly the tree of `8cc2669`, the commit that was `HEAD` when this session opened one minute later, and which is now **six commits behind** current `HEAD`.

This one fact generates every alarming thing `git status` reports, none of which is real:

| What `git status` shows | What is actually true |
|---|---|
| 28 paths as `D ` (staged deletion) — including all seven new `agent_usage/` modules, `remote_shell.rs`, `get-antigravity-usage.sh` | Those files exist on disk and are committed at `HEAD`. They post-date the stale index, so it does not know them |
| The same paths *also* listed as `??` untracked | Same cause, seen from the other side |
| 74 paths as `MM` | Committed since `8cc2669`; the index has not caught up |
| `RD docs/plan/done/audit-1.11-1.15.md -> docs/plan/audit-1.11-1.15.md` | A committed lifecycle move the index predates |

Only **two** entries survive as genuine: `A  src-tauri/src/agent_usage.rs` and `A  scripts/get-antigravity-usage.js` — the §1 leftovers, which are absent from `HEAD` and present on disk under any reading of the index.

**This is the second reason a reader cannot trust what they see here, and it compounds the first.** A stale index does not merely omit information; it *manufactures* confident, specific, wrong claims — "seven Rust modules were deleted" is a sentence `git status` will print, in the standard format, with no warning attached. Refreshing it is a one-command fix, but it is a git-state mutation and therefore the owner's, not an auditor's (`RULE-agent-behavior.md` B5).

### 3. The build is broken on this box, provable without compiling

`src-tauri/src/lib.rs:1` declares `mod agent_usage;`. Both `src-tauri/src/agent_usage.rs` (the §1 leftover) and `src-tauri/src/agent_usage/mod.rs` exist on disk. rustc resolves a module declaration against exactly those two candidate paths and errors when both are present: **E0761, "file for module `agent_usage` found at both agent_usage.rs and agent_usage/mod.rs"**.

Static property of the file layout plus one line of source; no `cargo check` needed — which matters, because this box may not build Rust (`CLAUDE.local.md`) and an unavailable build is exactly the excuse under which such a fact goes unstated.

**Scope of the breakage is the open question, and it is not rhetorical.** A fresh `git clone` of `HEAD` compiles — the leftover is not in the commit. What breaks is any checkout where the file was never deleted from disk, which is at minimum the machine where `610fd93` was authored. If that machine is the Mac, the Mac cannot build this app right now. If commits are authored on this Linux box and the Mac only pulls, the Mac is clean. Nothing in the repo distinguishes these, and guessing which one it is would be exactly the auto-classification B5 forbids.

### 4. Why every reader was fooled — the general lesson

A file that a commit **removed from git but not from disk** is the one drift class a reader cannot detect by reading:

- It answers `grep` and `Read` with fully coherent, internally consistent, pre-refactor content.
- Its mtime is old *and genuinely so* — it really has not been touched — which reads as *stable and long-settled* rather than as *superseded*.
- Nothing about it is malformed, truncated, or marked. There is no artefact to notice.
- Every neighbouring file it references still exists, so cross-checks between ghost files pass.

Contrast the drift classes an audit normally catches: a doc contradicting code (visible by comparing two files), a stale status line (visible by comparing to a commit), a dead symbol (visible by grep returning no call sites). All of those are found by **reading more**. This one is immune to reading more, because every additional read comes from the same false layer. Eight agents cross-checking each other made the picture *more* confident, not more correct — the classic signature of correlated error, and the reason `RULE-agent-behavior.md`'s thinking floor says three prior agents agreeing is not evidence.

**The only thing that detects it is asking git rather than the filesystem** — `git cat-file -e HEAD:<path>`, `git log --diff-filter=D -- <path>`, `git status --porcelain`. None of which any agent ran, because none of them had a reason to doubt a file that was right there.

**Generalised rule, and the reusable output of this doc:** *an audit or design that reports "verified against source, not against status lines" must state which source — `HEAD` or the worktree — and must have checked that they are the same.* "I read the code" is not a provenance claim. On a repo with uncommitted work in flight, the worktree is a hypothesis, not a fact.

### 5. What the false picture actually cost

| Claim made during Phase A | Status |
|---|---|
| `plan-docs`: "WS-A, WS-B and WS-C are all still genuinely unimplemented — verified against source, not against their own status lines" | **false for WS-C.** The source it verified against was `agent_usage.rs`, the ghost |
| `docs/index.md` listing `plan/usage-probe-oop.md` as an active plan | **wrong**, and written by the lead from the ghost. Corrected 2026-07-30 |
| `backlog-jul27.md` D-7: "`done/usage-probe-oop.md` is a stale duplicate claiming `implemented` while the live copy is unstarted" | **inverted.** The `done/` copy is the only one at `HEAD` and is correct. The finding pointed a reader away from the truth |
| The owner's own framing, "OOP chưa hề viết", relayed back to him by the lead as confirmed | **wrong, and the confirmation was the lead's error rather than his.** He was reading the same tree |
| `oop-architect` convened to design a decomposition | The mandate was void on arrival; it graded the shipped one instead |

Nothing was implemented on top of the false picture — Phase A writes no code — so the cost was analysis and one wrong row in the index, not a bad diff. That is luck about *when* the discovery landed, not a property of the process.

### 6. The refactor, graded — since it exists and nobody had reviewed it

Read against `RULE-design-core.md` §C1:

- **No new abstraction was introduced.** Files are named by role and hold what their name says. This is the correct shape for a decomposition whose purpose is to make an 892-line file navigable.
- **The `UsageProbe` trait rejection is upheld, with better support than the original argument.** Everything the two probes genuinely share — the host, the script-dispatch funnel, `AgentUsageResult` — is *already* extracted as shared types and functions. A trait would be a second sharing mechanism layered over a working first one, for two implementors and one polymorphic call site. "OOP" was the owner's word for decomposition; it was not a request for a vtable, and `RULE-coding.md` A2 is not suspended by using the acronym.
- **One real leak survives:** `agent_usage/claudecode.rs:45-52` still takes an `agent_name` parameter and contains the literal string `"antigravity"`. Half of the agent-dispatch decision lives in the wrong module.

### 7. A defect the council predicted would be introduced was already shipped

Phase A produced an amendment to `usage-probe-oop.md` §3 warning that the JS probe's **P10** defect must be marked, or the shell port would bake in a missing CSRF token and the parity checklist would pass *because both sides are wrong*.

It had already been ported. In `scripts/get-antigravity-usage.sh`, the language-server branch extracts the token at `:93`, but the `agy` branch (`:224-250`) sets `proc_type="cli"` and never calls `extract_arg` — so `hdr_csrf` is empty at `:147-149`: no CSRF header, no seeded port.

**This matches the owner's issue #4 exactly** ("Antigravity cannot measure quota while `agy` runs in the in-app terminal"), in the build he is running now. The council's guess about the *cause* of #4 — "the OOP refactor broke the flow" — was right in substance and wrong in tense: the refactor did not break it, the refactor **carried a pre-existing break across intact**, which is why the symptom looks like a regression that started around a refactor.

### 8. Method findings from the same session, recorded because they recur

Three failures of this council are worth more than the bug they surrounded:

- **Escalating what the room should have reasoned out.** Four questions went to the owner; he rejected all four. Three were requests to run a command, on a repo carrying hundreds of logs and research docs sufficient to reason the flow out; one asked him to characterise his own bug. The correct default is that a specialist unable to *test* still owes a decision robust across the surviving candidates — "I cannot verify it" is a property of the answer, not permission to stop.
- **Closing a live question on a single source.** The Vietnamese-IME item was refused on the strength of one GitHub issue number, which turned out to be about a different input method than the one the owner uses. When it was reopened with instructions to find enough independent streams, it closed outright — and inverted the mechanism three prior research rounds had shared. See `terminal-vietnamese-ime-root-cause-4.md`.
- **Adopting the owner's causal diagnosis as a premise.** Three of the thirteen reports carried a stated cause. All three were wrong or unexecutable as literally worded, and each was caught only because the red-team agent's first assignment was the decomposition itself rather than the content. A report is evidence about the *symptom*; its embedded cause is a hypothesis with the same standing as any other.

## Verification

Every claim above is reproducible on this box, in this order:

```
# §1 — the refactor is reachable, and the file it removed is still on disk unchanged
git log -1 --format='%H %ad %s' --date=iso 610fd93
git merge-base --is-ancestor 610fd93 HEAD && echo reachable
git cat-file -e HEAD:src-tauri/src/agent_usage.rs 2>/dev/null || echo 'not at HEAD'
git cat-file -p 610fd93^:src-tauri/src/agent_usage.rs | cmp - src-tauri/src/agent_usage.rs && echo 'identical to pre-refactor'
stat -c '%y' src-tauri/src/agent_usage.rs      # old mtime => never deleted, not restored

# §2 — the index is stale: its tree is an ancestor commit's, not HEAD's
git write-tree                                  # c14e723...
git rev-parse HEAD^{tree}                       # 13e064a... — different
git rev-parse 8cc2669^{tree}                    # c14e723... — the index matches THIS
stat -c '%y' .git/index

# §3 — two candidate files for one module declaration
grep -n '^mod agent_usage' src-tauri/src/lib.rs && ls src-tauri/src/agent_usage.rs src-tauri/src/agent_usage/mod.rs

# §7 — the CSRF defect, live in the shipped shell probe
grep -n 'proc_type=\|extract_arg\|hdr_csrf' scripts/get-antigravity-usage.sh
```

Two caveats on method, both of which cost a wrong intermediate conclusion during this investigation and are recorded so the next reader skips them:

- **`git status` was the wrong instrument and produced a confidently wrong first answer** — the two leftovers were initially reported as "deliberately staged", which the `A ` code does mean under a *current* index and does not mean under a stale one. The reliable comparison is `git write-tree` against `git rev-parse HEAD^{tree}` **first**, before reading any status code.
- **`git diff HEAD` is also unreliable here**, because it enumerates paths through the index: files committed after the stale index appear as `D` (deleted) while simultaneously appearing in `git ls-files --others` as untracked. Both are artefacts. Only `git cat-file -e HEAD:<path>` plus `ls` answers "is it in the commit, is it on disk".

The E0761 conclusion is the one item derived rather than observed: it follows from rustc's documented module-resolution rule applied to the §3 output. Confirming it by build is Mac-only and would be redundant.

## Corroborating links

- `docs/plan/done/usage-probe-oop.md` — the WS-C plan, at `HEAD`, status `implemented`; the copy at `docs/plan/usage-probe-oop.md` is the leftover
- `docs/plan/backlog-jul27.md` — the live workstream tracker; findings D-7 (inverted), D-12 (non-compiling tree), D-13 (P10 live in the build)
- `docs/research/terminal-vietnamese-ime-root-cause-4.md` — the single-source failure named in §8, and its correction
- `RULE-agent-behavior.md` B5 — audits are read-only and never auto-classify ambiguous work; the rule that stopped this from becoming a cleanup commit
- `RULE-docs.md` §C — drift severity grading; this session re-graded its own output from wrong 0 to wrong 1 on the strength of §5

## Decision

**Follow-up action, owner-gated.** Nothing here is self-executing, and the two housekeeping items are git-state mutations that an audit may not perform (`RULE-agent-behavior.md` B5):

1. **Delete the leftovers** — `src-tauri/src/agent_usage.rs`, `scripts/get-antigravity-usage.js`, `docs/plan/usage-probe-oop.md`. Low risk in itself: all three are byte-identical to blobs still reachable in history, so nothing is lost. It is gated only because deleting files is the owner's call, and because of item 2.
2. **Ordering constraint on item 1:** `scripts/get-antigravity-usage.js` **is** the reference implementation for the JS↔shell parity check, and the existing fixtures were derived from the same reading of it — so they cannot catch an error shared with that reading. The parity run happens *before* the deletion, or the only independent check is destroyed. §7 is evidence this is not hypothetical: one defect did cross the port unnoticed.
3. **Refresh the index** (`git status` will do it, or `git update-index --refresh`) so the working tree becomes readable again. Until then no `git status`, `git diff` or `git add -p` output from this box means what it says.
4. **Establish whether the Mac has the same leftover** (§3) — one `ls src-tauri/src/agent_usage.rs` there. If it does, the app does not build on the only machine that can build it, and that is the highest-priority item in this doc.
5. **The P10 / CSRF defect (§7) is a code fix**, scheduled as issue #4 in this batch, and is shell-only — no Rust build required to land it.
6. **The `claudecode.rs` leak (§6) is a small follow-up**, not scheduled here.

**No action** on the refactor itself: it is sound, and the trait rejection stands.

**Cross-references:** the seven plan docs produced by the same session (`terminal-input-surface`, `ag-usage-pin-vs-live`, `cc-account-identity-ssot`, `dev-build-visibility`, `dev-build-in-app-launch`, `ui-sweep-misses`, `project-visibility-toggle`) were all written against the worktree. Two of the thirteen issues were affected and have been re-checked rather than assumed: WS-C's inventory row (corrected), and issue #4, whose cause moved from "the refactor broke it" to "the refactor carried an existing break across". The remaining plan docs read frontend `.vue`/`.js` and shell files, none of which has a leftover twin.
