# Plan: Antigravity Gemini 4 Argon Quota Pool & Compact Bar UI (`agy-argon4-quota-bars`)

- **Council Session**: `/Users/aki/.aki/agent-council/aki-dev-sync/2026.10.25-1900-agy-argon4-quota-bars`
- **Status**: Phase A Closed (`council_verify.py` PASS across 9 seats) → Executing Phase B

## 1. Anchor & Requirements (`REQ-1..6`)

Verbatim owner anchor:
> chạy phiên /akiflow /akiopen để ra mắt đợt này xử lý cái chuyện agy có thêm quota argon 4 riêng. có thể ta phải làm thành các bar thì mới đủ diện tích thể hiện được mọi thông tin như hiện tại. vì vậ cần full 8-10 rosters phân tích kỹ lưỡng ở phase A rồi mới cho triển khai code ở phase B.

Owner follow-up clarifications:
- Include `agy` statusline routing for Gemini 4 Argon (`statusline-unified.sh`, `statusline.rs`, `ClaudeSettingModal.vue`, spec docs).
- Remove stale `task-1787103368182` ("Chờ owner xác nhận — nhiều account Claude Code") from `docs/plan/backlog.md` (already confirmed done in `notes.json`).

## 2. Phase A Verified Facts

1. **Live Payload (`hands-live-probe #10`)**:
   - `agy -p "/usage" --output-format json` returns 3 groups in `command.data.groups`:
     - `[0]` `"Gemini Models"` (`gemini-weekly`, `gemini-5h`)
     - `[1]` `"Claude and GPT models"` (`3p-weekly`, `3p-5h` with `disabled: true` when weekly is exhausted)
     - `[2]` `"Gemini 4 Argon"` (single bucket: `id: "gemini-argon-wtu-weekly"`, `name: "Gemini 4 Argon Weekly Limit"`, `window: "weekly"`, no `5h` bucket).
2. **Backend Normalization (`judge-pattern #30`)**:
   - `src-tauri/src/agent_usage/antigravity_payload.rs:173-210` (`quota_summary_from_cli_usage`) already forwards all groups dynamically into `quotaSummary.groups`.
   - Gap: `BUCKET_FIELDS` (`antigravity_payload.rs:176-183`) omits `("disabled", "disabled")`. Adding it preserves `disabled: true` on `3p-5h`.
3. **AGY Statusline (`judge-flow #61`)**:
   - `src-tauri/src/statusline-unified.sh:124` classifies `.model` via `if contains("gemini") then "gemini" else "3p"`, mis-routing `"Gemini 4 Argon"` (`4Argon`) to `"gemini"` (`gemini-weekly` instead of `gemini-argon-wtu-weekly`).
   - Fix: detect `contains("argon") -> "argon"` before `contains("gemini")`, and add `qfrac_pool($p; $win)` / `qreset_pool($p; $win)` mapping `argon` weekly to `gemini-argon-wtu-weekly` (with fallbacks `gemini-argon-weekly`, `argon-weekly`) and `5h` to `gemini-argon-5h` / `argon-5h` (returning `-1` when absent, naturally suppressing `5h:` and rendering `7d: XX% [ETA]`).
   - Bump `STATUSLINE_HOOK_VERSION` in `src-tauri/src/statusline.rs` (if used for hook auto-upgrade) and add unit test `agy_renders_argon_quota`.
4. **Card UI & Subtraction (`judge-ui #40`, `judge-ux #50`, `challenger #80`)**:
   - Delete `src/components/UsageCircle.vue` (-469 lines; 0 external callers) and its 5 exception entries in `scripts/ui-audit.config.json`.
   - Replace hardcoded 2-pool `geminiGroup`/`claudeGroup` in `src/components/AgentUsage.vue` with data-driven `agPools` computed:
     - Match `argon` before `gemini` (`Gemini` -> `"Gemini"`, `Claude/GPT` -> `"Claude/GPT"`, `Argon 4` -> `"Argon 4"`, fallback strips trailing `" Models"`).
     - Sort pools canonically (`gemini`, `claude`, `argon`, then any future pools) and sort windows within each pool (`5H` before `7D`).
     - Fall back to `props.data?.models` when `quotaSummary.groups` is absent; support 2-pool cached entries in `aki-antigravity-usage-cache-v4` seamlessly.
   - Render 3 stacked compact `.ag-pool-card` rows (`~28px` height each, `gap: 4px` -> `92px <= 96px` inside `ROW_HEIGHT_PX = 161`, matching Claude Code's `86px` slot height):
     - `.ag-pool-title` (`9.5px` uppercase bold `var(--text-muted)`).
     - `.ag-pool-windows` (`display: flex; gap: 6px; width: 100%`) containing 1 or 2 `.ag-win-col` (`flex: 1; min-width: 0`) cells with `:class="{ 'is-muted': w.muted }"` and `:title="w.tooltip"`.
     - Inside `.ag-win-col`: `.ag-win-meta` (`5H 5%` on left with `.ag-pct-val` colored by `w.colorClass`; inline countdown `3h43m` / `6d15h` / `ready` / `N/A` on right) + `.ag-track` (`4px` height, `var(--surface-hover)`) with `.ag-track-fill` (`background` colored by `w.colorClass`).
     - Separate `.ag-pct-val.color-*` (`color` only) from `.ag-track-fill.color-*` (`background` only).
     - Watch `agoNow` in `AgentUsage.vue` to emit `'retry'` when any active AG bucket's `resetsAt` crosses `agoNow`.
     - Replace `.usage-circles-skeleton` with 3 compact bar skeleton rows.

## 3. Execution Checklist (Phase B)

- [x] **B1 — Backend & Statusline (`src-tauri/`)**:
  - [x] Add `("disabled", "disabled")` to `BUCKET_FIELDS` in `src-tauri/src/agent_usage/antigravity_payload.rs` and add unit test for 3-group CLI `/usage` payload (including `Gemini 4 Argon` and `disabled: true`).
  - [x] Update `src-tauri/src/statusline-unified.sh` to route `argon` models to `gemini-argon-wtu-weekly` (and suppress missing `5h`), update `STATUSLINE_HOOK_VERSION` if needed, and add unit test `agy_renders_argon_quota` in `src-tauri/src/statusline.rs`.
- [x] **B2 — Frontend UI & Subtraction (`src/` & `scripts/ui-audit.config.json`)**:
  - [x] Refactor `src/components/AgentUsage.vue` to render `agPools` as compact pool-grouped bars with full 11-datum parity, bar skeleton loader, and `agoNow` reset-expiry auto-retry.
  - [x] Delete `src/components/UsageCircle.vue` and remove its 5 entries from `scripts/ui-audit.config.json` (plus any removed `.zone-fieldset`/`.skeleton-circle` entries for `AgentUsage.vue`).
  - [x] Update `src/components/modals/ClaudeSettingModal.vue:366` note to `'AGY: pool follows the selected model (Gemini, Claude, or Argon)'`.
  - [x] Update `dev/html-render/mock/views.js` mock usage card to match the bar layout.
- [x] **B3 — Docs, Notes & Backlog Sync**:
  - [x] Update `docs/arch/usage-antigravity.md`, `docs/ref/statusline-unified-spec.md`, `docs/feat/statusline-customizer.md`, `README.md`, `src/components/modals/IntroModal.vue`, and `docs/index.md`.
  - [x] Remove stale `## Chờ owner xác nhận` (`task-1787103368182`) section from `docs/plan/backlog.md`.
  - [x] Mark `task-1792914025278` done in `.akidevsync/notes.json` via `notes_cli.py`.
- [x] **B4 — Verification & Adversarial Review**:
  - [x] Run `cargo test --manifest-path src-tauri/Cargo.toml`, `npm run audit:ui`, `npm run build`, and `scythe.py`.
  - Run Phase B verifier + drift sweep and `aki-challenger` adversarial review on the diff.

Closed 2026-10-27 without re-run: closed by 52d77f3, shipped in 1.35.0 (CHANGELOG.md:6).
