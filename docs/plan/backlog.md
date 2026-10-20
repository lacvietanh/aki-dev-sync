# Backlog

Snapshot từ `.akidevsync/notes.json` và trạng thái repo ngày 2026-10-20. Chỉ giữ việc còn mở; việc đã làm nằm trong `docs/plan/done/`, kết quả điều tra nằm trong `docs/research/`.

## Đã quyết (2026-09-20, /akithink tự chạy)

- **Refresh all gọi `loadData()`:** giữ. Decided: giữ Refresh = reload config + `refreshAllProjects()` · because owner note `task-1787801054655` gọi restart app là tối kỵ (mất mọi terminal), research `startup-cached-state-audit.md` đã chọn đúng hướng này và dặn kiểm migration idempotent (đã kiểm, cả bốn đều guard theo field/entry), `loadData` đã có `isReloading`, nút bị khoá khi đang sync, epoch/`refreshCount` reset giống `bumpEpoch`; điều `refresh-controller.md` cấm thật ra là ba cơ chế refresh không liên quan, không phải việc đọc lại config · rejected: nút "Reload config" riêng (thêm control trái nguyên tắc extreme-narrow, người dùng phải biết bấm nút nào) · reopen if: nhấn Refresh lúc đang lưu config làm mất chỉnh sửa, hoặc reload chậm rõ rệt với danh sách dự án lớn. Doc đã sửa: `docs/arch/refresh-controller.md`, `docs/feat/sync-check-and-usage-switches.md`, amendment trong research.
- **Thang token `rgba()`:** đã chốt và áp dụng, xem `docs/plan/audit-akirule-2026-09-18.md` § Thang token. Finding 3: giữ `<style scoped>`.

## Việc của owner trên máy này

Không còn — statusline đã apply lại xong (owner xác nhận 2026-09-27).

## Runtime ledger

Không còn mục nào. Owner đã tận mắt thấy chạy đúng thì mục đó đóng ngay, không test lại.

Đóng, 2026-10-20:
- **Điện thoại companion, nhiều tab**: owner xác nhận chạy đúng; agent cũng đã ghép cặp, mở hai tab, tự nối lại sau khi host khởi động lại (`docs/ref/agent-control.md`).
- **PUSH làm đứng app** (`task-1791428905665`): owner bấm PUSH không đứng; R1 qua (`docs/research/push-freeze-diagnosis.md` § Amendments).
- **Badge PULL 142 / 376 sau `touch` hàng loạt** (#9 cũ): ra trong 1.33.1, app trên Mac đang chạy 1.33.1; `project.json` của aki-gegrok-bot trên `grokvm` và trên Mac giờ giống hệt (`cmp`), nên ca F1 không còn.

Đóng, 2026-09-27:
- **Modal/màu token**: đợt đổi hex→`var()` đã chạy production 1 tuần (từ 1.31.0, 2026-09-20) qua dùng thật hàng ngày, owner xác nhận không thấy lệch màu — coi là đã qua ledger, không cần buổi duyệt riêng.
- **Refresh all không mất terminal**: xác nhận bằng đọc code, không cần tay bấm — `loadData()` (`src/composables/useProjectConfig.js:138`) chỉ ghi vào `projects.value`/`projectRuntime`/notes, không chạm bất kỳ state hay lệnh PTY nào (terminal tab sống hoàn toàn ở phía Rust, độc lập với danh sách project). Nút **Refresh all** là icon vòng xoay (`.btn-refresh-main`) ở góc trên bên phải titlebar, có từ 1.31.0 (2026-09-20) — nó đã bao gồm cả việc đọc lại `projects.json`/SSH hosts từ đĩa (`handleRefresh()` gọi `loadData()` rồi mới `refreshAllProjects()`), không phải nút refresh-per-project riêng (nút đó là icon nhỏ trên từng dòng project, chỉ chạy `refreshProject(p)`, không đọc lại config).

## Research / thiết kế

| # | Việc | Note | Bước kế tiếp |
|---|---|---|---|
| 5 | Phục hồi phiên làm việc sau quit/relaunch | `task-1789604852739` | Research ranh giới có thể phục hồi: metadata tab, cwd, title, pin; không hứa phục hồi process nếu PTY đã chết |
| 8 | Lấy usage thật của Claude Code qua API `/usage` (RE từ CLI), không cần probe hay phiên đang chạy | `task-1790294479640` | Debug/RE CLI để tìm API và cách xác thực; ghi kết quả vào `docs/research/`. Đối chiếu `docs/arch/usage-claudecode.md`: hiện statusLine hook là nguồn duy nhất, không có luồng active — đổi nguồn là đổi ràng buộc đó |

## Nợ kỹ thuật không chặn ship

- Code sạch `[WRAP]/[YAP]` và `cargo fmt`/`clippy --all-targets` (2026-10-20). `scythe.py --all` còn 9 `[YAP]`, toàn là code trích trong `docs/plan/done/` và `docs/research/`: bản ghi sự kiện, giữ nguyên có chủ ý.
- `npm run audit:ui` đã xanh (2026-09-29): detector sửa xong, literal lặp thành token, phần còn lại ghi exception có owner + lý do trong `scripts/ui-audit.config.json`. Không còn nợ từ `docs/plan/done/audit-akirule-2026-09-18.md`.

## Chờ owner xác nhận

- `task-1787103368182` — nhiều account Claude Code: code xong (2026-10-20, `[Unreleased]`, `docs/arch/usage-claudecode.md` § Nhiều account trên một máy). Còn lại: build bản mới, bấm Apply trong Statusline Customizer để cài hook v6, chạy turn ở cả hai account rồi xem menu email trên thẻ CC có đủ hai account với số riêng. Thấy đúng thì đóng note.

## Ngoài backlog này

- **Remote ingress**: code xong, plan đã vào `docs/plan/done/remote-ingress-rework.md`. Phần còn lại là cấu hình trên máy owner, không phải việc của repo: cài đặt Remote trên Mac này vẫn trỏ `https://lvamac.akitao.com`, một tên cũ không có DNS cũng không có tunnel, nên lúc này chỉ dùng được qua LAN. Hướng đã chốt nằm ở `docs/research/remote-ingress-shared-vs-separate.md`: Dev Sync dùng subdomain và tunnel riêng, không chung với tunnel `mba20` của aki-mcp-sv.
- **Release**: 1.33.1 ra 2026-10-20, đóng `task-1789578897581` (usage AGY CLI). 1.33.0 ra 2026-10-07, đóng `task-1789604877926` (kéo-thả file từ ngoài làm app mất trang, commit `e37437c`). Chuỗi kiểm tra runtime của ba plan 1–3 đóng 2026-10-07: 1.32.0 chạy hằng ngày trên Mac từ hôm ra mà không lỗi, owner xác nhận; plan và `docs/plan/done/1.32.0-mac-handoff.md` đã vào `done/`, các task note #6 đã done.
- Các pinned note đã `done: true` không phải backlog. Nếu cần dọn UI, unpin/xoá chúng trong Task Notes thay vì giữ lịch sử ở file này.
