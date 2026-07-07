# Docs Index — Aki Dev Sync

## Features
- [docs/feat/project-task-list.md](feat/project-task-list.md) — Per-project task list in a hover popover (TASKS column); doing-first ordering, colored status tags, persisted on `projects.json` via serde-default
- [docs/feat/background-refresh.md](feat/background-refresh.md) — Background polling for git status, remote diff (push/pull buttons), and agent usage; `.git/` mtime quirk and fix
- [docs/feat/drag-and-drop.md](feat/drag-and-drop.md) — Drag & Drop Live Sorting for list reordering, Tauri v2 native file drop conflict solution, and midpoint geometric threshold logic
- [docs/feat/open-popup.md](feat/open-popup.md) — Unified Open Popup for Local & Remote SSH actions, macOS open consolidation, and IDE availability checks
- [docs/feat/sync-flow.md](feat/sync-flow.md) — Core sync flow: Push, Pull, Select special files, Dry-Run and status indicators
- [docs/feat/app-update-check.md](feat/app-update-check.md) — Automatic and manual App Update Check mechanism using GitHub Releases API and semantic version comparison
- [docs/feat/git-operations.md](feat/git-operations.md) — Git Modal Operations (Commit, Fetch, Push, Status logs) and logging policies (loud vs silent)
- [docs/feat/remote-mode.md](feat/remote-mode.md) — Single global Remote Mode master switch: what it gates (sync buttons, Open popup, background/manual diff checks, CC remote usage), where the switch lives



## Architecture
- [docs/arch/usage-claudecode.md](arch/usage-claudecode.md) — Claude Code quota monitoring: statusLine hook, Hybrid Patching, known limitations (Lỗi A/B/C)
- [docs/arch/usage-antigravity.md](arch/usage-antigravity.md) — Antigravity quota monitoring: local Language Server Connect RPC flow
- [docs/arch/logger.md](arch/logger.md) — Logger module: 3 levels (error/info/debug), production-silent default, auto-truncate 1MB

## Reference
- [docs/ref/deepresearch-claudecode-antigravity-quota-measurement.md](ref/deepresearch-claudecode-antigravity-quota-measurement.md) — 104-agent research: 5 phương pháp đo quota (P1–P5), so sánh 15+ repo công khai
- [docs/ref/dragging-titlebar.md](ref/dragging-titlebar.md) — Custom frameless window drag implementation
- [docs/ref/titlebar-sacred-boundary.md](ref/titlebar-sacred-boundary.md) — Sacred boundary rule: no UI element may overlap the custom titlebar zone (top 42px)

## Research
- [docs/research/claude-app-usage-measurement.md](research/claude-app-usage-measurement.md) — Đo usage tài khoản Claude khi chỉ dùng Claude app: pool chung Pro/Max, endpoint `oauth/usage` (P3), endpoint claude.ai + sessionKey, tool cộng đồng, khuyến nghị fix điểm mù freshness (Lỗi C)
- [docs/research/aki-dev-sync-ag-cc-usage-flow.md](research/aki-dev-sync-ag-cc-usage-flow.md) — So sánh flow của project vs cộng đồng, lợi thế native approach (phục vụ bài viết akidev)
- [docs/research/claude-usage-1.2.x-analyze.md](research/claude-usage-1.2.x-analyze.md) — Phân tích các vấn đề bug quota display + đề xuất cải thiện (v1.2.x)
- [docs/research/claude-usage-dash-pipefail-regression.md](research/claude-usage-dash-pipefail-regression.md) — Post-mortem: `set -o pipefail` giết dash → force-sync chết im lặng → "load mãi / no data sau reset" (root cause + fix + phòng ngừa)
- [docs/research/sync-button-semantic-analysis.md](research/sync-button-semantic-analysis.md) — PUSH/PULL button semantic intent vs reality: incident log, code analysis, EC-1..EC-7 + EC-3-sym, `-u`+`--delete` incoherence, Tier 2 baseline (bidirectional, appDataDir), DRY RUN guard bug — all resolved

## Plans (Active)
- [docs/plan/claudecode-oauth-usage-p3.md](plan/claudecode-oauth-usage-p3.md) — P3 OAuth usage polling: fix điểm mù freshness Lỗi C (usage đứng im khi chỉ dùng Claude app). Phase 1 code landed but no-op on Mac (no `.credentials.json`, keychain-only). **Toàn bộ plan tạm dừng 2026-07-08** (deprioritized cho release) — cũng ghi nhận 1 bug liên quan chưa fix: email header kẹt ở account cũ sau khi đổi account CC.
- [docs/plan/deferred-auto-update.md](plan/deferred-auto-update.md) — Tauri v2 self-update (deferred)
- [docs/plan/investigate-ag-account-switch-detection.md](plan/investigate-ag-account-switch-detection.md) — AG doesn't pick up an in-app account switch until Antigravity is quit+reopened; hypothesis is Antigravity itself doesn't restart its language_server process on switch (external limitation, not our polling). Light PID-check test protocol for Mac, no rebuild needed.

## Plans (Completed)
- [docs/plan/done/verify-antigravity-account-stability.md](plan/done/verify-antigravity-account-stability.md) — AG account-switch stability: item A superseded (wrong expectation corrected — see `docs/arch/usage-antigravity.md`), item B done, item C dropped (diagnostic-only, no bug depended on it)
- [docs/plan/done/fix-sync-divergence-safety.md](plan/done/fix-sync-divergence-safety.md) — Prevent accidental destructive PUSH: DIVERGED state, delete-confirm, `-u`+`--delete` fix, Tier 2 baseline, DRY RUN guard — all done
- [docs/plan/done/project-task-list.md](plan/done/project-task-list.md) — Per-project task list: data model, hover-popover UX, file changes
- [docs/plan/done/total-refactor-by-akirule.md](plan/done/total-refactor-by-akirule.md) — Full PROCODING refactor plan
- [docs/plan/done/open-popup-consolidation.md](plan/done/open-popup-consolidation.md) — Consolidation of Local and Remote SSH popup actions
- [docs/plan/done/akirule-audit-round2.md](plan/done/akirule-audit-round2.md) — Secondary audit for rule compliance
- [docs/plan/done/fix-claude-flow-24jun.md](plan/done/fix-claude-flow-24jun.md) — Fix Claude Code sync flow: STALE_RESET auto-recovery, concurrency guard, JSONL cleanup
