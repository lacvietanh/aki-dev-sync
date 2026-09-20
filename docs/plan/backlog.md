# Backlog

Snapshot từ `.akidevsync/notes.json` và trạng thái repo ngày 2026-09-20 (HEAD `d73d0e7`). Chỉ giữ việc còn mở; việc đã làm nằm trong `docs/plan/done/`, kết quả điều tra nằm trong `docs/research/`.

## Đã quyết (2026-09-20, /akithink tự chạy)

- **Refresh all gọi `loadData()`:** giữ. Decided: giữ Refresh = reload config + `refreshAllProjects()` · because owner note `task-1787801054655` gọi restart app là tối kỵ (mất mọi terminal), research `startup-cached-state-audit.md` đã chọn đúng hướng này và dặn kiểm migration idempotent (đã kiểm, cả bốn đều guard theo field/entry), `loadData` đã có `isReloading`, nút bị khoá khi đang sync, epoch/`refreshCount` reset giống `bumpEpoch`; điều `refresh-controller.md` cấm thật ra là ba cơ chế refresh không liên quan, không phải việc đọc lại config · rejected: nút "Reload config" riêng (thêm control trái nguyên tắc extreme-narrow, người dùng phải biết bấm nút nào) · reopen if: nhấn Refresh lúc đang lưu config làm mất chỉnh sửa, hoặc reload chậm rõ rệt với danh sách dự án lớn. Doc đã sửa: `docs/arch/refresh-controller.md`, `docs/feat/sync-check-and-usage-switches.md`, amendment trong research.
- **Thang token `rgba()`:** đã chốt và áp dụng, xem `docs/plan/audit-akirule-2026-09-18.md` § Thang token. Finding 3: giữ `<style scoped>`.

## Việc của owner trên máy này

| # | Việc | Lý do |
|---|---|---|
| C | Mở app → Statusline → **Apply lại** cho Claude Code và Antigravity | `~/.claude/statusline-command.sh` và `~/.gemini/antigravity-cli/statusline.sh` vẫn là bản cũ, còn ghi payload (email, cwd, usage) ra `/tmp/statusline_stdin_dump.json` mỗi lượt. Kiểm: `grep -c statusline_stdin_dump` trên hai file phải ra 0. |

## Runtime ledger — chỉ người mới xác nhận được (gom một lượt)

Chạy một lần ở trạng thái cuối, không lặp theo từng mục (`coding.B3`):

- Mở app: duyệt modal family và các bề mặt UI đã đổi (`docs/plan/audit-akirule-2026-09-18.md` bước 7).
- Điện thoại companion: kết nối lại khi có nhiều tab, xác nhận tab nguội hiện lịch sử khi mở (plan `done/companion-replay-bound.md` ghi "verified by reading").
- Mở nhiều hơn 16 tab in-app terminal, xác nhận không có giới hạn và không treo.
- Refresh all: sửa `projects.json` bằng tay rồi bấm, xác nhận áp dụng không cần restart và không mất terminal đang mở.

## Sửa lỗi — điều tra trước khi đổi

| # | Việc | Note | Bước kế tiếp |
|---|---|---|---|
| 2 | Kéo-thả file làm app crash | `task-1789604877926` | Reproduce và lấy crash/log trước; ưu tiên cao vì làm mất phiên đang chạy |
| 3 | AGY usage hỏng từ khoảng 1.29–1.30 | `task-1789578897581` | Trace nguồn quota, cache và cadence. Cờ "thiếu tool" của AG đã hết hạn sau 5 phút (`a4be4ef`) — đó là mục 4 của research cache, không chứng minh là nguyên nhân hỏng usage, nên note vẫn mở |
| 4 | Cache/state chỉ nạp lúc startup | `task-1787801054655`, `task-1789390613266` | Project list, SSH hosts và cờ tool-missing AG đã xử lý (Refresh all, TTL 5 phút). Còn lại: các mục research chưa nêu là 'để nguyên'. Không thêm filesystem watcher; cân nhắc đóng note khi runtime ledger qua |

## Research / thiết kế

| # | Việc | Note | Bước kế tiếp |
|---|---|---|---|
| 5 | Phục hồi phiên làm việc sau quit/relaunch | `task-1789604852739` | Research ranh giới có thể phục hồi: metadata tab, cwd, title, pin; không hứa phục hồi process nếu PTY đã chết |
| 6 | Badge hai phía kèm prompt chạy AGY/Claude | `task-1787393248179` | Làm rõ entry point và prompt trước khi lập plan |
| 7 | Config project: đọc theo yêu cầu (RAM) hay nạp lại khi file đổi | note `open` "project setting: cố vấn…" | Đã có câu trả lời: RAM là nguồn hằng ngày, đọc lại từ đĩa tại thời điểm dùng (nút Refresh), không watcher (`startup-cached-state-audit.md`). Đóng note khi runtime ledger qua |

## Nợ kỹ thuật không chặn ship

- `scythe.py` báo 623 `[WRAP]/[YAP]` toàn repo, chủ yếu `src-tauri/src/pty.rs` (121), `system.rs` (62), `statusline.rs` (37) và các doc trong `docs/plan/done/`. Số ngang baseline trước đợt hardening, nên là nợ cũ. Xử lý theo từng file khi chạm tới, không quét hàng loạt.
- `cargo fmt --check` còn lệch ở vài file `src-tauri/src` (ví dụ `agent_usage/antigravity.rs`); chạy `cargo fmt` một lần trong commit riêng.

## Hoãn có chủ ý

- `task-1787103368182` — nhiều account Claude Code: owner đã hoãn vì cần debug trực tiếp; không lên lịch lại khi chưa được mở lại.

## Ngoài backlog này

- `docs/plan/remote-ingress-rework.md` đã qua build và Rust tests; chỉ còn protocol runtime trên Mac + điện thoại/edge, nên tiếp tục theo chính plan đó.
- `docs/plan/audit-akirule-2026-09-18.md` — plan UI đang chạy, có danh sách bước còn lại riêng.
- **Release**: 1.31.0 đã mint 2026-09-20 (`CHANGELOG.md` `[1.31.0]`).
- Các pinned note đã `done: true` không phải backlog. Nếu cần dọn UI, unpin/xoá chúng trong Task Notes thay vì giữ lịch sử ở file này.
