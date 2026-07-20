# Nghiên cứu: Giải Pháp Giám Sát Quota Thời Gian Thực Cho Antigravity IDE và Claude Code

> Bài viết phân tích chuyên sâu về kiến trúc đo lường hạn mức sử dụng (Quota) của các AI Agent hàng đầu hiện nay. Đối chiếu kết quả thực tế của dự án **Aki-Dev-Sync** với cuộc khảo sát diện rộng dựa trên **104/108 Agent Deep-Research** (~1.43M Tokens).
>
> * **Tác giả:** Aki / Lạc Việt Anh Workflow
> * **Ngày thực hiện:** 2026-06-23 18:00 ICT (Cập nhật ngày 2026-06-24 12:40 ICT)
> * **Nguồn tham chiếu:** [dev.akitao.com](https://dev.akitao.com)

---

## 📌 Đặt Vấn Đề (The Context & Challenge)

Trong quy trình làm việc hiện đại như **Lạc Việt Anh Workflow** (sử dụng AI Agent cực kỳ chuyên sâu ở môi trường Remote và code nhẹ/commit ở Local), việc kiểm soát hạn ngạch sử dụng (Quota) của các AI Agent là yếu tố sống còn để duy trì hiệu suất làm việc liên tục:
* **Hạn mức 5 giờ / 7 ngày** của các AI Agent (như Claude Code hay Antigravity IDE/Windsurf) thường bị giới hạn rất gắt.
* Việc bị **Rate-Limit (429)** ngay giữa một chuỗi lập trình (deep-session) gây đứt gãy mạch tư duy của lập trình viên và AI.
* **Thách thức cốt lõi:** Cả Anthropic (Claude Code) và Google (Antigravity IDE) đều không cung cấp API công khai, có tài liệu rõ ràng để truy vấn giới hạn quota từ ứng dụng bên ngoài. 

Để giải quyết bài toán này, dự án **Aki-Dev-Sync** đã thiết lập hệ thống thu thập telemetry tự động, không chỉ chính xác mà còn phải đảm bảo các tiêu chí: **Native (Đơn giản, gọn nhẹ), An toàn (Không vi phạm Điều khoản dịch vụ - ToS), và Tiết kiệm Token (0-Token cost).**

---

## 🔬 Nghiên Cứu Quy Mô Lớn: 5 Họ Phương Pháp Giám Sát Quota

Thông qua một hệ thống nghiên cứu tự động quy mô lớn gồm **104 Agent kiểm chứng độc lập** và tiêu thụ khoảng **1.43 triệu token**, chúng tôi đã tổng hợp toàn bộ các phương pháp giám sát quota hiện tại trong cộng đồng mã nguồn mở thành **5 họ cơ chế (P1 - P5)**:

### 1. Họ P1 — Đọc stream/stdin do chính công cụ cấp (Statusline Hook)
* **Cách hoạt động:** Các công cụ CLI hiện đại như Claude Code (từ v2.1.x trở lên) tự động xuất ra dữ liệu JSON chứa `.rate_limits` (`five_hour` / `seven_day`) thông qua luồng vào chuẩn (`stdin`) của các statusline hook.
* **Độ ổn định:** ★★★★★ (Cao nhất).
* **Nhược điểm:** Phụ thuộc vào phiên bản công cụ mới nhất và gói tài khoản (Pro/Max).

### 2. Họ P2 — Phân tích file log cục bộ JSONL
* **Cách hoạt động:** Cộng dồn số token từ lược sử hội thoại được lưu tại `~/.claude/projects/` rồi tự tính toán cost ước lượng dựa trên bảng giá mô hình.
* **Độ ổn định:** ★★★★☆ (Thuần local).
* **Nhược điểm:** Chỉ là **ước lượng nội bộ**, dễ bị sai lệch khi nhà cung cấp thay đổi chính sách tính phí hoặc cấu trúc ngữ cảnh (Context Window).

### 3. Họ P3 — Gọi trực tiếp endpoint OAuth Usage
* **Cách hoạt động:** Đọc token xác thực cục bộ của người dùng (ví dụ: `~/.claude/.credentials.json`) rồi gửi yêu cầu `GET` lên API bán-công khai (như `api.anthropic.com/api/oauth/usage`).
* **Độ ổn định:** ★★★☆☆.
* **Nhược điểm:** Endpoint không có tài liệu kỹ thuật chính thức, có thể bị thay đổi hoặc khóa bất cứ lúc nào.

### 4. Họ P4 — Truy vấn Endpoint Language-Server cục bộ (Local Port Probe)
* **Cách hoạt động:** Giao tiếp trực tiếp với Language Server của IDE đang chạy ngầm thông qua RPC cục bộ sử dụng mã CSRF token.
* **Độ ổn định:** ★★★★☆.
* **Nhược điểm:** Phụ thuộc vào việc phát hiện port động và xử lý xác thực bảo mật (CSRF).

### 5. Họ P5 — Proxy Intercept HTTPS (MITM)
* **Cách hoạt động:** Sử dụng máy chủ proxy nội bộ, cài chứng chỉ Root CA tự ký và cấu hình chuyển hướng hosts để chặn bắt dữ liệu giữa client và cloud.
* **Độ ổn định:** ★★☆☆☆ (Thấp).
* **Nhược điểm:** Rủi ro bảo mật hệ thống nghiêm trọng khi cài Root CA lạ; nguy cơ bị nhà cung cấp cấm tài khoản (ToS ban) rất cao.

---

## 💎 Tinh Túy Thiết Kế Trong Aki-Dev-Sync

Đối chiếu với các cơ chế trên, **Aki-Dev-Sync** đã lựa chọn các giải pháp tối ưu nhất cho từng Agent để tích hợp vào Dashboard quản lý:

### 1. Đối Với Antigravity IDE (Giải Pháp P4 Tuyệt Đối Native)

Giải pháp của Antigravity được hiện thực hóa thông qua tệp mã nguồn [get-antigravity-usage.js](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/get-antigravity-usage.js). 

Thay vì sử dụng cách can thiệp proxy thô bạo (MITM) của các công cụ như `9router`, Aki-Dev-Sync chọn cách giao tiếp trực tiếp với Language Server của Antigravity đang chạy cục bộ thông qua **gRPC-Web/Connect-RPC**:

* **Quét Tiến Trình Không Bị Cắt Cụm (No Truncation):** 
  Sử dụng lệnh `ps auxww` trên macOS thay vì `ps aux` thông thường. Điều này đảm bảo toàn bộ tham số khởi chạy của tiến trình (bao gồm cả mã token bảo mật `--csrf_token` dài) được ghi nhận đầy đủ, không bị cắt ngắn bởi giới hạn cột của shell.
  
* **Lọc Tiến Trình Gốc (Precise Binary Targeting):** 
  Chỉ nhắm mục tiêu vào các tên tệp thực thi nhị phân chính thức của Antigravity (`language_server_macos_arm`, `language_server_macos_x64`,...). Giải pháp này triệt tiêu hoàn toàn lỗi nhận diện nhầm thường thấy ở các thư viện NPM khi quét các tiến trình Node.js của Volar (Vue Language Server) hay CSS Language Server.
  
* **Dò Cổng Động Thông Minh (Port Discovery):** 
  Chạy lệnh `lsof -nP -iTCP -sTCP:LISTEN -a -p <PID>` để lấy chính xác cổng TCP cục bộ mà tiến trình đang lắng nghe. Đồng thời kết hợp nạp thêm cổng mặc định và cổng kế tiếp làm "seed" dự phòng để tối ưu hóa thời gian dò.
  
* **Giao Thức Connect-RPC An Toàn:** 
  Thực hiện gửi yêu cầu `POST /exa.language_server_pb.LanguageServerService/GetUserStatus` đính kèm tiêu đề xác thực `X-Codeium-Csrf-Token`. Dữ liệu quota thực tế được giải mã trực tiếp từ payload JSON trả về của IDE:
  ```json
  "quotaInfo": {
    "remainingFraction": 0.85,
    "resetTime": "2026-06-23T23:59:59.000Z"
  }
  ```
  **Thời gian phản hồi chỉ mất khoảng 40ms, tiêu tốn 0 token mạng.**

---

### 2. Đối Với Claude Code (Cơ Chế P1 Thụ Động Thuần Túy)

Giám sát quota của một công cụ CLI trên máy Remote là bài toán khó hơn. Aki-Dev-Sync giải quyết bằng đúng một lớp — không có lớp fallback chủ động nào chạy `claude` để "ép" dữ liệu:

#### Đọc Thụ Động Không Hao Phí Token (P1 - Zero-Token Cache)
* Ứng dụng thực hiện vá (patch) tệp tin hook dòng trạng thái của Claude Code (`~/.claude/statusline-command.sh`) thông qua script [provision-claudecode.sh](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/provision-claudecode.sh).
* Mỗi khi người dùng gõ lệnh tương tác với Claude, Claude Code sẽ đẩy dữ liệu trạng thái sử dụng vào `stdin`. Hook đã vá sẽ trích xuất cụm `.rate_limits` này và ghi đè vào tệp cache cục bộ `~/.claude/rate-limits-cache.json`.
* Giao diện GUI chỉ cần đọc tệp cache này. Cơ chế này đạt độ ổn định tuyệt đối và hoàn toàn không tốn tài nguyên mạng.
* **Đã từng có, đã bị gỡ bỏ:** một phiên bản trước từng chạy `claude --model haiku -p /usage` như một "Force Sync" (họ P3) khi cache đóng băng qua chu kỳ reset, cộng với một luồng gọi endpoint bán-công khai `api.anthropic.com/api/oauth/usage`. Cả hai bị xoá hẳn — không phải rút gọn — sau khi đo thật cho thấy một lượt gọi headless như vậy **chỉ trả về mốc reset, không trả phần trăm đã dùng**, và luồng đó từng gây rò rỉ tiến trình `claude` mồ côi trên một máy remote (19 phiên, 6GB RAM). Khi qua mốc reset mà chưa có lượt tương tác mới, GUI giờ giữ nguyên số liệu cũ, đánh dấu "cached" và hiện dòng chờ, thay vì cố ép một nguồn dữ liệu không tồn tại.

---

## ⚖️ So Sánh Aki-Dev-Sync và Giải Pháp MITM (9router)

Một trong những phát hiện thú vị nhất của đợt Deep-Research là đối chiếu cơ chế của **9router / n9router**:
* **9router** sử dụng cách can thiệp **MITM (Họ P5)** bằng cách chuyển hướng DNS `/etc/hosts` của `cloudcode-pa.googleapis.com` (Antigravity Cloud API) về local và cài đặt Root CA tự ký.
* Cách này hữu dụng khi cần **can thiệp / sửa đổi** request (như tự động xoay vòng nhiều tài khoản khác nhau). Tuy nhiên, nếu chỉ phục vụ nhu cầu hiển thị Quota, nó là một giải pháp cực kỳ cồng kềnh (over-engineering), dễ vỡ khi Google/Anthropic thắt chặt cơ chế bảo mật (SSL Pinning) và tiềm ẩn nguy cơ bảo mật hệ thống nghiêm trọng.

**Aki-Dev-Sync** lựa chọn phương pháp **Observer thụ động (P4 & P1)**:
* Không sửa hosts, không cài CA, không can thiệp dòng lưu lượng mạng.
* Đọc trực tiếp từ API Connect-RPC nội bộ của IDE.
* Đảm bảo an toàn tuyệt đối cho tài khoản của nhà phát triển, tuân thủ nguyên lý thiết kế **SRP (Single Responsibility Principle)** và sự đơn giản tối đa.

---

## 📝 Kết Luận

Giải pháp giám sát quota trong **Aki-Dev-Sync** là minh chứng rõ nét cho triết lý **Native Flow** của **Lạc Việt Anh Workflow**:
1. **Tìm kiếm nguồn dữ liệu gốc đáng tin cậy nhất** (Local Connect-RPC và CLI Statusline stdin).
2. **Loại bỏ trung gian độc hại** (Không dùng Proxy MITM, không cài chứng chỉ không an toàn).
3. **Tối ưu hóa chi phí vận hành** (Chỉ đọc cache thụ động — không có lượt gọi `claude` hay HTTP nào để lấy số liệu, nên chi phí token và rủi ro vận hành đều bằng 0).

Sự kết hợp hoàn hảo này mang lại một công cụ Command Center mượt mà, ổn định và thực sự phục vụ tốt cho quy trình phát triển phần mềm thế hệ mới kết hợp cùng các AI Agent.
