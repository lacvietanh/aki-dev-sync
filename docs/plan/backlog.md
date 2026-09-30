# Backlog

Snapshot từ `.akidevsync/notes.json` và trạng thái repo ngày 2026-09-20 (HEAD `d73d0e7`). Chỉ giữ việc còn mở; việc đã làm nằm trong `docs/plan/done/`, kết quả điều tra nằm trong `docs/research/`.

## Đã quyết (2026-09-20, /akithink tự chạy)

- **Refresh all gọi `loadData()`:** giữ. Decided: giữ Refresh = reload config + `refreshAllProjects()` · because owner note `task-1787801054655` gọi restart app là tối kỵ (mất mọi terminal), research `startup-cached-state-audit.md` đã chọn đúng hướng này và dặn kiểm migration idempotent (đã kiểm, cả bốn đều guard theo field/entry), `loadData` đã có `isReloading`, nút bị khoá khi đang sync, epoch/`refreshCount` reset giống `bumpEpoch`; điều `refresh-controller.md` cấm thật ra là ba cơ chế refresh không liên quan, không phải việc đọc lại config · rejected: nút "Reload config" riêng (thêm control trái nguyên tắc extreme-narrow, người dùng phải biết bấm nút nào) · reopen if: nhấn Refresh lúc đang lưu config làm mất chỉnh sửa, hoặc reload chậm rõ rệt với danh sách dự án lớn. Doc đã sửa: `docs/arch/refresh-controller.md`, `docs/feat/sync-check-and-usage-switches.md`, amendment trong research.
- **Thang token `rgba()`:** đã chốt và áp dụng, xem `docs/plan/audit-akirule-2026-09-18.md` § Thang token. Finding 3: giữ `<style scoped>`.

## Việc của owner trên máy này

Không còn — statusline đã apply lại xong (owner xác nhận 2026-09-27).

## Runtime ledger — chỉ người mới xác nhận được (gom một lượt)

`>16 tab in-app terminal` đã bỏ khỏi ledger: đợt hardening spawn-lock (`docs/plan/done/terminal-pty-spawn-lock-isolation.md`) tự chạy 6 test tự động dựng shell thật, bao gồm mở đồng thời 8–10 tab và kill/restart giữa chừng — coi như đã xác nhận, không cần tay làm lại.

Modal/màu token (item cũ #1) và Refresh all (item cũ #3) đóng ở đây — xem lý do bên dưới. Còn đúng 1 mục:

1. **Điện thoại companion, nhiều tab** — mở app trên điện thoại, kết nối lại (reconnect) khi Mac đang có nhiều tab terminal mở, mở một tab đã "nguội" (không hoạt động gần đây) và xem nó có hiện lại lịch sử cũ (scrollback) hay màn hình trắng. Qua nếu tab nguội hiện đúng lịch sử.

Đóng, 2026-09-27:
- **Modal/màu token**: đợt đổi hex→`var()` đã chạy production 1 tuần (từ 1.31.0, 2026-09-20) qua dùng thật hàng ngày, owner xác nhận không thấy lệch màu — coi là đã qua ledger, không cần buổi duyệt riêng.
- **Refresh all không mất terminal**: xác nhận bằng đọc code, không cần tay bấm — `loadData()` (`src/composables/useProjectConfig.js:138`) chỉ ghi vào `projects.value`/`projectRuntime`/notes, không chạm bất kỳ state hay lệnh PTY nào (terminal tab sống hoàn toàn ở phía Rust, độc lập với danh sách project). Nút **Refresh all** là icon vòng xoay (`.btn-refresh-main`) ở góc trên bên phải titlebar, có từ 1.31.0 (2026-09-20) — nó đã bao gồm cả việc đọc lại `projects.json`/SSH hosts từ đĩa (`handleRefresh()` gọi `loadData()` rồi mới `refreshAllProjects()`), không phải nút refresh-per-project riêng (nút đó là icon nhỏ trên từng dòng project, chỉ chạy `refreshProject(p)`, không đọc lại config).

## Sửa lỗi — điều tra trước khi đổi

| # | Việc | Note | Bước kế tiếp |
|---|---|---|---|
| 2 | Kéo-thả file làm app crash | `task-1789604877926` | Reproduce và lấy crash/log trước; ưu tiên cao vì làm mất phiên đang chạy |
| 3 | AGY usage hỏng từ khoảng 1.29–1.30 | `task-1789578897581` | Trace nguồn quota, cache và cadence. Cờ "thiếu tool" của AG đã hết hạn sau 5 phút (`a4be4ef`) — đó là mục 4 của research cache, không chứng minh là nguyên nhân hỏng usage, nên note vẫn mở |

## Research / thiết kế

| # | Việc | Note | Bước kế tiếp |
|---|---|---|---|
| 5 | Phục hồi phiên làm việc sau quit/relaunch | `task-1789604852739` | Research ranh giới có thể phục hồi: metadata tab, cwd, title, pin; không hứa phục hồi process nếu PTY đã chết |
| 6 | Tách setting/state theo đúng chủ sở hữu; badge hai phía; Deploy | `task-1785676763350`, `task-1789390613266`, `task-1787393248179`, `task-1790401977743`, `task-1790490810608` | Thiết kế xong (`docs/research/akidevsync-project-config-scope-2.md`), ba plan cho 1.32.0: (1) `docs/plan/settings-and-state-layout.md` — project / máy / máy×host, `.akidevsync/` vẫn là data thường của project (không đi đường riêng), sửa F2 (baseline quên file đã xoá); (2) `docs/plan/conflict-detection-and-agy-report.md` — phân loại đụng độ, agy chỉ diễn giải; (3) `docs/plan/deploy-action.md` — DEPLOY cạnh DEV/BUILD, hook chỉ cho sync, F3 (3 hook deploy chạy nhầm trên Mac, lỗi bị nuốt). Thứ tự: 1 trước, 2 và 3 độc lập |

## Nợ kỹ thuật không chặn ship

- `scythe.py` báo 623 `[WRAP]/[YAP]` toàn repo, chủ yếu `src-tauri/src/pty.rs` (121), `system.rs` (62), `statusline.rs` (37) và các doc trong `docs/plan/done/`. Số ngang baseline trước đợt hardening, nên là nợ cũ. Xử lý theo từng file khi chạm tới, không quét hàng loạt.
- `cargo fmt --check` còn lệch ở vài file `src-tauri/src` (ví dụ `agent_usage/antigravity.rs`); chạy `cargo fmt` một lần trong commit riêng.
- `npm run audit:ui` đã xanh (2026-09-29): detector sửa xong, literal lặp thành token, phần còn lại ghi exception có owner + lý do trong `scripts/ui-audit.config.json`. Không còn nợ từ `docs/plan/done/audit-akirule-2026-09-18.md`.

## Hoãn có chủ ý

- `task-1787103368182` — nhiều account Claude Code: owner đã hoãn vì cần debug trực tiếp; không lên lịch lại khi chưa được mở lại.

## Ngoài backlog này

- `docs/plan/remote-ingress-rework.md` đã qua build và Rust tests; chỉ còn protocol runtime trên Mac + điện thoại/edge. Owner hẹn bàn lại sau (2026-09-27) — nhắc lại khi quay lại plan này.
- **Release**: 1.32.0 ra 2026-09-30 (gồm phần từng gọi là 1.31.1/1.32.1). Tồn đọng chuyển sang 1.32.1: (a) chuỗi kiểm tra runtime trên Mac trong `docs/plan/1.32.0-mac-handoff.md` § 3 (chưa chạy; ba plan 1–3 giữ ở `docs/plan/` đến khi chuỗi này xong, các task note #6 giữ mở); (b) `scythe.py` báo 41 `[YAP]` (review, không có `[WRAP]`) ở file mới của đợt này — `conflict.rs`, `sync_state.rs`, `sync.rs`, `migrate-deploy-hooks.mjs`; (c) `cargo fmt` một commit riêng.
- Các pinned note đã `done: true` không phải backlog. Nếu cần dọn UI, unpin/xoá chúng trong Task Notes thay vì giữ lịch sử ở file này.
