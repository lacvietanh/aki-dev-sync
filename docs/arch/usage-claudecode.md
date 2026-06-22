# Kiến trúc Theo dõi & Đồng bộ Usage Claude Code

Tài liệu này đúc kết toàn bộ cơ chế, các giới hạn kỹ thuật từ phía Anthropic/Claude CLI, và giải pháp kiến trúc "Hybrid Patching" mà Aki-RemoteDevMan sử dụng để quản lý Quota (Hạn mức) của Claude Code một cách chính xác 100% mà không gây hao tốn token.

## 1. Bản chất của hệ thống Telemetry trong Claude Code
Claude Code CLI (một tool Node.js) sử dụng cơ chế **hook ngầm** có tên là `statusLine` để xuất (export) dữ liệu Telemetry dưới dạng JSON ra ngoài sau mỗi lượt (turn) tương tác với LLM.
- Hook này được định nghĩa trong `~/.claude/settings.json` bằng trường `"statusLine"`.
- Dữ liệu JSON xuất ra chứa các thông tin quý giá: Tổng token (input/output), cost, context window, thông tin thư mục làm việc, và quan trọng nhất là `rate_limits` (Hạn mức API do Anthropic trả về).

App của chúng ta đọc dữ liệu này thông qua một bash script `~/.claude/statusline-command.sh` (được cấu hình để hứng `stdin` từ Claude CLI và ghi ra file `~/.claude/rate-limits-cache.json`). Hàm `get_agent_usage` (Rust Backend) định kỳ đọc file này lên UI.

## 2. Vấn đề giới hạn kỹ thuật (Technical Limitations)

Trong quá trình sử dụng thực tế, chúng ta đã phát hiện 2 điểm mù (Blind Spots) chết người của Claude CLI và API Anthropic:

### Lỗi A: Hội chứng "Mất tích" Rate Limits (Khi chạm nóc 100%)
Khi người dùng dùng cạn kiệt Quota, API của Anthropic sẽ trả về lỗi HTTP `429 Too Many Requests`. Thay vì truyền nguyên vẹn gói JSON cũ với thông báo 100%, Claude Code CLI lại tự ý **CẮT BỎ HOÀN TOÀN** cục `rate_limits` ra khỏi luồng JSON truyền vào `statusLine`.
- **Hệ quả:** File cache bị mất key `rate_limits`, UI của App không đọc được % và thời gian reset, dẫn đến việc thanh Progress Bar biến mất hoàn toàn.

### Lỗi B: Force Sync bị "Bất lực" ở chế độ Non-Interactive
Để ép đồng bộ Quota khi chưa phát sinh lượt chat mới, ban đầu App chạy ngầm lệnh `claude -p 'ok'` (cờ `-p` là Print mode). 
- **Nguyên lý:** Cờ `-p` báo hiệu cho Claude CLI biết nó đang bị Pipe (không chạy trên một Terminal tương tác thật).
- **Hệ quả:** Để tối ưu luồng Pipe không bị rác, Claude Code CLI **CHỦ ĐỘNG TẮT** tính năng `statusLine`. Dẫn đến việc dù API có trả kết quả về, hook cập nhật JSON của chúng ta không bao giờ được gọi. Nút Force Sync trở nên vô dụng.
- **Hạn chế API:** Chúng ta cũng không thể dùng `curl` gọi API Anthropic vì Anthropic **KHÔNG CÓ** endpoint nào tên là `/rate_limits`. Thông tin quota chỉ được nhúng vào trong Headers (`anthropic-ratelimit-*`) của một Response khi và chỉ khi có một Request trừ token thực sự diễn ra.

## 3. Kiến trúc Giải pháp: "Hybrid Patching"

Để vượt qua các giới hạn trên, Aki-RemoteDevMan áp dụng giải pháp **Hybrid Patching** (Vá thông minh kết hợp 2 luồng) được xử lý ngay từ tầng Rust Backend bằng Python và Bash script qua SSH.

### Luồng Bị động (Passive Flow - Quá trình Provision)
Bắt trọn mọi hoạt động chat của User và đề phòng Lỗi A.
- Khi người dùng kết nối, lệnh `provision_agent_usage` sẽ tiêm (inject) một đoạn mã jq + bash thông minh vào script `statusline-command.sh` trên máy chủ từ xa.
- **Thuật toán thông minh:** Nếu luồng JSON của Claude đẩy ra *KHÔNG CÓ* `rate_limits` (do lỗi 429), script sẽ tự động chui vào file `rate-limits-cache.json` cũ, **COPY** lại mốc thời gian Reset, ép giá trị `% used = 100`, rồi hòa trộn (Merge) vào JSON mới để ghi ra file. 
- Mọi dữ liệu xịn xò (session, cwd, tokens, v.v.) được giữ nguyên. UI không bao giờ bị sập thanh Progress.

### Luồng Chủ động (Active Flow - Nút Force Sync)
Bắt ép làm mới giao diện 100% chuẩn xác mà không tốn Token và lách được Lỗi B.
- Rust Backend sẽ kích hoạt lệnh `claude --model haiku -p /usage`. Rất may mắn là lệnh `/usage` vẫn in ra dạng Text trong chế độ `-p`, và nó lấy thẳng thông tin từ RAM (bộ đệm cục bộ) của Claude mà không tốn 1 Request nào.
- Dữ liệu in ra (Stdout) có dạng chuẩn: `Current session: 3% used · resets Jun 22, 10:10pm (Asia/Singapore)`.
- Backend nhúng ngay một **Python Script** (Python 3) ngay trong luồng SSH:
  1. Dùng biểu thức chính quy (Regex) cắt lấy số `%` và `Chuỗi ngày giờ`.
  2. Dùng thư viện `datetime` chuyển đổi chuỗi chữ "Jun 22, 10:10pm" thành Unix Timestamp siêu chuẩn.
  3. Đọc file `rate-limits-cache.json` lên, **CHỈ GHI ĐÈ** cụm `rate_limits.five_hour` bằng `%` và `Timestamp` vừa tính. Ghi ngược file lại.
- **Lợi ích:** Đạt tốc độ bàn thờ, cực kỳ đáng tin cậy, không bị ngắt quãng bởi Trust Workspace Prompt của Claude, và bảo toàn 100% các dữ liệu token/cost của session hiện hành.

---
**Tổng kết:** Với kiến trúc Hybrid Patching này, App đóng vai trò như một lớp vỏ bọc hoàn hảo, vá tất cả các điểm yếu trong hệ thống Telemetry mặc định của Claude Code, đem đến một UI ổn định tuyệt đối trong mọi tình huống tương tác.
