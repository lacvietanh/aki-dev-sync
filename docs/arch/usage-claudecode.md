# Kiến trúc Theo dõi & Đồng bộ Usage Claude Code

Tài liệu này đúc kết toàn bộ cơ chế, các giới hạn kỹ thuật từ phía Anthropic/Claude CLI, và giải pháp kiến trúc "Hybrid Patching" mà Aki Dev Sync sử dụng để quản lý Quota (Hạn mức) của Claude Code một cách chính xác 100% mà không gây hao tốn token.

## 1. Bản chất của hệ thống Telemetry trong Claude Code

Claude Code CLI (một tool Node.js) sử dụng cơ chế **hook ngầm** có tên là `statusLine` để xuất (export) dữ liệu Telemetry dưới dạng JSON ra ngoài sau mỗi lượt (turn) tương tác với LLM.
- Hook này được định nghĩa trong `~/.claude/settings.json` bằng trường `"statusLine"`.
- Dữ liệu JSON xuất ra chứa các thông tin quý giá: Tổng token (input/output), cost, context window, thông tin thư mục làm việc, và quan trọng nhất là `rate_limits` (Hạn mức API do Anthropic trả về).
- **Chỉ hoạt động với tài khoản Claude.ai Pro/Max** — không áp dụng cho API key thông thường.

App của chúng ta đọc dữ liệu này thông qua một bash script `~/.claude/statusline-command.sh` (được cấu hình để hứng `stdin` từ Claude CLI và ghi ra file `~/.claude/rate-limits-cache.json`). Hàm `get_agent_usage` (Rust Backend) định kỳ đọc file này lên UI.

### JSON Payload Structure

Dữ liệu `stdin` của `statusLine` hook có cấu trúc:

```json
{
  "rate_limits": {
    "five_hour":  { "used_percentage": 42, "resets_at": 1782034800 },
    "seven_day":  { "used_percentage": 18, "resets_at": 1782288000 }
  },
  "cwd": "/home/user/project",
  "transcript_path": "/home/user/.claude/projects/..."
}
```

- `resets_at`: Unix epoch seconds, UTC.
- `used_percentage`: Phần trăm đã dùng trong cửa sổ thời gian tương ứng.
- `statusLine` hook được gọi tự động sau **mỗi turn** — đây là số liệu thật từ server, không phải ước lượng.

### Local File Layout on Remote Machine

```
~/.claude/settings.json              → Cấu hình, trỏ đến statusLine script
~/.claude/statusline-command.sh      → Script nhận stdin JSON từ Claude Code
~/.claude/rate-limits-cache.json     → File cache (do chúng ta tạo bằng cách dump stdin)
~/.claude/.credentials.json          → OAuth credentials (subscriptionType, rateLimitTier, accessToken)
~/.claude/projects/**/*.jsonl        → Transcript files (fallback ước lượng token nếu cần)
```

### Self-Provisioning Logic

Vì dữ liệu thật chỉ tồn tại thoáng qua trong `stdin` khi `statusLine` script chạy, chúng ta phải persist nó:

1. SSH vào remote host.
2. Đọc `~/.claude/statusline-command.sh`.
3. Kiểm tra chuỗi `rate-limits-cache`.
4. Nếu chưa có: dùng `sed` để inject đoạn mã jq + bash ngay sau dòng `input=$(cat)`.
5. Nếu đã có: bỏ qua (idempotent).
6. Từ đó trở đi: đọc `~/.claude/rate-limits-cache.json` qua SSH.

---

## 2. Vấn đề giới hạn kỹ thuật (Technical Limitations)

Trong quá trình sử dụng thực tế, chúng ta đã phát hiện 2 điểm mù (Blind Spots) của Claude CLI và API Anthropic:

### Lỗi A: Hội chứng "Mất tích" Rate Limits (Khi chạm nóc 100%)
Khi người dùng dùng cạn kiệt Quota, API của Anthropic sẽ trả về lỗi HTTP `429 Too Many Requests`. Thay vì truyền nguyên vẹn gói JSON cũ với thông báo 100%, Claude Code CLI lại tự ý **CẮT BỎ HOÀN TOÀN** cục `rate_limits` ra khỏi luồng JSON truyền vào `statusLine`.
- **Hệ quả:** File cache bị mất key `rate_limits`, UI của App không đọc được % và thời gian reset, dẫn đến việc thanh Progress Bar biến mất hoàn toàn.

### Lỗi B: Force Sync bị "Bất lực" khi không có session đang chạy
Để ép đồng bộ Quota khi chưa phát sinh lượt chat mới, App chạy ngầm `claude --model haiku -p /usage`.
- **`-p` thực sự là gì:** Theo `claude --help`, `-p` / `--print` chỉ đơn giản là *"Print response and exit"* — không có ghi chú nào về việc tắt `statusLine`. (Ghi chú cũ trong tài liệu này về việc `-p` tắt `statusLine` là **sai**, đã được sửa sau thực nghiệm 2026-06-23.)
- **Lý do thực sự thất bại:** `statusLine` bị tắt vì **stdout không phải TTY** (chạy qua SSH pipe/redirect) — đây là điều Claude Code tự detect, không liên quan đến cờ `-p`.
- **Vấn đề căn bản hơn:** Mỗi lần gọi `claude -p` là một **process riêng biệt, RAM riêng biệt**. Lệnh `/usage` đọc thông tin rate-limit từ RAM của invocation hiện tại. Nếu không có session Claude Code nào đang chạy trên remote, RAM không có data → `/usage` trả về rỗng hoặc không match được format → parse thất bại → cache cũ giữ nguyên.
- **Xác nhận thực tế (2026-06-23):** Quan sát 1 ngày: sau mỗi lần quota reset mà người dùng không dùng Claude Code, bấm Force Sync không có tác dụng. Test thực nghiệm SSH xác nhận `/usage` chỉ trả về data đúng khi máy remote đang có session Claude Code hoạt động.
- **Hạn chế API:** Anthropic không có endpoint REST công khai tên `/rate_limits`. Thông tin quota được nhúng vào Headers (`anthropic-ratelimit-*`) của Response khi có Request thực sự, hoặc có thể truy cập qua endpoint OAuth không chính thức `api.anthropic.com/api/oauth/usage` (xem thêm tài liệu nghiên cứu).

---

## 3. Kiến trúc Giải pháp: "Hybrid Patching"

Để vượt qua các giới hạn trên, Aki Dev Sync áp dụng giải pháp **Hybrid Patching** (Vá thông minh kết hợp 2 luồng) được xử lý ngay từ tầng Rust Backend bằng Python và Bash script qua SSH.

### Luồng Bị động (Passive Flow - Quá trình Provision)
Bắt trọn mọi hoạt động chat của User và đề phòng Lỗi A.
- Khi người dùng kết nối, lệnh `provision_agent_usage` sẽ tiêm (inject) một đoạn mã jq + bash thông minh vào script `statusline-command.sh` trên máy chủ từ xa.
- **Thuật toán thông minh:** Nếu luồng JSON của Claude đẩy ra *KHÔNG CÓ* `rate_limits` (do lỗi 429), script sẽ tự động chui vào file `rate-limits-cache.json` cũ, **COPY** lại mốc thời gian Reset, ép giá trị `% used = 100`, rồi hòa trộn (Merge) vào JSON mới để ghi ra file.
- Mọi dữ liệu xịn xò (session, cwd, tokens, v.v.) được giữ nguyên. UI không bao giờ bị sập thanh Progress.

### Luồng Chủ động (Active Flow - Nút Force Sync)
Bắt ép làm mới giao diện 100% chuẩn xác mà không tốn Token và lách được Lỗi B.
- Rust Backend sẽ kích hoạt lệnh `claude --model haiku -p /usage`. Lệnh `/usage` in ra dạng Text trong chế độ `-p`, lấy thông tin từ RAM (bộ đệm cục bộ) của Claude trong invocation đó.
- Dữ liệu in ra (Stdout) có dạng chuẩn: `Current session: 3% used · resets Jun 22, 10:10pm (Asia/Singapore)`.
- Backend nhúng ngay một **Python Script** (Python 3) ngay trong luồng SSH:
  1. Dùng biểu thức chính quy (Regex) cắt lấy số `%` và `Chuỗi ngày giờ`.
  2. Dùng thư viện `datetime` chuyển đổi chuỗi chữ "Jun 22, 10:10pm" thành Unix Timestamp siêu chuẩn.
  3. Đọc file `rate-limits-cache.json` lên, **CHỈ GHI ĐÈ** cụm `rate_limits.five_hour` bằng `%` và `Timestamp` vừa tính. Ghi ngược file lại.
- **Giới hạn đã biết:** Active Flow chỉ hoạt động khi máy remote đang có session Claude Code đang chạy. Nếu idle sau reset, flow này thất bại. Xem `docs/research/claude-usage-1.2.7-analyze.md` để biết các hướng cải thiện.

---

## 4. Official References

- [StatusLine Documentation](https://code.claude.com/docs/en/statusline)
- [Claude Code Changelog](https://code.claude.com/docs/en/changelog)
- [Rate Limits Documentation](https://platform.claude.com/docs/en/api/rate-limits)
