# Research - Sync Button Semantic Analysis

**Date:** 2026-06-29 - 2026-06-30  
**Triggered by:** Real incident + hands-on reproduction session  
**Status:** All items resolved. Tier 1 + Tier 2 (baseline manifest, bidirectional) + DRY RUN guard bug + EC-3 symmetric - complete.

Cross-refs:
- Feature doc: `docs/feat/sync-flow.md` (CRITICAL section + Status Checker)
- Plan / fix design: `docs/plan/done/fix-sync-divergence-safety.md`
- Code: `src-tauri/src/sync.rs` → `rsync_change_files`, `compute_sync_counts`, `build_rsync_args`, `write_baseline`
- Guard: `src/composables/useSync.js` (line ~28, `isDeleteOp`)

---

## 1. Design Intent (authoritative)

> Stated by owner (2026-06-29):

| Button | Should light when… |
|--------|-------------------|
| **PUSH** | Local has files or changes Remote does not yet have |
| **PULL** | Remote has files or changes Local does not yet have |

This is directional freshness, not rsync work-to-do. A button lighting means "**that side originated something the other side should receive.**" It does NOT mean "rsync has operations to perform in that direction" - those are related but not the same concept.

---

## 2. Observed Incidents

### 2a. Production incident (2026-06-29, `app.akinet.me`)

- Remote (AI engine) had been editing files while Mac held older snapshot.
- Background poll showed both PUSH and PULL lit.
- `delete_on_push` was ON. Operator pressed PUSH.
- Outcome on remote:
  - Remote-only new files → **deleted** (--delete removed them)
  - Previously deleted remote files → **restored** from local (they existed locally, --delete doesn't apply)
  - Remote-edited files (newer mtime) → **survived** (-u skipped them)
- Diagnostic fingerprint: asymmetric "Frankenstein" result - exactly what `-u --delete` produces.

### 2b. Reproduction session (2026-06-29)

**Step 1:** `rm -fR ./*` on remote server (`bien:~/aki/web/app.akinet.me/`)

`rm -fR ./*` uses bash glob `*`. In bash without `dotglob`, `*` does NOT match dotfiles. Result: all regular files removed; `.gitignore` survived on remote with its original mtime.

**Observed:** After re-scan, PULL button lit. User expectation: only PUSH should light ("remote is now empty, nothing new there to pull").

**Step 2:** PULL dry-run to investigate:

```
rsync -avzu --dry-run --delete bien:~/aki/web/app.akinet.me/ /Volumes/DEV/www/app.akinet.me/
receiving incremental file list
deleting server/utils/verifyFirebaseToken.ts
deleting server/api/chat.post.ts
... (hundreds of deleting lines)
./
.gitignore
```

The dry-run shows: hundreds of `deleting …` lines (files in LOCAL that remote doesn't have), and only `.gitignore` and `./` as actual transfers from remote.

**Step 3:** User PUSH with `delete_on_push=true` (and `delete_on_pull=true`). Expected: remote now matches local. Observed: **PULL still lit**. PULL dry-run shows only `.gitignore`.

---

## 3. Code Analysis

### 3a. `count_rsync_changes` - the status check (sync.rs ~312)

```rust
fn count_rsync_changes(project: &SyncProject, is_push: bool) -> Result<usize, String> {
    // builds args using build_rsync_args - inherits --delete if project.delete_on_push/pull
    let mut args = build_rsync_args(project, is_push, true, &[], sync_git, src, dest);
    args.insert(insert_pos, "--modify-window=2".to_string());
    // runs rsync --dry-run, counts non-noise output lines
    let count = stdout.lines().filter(|line| {
        let l = line.trim();
        !l.is_empty()
            && !l.starts_with("sending ")
            // ... other noise filters ...
            && !l.ends_with('/')
    }).count();
}
```

**Bug (pre-fix):** The filter did NOT exclude `deleting …` lines. Those lines represent files the DESTINATION has that the SOURCE lacks - i.e., the opposite direction's signal. Counting them toward the current direction's button was wrong.

**Example of wrong counting:**  
PULL dry-run (source=remote, dest=local). Remote is empty. Local is full. `--delete` causes rsync to list all local files as `deleting …`. Old code counted those → `pull_count = 500+` → PULL lit. But remote has nothing new; PULL lighting was incorrect.

**Fix applied (2026-06-29):** Added `&& !l.starts_with("deleting ")` to the filter. Count is now additive-only: each direction counts only content the source is offering the destination.

### 3b. `build_rsync_args` - the `-u` + `--delete` incoherence (sync.rs ~129)

```rust
let mut args = vec!["-avzu".to_string()];  // -u always present
// ...
if (!is_push && project.delete_on_pull) || (is_push && project.delete_on_push) {
    args.push("--delete".to_string());
}
```

`-u` (--update): skip destination files **newer** than their source counterpart.  
`--delete`: remove destination files that have **no source counterpart**.

These two flags have contradictory semantics when the intent is "mirror":

- Mirror intent: "make destination exactly match source - source is authoritative."
- `-u` says: "if destination file is newer, protect it - don't overwrite."
- Together: `--delete` removes destination-only files ✓, but `-u` protects destination-newer files ✗ (they should be overwritten in a true mirror).

**Concrete manifestation (EC-1):**  
After `rm -fR ./*` on remote, `.gitignore` survived with its original mtime. Local's `.gitignore` may have a different (possibly older) mtime from a prior sync. When PUSH runs with `-u --delete`:
- `--delete` removes all remote files that local doesn't have → correct
- `-u` sees remote's `.gitignore` mtime ≥ local's → **skips it, does not overwrite**
- After PUSH: remote's `.gitignore` still holds its pre-rm mtime
- PULL dry-run: remote's `.gitignore` is "newer" → counts as transfer → PULL stays lit
- User is stuck in a loop: PUSH cannot fully mirror because `-u` protects remote-newer files

**This is not a bug in either `-u` or `--delete` individually.** Each is correct for its own purpose. The bug is combining them: mirror mode should use `--delete` WITHOUT `-u`. Merge mode should use `-u` WITHOUT `--delete`.

### 3c. Safety guard (useSync.js:24-39)

```js
if (direction === 'push' && project.delete_on_push && specificPaths.length === 0) {
  if (projectRuntime.value[project.id]?.hasPendingPull === true) {
    // show Swal confirm
  }
}
```

The guard reads `hasPendingPull` from `projectRuntime` - the value set by the last background poll (up to 60s ago). If the poll just ran and cleared the flag (or if the flag was cleared optimistically after a prior push), the guard silently passes even if remote actually has pending changes.

Additionally: the guard shows a warning message but no concrete list of what will be deleted, and requires only one click to dismiss.

### 3d. `--modify-window=2` asymmetry

`count_rsync_changes` inserts `--modify-window=2` to tolerate APFS (nanosecond) vs ext4 (1-second) mtime precision differences. The actual `run_sync` path does NOT apply this flag.

Consequence: the status check can show "in sync" while an actual sync would still transfer files (if the mtime delta is <2s but >0). This is a cosmetic inconsistency, not a data-loss risk.

---

## 4. Edge Cases

| ID | Scenario | Button behavior (pre-fix) | Button behavior (post deleting-fix) | Root | Remaining fix |
|----|----------|--------------------------|-------------------------------------|------|--------------|
| EC-1 | Dotfile (`.gitignore`) survives `rm -fR ./*`; PUSH with delete doesn't overwrite it; PULL stays lit | PULL sáng (deleting lines + mtime issue) | PULL still lit (mtime issue persists) | `-u` + `--delete` incoherence | Tier 1 #3: drop `-u` in mirror mode |
| EC-2 | Remote emptied; PULL lights from deleting lines | PULL sáng (wrong) | **Fixed** - PULL off ✓ | `deleting` lines counted in pull_count | Done |
| EC-3 | Local deletes file; remote still has it | PULL sáng (rsync sees remote has file local doesn't) | **Fixed (v1.7)** - reclassified to push_count via baseline | No baseline; rsync can't distinguish "remote created" vs "local deleted" | Tier 2: PULL file + in baseline + absent locally → push_count |
| EC-3-sym | Remote deletes file; Mac still has it | PUSH sáng (rsync sees Mac has file remote doesn't) | **Fixed (v1.7)** - suppressed from push_count via baseline | Same stateless ambiguity, opposite direction | Tier 2: PUSH file + in baseline → remote deleted → suppress push_count |
| EC-4 | `--modify-window=2` in status but not in sync | Status "clean" but sync transfers ≤2s delta files | Same | Asymmetric flag | Document; optionally apply to run_sync |
| EC-5 | Safety guard uses stale `hasPendingPull` (up to 60s old) | Guard may silently pass when it shouldn't | Same | Cached poll value, not fresh check | Tier 1 #2: fresh dry-run at click time |
| EC-6 | Optimistic `hasPendingPush=false` set before 3s recheck | 3s window shows false "all clear" | Same | Optimistic clear without confirmation | Accept or delay optimistic clear |

### EC-1 deep trace (dotfile `-u` loop)

```
Initial state: local and remote in sync, .gitignore mtime = T₀ on both sides

rm -fR ./*  (on remote)
  → remote: .gitignore survives at mtime T₀
  → all other files gone

PUSH (rsync -avzu --delete LOCAL/ REMOTE/):
  → --delete: removes no extra files (remote already empty of non-dotfiles) ✓
  → transfers all local files to remote ✓
  → .gitignore: remote mtime T₀, local mtime T₀ (or T₀-ε from prior sync)
    → -u: T₀ >= T₀-ε → remote is "not older" → SKIP .gitignore

After PUSH:
  → remote has all files ✓
  → remote .gitignore mtime = T₀ (unchanged)
  → local .gitignore mtime = T₀-ε (older)

PULL status check (rsync -avzu --dry-run --delete REMOTE/ LOCAL/):
  → .gitignore: remote T₀ > local T₀-ε → -u says "source newer → transfer"
  → pull_count++ → PULL lights

User PULLs .gitignore → local mtime = T₀
Next PUSH: T₀ = T₀ → equal → rsync skips (no update needed) → stable ✓

But if user had just PUSHed without PULLing .gitignore first:
  → loop continues because PUSH never overwrites T₀ on remote
```

The resolution is either: PULL the `.gitignore` once (restores local to T₀), or fix the root by dropping `-u` in mirror mode (PUSH forcibly writes local version regardless of remote mtime).

### EC-3 fundamental ambiguity (and its symmetric case)

Both directions of the ambiguity are now resolved via the Tier 2 baseline manifest.

**Direction A - PULL side (Mac deleted, remote still has):**
```
State: local and remote synced. File X exists on both. Baseline records X.

User deletes X locally:
  local: X gone
  remote: X exists at mtime Tₓ

rsync perspective (PULL dry-run, remote→local):
  "remote has X, local doesn't → offer X to local"
  → pull_count++ → PULL lights (wrong)

Resolution (v1.7.0 baseline):
  X in pull_files + X in baseline + X absent locally
  → Mac deleted X → reclassify to push_count ✓
  → PUSH lights (needs PUSH --delete to propagate deletion) ✓
```

**Direction B - PUSH side / EC-3-sym (remote deleted, Mac still has):**
```
State: local and remote synced. File Y exists on both. Baseline records Y.

Remote deletes Y (e.g. AI agent removes a file during refactor):
  remote: Y gone
  local (Mac): Y still exists (unchanged since last sync)

rsync perspective (PUSH dry-run, local→remote):
  "Mac has Y, remote doesn't → offer Y to remote"
  → push_count++ → PUSH lights (wrong)

Resolution (v1.7.0 baseline):
  Y in push_files + Y in baseline
  → remote deleted Y → suppress from push_count ✓
  → PUSH badge stays dark ✓
  → correct action: PULL --delete removes Y from Mac too

This is the dominant case when 75%+ of coding happens on the remote server.
Without the fix, every file deleted on the remote falsely inflates the PUSH badge on Mac.
```

**Baseline location:** `{appDataDir}/baselines/{project_id}.json` (Tauri appDataDir, e.g. `~/Library/Application Support/Aki Dev Sync/baselines/` on macOS). Old builds wrote to `~/.aki/devsync-baselines/` - still read as fallback for smooth upgrades.

---

## 5. Status Check vs Actual Sync - Flag Separation

Status check và actual sync dùng flag khác nhau có chủ đích. Đây là lý do và các edge case.

### Tại sao phải tách

| | Status check (`count_rsync_changes`) | Actual sync (`run_sync`) |
|--|--------------------------------------|--------------------------|
| **Mục tiêu** | "Bên này có gì mới hơn để cung cấp cho bên kia?" | "Thực hiện đồng bộ theo mode đã chọn" |
| **Merge mode** | `-avzu` (no `--delete`) | `-avzu` (no `--delete`) |
| **Mirror mode** | `-avzu` (no `--delete`) - **giữ nguyên** | `-avz --delete` (bỏ `-u`) |
| **Vai trò của `-u`** | Phân biệt chiều: chỉ đếm file mà source mới hơn dest | Mirror: không cần (source fully authoritative) |

**Tại sao status check không được dùng `-avz` (không có `-u`):**  
Không có `-u`, rsync chỉ so mtime/size - thấy khác là list, không quan tâm bên nào mới hơn. Kết quả: local modify file → cả PUSH lẫn PULL đều show file đó → cả 2 nút sáng sai.

**`-u` hoạt động thế nào trong status check:**
- PULL check (source=remote, dest=local): `-u` skip nếu local mới hơn remote → chỉ đếm file remote mới hơn → đúng
- PUSH check (source=local, dest=remote): `-u` skip nếu remote mới hơn local → chỉ đếm file local mới hơn → đúng

### Edge case table

| Scenario | PUSH sáng? | PULL sáng? | Đúng không? | Ghi chú |
|----------|-----------|-----------|-------------|---------|
| Local modify file (local mới hơn) | ✅ Có | ❌ Không | ✓ | `-u` skip remote-older trong PULL check |
| Remote modify file (remote mới hơn) | ❌ Không | ✅ Có | ✓ | `-u` skip local-older trong PUSH check |
| File mới tạo ở local, remote chưa có | ✅ Có | ❌ Không | ✓ | dest không tồn tại → `-u` không áp dụng |
| File mới tạo ở remote, local chưa có | ❌ Không | ✅ Có | ✓ | idem |
| Cả 2 bên có thay đổi (true diverge) | ✅ Có | ✅ Có | ✓ → DIVERGED | Orange outline, count badge |
| EC-1: dotfile sau `rm -fR ./*` + PUSH mirror | ❌ Không (sau fix) | ❌ Không (sau fix) | ✓ | Actual sync bỏ `-u` → overwrite dotfile hoàn toàn → mtimes đồng nhất sau PUSH |
| EC-3: local xóa file, remote còn | ❌ Không | ✅ Có → **reclassified to push_count (v1.7)** | ✓ (fixed) | Baseline: PULL file + in baseline + absent locally → push_count |
| EC-3-sym: remote xóa file, Mac còn | ✅ Có → **suppressed (v1.7)** | ❌ Không | ✓ (fixed) | Baseline: PUSH file + in baseline → remote deleted → suppress |
| EC-4: mtime chênh <2s | ❌ Không (window=2 bỏ qua) | ❌ Không | ✓ (cosmetic) | Actual sync vẫn transfer nếu mtime khác; status show sạch. Không gây mất data. |
| Force-PULL khi local mới hơn (mirror mode) | - | Không sáng (đúng) | ✓ | Nếu user vẫn bấm PULL: mirror overwrite local bằng remote-older - expected behavior |

### Side effect của việc tách

**Có thể xảy ra:** Status "sạch" nhưng actual mirror PULL vẫn overwrite local-newer files nếu user chủ động bấm.  
**Không phải bug:** PULL không sáng = "remote không có gì mới". User bấm mirror PULL là chủ động chọn remote làm authoritative. Đây là đúng với mirror semantics.

---

## 6. What Was Fixed (2026-06-29)

| Fix | File | Mô tả |
|-----|------|--------|
| EC-2 | `sync.rs` | Exclude `deleting` lines khỏi `count_rsync_changes` - additive-only count |
| EC-1 root | `sync.rs:build_rsync_args` | Mirror mode: `-avz --delete` (bỏ `-u`). Merge: `-avzu`. Tách hoàn toàn |
| EC-7 | `sync.rs:count_rsync_changes` | Force `-avzu` + remove `--delete` sau `build_rsync_args` - status check không kế thừa mirror flags |
| EC-5 | `useSync.js` | Replace stale guard bằng fresh `get_sync_delete_preview` tại click time |
| EC-5 UX | `useSync.js` | `syncing: true` set ngay sau re-entry guard - button disable tức thì, không đơ |
| DIVERGED UI | `ProjectTable.vue` | Orange outline + `btn-sync-diverged` khi cả 2 pending. Count badge absolute overlay. Không thêm row |
| Per-side counts | `sync.rs`, `useSyncStatus.js`, `ProjectTable.vue` | `push_count`/`pull_count` từ Rust → runtime → badge trên button |
| New command | `sync.rs`, `lib.rs` | `get_sync_delete_preview` → list files sẽ bị xóa, dùng cho confirm dialog |
| CLAUDE.md | `CLAUDE.md` | Narrow UI principle ghi thành rule cứng |

## 7. Resolution Status (2026-06-30)

| Item | Status | Ghi chú |
|------|--------|---------|
| EC-1 (dotfile loop) | ✓ Done | Mirror mode: `-avz --delete` (bỏ `-u`) |
| EC-2 (PULL sáng khi remote trống) | ✓ Done | Filter `deleting` lines khỏi count |
| EC-3 (local delete vs remote create - PULL side) | ✓ Done | Tier 2 baseline: PULL file + in baseline + absent locally → push_count |
| EC-3-sym (remote delete vs Mac create - PUSH side) | ✓ Done (v1.7.0) | Tier 2 baseline: PUSH file + in baseline → remote deleted → suppress from push_count |
| EC-4 (`--modify-window=2` asymmetry) | Accepted | Cosmetic only, không gây mất data |
| EC-5 (stale 60s guard) | ✓ Done | Fresh `get_sync_delete_preview` tại click time |
| EC-6 (optimistic clear 3s window) | Accepted | 3s false "all clear" - minor, recheck corrects it |
| EC-7 (cả 2 nút sáng cùng lúc) | ✓ Done | Force `-avzu` trong status check |
| **DRY RUN guard bug** | ✓ Done | `isDeleteOp` skip khi `dry_run=true` - xem bên dưới |

### DRY RUN guard bug (2026-06-30)

**Phát hiện sau Tier 2:** `isDryRun` được đọc SAU block `isDeleteOp` trong `useSync.js`. Khi dry_run ON + delete_on_push ON: dialog type-to-confirm vẫn hiện (sai - sync sẽ là dry run, không xóa gì cả). User nhầm tưởng phải xác nhận xóa, nhưng thực ra không có gì bị xóa.

**Fix (2026-06-30):**
```js
// `isDryRun` move lên đầu startSync(), trước isDeleteOp
const isDryRun = project.dry_run
const isDeleteOp = !isDryRun && specificPaths.length === 0 &&
  ((direction === 'push' && project.delete_on_push) || ...)
```
- Toast cũng được cập nhật: `isDryRun ? 'Dry run complete' : 'Sync complete'`
- Baseline write trong Rust đã đúng: `if !dry_run && specific_paths.is_empty()` ✓
- Optimistic state clear đã đúng: `if (!isDryRun && ...)` ✓

**Còn lại:** EC-4, EC-6 accepted as-is. Không còn vấn đề nào tồn đọng.
