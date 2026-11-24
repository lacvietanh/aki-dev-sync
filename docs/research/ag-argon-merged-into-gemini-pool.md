# Antigravity: Gemini 4 Argon quota merged into the Gemini pool

1. **Start time:** 2026-11-24
2. **Initial purpose:** the usage cards in app 1.37.0 showed only two pools (GEMINI, CLAUDE/GPT) for every Antigravity account, Pro and Ultra alike, while the app, the statusline and `docs/ref/fact-antigravity-quota-tiers.md` still expected a third, weekly-only `Gemini 4 Argon` group on Pro (F-AG2). Question: does the separate Argon group still exist, and where does Argon quota go now?
3. **Strategy:** read the vendor's own quota surface once, no RPC probing: the app's usage cards across all registered accounts, the per-account quota frames the agy statusline writes on every turn, and one headless `/usage` read.
4. **Checklist:**
   - [x] Usage cards, app 1.37.0, 2026-11-24: every account (Pro and Ultra) shows two pools, no Argon pool.
   - [x] `~/.gemini/antigravity-cli/rate-limits/*.frame`, 7 accounts, most written 2026-11-24: quota keys are `gemini-5h`, `gemini-weekly`, `3p-5h`, `3p-weekly`; no `gemini-argon-wtu-weekly`.
   - [x] `~/.local/bin/agy -p "/usage" --output-format json`, agy 1.5.16, read 2026-11-25.
5. **Result:** the `Gemini 4 Argon` group is gone. `/usage` returns two groups, each with a weekly and a 5-hour bucket:
   - `Gemini Models`: `gemini-weekly` + `gemini-5h`. Vendor description: "Models within this group: Gemini 4 Neon, Gemini 4 Carbon, Gemini 4 Argon, Gemini 4 Hydrogen, Orchestrator (Neon), Orchestrator (Carbon), Orchestrator (Argon), Gemini 3.8 Flash".
   - `Claude and GPT models`: `3p-weekly` + `3p-5h`. Vendor description: "Models within this group: Claude Sonnet 4.6, Claude Opus 4.6, GPT-OSS 120B".
   - Global description: "Within each group, models share a weekly limit and a 5-hour limit. [...] while your weekly limit is tied directly to your individual tier."
   - **Verification:** three independent surfaces agree (cards, frames, `/usage`). The `/usage` read covers the one active agy account; the frames cover all seven, Pro and Ultra. Free tier was not re-read: F-AG1 (weekly-only) stands unverified against this change.
   - **Corroborating links:** contradicts F-AG2 and F-AG3 as written 2026-10-28 in `docs/ref/fact-antigravity-quota-tiers.md` (from `docs/research/ag-plan-badge-and-rule-violations.md` § 5.4).
6. **Decision:** Action.
   - `docs/ref/fact-antigravity-quota-tiers.md`: F-AG2 rewritten (two groups on paid tiers), F-AG3 rewritten (Argon in the Gemini pool on every paid tier), table and F-AG4.3 updated.
   - Code: `src-tauri/src/statusline-unified.sh` routes an Argon model to the Gemini pool; `src/components/AgentUsage.vue` drops the Argon pool. An unknown future group still renders under the vendor's own name through the existing fallback.
   - **Cross-references:** `docs/arch/usage-antigravity.md`, `docs/ref/statusline-unified-spec.md`, `docs/feat/statusline-customizer.md`.
