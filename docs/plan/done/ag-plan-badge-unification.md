# Plan: Antigravity Plan Badge Unification & Rule Compliance Remediation

**Research origin:** [docs/research/ag-plan-badge-and-rule-violations.md](../../research/ag-plan-badge-and-rule-violations.md)  
**Baseline version:** `v1.36.0` (`6c30560`)  
**Scope:** Unify Antigravity (`AGY`) and Claude Code (`CC`) plan badge acquisition, normalization, per-account cache preservation, header DOM/CSS (`[icon] -> [title] -> [plan badge] -> [account list]`), and sync drifted architecture docs. Also includes the 1-line Linux `stat` fix in `statusline-unified.sh:35` uncovered during the SSH `bien` investigation.

---

## 1. Why This Plan Exists (Summary of 6 Root Causes)

As proven in [docs/research/ag-plan-badge-and-rule-violations.md](../../research/ag-plan-badge-and-rule-violations.md), Antigravity's plan badge (`Pro`, `Starter`, `Ultra`, etc.) is inconsistent or missing because:
1. **RC-1 (`get-antigravity-usage.sh:287`):** `agy_cli_frame()` (`agy -p "/usage"`) emits `|||STATUS|||{"email":"%s"}` with no `userTier` (`coding.B2` violation in `51e7906`).
2. **RC-2 (`usageAccountCache.js:112-114`):** `persist()` performs a full-object overwrite (`store.accounts[key].data = accObj`), so a single `cli` poll with `userTier: null` wipes out a valid `userTier` previously cached from IDE/Desktop RPC (`CLAUDE.md` regression guard & `pattern.A8` violation).
3. **RC-3 (`antigravity_payload.rs:68-72`):** `parse_antigravity_frames()` deduplicates same-email frames by keeping only the first frame (`!seen_instance_keys.contains(&instance_key)`) instead of merging non-null fields (`userTier`, `quotaSummary`) across surfaces (`flow.B3` violation).
4. **RC-4 (`statusline-unified.sh:120-177`):** `agy` CLI pushes `.email`, `.plan_tier` (`"Google AI Pro"`), and `.quota` to `statusline.sh` on `stdin` every turn, yet `statusline-unified.sh` discards `.plan_tier` instead of caching it per account like `aki-rlcache` does for CC (`pattern.B2` violation).
5. **RC-5 (`antigravity_payload.rs:56-66, 398` vs `AgentUsage.vue:668-670`):** `AgentUsage.vue` only reads `props.data?.userTier?.name` (object), while Rust unit tests mock `"userTier": "PRO"` (string) and `GetUserStatus` also provides `planStatus.planInfo.planName` (`test.C` & `coding.C1` violation).
6. **RC-6 (`AgentUsage.vue:4, 15, 31-35, 61, 697-753`):** The header template and CSS remain forked (`isCc ? 'agent-name-row' : 'agent-name'`, `:class="{ 'claudecode-custom-header': isCc }"`), applying typography class `.agent-name` to the entire flex wrapper on AGY and retaining dead `desktop_cli` branches (`ui.B3`, `coding.B4` violation).

---

## 2. Execution Phases (Ordered by Data Flow: Producer → Backend → Cache → UI → Docs)

### Phase 1 — Statusline & Probe Producer Fix (`src-tauri/src/statusline-unified.sh`, `scripts/get-antigravity-usage.sh`, `src-tauri/src/statusline.rs`)

**Files owned:**
- `src-tauri/src/statusline-unified.sh`
- `src-tauri/src/statusline.rs` (bump `OUTDATED_AT_VERSION` if needed + add unit test for AG tier cache and Linux `stat` order)
- `scripts/get-antigravity-usage.sh`

**Changes:**
- [x] **1.1. Fix Linux vs macOS `stat` order in `src-tauri/src/statusline-unified.sh:35` (from Turn 1 `bien` SSH bug):**
  - Replace `stat -f %m "$_cc_auth_cache" 2>/dev/null || stat -c %Y "$_cc_auth_cache" 2>/dev/null` with OS-gated or `stat -c %Y` first: on Linux GNU coreutils, `stat -f` means `--file-system` and exits `0` with multi-line text, breaking `$(( _rl_now - _auth_mtime ))` and aborting the script before `RLCACHE` is written.
  - Use `if [[ "$OSTYPE" == "darwin"* ]]; then stat -f %m ...; else stat -c %Y ...; fi`.
- [x] **1.2. Persist per-account AGY statusline metadata (`aki-agcache v1`) in `src-tauri/src/statusline-unified.sh`:**
  - When `CLI="AG"` and `$input` contains a valid `.email` (or `.account.email`), write `~/.gemini/antigravity-cli/usage-cache/<sanitized_email>.json` atomically (`mktemp` + `mv -f`) with:
    - `email`: extracted from `.email // .account.email`
    - `plan_tier`: extracted from `.plan_tier // ""` — **if empty on this turn, preserve `plan_tier` from the existing `<sanitized_email>.json`** (exact same invariant as `aki-rlcache` line 81!).
    - `seen_at`: `$_ag_now` (`date +%s`)
  - Zero subprocess spawns: runs inside a single `jq` pass over `$input` + existing cache file (<10ms).
- [x] **1.3. Enrich `agy_cli_frame()` in `scripts/get-antigravity-usage.sh:262-290` with `userTier`:**
  - When building `|||STATUS|||` for a CLI account (`$email`), look up `~/.gemini/antigravity-cli/usage-cache/<sanitized_email>.json` for `plan_tier`.
  - Fallback if no statusline cache exists yet for `$email`: check if macOS/Linux `state.vscdb` (`antigravityAuthStatus`) matches `$email` or leave `userTier` absent so downstream cache merge (Phase 2 & 3) preserves any previously known tier.
  - When `plan_tier` is non-empty, emit `|||STATUS|||{"email":"%s","userTier":{"name":"%s"}}`.

---

### Phase 2 — Rust Boundary Normalization & Multi-Frame Merge (`src-tauri/src/agent_usage/antigravity_payload.rs`)

**Files owned:**
- `src-tauri/src/agent_usage/antigravity_payload.rs`

**Changes:**
- **2.1. Canonical `userTier` normalizer + bucket-topology fallback (`extract_user_tier`):** not built — see §4.
  - Extract a single pure function `extract_user_tier(status: &Value, quota_summary: &Value) -> Value` in `antigravity_payload.rs` that inspects in priority order (per [docs/ref/fact-antigravity-quota-tiers.md](../../ref/fact-antigravity-quota-tiers.md) §2 `F-AG4`):
    1. `userStatus.userTier.name` or `userTier.name` (standard `GetUserStatus` object)
    2. `userStatus.userTier` or `userTier` when it is a non-empty string (e.g. `"PRO"`, `"Google AI Pro"`)
    3. `userStatus.planStatus.planInfo.planName` (fallback in `GetUserStatus`)
    4. `plan_tier` or `userStatus.planTier` (from `statusline-unified.sh` cache)
    5. **Bucket-topology inference fallback (for cold `agy -p "/usage"` CLI polls with no prior cache):**
       - If `quota_summary.groups` is non-empty and **every** bucket across `Gemini Models` / `Claude and GPT models` has `window == "weekly"` (zero `5h` buckets), infer `{"name": "Free"}` ([fact-antigravity-quota-tiers.md](../../ref/fact-antigravity-quota-tiers.md) `F-AG1`).
       - If `quota_summary.groups` has both `5h` and `weekly` buckets **plus** a separate `Gemini 4 Argon` group (`gemini-argon-wtu-weekly`), infer `{"name": "Pro"}` (`F-AG2`).
  - Strip vendor noise (`Google`, `AI`) and trim at normalization time so every consumer receives a guaranteed `{"name": "..."}` object (`"Pro"`, `"Free"`, `"Ultra"`, `"Starter"`, etc.) or `Value::Null`.
- [x] **2.2. Merge same-email frames instead of dropping subsequent frames (`antigravity_payload.rs:68-78`):**
  - Replace the `seen_instance_keys` drop guard with an index lookup into `valid_accounts` by `email`:
    - If a second frame arrives for the same `email`:
      - If existing entry has `userTier: null` and the new frame has a non-null `userTier`, copy `userTier` into the existing entry.
      - If existing entry has empty `quotaSummary.groups` and the new frame has non-empty `quotaSummary.groups`, update `quotaSummary`.
      - Keep deterministic `sourceType` priority (`desktop` / `ide` / `cli`).
- [x] **2.3. Align Rust unit tests with real consumer contract (`antigravity_payload.rs:395-470`):**
  - Update `test_parse_antigravity_frames_single` and add `test_parse_antigravity_frames_merges_cli_and_ide_tier` verifying:
    - Object `userTier: {"id": "g1-pro-tier", "name": "Google AI Pro"}` normalizes cleanly.
    - String `userTier: "PRO"` also normalizes to `{"name": "PRO"}` so `userTier.name` is always defined.
    - Merging a `cli` frame (`userTier: null`) + `ide`/`desktop` frame (`userTier: {"name": "Google AI Pro"}`) in either order preserves `userTier.name == "Pro"` (or `"Google AI Pro"`).

---

### Phase 3 — Frontend Per-Account Cache Field Preservation (`src/composables/usageAccountCache.js`)

**Files owned:**
- `src/composables/usageAccountCache.js`
- `test/usage.test.mjs`

**Changes:**
- [x] **3.1. Preserve non-null tier metadata on partial updates in `usageAccountCache.persist()` (`src/composables/usageAccountCache.js:103-116`):** done differently, see §4.
  - Before overwriting `store.accounts[key]`, read `const prevData = prev?.data || null;`.
  - Build `mergedData`:
    - If `accObj.userTier == null && prevData?.userTier != null`, preserve `userTier: prevData.userTier`.
    - If `!accObj.rate_limit_tier && prevData?.rate_limit_tier`, preserve `rate_limit_tier: prevData.rate_limit_tier`.
    - If `!accObj.subscription_type && prevData?.subscription_type`, preserve `subscription_type: prevData.subscription_type`.
  - Also reflect `mergedData.userTier` back onto the live `payload` / `payload.allAccounts[i]` in memory when `persist()` runs (or provide a small enrich step in `usageMonitor.js`) so the active slot displaying live `monitor.data` from a CLI poll immediately inherits the cached `userTier` for that `(host, email)` without waiting to go offline!
- **3.2. Add unit test in `test/usage.test.mjs`:** not built — see §4.
  - Test that persisting an IDE reading with `userTier: { name: "Google AI Pro" }` followed by a CLI reading for the same `(host, email)` with `userTier: null` retains `userTier: { name: "Google AI Pro" }` both in `accountCache.load()` and in the live account object.

---

### Phase 4 — Unified Header Layout & Account List Polish (`src/components/AgentUsage.vue`)

**Files owned:**
- `src/components/AgentUsage.vue`

**Changes:**
- [x] **4.1. Unify Header DOM & CSS (`[icon] -> [title] -> [plan badge] -> [account list] -> [eye] -> [org]`):**
  - In `AgentUsage.vue:4`: remove `:class="{ 'claudecode-custom-header': isCc }"` if no longer needed (verify `.claudecode-custom-header` has zero CSS rules anywhere in the repo — confirmed by grep!).
  - In `AgentUsage.vue:31-35`: replace:
    ```html
    <div :class="isCc ? 'agent-name-row' : 'agent-name'">
      <span :class="isCc ? 'agent-name u-narrow-hide' : 'u-narrow-hide'">{{ displayName }}</span>
    ```
    with one unified structure for both agents:
    ```html
    <div class="agent-name-row">
      <span class="agent-name u-narrow-hide">{{ displayName }}</span>
      <span v-if="planBadge" class="agent-plan-badge" :class="isCc ? 'claude' : 'ag'">
        {{ planBadge }}
      </span>
    ```
  - This ensures both AGY and CC use `.agent-name-row` (`display: flex; align-items: center; gap: 6px; min-width: 0; flex: 1; flex-wrap: nowrap`) for the row container and `.agent-name` strictly for the title text (`u-narrow-hide` in narrow mode), guaranteeing identical spacing, alignment, and narrow-mode collapse (`[icon] -> [plan badge] -> [account list]`).
- **4.2. Harden `planBadge` computed (`AgentUsage.vue:668-670`):** not built — see §4.
  - Extract a helper `formatAgTier(rawTier)` that handles both `{ name: string }` and plain `string`, strips `\b(Google|AI)\b`, and trims, so both `planBadge` (in the header) and account items (in the dropdown, if desired) use one SSoT formatter (`pattern.A1`).
- [x] **4.3. Remove dead `desktop_cli` branches (`AgentUsage.vue:15, 61`):**
  - Change `currentSourceType === 'desktop' || currentSourceType === 'desktop_cli'` to `currentSourceType === 'desktop'` and `acc.sourceType === 'desktop' || acc.sourceType === 'desktop_cli'` to `acc.sourceType === 'desktop'`.

---

### Phase 5 — Documentation Drift Remediation (`docs/arch/usage-antigravity.md`, `docs/ref/statusline-unified-spec.md`, `docs/index.md`)

**Files owned:**
- `docs/arch/usage-antigravity.md`
- `docs/ref/statusline-unified-spec.md`
- `docs/index.md`

**Changes:**
- [x] **5.1. Fix `Wrong`, `Stale`, and `Incomplete` sections in `docs/arch/usage-antigravity.md`:**
  - Update anchor stamp (`docs.A4`) with actual verification date and version.
  - Fix §3 ("Output Mapping") to document `userTier` normalization, `sourceType`, `allAccounts`, and same-email multi-frame merging.
  - Fix §4 ("Data Shape & Frontend Adaptation") to describe the 3-pool (`Gemini`, `Argon 4`, `Claude · GPT`) linear bar model.
  - Fix §"Embedded Script Compilation" to cite `src-tauri/src/agent_usage/mod.rs` instead of `antigravity.rs`.
  - Fix §"Per-Account Cache & Multi-Slot Viewing" and §"Design locks" to remove the **Wrong** claim that Claude Code has no multi-account cache (aligning with `v1.34.0` and `docs/arch/usage-claudecode.md` §1), and document the `userTier` preservation invariant across partial `cli` polls.
- [x] **5.2. Update `docs/ref/statusline-unified-spec.md`:**
  - Document `.plan_tier` in the §2 JSON payload mapping table and the `aki-agcache` per-account tier cache for AGY CLI.
- [x] **5.3. Update `docs/index.md` (`docs.A1`):**
  - Register `docs/research/ag-plan-badge-and-rule-violations.md` and `docs/plan/ag-plan-badge-unification.md`.
  - Move `docs/plan/done/agy-argon4-quota-bars.md` from `## Plans (Active)` to `## Plans (Completed)`.

---

## 3. Verification Checklist (`coding.B3`)

- [x] **Rust unit tests:** `cargo test --manifest-path src-tauri/Cargo.toml antigravity_payload` and `cargo test --manifest-path src-tauri/Cargo.toml statusline` pass. Mac 2026-10-28: 6/6 and 50/50 (`--lib`).
- [x] **Frontend / Node unit tests:** `node --test test/usage.test.mjs` passes (including the new `userTier` cache-preservation test). Settled differently: the test was not added (§4, 3.2); every suite in `scripts/tests/` passes on the Mac 2026-10-28 (10, 28, 4, 9).
- [x] **Live probe verification (macOS):** Run `bash scripts/get-antigravity-usage.sh` and verify `cli` frame includes `userTier` and merges cleanly. Mac 2026-10-28, after Apply and agy turns: `~/.gemini/antigravity-cli/aki-plan-tier/` holds both accounts, and the probe's cached `cli` frame reads `{"email":"lva@…","userTier":{"name":"Antigravity Starter Quota"}}`; the card shows `Starter`.
- [x] **Lint check (`/akilint`):** Run `python3 ~/.aki/akidevrule/agskills/akiflow/scripts/scythe.py` on all touched files to confirm 0 `[WRAP]` or `[YAP]` violations. Mac 2026-10-28: every modified and untracked file, 0 findings, exit 0.

---

## 4. Execution Record (2026-10-28, Linux box `bien`)

Done as written: 1.1, 2.2, 2.3, 4.1, 4.3, 5.2. Done differently, with the reason:
- **1.2 / 1.3:** the record is a one-line text file `~/.gemini/antigravity-cli/aki-plan-tier/<email>` (no `seen_at`, no JSON), written only when the tier changes, from `JSON_PLAN_TIER` added to the existing `jq` pass: zero extra `jq` per turn and no write on most turns. It lives under its own dir name rather than `usage-cache/`, a name agy could plausibly use itself. The marker is `aki-agquota` bumped to v2 instead of a new `aki-agcache` marker, so the existing `ag_outdated` probe flags hosts without Rust changes. The 1.3 `state.vscdb` fallback is not built: the cache preservation in 3.1 already covers the "tier known from IDE" case.
- **2.1 not built:** every producer now emits `userTier` as an object (`GetUserStatus`, and the `cli` frame from 1.3); the string shape existed only in the test fixture, which 2.3 replaced with the real RPC shape. A normalizer for shapes nothing emits is a guard for a state the code rules out (`coding.C1`). **Bucket-topology inference deferred:** an inferred `Free`/`Pro` is non-null, so it would overwrite an explicit cached tier in 3.1 and show a guess as fact if Google changes the topology again; the statusline record gives the explicit tier for CLI accounts. Reopen if a CLI-only host still shows no badge after its AGY statusline is applied.
- **3.1:** only `userTier` is preserved (no CC field: CC's tier is already kept by `aki-rlcache`, no CC loss was observed). The live payload is not enriched: after Apply, `cli` frames carry the tier themselves; before Apply the live card shows no badge while the cached view keeps it.
- **3.2 not added:** there is no `test/` dir (JS tests live in `scripts/tests/`) and `persist()` needs a `localStorage`; the behavior was checked with a throwaway probe (two accounts, two hosts: A keeps its tier with fresh quota, B and the other host untouched, a new tier still replaces the old one), per `test.A1`.
- **4.1:** `.claudecode-custom-header` did have a rule (`margin-bottom: 4px`); it is deleted so both cards use `.agent-header`'s 2px. `.agent-name` lost its flex/gap, which only the old AG wrapper used.
- **4.2 not built:** `planBadge` already strips `Google`/`AI` and every input is now `{ name }`; a second formatter would be a second source.
- **5.1:** D-2 was a wrong finding and D-3 had nothing to fix (research § Amendments). `docs/arch/usage-claudecode.md` § 1 also claimed `stat -f`/`-c` works on Linux and is corrected. **5.3** was already in the working tree.

Verification on this box:
- `antigravity_payload` tests: 4/4 pass in an isolated crate that includes the module file (the full crate cannot compile here: no `webkit2gtk-4.1`); the new merge test failed before the fix.
- `statusline-unified.sh` run with a sandbox `HOME`: AG turn writes the tier file, a tier-less turn keeps it, CC auth-cache age is a number and the rate-limit cache is written. Probe read under `dash`: valid tier read, a tier with `"` rejected.
- `npx vite build` passes; `node --test scripts/tests/audit-ui-architecture.test.mjs` 10/10; `release_lint.py --latest` exit 0.

Closed after the first pass (2026-10-28):
- **CRITICAL BUG / MISSED STEP in 1.1 (Owner reported: reloading Mac app still shows `No data` for Claude Code on `ssh bien`)**:
  1. **Unbumped `# aki-rlcache v6` marker (`src-tauri/src/statusline-unified.sh:23` & `src-tauri/src/statusline.rs:394-396`):** Phase 1.1 fixed `stat -c %Y` inside the `aki-rlcache` block on line 36, and Phase 1.2 bumped `# aki-agquota v2` on line 121, **but left `# aki-rlcache v6` at `v6`!** Because `bien`'s installed `~/.claude/statusline-command.sh` already carries `# aki-rlcache v6` (with the broken `stat -f %m`), `StatuslineHostStatus::from_probe` (`installed < shipped` $\rightarrow$ `6 < 6 == false`) reports `cc_outdated = false`, and `upgrade_outdated_statusline_hooks()` on app startup **skips upgrading `~/.claude/statusline-command.sh` on `bien`**!
  2. **Live `~/.claude/statusline-command.sh` on `bien` was never updated (and WebView reload does not recompile `include_str!("statusline-unified.sh")`):** The sandbox test in §4 tested `statusline-unified.sh` in a throwaway `HOME`, leaving `bien`'s actual `~/.claude/statusline-command.sh` (and `~/.gemini/antigravity-cli/statusline.sh`) still running the old script with `stat -f %m`! Every turn in the active Claude session on `bien` therefore kept failing at `_auth_cache_age=$(( _rl_now - _auth_mtime ))` and never wrote `~/.claude/rate-limits/<email>.json`.
  3. **Required actions for the `ssh bien` session to audit & complete:**
     - [x] Bump `# aki-rlcache v6` $\rightarrow$ `# aki-rlcache v7` in `src-tauri/src/statusline-unified.sh:23` (and sync the marker number in `docs/feat/statusline-customizer.md`, `docs/arch/usage-claudecode.md`, `docs/ref/statusline-unified-spec.md`, `CHANGELOG.md`) so `cc_outdated` and boot-time `upgrade_outdated_statusline_hooks()` automatically upgrade every Linux host still on `v6`. Done 2026-10-28; `statusline-unified-spec.md` names no current rlcache version (only the historical v3/v4 comparison), so it needed no edit; `statusline-customizer.md` § Quota cache now states the bump-on-any-change rule.
     - [x] Patch/install the fixed logic directly into `bien`'s live `~/.claude/statusline-command.sh` (preserving its `AKI-GENERATED-CONFIG` block) and `~/.gemini/antigravity-cli/statusline.sh` on `bien`, then verify `~/.claude/rate-limits/<email>.json` is actually written on `bien` and `bash scripts/get-claudecode-usage.sh` on `bien` outputs valid `rate_limits` JSON (so the Mac app's remote poll of `bien` works immediately on reload without waiting for a Mac `.app` rebuild). Done 2026-10-28: both live scripts are the v7 template with their own generated region (identical to the pre-patch `.aki-bak-1793162652/3` region; the pre-patch file is that backup byte for byte); the next statusline render wrote `~/.claude/rate-limits/ntu_genai@masic.ai.json`, and the probe's JSON line carries `rate_limits.five_hour` (no `seven_day`: the live payload sent none).
- **Follow-up fixes of the 2026-10-28 audit (landed, unreleased):** the v7 auth-cache mtime read is GNU-first (`stat -c %Y || stat -f %m`; v6's BSD-first order meant Linux hosts never wrote the per-account cache) and a non-numeric mtime no longer aborts the render; `claude auth status` runs once, bounded by `timeout`/`gtimeout` when present; a test pins each statusline marker's version to a hash of its block, so an unbumped edit fails `cargo test`; the `aki-agquota` v2 record is written only when the turn's `plan_tier` is non-empty and changed and is never cleared (known limit: a Pro to Free downgrade keeps showing Pro for CLI-only accounts until a turn reports a tier; unverified what agy sends for Free); same-email frames merge case-insensitively with the first frame keeping source/icon, a stale record tier can fill a null IDE tier, and `persist()` keeps a cached `userTier` for the cached/offline view only (the live card shows the reading as fetched, so a CLI-only live reading shows no badge until the agy statusline is applied); AG and CC headers share one `.agent-header` > `.agent-name-row` (6px gap); `provision-claudecode.sh` uses `mktemp` instead of `/tmp/patch.sh`, `get-claudecode-usage.sh` prints frames with `printf '%s\n'`, and the RAM segment matches every `linux*` OSTYPE (musl included).
