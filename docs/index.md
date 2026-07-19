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
- [docs/research/claude-headless-rate-limit-event-2026-07-09.md](research/claude-headless-rate-limit-event-2026-07-09.md) — ĐÍNH CHÍNH: headless `-p` KHÔNG fire statusLine hook; `--output-format json` trả `rate_limit_info.resetsAt` — nguồn reset-time native, một turn, mọi máy, không keychain (bằng chứng thực nghiệm Mac)

## Plans (Active)
- [docs/plan/investigate-ag-account-switch-detection.md](plan/investigate-ag-account-switch-detection.md) — AG doesn't pick up an in-app account switch until Antigravity is quit+reopened; hypothesis is Antigravity itself doesn't restart its language_server process on switch (external limitation, not our polling). Light PID-check test protocol for Mac, no rebuild needed.

## Plans (Completed)
- [docs/plan/done/push-only-paths.md](plan/done/push-only-paths.md) — Push-only paths bằng exclude-list semantics (R1 transfer theo chiều, R2 badge dùng union excludes, R3 mirror-push auto-approve deletion trong push-only dir); bỏ toggle `.git`, migration `sync_git` → exclude list. **✅ DONE 1.13.0** (2026-07-20) — verify trên Mac ở §6b: migration khớp dự đoán tuyệt đối trên 17 project thật (0 thay đổi ngoài dự kiến), idempotent xác nhận thực nghiệm, R2 có bằng chứng runtime từ tiến trình rsync.
- [docs/plan/done/claudecode-oauth-usage-p3.md](plan/done/claudecode-oauth-usage-p3.md) — P3 OAuth usage polling (Lỗi C freshness fix): Phase 1 code landed but no-op on Mac (keychain-only credentials) — **deprioritized/closed, not pursued further**. Embedded bug (email header kẹt ở account cũ sau khi đổi account CC) — **✅ FIXED 1.9.7**, verified by user.
- [docs/plan/done/verify-antigravity-account-stability.md](plan/done/verify-antigravity-account-stability.md) — AG account-switch stability: item A superseded (wrong expectation corrected — see `docs/arch/usage-antigravity.md`), item B done, item C dropped (diagnostic-only, no bug depended on it)
- [docs/plan/done/fix-sync-divergence-safety.md](plan/done/fix-sync-divergence-safety.md) — Prevent accidental destructive PUSH: DIVERGED state, delete-confirm, `-u`+`--delete` fix, Tier 2 baseline, DRY RUN guard — all done
- [docs/plan/done/project-task-list.md](plan/done/project-task-list.md) — Per-project task list: data model, hover-popover UX, file changes
- [docs/plan/done/total-refactor-by-akirule.md](plan/done/total-refactor-by-akirule.md) — Full PROCODING refactor plan
- [docs/plan/done/open-popup-consolidation.md](plan/done/open-popup-consolidation.md) — Consolidation of Local and Remote SSH popup actions
- [docs/plan/done/akirule-audit-round2.md](plan/done/akirule-audit-round2.md) — Secondary audit for rule compliance
- [docs/plan/done/fix-claude-flow-24jun.md](plan/done/fix-claude-flow-24jun.md) — Fix Claude Code sync flow: STALE_RESET auto-recovery, concurrency guard, JSONL cleanup
- [docs/plan/done/fix-usage-monitor-freeze.md](plan/done/fix-usage-monitor-freeze.md) — Reset time CC đứng im/không ổn định: root cause = WKWebView suspend `setInterval` + không có resume handling (P1); kèm AG IPC không timeout → deadlock `isChecking` (P2), statusline hook v2 clobber `seven_day` (P3, runtime-confirmed), CC thiếu boundary-trigger như AG (P4), provision/force_sync thiếu spawn_blocking (P5). P1–P5 implemented 1.12.0 (2026-07-18) — chi tiết `docs/arch/usage-claudecode.md` §3d.
- [docs/plan/done/deferred-auto-update.md](plan/done/deferred-auto-update.md) — Tauri v2 self-update (auto-update): **DROPPED (won't-do)** 2026-07-19 — quyết định là không cần thiết, kể cả bản "notify only".
- [docs/plan/done/share-statusline.md](plan/done/share-statusline.md) — Share `share/aki-statusLine/` (statusline.sh + demo.png) cho cộng đồng dùng độc lập với app; **DONE** 2026-07-15 (`e461460`), README đã có section giới thiệu.
- [docs/plan/done/statusline-customizer.md](plan/done/statusline-customizer.md) — Plan gốc cho UI Statusline Customizer trong app (chọn field, sắp thứ tự, đổi màu, xả cấu hình ra nhiều máy); marked "Planned, not started" trong file nhưng tính năng đã thực sự ship (xem `TODO-MAC.md` + CHANGELOG 1.10.0) — file tự thân chưa được cập nhật Status.
- [docs/plan/done/fix-copy-remote-path-blocking.md](plan/done/fix-copy-remote-path-blocking.md) — Nút COPY remote path (open popup) lag + đơ UI: root cause = re-fetch `$HOME` qua SSH đồng bộ trên main thread mỗi lần bấm. **✅ FIXED** 2026-07-08 (code tĩnh) — copy verbatim + cache + `resolve_remote_path` chuyển async/`spawn_blocking`; còn chờ rebuild+verify trên Mac.
- [docs/plan/done/fix-to-1.3.1.md](plan/done/fix-to-1.3.1.md) — Audit toàn codebase 2026-06-24: danh sách bug/pattern phân loại CRITICAL/HIGH/MEDIUM/LOW/FEATURE (concurrency guard, silent clipboard catch, poisoned-mutex unwrap, script cleanup, Vue anti-patterns, a11y, Changelog Modal refactor, Countdown Ring feature) — file không có dòng Status tổng kết cuối cùng cho biết mục nào đã áp dụng.
- [docs/plan/done/improve-jun24.md](plan/done/improve-jun24.md) — Plan cải tiến đợt 2026-06 (account info Claude Code, table alignment, CSS variables/empty states/toast, Antigravity remote fix, auto-update). **✅ Done (core items)**, base 1.2.8 → completed 1.3.0–1.3.1; Toast positioning và Auto-update tách ra deferred riêng.
- [docs/plan/done/TODO-MAC.md](plan/done/TODO-MAC.md) — Checklist bàn giao cho Mac sau khi code Statusline Customizer + Aki StatusLine share (1.10.0) xong trên máy dev Linux (không compile được ở đó): build/smoke-test, functional checks với ≥1 remote host, trước khi release.
