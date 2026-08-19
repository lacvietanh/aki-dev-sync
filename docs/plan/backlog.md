# Backlog aug17 — các note còn mở trong `.akidevsync/notes.json`

Chốt từ 10 task chưa `done` trong `.akidevsync/notes.json` (đọc 2026-08-17). Hai note pin cụ thể nhất đã tách sang `docs/plan/1.26.1-improve.md`; file này giữ phần còn lại, xếp theo thứ tự cần-nghiên-cứu-trước → làm-được-ngay.

| # | Note | Loại | Trạng thái |
|---|---|---|---|
| B1 | app data dir → `~/.aki/devsync/` | research trước | code shipped 2026-08-19, chờ verify trên Mac |
| B2 | project config vào `.akidevsync/` trong repo | research trước | pin |
| B3 | GPU cao khi app idle | research trước | shipped 2026-08-19, số đo GPU chưa đo lại trên Mac |
| B9 | Claude Code: xem được nhiều account | code | tạm bỏ qua (owner) |
| B4 | Task Notes: nới giới hạn text, dùng chung pattern | code | pin |
| B5 | AGY pre-allow list: bổ sung mục | code | — |
| B6 | REPORT: định nghĩa "file mới" theo chu kỳ check | code | wish |
| B7 | Link Google Search Console cho project web | wish | wish |
| B8 | `task-1786949417624` rỗng | rác | xoá |

---

## B1 — Chuyển app data dir vào `~/.aki/devsync/`

Note `task-1786953511448`. Mục tiêu: chuẩn hoá theo hệ sinh thái `~/.aki/`, kèm migration một chiều — bản mới thấy nơi cũ có file mà nơi mới chưa có thì chuyển hết rồi xoá nơi cũ.

Hiện trạng: mọi thứ nằm ở `app_data_dir()` của Tauri (`~/Library/Application Support/aki.devsync/` trên macOS) — `projects.json`, `usage.log`, baseline sync, cache SSH. Call site: `src-tauri/src/projects.rs:127,137` (`get_app_data_dir`, là funnel chính), `src-tauri/src/logger.rs:39`, `src-tauri/src/sync.rs:25-27,422-447,538` (đã có sẵn một migration cũ từ `~/.aki/devsync-baselines` — đọc trước khi viết cái mới, đây chính là tiền lệ đúng để copy), `src-tauri/src/ssh.rs:67,84,95`.

Hành vi đã chốt trong note, không bàn lại: bản mới thấy nơi cũ có file mà nơi mới chưa có → chuyển hết các file cần thiết sang nơi mới → xoá khỏi nơi cũ.

Việc:
1. Chỉ sửa **một** funnel `get_app_data_dir` — không rải path mới ra từng call site (`pattern.A1`).
2. Migration đặt ở đầu vòng đời khởi động, trước mọi lệnh đọc/ghi appdata; copy xong mới xoá nguồn để crash giữa chừng không mất dữ liệu. Copy pattern từ migration baseline sẵn có ở `src-tauri/src/sync.rs:538`.
3. Migration là hành động lên hệ thống thật, không phải chỉ viết code (`coding.B3`): phải chạy trên máy Mac có dữ liệu thật mới được đóng.

**Code shipped 2026-08-19**: `src-tauri/src/app_paths.rs` là funnel duy nhất (`app_data_dir()` + `migrate_legacy_app_data()`), chạy ở đầu `setup()` trong `lib.rs`, trước cả `logger::init`. Chi tiết migration, checklist verify: `docs/plan/done/appdata-dir-to-aki-devsync.md`. Chưa đóng theo mục 3 ở trên — chưa chạy trên Mac có dữ liệu thật.

## B2 — Đưa project config vào chính repo, ở `.akidevsync/`

Note `task-1785676763350`. Đã có tiền lệ: `notes.json` của tính năng task list đã nằm trong `.akidevsync/` của từng repo từ 1.22.0 (`docs/feat/project-task-list.md`).

Ràng buộc mà chính note đã nêu: config chứa **đường dẫn máy cụ thể**. Repo được mirror từ Mac lên remote thì đường dẫn hai bên khác nhau — commit path của Mac vào repo là làm hỏng phía remote, mà repo lại là thứ dùng chung.

Note đã chỉ định đúng quy trình: research + phân tích ưu nhược trước, rồi hoặc ra giải pháp tốt nhất, hoặc giữ nguyên sau khi phân tích kỹ. Research doc (`docs/research/`, schema `docs.B2`) phải phân tách rõ: trường nào là *thuộc tính của project* (mang theo repo được — tên, thứ tự, toggle tính năng) và trường nào là *thuộc tính của máy* (path local, host SSH, credential — không bao giờ vào repo).

## B3 — GPU cao khi app chỉ ngồi idle

Note `task-1786650061322`, pin. Câu hỏi của note: vì sao app ăn GPU nhiều dù không làm gì — có hiệu ứng CSS/JS nào chạy sai logic, hay flow rác nào lặp vô ích không.

Đây là việc đo trước, sửa sau. Nghi phạm phải kiểm tra bằng số liệu chứ không đoán: animation/transition CSS chạy vĩnh viễn (`animation: … infinite`, thanh progress, con trỏ nhấp nháy của terminal khi tab không hiển thị), timer/interval của usage monitor và refresh controller vẫn quay khi cửa sổ bị che, xterm render liên tục, và mọi `requestAnimationFrame` không có điều kiện dừng.

Ràng buộc: chỉ đo được trên Mac với app chạy thật (`coding.B3`) — Activity Monitor/`powermetrics` cho số nền, Safari Web Inspector cho phần web (nhớ chọn đúng target `localhost`, không phải `Main.html` của chính inspector). Kết quả vào một research doc (`docs/research/`, schema `docs.B2`) trước khi mở việc sửa.

**Shipped 2026-08-19**: research đã có ở `docs/research/perf-idle-gpu-cpu.md` (đợt trước) và plan `docs/plan/done/fix-idle-gpu-webkit-compositor.md` (đợt này, phần "Implemented"). `backdrop-filter` gỡ khỏi chrome thường trực, đưa vào công tắc "Glass Effect" (mặc định tắt); con trỏ terminal chỉ nhấp nháy ở tab đang active + đang focus; `RefreshRing` đổi sang bước ~1 giây thay vì animate mỗi frame. Số đo GPU/CPU thật chưa được đo lại trên Mac — mục tiêu `< 2%`/`< 5%` của plan gốc vẫn còn để ngỏ.

## B4 — Task Notes: nới giới hạn text, dùng chung pattern

Note `task-1786650384549`. Giới hạn hiện tại rải rác, không có nguồn duy nhất:

| Nơi | Giới hạn |
|---|---|
| `src/components/tasks/NotesField.vue:27` | `maxlength` mặc định 1500 |
| `src/components/modals/ProjectTasksModal.vue:28` | 1500 (truyền tay, trùng giá trị trên) |
| `src/components/tasks/TaskListPanel.vue:26,77` | 200 (title) |
| `src/components/tasks/TaskListPanel.vue:86` | 500 (detail) |
| `src/components/modals/GlobalNoteModal.vue:13` | 100000 |

Yêu cầu của note: xử lý cả global task notes, **dùng chung một pattern tái sử dụng** chứ không sửa lẻ từng chỗ. Việc: gom các hằng số về một nơi (`pattern.A1`), đặt tên theo vai trò (`TASK_TITLE_MAX`, `TASK_DETAIL_MAX`, `NOTES_MAX` — `pattern.A7`), rồi nâng giá trị. Nâng bao nhiêu là câu hỏi mở; ràng buộc thật là `notes.json` nằm trong repo người dùng nên không được phình vô hạn.

## B5 — Bổ sung AGY pre-allow list

Note `task-1785805753407`. Tính năng đã có: `src-tauri/src/gemini_allowlist.rs` + `src/components/modals/GeminiAllowlistModal.vue`, mô tả ở `docs/feat/agy-command-allowlist.md`.

Thêm vào bộ lệnh khuyến nghị được check sẵn:
- mọi đường dẫn script mà skill trong `~/.aki/akidevrule` gọi tới;
- mọi lệnh READONLY mà project `aki-mcp-sv` dùng.

Cả hai danh sách phải liệt kê từ nguồn thật trước khi sửa, không đoán. Đây chỉ là dữ liệu, không đụng logic merge/backup.

## B6 — REPORT: "mới" tính theo chu kỳ check, không theo mốc thời gian cứng

Note `task-1783626118506`. Khi check sync thấy file REPORT mới thì bật swal xác nhận mở (Enter = mở). Vấn đề: chưa định nghĩa được thế nào là "mới".

Định nghĩa note đưa ra: trong khoảng ~2 phút **hoặc ~2 chu kỳ fetch** — và vì chu kỳ này người dùng chỉnh được trong settings, ngưỡng phải suy ra từ chu kỳ hiện hành chứ không hardcode phút. Nguồn chu kỳ: `docs/arch/refresh-controller.md`.

## B7 — Link Google Search Console cho project dạng web

Note `task-1782727844849`, wish. Thêm lối mở nhanh Search Console cho project web, dạng `https://search.google.com/search-console/inspect?resource_id=sc-domain%3A<domain>`.

Chưa đủ để làm: cần chốt (a) app biết project là "web" và biết domain của nó bằng cách nào — hiện project config không có trường domain; (b) đặt ở đâu cho khỏi phạm UI Principle Extreme Narrow — ứng viên tự nhiên là popup OPEN (`docs/feat/open-popup.md`) chứ không phải một nút mới trên hàng.

## B8 — Note rỗng

`task-1786949417624` không có title lẫn detail. Xoá trong app, không phải việc code.

## B9 — Claude Code: xem được nhiều account (tạm bỏ qua)

Yêu cầu ban đầu của owner trong đợt 1.28: theo dõi được nhiều account Claude Code, giống pattern app đã làm cho Antigravity — nhưng cách detect account khác nhau giữa hai CLI (`docs/arch/usage-claudecode.md` §1 vs `docs/arch/usage-antigravity.md`).

Owner tự hoãn nguyên văn: "mất thời gian cho cái này vì cần tôi giúp debug -> tạm bỏ qua tính năng này". Không có code nào cho mục này trong đợt 1.28. Không lên lịch lại cho tới khi owner chủ động mở lại.

## B10 — SSH terminal: khoá mouse reporting để copy chắc chắn

Copy text từ `claude` chạy qua SSH (in-app terminal lẫn Terminal.app) bị hên xui — Option+kéo-chọn rồi nhả chuột, selection thường bị mất theo lúc nhả (~2-3/100 lần giữ được). Điều tra 2026-08-19: xterm.js's `shouldForceSelection` chỉ đọc `e.altKey` — về lý thuyết đủ, nhưng lỗi tái hiện y hệt ở Terminal.app (không dùng xterm.js) với cùng target `claude` qua SSH, nên nguyên nhân nằm ở `claude`'s mouse-tracking mode phối hợp với độ trễ báo modifier-key của macOS khi kéo nhanh — không phải bug trong code app này, không sửa được bằng cách đổi logic chọn.

Đề xuất: thêm toggle "khoá mouse reporting" riêng cho SSH tab — bật lên thì PTY không nhận mouse event nữa, kéo chọn ăn chắc 100% không phụ thuộc timing Option key. Cần quyết định UI (project này theo luật Extreme Narrow — không thêm row/button tuỳ tiện), owner đã chọn "làm task riêng" thay vì bỏ qua (2026-08-19). Chưa có code cho mục này.

---

## Ghi chú

Mọi mục ở đây chưa được lên lịch vào version cụ thể; đây là backlog, không phải cam kết cho 1.26.1.
