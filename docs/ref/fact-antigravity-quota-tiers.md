# Fact: Google Antigravity Quota Tiers & Bucket Topology (Free vs Pro vs Ultra)

> updated 2026-10-28 · v1.36.0

Canonical external-world fact reference (`RULE-docs.md` §A6) for how Google Antigravity (`agy` CLI, Antigravity Desktop, and Antigravity IDE Language Server) structures quota pools, reset windows, and tier metadata across subscription tiers.

---

## 1. Tier Quota Window Matrix (Verified 2026-10-28 on `agy` 1.4.8 / Language Server 1.4.8)

| Tier | `userTier.id` | `userTier.name` / `plan_tier` | `planInfo.planName` | `Gemini Models` Buckets | `Claude and GPT models` Buckets | `Gemini 4 Argon` Group |
|---|---|---|---|---|---|---|
| **Free** (Individual Free) | *(omitted in `/usage`; `free-tier` on RPC: unverified (vendor string only))* | `Free` (or omitted): unverified (vendor string only) | `Free` / `Starter`: unverified (vendor string only) | **`gemini-weekly` only** (no `gemini-5h`) | **`3p-weekly` only** (no `3p-5h`) | **Absent** (no separate group) |
| **Pro** (Google AI Pro) | `g1-pro-tier` | `Google AI Pro` | `Pro` (`TEAMS_TIER_PRO`) | `gemini-5h` + `gemini-weekly` | `3p-5h` + `3p-weekly` | **Present** (`gemini-argon-wtu-weekly`, weekly-only) |
| **Ultra** (Google AI Ultra): unverified (vendor string only) | `g1-ultra-tier`: unverified (vendor string only) | `Google AI Ultra`: unverified (vendor string only) | `Ultra`: unverified (vendor string only) | `gemini-5h` + `gemini-weekly` *(includes Argon quota)*: unverified (vendor string only) | `3p-5h` + `3p-weekly`: unverified (vendor string only) | **Merged into `Gemini Models`**: unverified (vendor string only) |

### Model ID to Quota Group Mapping (Verified 2026-10-28 via `agy models` on `agy 1.4.8`)

| Quota Group (`quotaSummary.groups[].displayName`) | Bucket IDs (`bucketId`) | `agy models` CLI Model IDs (`--model <id>`) | Display Labels |
|---|---|---|---|
| **Gemini 4 Argon** *(Pro separate; Ultra merged into Gemini Models)* | `gemini-argon-wtu-weekly` *(weekly only)* | `gemini-4-argon-low`, `gemini-4-argon-medium`, `gemini-4-argon-high` | `Gemini 4 Argon (Low / Medium / High)` |
| **Gemini Models** | `gemini-5h` *(Pro/Ultra)*, `gemini-weekly` | `gemini-3.8-flash-low`, `gemini-3.8-flash-medium`, `gemini-3.8-flash-high` | `Gemini 3.8 Flash (Low / Medium / High)` |
| **Claude and GPT models** | `3p-5h` *(Pro/Ultra)*, `3p-weekly` | `claude-opus-5-5-low`, `claude-opus-5-5-medium`, `claude-opus-5-5-high`, `claude-sonnet-5-5-low`, `claude-sonnet-5-5-medium`, `claude-sonnet-5-5-high`, `gpt-oss-120b-medium` | `Claude Opus 5.5 (Low / Medium / High)`, `Claude Sonnet 5.5 (Low / Medium / High)`, `GPT-OSS 120B (Medium)` |

---

## 2. Verified Claims & Evidence Trail (`docs.A6`)

### F-AG1. Antigravity Free tier uses Weekly-only quota (5-hour window removed)
- **Claim:** On the Free tier, Antigravity no longer enforces or emits 5-hour sliding buckets (`gemini-5h`, `3p-5h`). Both model groups (`Gemini Models` and `Claude and GPT models`) emit only a single `window: "weekly"` bucket (`gemini-weekly` and `3p-weekly`), and the global quota description omits all mention of a 5-hour limit: `"Within each group, models share a weekly limit. Quota is consumed proportionally to the cost of the tokens. Thus, limits will last longer with shorter tasks or using more cost-effective models. Your weekly limit is tied directly to your individual tier."`
- **Source:** Live `agy -p "/usage" --output-format json` output on `agy 1.4.8` (macOS, account `nxkxb294@gmail.com`, read 2026-10-28).
- **Research origin:** [docs/research/ag-plan-badge-and-rule-violations.md](../research/ag-plan-badge-and-rule-violations.md) § 5.4 & § 5.5.

### F-AG2. Paid Pro tier carries dual 5h + Weekly buckets plus a separate weekly-only Argon 4 group
- **Claim:** On `Google AI Pro` (`g1-pro-tier`), `RetrieveUserQuotaSummary` and `/usage` return 3 groups: `Gemini Models` (`gemini-weekly` + `gemini-5h`), `Claude and GPT models` (`3p-weekly` + `3p-5h`), and `Gemini 4 Argon` (`gemini-argon-wtu-weekly`, `window: "weekly"` only). The global description explicitly states: `"Within each group, models share a weekly limit and a 5-hour limit... The 5-hour limit smooths out aggregate demand to fairly distribute global capacity across all users, while your weekly limit is tied directly to your individual tier."`
- **Source:** Live Connect RPC `/exa.language_server_pb.LanguageServerService/RetrieveUserQuotaSummary` and `/GetUserStatus` on Language Server `1.4.8` (macOS, account `lacvietanh@gmail.com`, read 2026-10-28).
- **Research origin:** [docs/research/ag-plan-badge-and-rule-violations.md](../research/ag-plan-badge-and-rule-violations.md) § 5.4 & § 5.5.

### F-AG3. Ultra tier pools Argon quota into the main Gemini bucket
- **Claim:** On `Google AI Ultra`, Argon quota is not separated into a standalone `Gemini 4 Argon` group; instead, higher Argon quota is included directly inside the larger `Gemini Models` quota bucket (`"For paid Pro plans, Argon quota is separate. Higher Argon quota (included as part of the larger Gemini quota bucket) is available on Ultra plans."`).
- **Source:** Vendor description string in `RetrieveUserQuotaSummary` response on Language Server `1.4.8` (read 2026-10-28). No Ultra account was probed: unverified (vendor string only).
- **Research origin:** [docs/research/ag-plan-badge-and-rule-violations.md](../research/ag-plan-badge-and-rule-violations.md) § 5.4.

### F-AG4. Where Tier Identity (`userTier` / `plan_tier`) is exposed vs omitted
- **Claim:**
  1. **Language Server Connect RPC (`/GetUserStatus`):** Exposes `userStatus.userTier` (`{"id": "g1-pro-tier", "name": "Google AI Pro", "description": "Google AI Pro"}`) and `userStatus.planStatus.planInfo.planName` (`"Pro"`).
  2. **Interactive `agy` CLI statusline (`stdin` to `~/.gemini/antigravity-cli/statusline.sh`):** Exposes root field `"plan_tier": "Google AI Pro"` (`json:"plan_tier,omitempty"` in `agy` 1.4.8 binary) alongside `"email"` and `"quota"`. How this repo records and reads it: `docs/arch/usage-antigravity.md` § Plan tier.
  3. **Headless `agy -p "/usage" --output-format json`:** Omits `userTier` and `plan_tier` completely. Structurally, `/usage` bucket topology differs by tier (zero `5h` buckets across all groups $\rightarrow$ `Free`; `5h` buckets + separate `Gemini 4 Argon` group $\rightarrow$ `Pro`). This repo does not infer a tier from it, and why: `docs/arch/usage-antigravity.md` § Plan tier.
- **Source:** `agy 1.4.8` binary symbol inspection (`strings ~/.local/bin/agy`), `agy models`, `/usage` JSON output, and `/GetUserStatus` RPC response (read 2026-10-28).
- **Research origin:** [docs/research/ag-plan-badge-and-rule-violations.md](../research/ag-plan-badge-and-rule-violations.md) § 5.1, § 5.4 & § 5.5.
