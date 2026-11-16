# Plan: Antigravity Multi-Account Statusline Cache & Hybrid Quota Monitoring

Status: done 2026-11-16. Phases 3-4 already existed in 1.36.0 (`antigravity_payload.rs` keeps one account per email in `allAccounts`; `usageAccountCache.js` and `AgentUsageSlot.vue` persist and match every entry), so only phases 1-2 were built, with two deviations: the record is `rate-limits/<email>.frame` (the two frame bodies, so the probe needs no `jq`), not `<email>.json`, and the headless `/usage` call stays as it was (the probe cannot know the Keychain account without running it); its frame comes first and wins for its own account. Design: `docs/arch/usage-antigravity.md` § Per-account quota cache. Verified by `cargo test` (395 pass): the template run as AGY writes the frame for its account with the pool grouping and reset times, writes none without quota or from Claude Code, and two such frames parse into two `allAccounts` entries. Live: files appear from the first agy turn after Apply on each host.

**Origin:** Turn diagnosis (2026-10-30) — concurrent `agy` in-RAM sessions missed by Keychain-only headless probe.  
**Baseline version:** `v1.36.0` (`6c30560`)  
**Scope:** Implement a lightweight, zero-latency hybrid quota monitoring system for Antigravity (`AGY`) in Aki-Dev-Sync. Modelled after the proven Claude Code pattern (`docs/arch/usage-claudecode.md`), this plan leverages the turn payload `agy` already pipes to `statusline.sh` on every prompt to capture per-account in-RAM metrics (`kimoanh`, `lacvietanh`, `lva`, etc.), eliminating the single-account Keychain blind spot and 1-minute probe lockouts.

---

## 1. Problem Statement & Root Cause

### The Problem
When users run multiple concurrent `agy` CLI sessions under different profiles (`_AGY_PROFILES`: `lac`, `lva`, `kim`, `vie`):
1. **Keychain-Only Blind Spot:** `scripts/get-antigravity-usage.sh` executes headless `agy -p "/usage"`. This invocation reads the credentials currently stored in the macOS Keychain (`security find-generic-password -s gemini -a antigravity`). It cannot see or query the in-RAM credentials of concurrent terminal sessions authenticated under other accounts.
2. **False Staleness & Pinned Slot Fallback:** If a slot in Aki-Dev-Sync is pinned to `kimoanh`, but Keychain currently holds `lacvietanh`, the live probe only returns data for `lacvietanh`. The `kimoanh` slot falls back to `localStorage` offline cache, showing stale data (e.g. `103h ago`) even after a turn was just completed in that session.
3. **Heavyweight & 1-Minute Cache Lock:** Spawning `agy -p "/usage"` costs 3–5 seconds and ~200MB RSS, occasionally hanging on migrations. The probe script applies a 15s watchdog and a strict 1-minute file cache (`/tmp/aki-devsync-agy-usage.<uid>`), causing manual refresh clicks within 60s to be ignored.

### The Solution: The Dual/Hybrid Strategy
* **Direct Process RAM Access:** Blocked by macOS SIP and sandbox boundaries.
* **Statusline Export (The Existing Seam):** Every interactive `agy` session already executes `~/.gemini/antigravity-cli/statusline.sh` on every turn, piping a process-specific JSON payload via `stdin` that contains:
  ```json
  {
    "account": { "email": "kimoanh240501@gmail.com" },
    "quota": {
      "gemini-5h": { "remaining_fraction": 1.0, "reset_in_seconds": 18000 },
      "gemini-weekly": { "remaining_fraction": 0.97, "reset_in_seconds": 604800 },
      "gemini-argon-wtu-weekly": { "remaining_fraction": 0.17, "reset_in_seconds": 604800 },
      "3p-weekly": { "remaining_fraction": 1.0, "reset_in_seconds": 604800 }
    },
    "plan_tier": "Google AI Pro"
  }
  ```
* By persisting a lightweight JSON cache file per account on each statusline turn (`~/.gemini/antigravity-cli/rate-limits/<email>.json`), the probe can read all active sessions in **0ms** without spawning `agy`, while preserving IDE Connect RPC and headless CLI as fallbacks.

---

## 2. Architecture & Data Flow

```
[ Terminal Tab A (agy kim) ]      [ Terminal Tab B (agy lac) ]      [ Antigravity IDE ]
             │                                 │                            │
      turn   │                          turn   │                            │ Connect RPC
             ▼                                 ▼                            ▼
  statusline-unified.sh             statusline-unified.sh              Language Server
             │                                 │                            │
             ▼                                 ▼                            │
  rate-limits/kim.json              rate-limits/lac.json                    │
             │                                 │                            │
             └─────────────────┬───────────────┘                            │
                               ▼                                            │
               scripts/get-antigravity-usage.sh <───────────────────────────┘
                               │
                               ▼ (Delimited |||AGPROC||| frames)
                  Rust: antigravity_payload.rs
                               │
                               ▼ (allAccounts array payload)
                      Vue: usageAccountCache.js
                               │
                               ▼
            AgentUsageSlot.vue (Multi-Account Slots A..F)
```

---

## 3. Execution Phases

### Phase 1 — Statusline Hook Extension (`src-tauri/src/statusline-unified.sh`)

**Target:** `src-tauri/src/statusline-unified.sh` (marker `# aki-agquota v3`)  
**Scope:** Extend `statusline-unified.sh` to capture `.quota` alongside `.plan_tier` when `CLI="AG"`.

1. **Per-Account Rate Limit Cache (`~/.gemini/antigravity-cli/rate-limits/<email>.json`):**
   - Extract `.account.email // .user.email // .email` as `$JSON_ACCOUNT_EMAIL`.
   - In the existing `jq` pass, extract the quota object and plan tier.
   - When `$CLI = "AG"` and `$JSON_ACCOUNT_EMAIL` is non-empty, write `~/.gemini/antigravity-cli/rate-limits/<sanitized_email>.json` atomically (`mktemp` + `mv -f`):
     ```json
     {
       "email": "kimoanh240501@gmail.com",
       "plan_tier": "Google AI Pro",
       "quota": { ... },
       "seen_at": 1793370695
     }
     ```
   - **Zero Subprocess Overhead:** Runs within the single existing `jq` pipeline inside `statusline-unified.sh`, adding <1ms per turn.
2. **Version Marker Bump:**
   - Bump `# aki-agquota v2` $\rightarrow$ `# aki-agquota v3` in `src-tauri/src/statusline-unified.sh` and `src-tauri/src/statusline.rs`.

---

### Phase 2 — Probe Aggregator (`scripts/get-antigravity-usage.sh`)

**Target:** `scripts/get-antigravity-usage.sh`  
**Scope:** Read all active statusline cache files first before resorting to headless CLI.

1. **Check Statusline Cache Directory (`~/.gemini/antigravity-cli/rate-limits/`):**
   - If files exist and their `mtime` is fresh (< 6 hours old), parse all `*.json` files.
   - For each fresh account, emit a dedicated `|||AGPROC|||` frame:
     - `TYPE`: `cli`
     - `STATUS`: `{"email": "<email>", "userTier": {"name": "<plan_tier>"}}`
     - `SUMMARY`: Quota JSON mapped into the `command.data.groups` schema.
   - Mark `FRAMES_EMITTED > 0`.
2. **Hybrid Fallback:**
   - If no fresh statusline cache file exists for the on-disk/Keychain account, run `agy_cli_frame` (`agy -p /usage`) as the fallback source.
   - Existing IDE Connect RPC probe remains unchanged and takes precedence for IDE instances.

---

### Phase 3 — Backend Ingestion & Payload Assembly (`src-tauri/src/agent_usage/antigravity_payload.rs`)

**Target:** `src-tauri/src/agent_usage/antigravity_payload.rs`  
**Scope:** Ensure multiple CLI frames are cleanly merged and exposed via `allAccounts`.

1. **Multi-Account Aggregation:**
   - `parse_antigravity_frames` loops over all emitted `cli` and `ide` chunks.
   - Ensure snapshots for different emails are preserved in the `snapshots` vector.
   - Primary snapshot holds the active or newest session; `allAccounts` holds the complete list of all active sessions (`email`, `userTier`, `quotaSummary`, `sourceType`, `timestamp`).
2. **Unit Tests:**
   - Add unit test verifying multi-account frame parsing: 2 distinct CLI frames (`kim` + `lac`) must result in `primary.email == "kim"` and `primary.allAccounts.len() == 2`.

---

### Phase 4 — Frontend Persistence & Slot Matching (`usageAccountCache.js`, `AgentUsageSlot.vue`)

**Target:** `src/composables/usageAccountCache.js`, `src/components/AgentUsageSlot.vue`  
**Scope:** Store all returned accounts and display live data in pinned slots.

1. **Multi-Account Ingestion (`usageAccountCache.persist`):**
   - Iterate over `dataObj.allAccounts` and update `store.accounts["<host>|<email>"]` for each account present in the payload.
2. **Pinned Slot Matching (`AgentUsageSlot.vue`):**
   - When a slot is pinned to `kimoanh`, check `src.data.allAccounts` for `a.email === "kimoanh"`.
   - If found, mark `isCached: false` and render live metrics with the account's own `seenAt`.
   - If absent, fall back to offline cache from `accountCache.load()`.

---

## 4. Verification & Testing Checklist

- [ ] **Statusline Hook Test:** Run an interactive `agy` turn in terminal and verify `~/.gemini/antigravity-cli/rate-limits/<email>.json` is written atomically with valid JSON.
- [ ] **Multi-Terminal Isolation Test:**
  - Launch `agy-kim` in Terminal 1, type a prompt. Verify `rate-limits/kimoanh240501@gmail.com.json` is updated.
  - Launch `agy-lac` in Terminal 2, type a prompt. Verify `rate-limits/lacvietanh@gmail.com.json` is updated.
- [ ] **Script Probe Output Test:** Run `bash scripts/get-antigravity-usage.sh` and confirm it outputs multiple `|||AGPROC|||` frames within <100ms.
- [ ] **UI Slot Verification:**
  - Open Aki-Dev-Sync with Slot B pinned to `lacvietanh` and Slot E pinned to `kimoanh`.
  - Both cards show live status (`isCached: false`) with their respective in-RAM quota numbers and model pools.
- [ ] **Fallback Verification:** Test cold start without active statusline cache to confirm fallback to `agy -p /usage` functions correctly.
- [ ] **Lint & Rule Receipt:** Run `python3 ~/.agents/skills/akiflow/scripts/scythe.py` to confirm 0 `[WRAP]` and 0 `[YAP]` formatting violations.
