# Fact: Google Antigravity Quota Tiers & Bucket Topology (Free vs Pro vs Ultra)

> updated 2026-11-25 · v1.37.0

Canonical external-world fact reference (`RULE-docs.md` §A6) for how Google Antigravity (`agy` CLI, Antigravity Desktop, and Antigravity IDE Language Server) structures quota pools, reset windows, and tier metadata across subscription tiers.

---

## 1. Tier Quota Window Matrix

Paid rows verified 2026-11-25 on `agy` 1.5.16 (F-AG2, F-AG3); Free row and the tier identity columns verified 2026-10-28 on `agy` / Language Server 1.4.8.

| Tier | `userTier.id` | `userTier.name` / `plan_tier` | `planInfo.planName` | `Gemini Models` Buckets | `Claude and GPT models` Buckets |
|---|---|---|---|---|---|
| **Free** (Individual Free) | *(omitted in `/usage`; `free-tier` on RPC: unverified (vendor string only))* | `Free` (or omitted): unverified (vendor string only) | `Free` / `Starter`: unverified (vendor string only) | **`gemini-weekly` only** (no `gemini-5h`) | **`3p-weekly` only** (no `3p-5h`) |
| **Pro** (Google AI Pro) | `g1-pro-tier` | `Google AI Pro` | `Pro` (`TEAMS_TIER_PRO`) | `gemini-5h` + `gemini-weekly` *(Argon included)* | `3p-5h` + `3p-weekly` |
| **Ultra** (Google AI Ultra) | `g1-ultra-tier`: unverified (vendor string only) | `Google AI Ultra`: unverified (vendor string only) | `Ultra`: unverified (vendor string only) | `gemini-5h` + `gemini-weekly` *(Argon included)* | `3p-5h` + `3p-weekly` |

### Quota Group to Model Mapping

| Quota Group (`/usage` `groups[].name`) | Bucket IDs | Models, vendor group description (`agy` 1.5.16, 2026-11-25) | `agy models` CLI Model IDs (`agy` 1.4.8, 2026-10-28) |
|---|---|---|---|
| **Gemini Models** | `gemini-5h` *(paid)*, `gemini-weekly` | Gemini 4 Neon, Gemini 4 Carbon, Gemini 4 Argon, Gemini 4 Hydrogen, Orchestrator (Neon / Carbon / Argon), Gemini 3.8 Flash | `gemini-4-argon-low`, `gemini-4-argon-medium`, `gemini-4-argon-high`, `gemini-3.8-flash-low`, `gemini-3.8-flash-medium`, `gemini-3.8-flash-high` |
| **Claude and GPT models** | `3p-5h` *(paid)*, `3p-weekly` | Claude Sonnet 4.6, Claude Opus 4.6, GPT-OSS 120B | `claude-opus-5-5-low`, `claude-opus-5-5-medium`, `claude-opus-5-5-high`, `claude-sonnet-5-5-low`, `claude-sonnet-5-5-medium`, `claude-sonnet-5-5-high`, `gpt-oss-120b-medium` |

---

## 2. Verified Claims & Evidence Trail (`docs.A6`)

### F-AG1. Antigravity Free tier uses Weekly-only quota (5-hour window removed)
- **Claim:** On the Free tier, Antigravity no longer enforces or emits 5-hour sliding buckets (`gemini-5h`, `3p-5h`). Both model groups (`Gemini Models` and `Claude and GPT models`) emit only a single `window: "weekly"` bucket (`gemini-weekly` and `3p-weekly`), and the global quota description omits all mention of a 5-hour limit: `"Within each group, models share a weekly limit. Quota is consumed proportionally to the cost of the tokens. Thus, limits will last longer with shorter tasks or using more cost-effective models. Your weekly limit is tied directly to your individual tier."`
- **Source:** Live `agy -p "/usage" --output-format json` output on `agy 1.4.8` (macOS, account `nxkxb294@gmail.com`, read 2026-10-28).
- **Research origin:** [docs/research/ag-plan-badge-and-rule-violations.md](../research/ag-plan-badge-and-rule-violations.md) § 5.4 & § 5.5.

### F-AG2. Paid tiers carry two groups, each with a 5h and a weekly bucket
- **Claim:** On paid tiers (Pro and Ultra), `/usage` returns exactly 2 groups: `Gemini Models` (`gemini-weekly` + `gemini-5h`) and `Claude and GPT models` (`3p-weekly` + `3p-5h`). The separate weekly-only `Gemini 4 Argon` group (`gemini-argon-wtu-weekly`) no longer exists. Global description: `"Within each group, models share a weekly limit and a 5-hour limit. [...] while your weekly limit is tied directly to your individual tier."`
- **Source:** `~/.local/bin/agy -p "/usage" --output-format json` on `agy` 1.5.16 (read 2026-11-25); per-account quota frames from 7 Pro and Ultra accounts (written 2026-11-24).
- **Research origin:** [docs/research/ag-argon-merged-into-gemini-pool.md](../research/ag-argon-merged-into-gemini-pool.md) § 5.

### F-AG3. Gemini 4 Argon spends the Gemini Models pool
- **Claim:** Gemini 4 Argon (and Orchestrator (Argon)) are listed inside the `Gemini Models` group, so an Argon model spends `gemini-5h` and `gemini-weekly` like every other Gemini model, on every paid tier.
- **Source:** `Gemini Models` group description in `agy -p "/usage" --output-format json`, `agy` 1.5.16 (read 2026-11-25).
- **Research origin:** [docs/research/ag-argon-merged-into-gemini-pool.md](../research/ag-argon-merged-into-gemini-pool.md) § 5.

### F-AG4. Where Tier Identity (`userTier` / `plan_tier`) is exposed vs omitted
- **Claim:**
  1. **Language Server Connect RPC (`/GetUserStatus`):** Exposes `userStatus.userTier` (`{"id": "g1-pro-tier", "name": "Google AI Pro", "description": "Google AI Pro"}`) and `userStatus.planStatus.planInfo.planName` (`"Pro"`).
  2. **Interactive `agy` CLI statusline (`stdin` to `~/.gemini/antigravity-cli/statusline.sh`):** Exposes root field `"plan_tier": "Google AI Pro"` (`json:"plan_tier,omitempty"` in `agy` 1.4.8 binary) alongside `"email"` and `"quota"`. How this repo records and reads it: `docs/arch/usage-antigravity.md` § Plan tier.
  3. **Headless `agy -p "/usage" --output-format json`:** Omits `userTier` and `plan_tier` completely. Structurally, `/usage` bucket topology tells only free from paid (zero `5h` buckets $\rightarrow$ `Free`); Pro and Ultra return the same two groups. This repo does not infer a tier from it, and why: `docs/arch/usage-antigravity.md` § Plan tier.
- **Source:** `agy 1.4.8` binary symbol inspection (`strings ~/.local/bin/agy`), `agy models`, `/usage` JSON output, and `/GetUserStatus` RPC response (read 2026-10-28).
- **Research origin:** [docs/research/ag-plan-badge-and-rule-violations.md](../research/ag-plan-badge-and-rule-violations.md) § 5.1, § 5.4 & § 5.5.
