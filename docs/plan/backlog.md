# Backlog

Snapshot từ `.akidevsync/notes.json` và trạng thái repo ngày 2026-10-19 (HEAD `2d8567f`). Chỉ giữ việc còn mở; việc đã làm nằm trong `docs/plan/done/`, kết quả điều tra nằm trong `docs/research/`.

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
| 7 | Một nút PUSH sync từng làm đứng cả app: audit async | `task-1791428905665` | Quét tĩnh 2026-10-19: 0/86 command vi phạm quy tắc `spawn_blocking`; H1, H3, H4 không có bằng chứng; H2 (mỗi dòng `sync-log` gây một lần cập nhật store + cuộn) đã gộp theo lô 50 ms trong `useLogs.js`, chưa kiểm ở runtime. Note mở đến khi chạy xong R1 trên Mac (dev build, Safari Web Inspector, PUSH ≥1000 file rồi PUSH ~10 file). Chi tiết và R1–R3: `docs/research/push-freeze-diagnosis.md`. Bài học cho UNIDOC Tauri mới ở dạng đề xuất trong doc đó, chưa áp dụng |
| 9 | Badge PULL 142 (aki-gegrok-bot) / 376 (akiprx) sau khi server `touch` hàng loạt | — | Đã sửa trong code (Unreleased): file cùng size được kiểm nội dung, file chỉ khác mtime tách ra `time_only_*`; probe thật gegrok PULL 142 → 3. Còn: (a) owner cài bản mới để xem tooltip/popover thật; (b) owner quyết có mở lại F1 không — `project.json` của gegrok trên server cũ hơn mà mtime mới hơn, PULL sẽ xoá exclude `data/` và lệnh deploy (`docs/research/akidevsync-project-config-scope-2.md` § Amendments 2026-10-19). Chi tiết: `docs/research/sync-badge-time-only.md` |

## Research / thiết kế

| # | Việc | Note | Bước kế tiếp |
|---|---|---|---|
| 5 | Phục hồi phiên làm việc sau quit/relaunch | `task-1789604852739` | Research ranh giới có thể phục hồi: metadata tab, cwd, title, pin; không hứa phục hồi process nếu PTY đã chết |
| 8 | Lấy usage thật của Claude Code qua API `/usage` (RE từ CLI), không cần probe hay phiên đang chạy | `task-1790294479640` | Debug/RE CLI để tìm API và cách xác thực; ghi kết quả vào `docs/research/`. Đối chiếu `docs/arch/usage-claudecode.md`: hiện statusLine hook là nguồn duy nhất, không có luồng active — đổi nguồn là đổi ràng buộc đó |

## Nợ kỹ thuật không chặn ship

- `scythe.py --all` (2026-10-19) báo 776 `[WRAP]/[YAP]` toàn repo, chủ yếu `src-tauri/src/pty.rs` (128), `system.rs` (65), `statusline.rs` (37) và các doc trong `docs/plan/done/`; nợ cũ, không phải do đợt gần đây. Xử lý theo từng file khi chạm tới, không quét hàng loạt.
- `cargo fmt --check` còn lệch ở `src-tauri/src` (2026-10-19): `sync.rs`, `sync_state.rs`, `conflict.rs`, `project_config.rs`, `projects.rs`, `system.rs`, `lib.rs`; chạy `cargo fmt` một lần trong commit riêng.
- `npm run audit:ui` đã xanh (2026-09-29): detector sửa xong, literal lặp thành token, phần còn lại ghi exception có owner + lý do trong `scripts/ui-audit.config.json`. Không còn nợ từ `docs/plan/done/audit-akirule-2026-09-18.md`.

## Hoãn có chủ ý

- `task-1787103368182` — nhiều account Claude Code: owner đã hoãn vì cần debug trực tiếp; không lên lịch lại khi chưa được mở lại.

## Ngoài backlog này

- `docs/plan/remote-ingress-rework.md` đã commit (`00f4e68`, 2026-09-10), qua build và Rust tests; chỉ còn protocol runtime trên Mac + điện thoại/edge. Owner hẹn bàn lại sau (2026-09-27) — nhắc lại khi quay lại plan này.
- **Release**: 1.33.1 ra 2026-10-20, đóng `task-1789578897581` (usage AGY CLI). 1.33.0 ra 2026-10-07, đóng `task-1789604877926` (kéo-thả file từ ngoài làm app mất trang, commit `e37437c`). Chuỗi kiểm tra runtime của ba plan 1–3 đóng 2026-10-07: 1.32.0 chạy hằng ngày trên Mac từ hôm ra mà không lỗi, owner xác nhận; plan và `docs/plan/done/1.32.0-mac-handoff.md` đã vào `done/`, các task note #6 đã done. Dọn code chưa lên lịch, không chặn release: (b) `scythe.py` báo `[YAP]` mức review có từ trước 1.32.0 ở các file đợt 1.32.0 — nhiều nhất `sync.rs` (39), `conflict.rs` (22), `sync_state.rs` (20), `projects.rs` (13), `useProjectConfig.js` (12); (c) `cargo fmt` một commit riêng.
- Các pinned note đã `done: true` không phải backlog. Nếu cần dọn UI, unpin/xoá chúng trong Task Notes thay vì giữ lịch sử ở file này.
