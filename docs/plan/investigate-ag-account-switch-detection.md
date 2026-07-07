# Plan — Antigravity account switch not detected without IDE restart

Status: **Open — cần Mac để test** (ghi nhận 2026-07-08, chưa root-cause được vì máy này không cài
Antigravity IDE để verify trực tiếp)

## Vấn đề (báo cáo của user, 2026-07-08)

Sau khi đổi account **ngay trong Antigravity IDE** (không quit app) — ví dụ dropdown chọn tài khoản
Google khác — app Aki Dev Sync **không nhận ra** account mới. Chỉ khi **quit hẳn Antigravity rồi mở
lại** thì app mới thấy đúng account/quota mới.

## Giả thuyết (chưa verify — cần Mac để xác nhận)

`scripts/get-antigravity-usage.js` **không cache gì ở phía app mình** — mỗi lần poll (mặc định 30s)
đều chạy lại toàn bộ chuỗi từ đầu:
1. `ps auxww` → tìm process `language_server_*` đang chạy → đọc `--csrf_token` +
   `--extension_server_port` **trực tiếp từ command-line arguments của chính process đó**
   (`detectOnUnix()`/`parseUnixProcessLine()`, `get-antigravity-usage.js:64-109`).
2. Dùng CSRF token đó gọi Connect RPC `GetUserStatus` để lấy email.

Vì bước 1 luôn re-đọc `ps auxww` mỗi 30s (không có state cũ nào bị pin ở tầng Rust/JS), nên nếu
Antigravity **spawn process mới** khi đổi account, app phải nhận được ngay ở poll kế tiếp — không
cần sửa gì. Vậy khả năng cao nhất: **Antigravity không restart `language_server` process khi đổi
account qua UI của chính nó** — process cũ (với CSRF token cũ, cấp lúc process khởi động) tiếp tục
chạy, và bản thân RPC endpoint đó trả về **context account cũ** vì chưa từng biết có sự kiện đổi
account nào xảy ra ở tầng session nội bộ của nó. Quit hẳn app buộc nó phải spawn process mới → CSRF
token mới → app mình đọc được account mới ngay từ poll đầu tiên.

Nếu giả thuyết đúng: đây là **giới hạn từ phía Antigravity (third-party)**, không sửa được từ script
polling bên ngoài — không có API public nào để ép một Connect RPC server refresh session nội bộ mà
không restart process.

## Test protocol (làm trên Mac, nhẹ — không cần build lại app)

1. Mở Antigravity, đang login account X. Lấy PID: `ps aux | grep language_server`, ghi lại số PID.
2. Trong Antigravity, đổi sang account Y **không quit app**.
3. Lập tức chạy lại `ps aux | grep language_server`, so PID với bước 1.

**Đọc kết quả:**
- **PID KHÔNG đổi** → xác nhận đúng giả thuyết trên. Antigravity tự nó không restart process khi đổi
  account → **không sửa được từ phía mình**. Đóng plan này ở trạng thái "confirmed external
  limitation, no fix possible", giữ nguyên hành vi hiện tại (quit+reopen là cách duy nhất) — có thể
  cân nhắc thêm 1 dòng tooltip/hint nhỏ trong UI kiểu "đổi account? quit & mở lại Antigravity" nếu
  muốn (không bắt buộc, chỉ là UX polish nhỏ).
- **PID CÓ đổi** (process mới được spawn) → giả thuyết trên SAI, tức Antigravity *có* restart nội bộ
  nhưng app mình vẫn không nhận ra kịp thời — lúc đó mới là bug thật ở code mình, cần điều tra tiếp:
  khả năng poll interval quá dài (mặc định 30s, xem `refreshStore.js`), hoặc `discoverPorts`/
  `probeForConnectAPI` không tìm ra port mới đủ nhanh, hoặc lỗi nào đó trong
  `agent_usage.rs::get_antigravity_usage` khi xử lý process detection thay đổi giữa 2 poll liên tiếp.

## Việc cần user làm

Chỉ 3 bước ở trên (nhẹ, không cần `--debug`, không cần rebuild) — làm khi rảnh, không phải gấp cho
release này. Kết quả (PID đổi hay không) dán lại đây hoặc báo trực tiếp để quyết định bước tiếp theo.

## Cross-refs

- `scripts/get-antigravity-usage.js` — process detection + Connect RPC probe logic.
- `docs/arch/usage-antigravity.md` — kiến trúc tổng thể AG usage monitoring.
- `src-tauri/src/agent_usage.rs::get_antigravity_usage` — nơi Rust gọi script này mỗi poll.
