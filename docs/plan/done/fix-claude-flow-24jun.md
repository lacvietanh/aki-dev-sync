# Fix: Claude Code Usage Sync Flow — 2026-06-24

**Status:** ✅ Done  
**Discovered:** 2026-06-24 via session audit & first-principles flow analysis  
**Scope:** `scripts/force-sync-claudecode.sh`, `scripts/get-claudecode-usage.sh`, `src/composables/useAgentUsage.js`  
**Related docs:** `docs/arch/usage-claudecode.md`, `docs/research/claude-usage-1.2.x-analyze.md`

---

## Context: How the flow works (must read first)

### Full pipeline

```
[Rust: force_sync_agent_usage]
  → SSH → force-sync-claudecode.sh
      ├─ run_usage()
      │    └─ claude --model haiku -p /usage  (in BLANK_DIR)
      │         ← offline: reads ~/.claude/projects/**/*.jsonl
      │         ← creates new JSONL in ~/.claude/projects/-tmp-aki-dev-sync-blank-dir/
      │
      ├─ output contains "resets"?
      │    YES → write cache, done
      │    NO  → [PROBE FIRES]
      │           claude --model haiku -p "respond with ok"  (in /tmp/aki-probe-<ts>)
      │           rm -rf /tmp/aki-probe-<ts>   ← removes working dir ONLY
      │           ⚠ ~/.claude/projects/-tmp-aki-probe-<ts>/*.jsonl NOT deleted
      │           run_usage() again  ← 2nd JSONL in BLANK_DIR, now sees probe session
      │
      └─ export CLAUDE_SYNC_OUT

[Rust: get_agent_usage]
  → SSH → get-claudecode-usage.sh
      ├─ reads ~/.claude/rate-limits-cache.json
      ├─ checks: NOW > resets_at?
      │    YES → echo |||STALE_RESET|||   → Rust: Ok(None) → JS: res = null
      │    NO  → cat cache file + metadata → Rust: Ok(Some(content))
      └─ (no file) → Rust: Ok(None) → JS: res = null

[JS: useAgentUsage.js — checkUsage()]
  → invoke get_agent_usage
  → res != null → parse JSON → update data.value, stale.value
      └─ if claudecode && !initialSyncDone && resets_at == 0 → forceSync()
  → res == null → data.value = null
      └─ if claudecode && !initialSyncDone → initialSyncDone=true, forceSync()

[JS: useAgentUsage.js — forceSync()]
  → invoke force_sync_agent_usage  (SSH → force-sync-claudecode.sh)
  → await checkUsage()
```

### When does Haiku probe actually fire?

The probe (`claude -p "respond with ok"`) fires **only** when:
- `force-sync-claudecode.sh` is called (i.e., `forceSync()` is called from JS)
- AND `claude -p /usage` output does NOT contain the word `"resets"`
- This happens when there is no Claude Code session in the current 5-hour billing window on the remote host

The probe consumes ~100 Haiku tokens (~$0.0001). It is rare under normal usage.

### What guarantees probe doesn't re-trigger?

After a successful probe:
1. Probe JSONL is created in `~/.claude/projects/-tmp-aki-probe-<ts>/`
2. `run_usage()` re-runs → sees probe session → output contains "resets" → cache written with `resets_at` = future timestamp
3. `checkUsage()` reads new cache → `fiveHour.resets_at > 0` and not past → `stale = false`
4. `initialSyncDone = true` was set **before** `forceSync()` was called (line 53/66) → guard prevents another auto-trigger

**The guard chain is correct and working.**

### All trigger sources for `claudeForceSync()`

| Source | Mechanism | Notes |
|---|---|---|
| Titlebar reload button | `manualRefreshCount++` → `watch` → `forceSync()` | Always fires for claudecode |
| Card header refresh button | `@retry` → `claudeForceSync` | Same as titlebar in effect |
| Empty state Force Sync button | `@force-sync` → `claudeForceSync` | Visible only when `data=null && !loading` |
| UsageCircle 5H `@timeout` | clock crosses `resetsAt` → `$emit('force-sync')` → `claudeForceSync` | Requires data != null |
| UsageCircle 7D `@timeout` | same | Requires data != null |
| Startup / host change | `watch(hostRef, immediate)` → `checkUsage()` → auto-forceSync if `resets_at=0` | guarded by `initialSyncDone` |

### When is the Force Sync button visible?

`AgentUsage.vue`: `v-else-if="!data && !loading"` and `agentId === 'claudecode'`

→ Visible only when: **cache returned STALE_RESET** (or no file) AND **not currently loading**.

---

## Issues Found

---

### ISSUE #1 — STALE_RESET can cause UI stuck indefinitely (HIGH)

**File:** `scripts/get-claudecode-usage.sh:18` + `src/composables/useAgentUsage.js`

**What happens:**

```
T+0h00m: reset window expires
T+0h00m (poll tick): checkUsage() → get-claudecode-usage.sh → NOW > resets_at
                     → |||STALE_RESET||| → res=null → data=null
                     → UsageCircle unmounted (v-else-if="data" = false)
                     → @timeout timer clearInterval'd
                     → initialSyncDone = true → no auto-forceSync

T+0h01m (poll tick): same → STALE_RESET → data=null → no auto-forceSync
T+0h02m: same... ∞
→ UI stuck on "No data — waiting for next session"
  until user manually clicks Force Sync or titlebar refresh
```

**Why it breaks:**
- `@timeout` from `UsageCircle` (the intended auto-recovery mechanism) depends on the circle being rendered
- Circle only renders when `data != null`
- STALE_RESET sets `data = null` → circle unmounts → `@timeout` dead
- `initialSyncDone = true` prevents auto-forceSync on subsequent polls
- Result: system goes silent, no self-healing

**Race that determines which path is taken:**
- If `@timeout` (10s interval) fires **before** the poll → forceSync runs → data refreshes → no stuck
- If poll fires **before** `@timeout` → STALE_RESET → stuck
- Which fires first depends on `usage_interval_s` setting and timer alignment — non-deterministic

**Fix (in `useAgentUsage.js`):**

Detect the transition from `data present + stale` → `data=null` (= STALE_RESET scenario) and auto-trigger forceSync. Use a dedicated flag to prevent loop.

```js
// Add alongside initialSyncDone:
let staleResetSyncDone = false;

// In checkUsage(), before processing res:
const hadData = data.value !== null;

// In the else (res = null) branch, after existing logic:
} else {
  data.value = null;
  provision();
  if (agentName === 'claudecode' && !initialSyncDone) {
    initialSyncDone = true;
    forceSync();
  } else if (agentName === 'claudecode' && hadData && !staleResetSyncDone) {
    // Transition: had data → now null = STALE_RESET. Auto-recover once.
    staleResetSyncDone = true;
    forceSync();
  }
}

// Reset staleResetSyncDone when data returns successfully:
// In the res != null branch, after data.value = JSON.parse(...):
staleResetSyncDone = false;

// Also reset on host change:
// In watch(hostRef):
staleResetSyncDone = false;
```

**Loop safety:** `staleResetSyncDone = true` prevents re-trigger on consecutive polls. It resets only when data returns successfully, meaning a new STALE_RESET cycle will auto-recover again correctly.

**Edge case:** If forceSync fails (SSH down), `checkUsage()` inside it returns null again. `hadData = false` at that point → `staleResetSyncDone` gate prevents loop. User sees persistent empty state — correct, since SSH is actually down.

---

### ISSUE #2 — No concurrency guard on `forceSync()` (MEDIUM)

**File:** `src/composables/useAgentUsage.js:78`

**What happens:**

Multiple trigger sources can call `forceSync()` with no mutual exclusion:

```
Scenario: 5H and 7D circles both emit @timeout in same 10-second interval
→ claudeForceSync() called twice in rapid succession
→ Two concurrent SSH sessions to the same remote host
→ Both write to rate-limits-cache.json
→ Race condition: second write may truncate or corrupt the file mid-write
```

**Or:** User clicks titlebar refresh while poll `checkUsage()` is still running → `loading = true` from checkUsage, then forceSync sets `loading = true` again and proceeds → state confusion.

**Current code (line 78-80):**
```js
const forceSync = async () => {
  if (!hostRef.value) return;   // only host check
  loading.value = true;
```

**Fix — Option A (minimal, 1 line):**
```js
const forceSync = async () => {
  if (!hostRef.value || loading.value) return;
```

**Tradeoff of Option A:** If `checkUsage()` poll is running (`loading=true`), a user-triggered forceSync is silently dropped. Acceptable since the poll completes in ~2s, but not immediately obvious to user.

**Fix — Option B (precise, dedicated flag):**
```js
let isSyncing = false;

const forceSync = async () => {
  if (!hostRef.value || isSyncing) return;
  isSyncing = true;
  loading.value = true;
  error.value = null;
  try {
    // ... existing logic
  } catch (e) {
    error.value = e.toString();
    loading.value = false;
  } finally {
    isSyncing = false;
  }
};
```

**Benefit of Option B:** User-triggered forceSync can proceed even if a poll checkUsage is running (since poll doesn't set `isSyncing`). More precise.

**Recommendation:** Option B. `isSyncing` flag also needs reset in `watch(hostRef)`.

---

### ISSUE #3 — JSONL session files accumulate from two sources (LOW)

**Files:** `scripts/force-sync-claudecode.sh`

**Source 1 — BLANK_DIR (every force-sync):**
- `run_usage()` runs `claude -p /usage` in `/tmp/aki-dev-sync-blank-dir`
- Claude CLI creates a new JSONL per invocation at `~/.claude/projects/-tmp-aki-dev-sync-blank-dir/<uuid>.jsonl`
- 17 files created today alone
- If probe fires: `run_usage()` runs twice → 2 files per force-sync call

**Source 2 — probe dirs (every actual probe):**
- Probe runs in `/tmp/aki-probe-<timestamp>`
- `rm -rf /tmp/aki-probe-<timestamp>` removes the working directory
- But `~/.claude/projects/-tmp-aki-probe-<timestamp>/<uuid>.jsonl` is NOT deleted
- Each probe leaves a permanent orphaned JSONL

**Why it matters:**
`claude /usage` (and the force-sync script itself) reads ALL `~/.claude/projects/**/*.jsonl` on every invocation. Over months of use with multiple force-sync calls per day, this accumulates thousands of tiny files (~3KB each), degrading `/usage` parse performance.

**Fix — add cleanup at end of `force-sync-claudecode.sh`:**
```sh
# Cleanup: remove JSONL files older than 7 days from BLANK_DIR project folder
BLANK_PROJECT_DIR="$HOME/.claude/projects/$(echo "$BLANK_DIR" | sed 's|^/||; s|/|-|g')"
find "$BLANK_PROJECT_DIR" -name "*.jsonl" -mtime +7 -delete 2>/dev/null || true

# Cleanup: remove orphaned probe project dirs older than 7 days
find "$HOME/.claude/projects" -maxdepth 1 -type d -name '-tmp-aki-probe-*' -mtime +7 \
  -exec rm -rf {} + 2>/dev/null || true
```

**Side effects:** None — `find -delete` on non-existent path is no-op. Only targets files older than 7 days. Does not touch active sessions.

---

## Checklist

### Pre-fix verification
- [ ] Confirm `usage_interval_s` setting value — determines likelihood of STALE_RESET race in Issue #1
- [ ] Run app, let 5h window expire, observe: does UI auto-recover or get stuck? (confirms Issue #1)
- [ ] Click titlebar refresh while another forceSync is in-flight, check network tab for duplicate SSH calls (confirms Issue #2)
- [ ] Count files in `~/.claude/projects/-tmp-aki-dev-sync-blank-dir/` before and after (confirms Issue #3)

### Fix #2 — Concurrency guard (fix this first, it's a safe 1-change)
- [ ] Add `let isSyncing = false` near `let initialSyncDone = false` in `useAgentUsage.js`
- [ ] Add `if (!hostRef.value || isSyncing) return;` at top of `forceSync()`
- [ ] Add `isSyncing = true` after the guard
- [ ] Add `finally { isSyncing = false }` block (or add to existing finally if present)
- [ ] Add `isSyncing = false` in `watch(hostRef)` reset block alongside `initialSyncDone = false`
- [ ] Test: rapid double-click of both refresh buttons → confirm only 1 SSH call in logs

### Fix #1 — STALE_RESET auto-recovery
- [ ] Add `let staleResetSyncDone = false` near other flags in `useAgentUsage.js`
- [ ] Add `const hadData = data.value !== null` at top of `checkUsage()` try block
- [ ] In `else` branch (res=null): add the `hadData && !staleResetSyncDone` auto-forceSync block
- [ ] In `res != null` success branch: add `staleResetSyncDone = false` reset after `data.value = JSON.parse(...)`
- [ ] In `watch(hostRef)` reset block: add `staleResetSyncDone = false`
- [ ] Test scenario A: let window expire with app open → confirm UI auto-recovers without user action
- [ ] Test scenario B: SSH down → confirm UI shows empty state and does NOT loop (no repeated SSH attempts)
- [ ] Test scenario C: startup with no cache → confirm forceSync fires once, not twice

### Fix #3 — JSONL cleanup
- [ ] Add cleanup block at end of `force-sync-claudecode.sh` (see code above)
- [ ] Verify: run force-sync manually, confirm only files >7 days are removed
- [ ] Check `~/.claude/projects/-tmp-aki-probe-*/` count before/after — should decrease over time

### Post-fix validation
- [ ] Full app restart → usage loads correctly
- [ ] Wait for / simulate 5h window expiry → auto-recovery without manual action
- [ ] Multiple rapid refreshes → no duplicate SSH in logs, no cache corruption
- [ ] JSONL count in BLANK_DIR stabilizes (not growing unbounded)
- [ ] No regression: Force Sync button still works from empty state
- [ ] No regression: Stale badge still appears when data is old

---

## Files to edit

| File | Change |
|---|---|
| `src/composables/useAgentUsage.js` | Fix #1 (staleResetSyncDone) + Fix #2 (isSyncing) |
| `scripts/force-sync-claudecode.sh` | Fix #3 (cleanup block at end) |

`scripts/get-claudecode-usage.sh` — **no change needed**. STALE_RESET behavior is correct; the fix belongs in JS layer.

---

## Completion

- [ ] All checklist items above checked
- [ ] Tested on real 5h reset cycle (not simulated)
- [ ] Committed with message referencing this plan
- [ ] Status updated to ✅ Done

**Done:** 2026-06-24 — see commit
