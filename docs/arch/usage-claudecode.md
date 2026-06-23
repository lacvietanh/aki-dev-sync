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

1. SSH vào remote host và patch `statusline-command.sh` bằng script [provision-claudecode.sh](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/provision-claudecode.sh).
2. Đọc `~/.claude/statusline-command.sh`.
3. Kiểm tra chuỗi `rate-limits-cache`.
4. Nếu chưa có: dùng `sed` để inject đoạn mã jq + bash ngay sau dòng `input=$(cat)`.
5. Nếu đã có: bỏ qua (idempotent).
6. Từ đó trở đi: đọc `~/.claude/rate-limits-cache.json` qua SSH bằng script [get-claudecode-usage.sh](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/get-claudecode-usage.sh).

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
- Khi người dùng kết nối, lệnh [provision_agent_usage](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/src-tauri/src/agent_usage.rs) sẽ tiêm (inject) một đoạn mã jq + bash thông minh vào script `statusline-command.sh` trên máy chủ từ xa thông qua [provision-claudecode.sh](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/provision-claudecode.sh).
- **Thuật toán thông minh:** Nếu luồng JSON của Claude đẩy ra *KHÔNG CÓ* `rate_limits` (do lỗi 429), script sẽ tự động chui vào file `rate-limits-cache.json` cũ, **COPY** lại mốc thời gian Reset, ép giá trị `% used = 100`, rồi hòa trộn (Merge) vào JSON mới để ghi ra file.
- Mọi dữ liệu xịn xò (session, cwd, tokens, v.v.) được giữ nguyên. UI không bao giờ bị sập thanh Progress.

### Luồng Chủ động (Active Flow - Nút Force Sync)
Bắt ép làm mới dữ liệu quota từ server mà không cần phát sinh lượt chat mới.
- Rust Backend ([agent_usage.rs](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/src-tauri/src/agent_usage.rs)) kích hoạt script [force-sync-claudecode.sh](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/force-sync-claudecode.sh):
  ```sh
  BLANK_DIR="/tmp/aki-dev-sync-blank-dir"
  mkdir -p "$BLANK_DIR"
  cd "$BLANK_DIR" && claude --model haiku -p /usage < /dev/null 2>/dev/null
  ```
- **Cơ chế thực tế (xác nhận 2026-06-23):** Lệnh `/usage` thực hiện **network call thực sự** đến Anthropic API, xác thực bằng OAuth token có sẵn trong `~/.claude/.credentials.json`. Không cần session đang mở, không đọc từ RAM — ~2 giây.
- **`< /dev/null`** là bắt buộc: nếu thiếu, Claude Code chờ stdin 3 giây không cần thiết trước khi xử lý.
- **Blank dir độc lập** (`/tmp/aki-dev-sync-blank-dir`): tránh dùng `/tmp` trực tiếp vì có thể có file bị nhặt làm project context; tạo mới nếu chưa tồn tại.
- Output (Stdout) có dạng chuẩn: `Current session: 3% used · resets Jun 22, 10:10pm (Asia/Singapore)`.
- Backend nhúng script [force-sync-parse.py](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/force-sync-parse.py) chạy inline:
  1. Dùng biểu thức chính quy (Regex) cắt lấy số `%` và `Chuỗi ngày giờ`.
  2. Dùng thư viện `datetime` chuyển đổi chuỗi chữ "Jun 22, 10:10pm" thành Unix Timestamp siêu chuẩn.
  3. Đọc file `rate-limits-cache.json` lên, **CHỂ GHI ĐÈ** cụm `rate_limits.five_hour` bằng `%` và `Timestamp` vừa tính. Ghi ngược file lại.
- **Giới hạn còn lại:** Nếu cache cũ có `resets_at` đã qua nhưng user chưa bấm Force Sync, UI vẫn hiển thị "Reset X ago" từ data cũ. Xem [claude-usage-1.2.7-analyze.md](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/docs/research/claude-usage-1.2.7-analyze.md) (Vấn đề B, C, D) để biết các hướng cải thiện tiếp theo.

> _Cập nhật 2026-06-23: Sửa lại mô tả cơ chế Active Flow. Thực nghiệm xác nhận `/usage` gọi network (OAuth), không đọc RAM. Bổ sung `< /dev/null` để loại 3s delay. Đổi `cd /tmp` thành blank dir riêng để đảm bảo context rỗng._

---

## 4. File Code Liên Quan (Related Source Files)

- **Backend / Scripts:**
  - [provision-claudecode.sh](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/provision-claudecode.sh) — Script patch statusline hook của Claude Code.
  - [get-claudecode-usage.sh](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/get-claudecode-usage.sh) — Script đọc file cache và detect stale reset.
  - [force-sync-claudecode.sh](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/force-sync-claudecode.sh) — Script kích hoạt live sync quota qua command `/usage`.
  - [force-sync-parse.py](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/force-sync-parse.py) — Python parsing dữ liệu stdout của `/usage`.
  - [agent_usage.rs](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/src-tauri/src/agent_usage.rs) — Tầng điều phối Tauri commands bên Rust.
- **Frontend:**
  - [useAgentUsage.js](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/src/composables/useAgentUsage.js) — Vue composable theo dõi & đồng bộ agent usage.
  - [AgentUsage.vue](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/src/components/AgentUsage.vue) — Component hiển thị card thông tin quota.
  - [UsageProgressBar.vue](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/src/components/UsageProgressBar.vue) — Component thanh progress và đếm ngược/tự kích hoạt sync.

---

## 5. Official References

- [StatusLine Documentation](https://code.claude.com/docs/en/statusline)
- [Claude Code Changelog](https://code.claude.com/docs/en/changelog)
- [Rate Limits Documentation](https://platform.claude.com/docs/en/api/rate-limits)
