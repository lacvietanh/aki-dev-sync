# Backlog

Snapshot từ `.akidevsync/notes.json` và trạng thái repo ngày 2026-09-20 (HEAD `d73d0e7`). Chỉ giữ việc còn mở; việc đã làm nằm trong `docs/plan/done/`, kết quả điều tra nằm trong `docs/research/`.

## Cần owner quyết trước

| # | Việc | Vì sao chặn | Khuyến nghị |
|---|---|---|---|
| A | Nút **Refresh all** đang gọi `loadData()` (commit `3c7eb82`), mâu thuẫn `docs/arch/refresh-controller.md` ("`loadData()` là việc lúc load app, không bao giờ do nút Refresh gọi") | Tài liệu đó tồn tại vì ba cơ chế "refresh" bị trộn lẫn, và `loadData` làm mọi nút mờ đi qua cờ `isReloading` | Giữ hành vi mới (đáp ứng note "config chỉ nạp lúc startup") và sửa `refresh-controller.md` + `sync-check-and-usage-switches.md` cho khớp, ghi rõ đánh đổi `isReloading`. Hoặc tách một nút "Reload config" riêng và trả Refresh về như cũ. Chưa sửa doc nào cho tới khi owner chọn. |
| B | Vocab alpha cho `rgba()` và cách hiểu finding 3 (`<style scoped>`) | Chặn migrate 524 hit màu còn lại và chặn điền `sfcResidents` | Xem `docs/plan/audit-akirule-2026-09-18.md` § Còn lại, bước 1 và 4 |

## Việc của owner trên máy này

| # | Việc | Lý do |
|---|---|---|
| C | Mở app → Statusline → **Apply lại** cho Claude Code và Antigravity | `~/.claude/statusline-command.sh` và `~/.gemini/antigravity-cli/statusline.sh` vẫn là bản cũ, còn ghi payload (email, cwd, usage) ra `/tmp/statusline_stdin_dump.json` mỗi lượt. Kiểm: `grep -c statusline_stdin_dump` trên hai file phải ra 0. |

## Runtime ledger — chỉ người mới xác nhận được (gom một lượt)

Chạy một lần ở trạng thái cuối, không lặp theo từng mục (`coding.B3`):

- Mở app: duyệt modal family và các bề mặt UI đã đổi (`docs/plan/audit-akirule-2026-09-18.md` bước 7).
- Điện thoại companion: kết nối lại khi có nhiều tab, xác nhận tab nguội hiện lịch sử khi mở (plan `done/companion-replay-bound.md` ghi "verified by reading").
- Mở nhiều hơn 16 tab in-app terminal, xác nhận không có giới hạn và không treo.
- Refresh all: sửa `projects.json` bằng tay rồi bấm, xác nhận áp dụng không cần restart (liên quan mục A).

## Sửa lỗi — điều tra trước khi đổi

| # | Việc | Note | Bước kế tiếp |
|---|---|---|---|
| 2 | Kéo-thả file làm app crash | `task-1789604877926` | Reproduce và lấy crash/log trước; ưu tiên cao vì làm mất phiên đang chạy |
| 3 | AGY usage hỏng từ khoảng 1.29–1.30 | `task-1789578897581` | Trace nguồn quota, cache và cadence. Đã đổi: thông báo "thiếu tool" của AG được báo lại mỗi 5 phút thay vì một lần (`a4be4ef`); chưa chứng minh đó là nguyên nhân hỏng, nên note vẫn mở |
| 4 | Cache/state chỉ nạp lúc startup | `task-1787801054655`, `task-1789390613266` | Phần project list/SSH hosts đã có qua Refresh all (xem mục A). Còn lại: TTL cache Antigravity. Không thêm filesystem watcher |

## Research / thiết kế

| # | Việc | Note | Bước kế tiếp |
|---|---|---|---|
| 5 | Phục hồi phiên làm việc sau quit/relaunch | `task-1789604852739` | Research ranh giới có thể phục hồi: metadata tab, cwd, title, pin; không hứa phục hồi process nếu PTY đã chết |
| 6 | Badge hai phía kèm prompt chạy AGY/Claude | `task-1787393248179` | Làm rõ entry point và prompt trước khi lập plan |
| 7 | Config project: đọc theo yêu cầu (RAM) hay nạp lại khi file đổi | note `open` "project setting: cố vấn…" | Trả lời sau khi mục A được chốt, vì cùng một câu hỏi |

## Nợ kỹ thuật không chặn ship

- `scythe.py` báo 623 `[WRAP]/[YAP]` toàn repo, chủ yếu `src-tauri/src/pty.rs` (121), `system.rs` (62), `statusline.rs` (37) và các doc trong `docs/plan/done/`. Số ngang baseline trước đợt hardening, nên là nợ cũ. Xử lý theo từng file khi chạm tới, không quét hàng loạt.
- `cargo fmt --check` còn lệch ở vài file `src-tauri/src` (ví dụ `agent_usage/antigravity.rs`); chạy `cargo fmt` một lần trong commit riêng.

## Hoãn có chủ ý

- `task-1787103368182` — nhiều account Claude Code: owner đã hoãn vì cần debug trực tiếp; không lên lịch lại khi chưa được mở lại.

## Ngoài backlog này

- `docs/plan/remote-ingress-rework.md` đã qua build và Rust tests; chỉ còn protocol runtime trên Mac + điện thoại/edge, nên tiếp tục theo chính plan đó.
- `docs/plan/audit-akirule-2026-09-18.md` — plan UI đang chạy, có danh sách bước còn lại riêng.
- **Release**: `CHANGELOG.md` `[Unreleased]` đang giữ các thay đổi chưa phát hành; version vẫn `1.30.0` (chỉ mint tại lúc release, `release.A`). Chưa push.
- Các pinned note đã `done: true` không phải backlog. Nếu cần dọn UI, unpin/xoá chúng trong Task Notes thay vì giữ lịch sử ở file này.
