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
~/.claude/auth-cache.json            → Auth info cache (email, orgName) — do provision tạo một lần/session
~/.claude/projects/**/*.jsonl        → Transcript files (fallback ước lượng token nếu cần)
```

### Self-Provisioning Logic

Vì dữ liệu thật chỉ tồn tại thoáng qua trong `stdin` khi `statusLine` script chạy, chúng ta phải persist nó:

1. SSH vào remote host và patch `statusline-command.sh` bằng script [provision-claudecode.sh](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/provision-claudecode.sh).
2. Đọc `~/.claude/statusline-command.sh`.
3. Kiểm tra chuỗi `rate-limits-cache`.
4. Nếu chưa có: dùng `sed` để inject đoạn mã jq + bash ngay sau dòng `input=$(cat)`. File tạm `/tmp/patch.sh` được dọn dẹp qua `trap EXIT`; file `.bak` của `sed -i.bak` cũng được xóa ngay.
5. Nếu đã có: bỏ qua (idempotent).
6. **Auth Info (v1.3.0):** Cuối `provision-claudecode.sh`, chạy `bash -lc 'claude auth status'` (login shell đảm bảo PATH có `claude`), ghi kết quả JSON vào `~/.claude/auth-cache.json`. Chạy một lần mỗi host session.
7. Từ đó trở đi: đọc `~/.claude/rate-limits-cache.json` qua SSH bằng script [get-claudecode-usage.sh](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/get-claudecode-usage.sh).

---

## 2. Vấn đề giới hạn kỹ thuật (Technical Limitations)

Trong quá trình sử dụng thực tế, chúng ta đã phát hiện 2 điểm mù (Blind Spots) của Claude CLI và API Anthropic:

### Lỗi A: Hội chứng "Mất tích" Rate Limits (Khi chạm nóc 100%)
Khi người dùng dùng cạn kiệt Quota, API của Anthropic sẽ trả về lỗi HTTP `429 Too Many Requests`. Thay vì truyền nguyên vẹn gói JSON cũ với thông báo 100%, Claude Code CLI lại tự ý **CẮT BỎ HOÀN TOÀN** cục `rate_limits` ra khỏi luồng JSON truyền vào `statusLine`.
- **Hệ quả:** File cache bị mất key `rate_limits`, UI của App không đọc được % và thời gian reset, dẫn đến việc thanh Progress Bar biến mất hoàn toàn.

### Lỗi B: Sự cố đồng bộ Quota khi chạy ngầm qua SSH
App chạy ngầm `claude --model haiku -p /usage` để cố lấy thông tin quota. Trong thực tế triển khai, lệnh này đã gặp phải 2 sự cố kỹ thuật:
- **Trễ 3 giây do thiếu `< /dev/null`:** Khi chạy qua SSH subshell không phải TTY, Claude CLI sẽ chờ nhập liệu từ stdin trong 3 giây trước khi xử lý tiếp, in cảnh báo ra stderr. Do đó, quá trình đồng bộ bị trễ từ ~2s lên hơn 5s.
- **Parser bị sập khi Quota Reset (0% used):** Khi quota reset hoàn toàn (hoặc chưa tiêu tốn token nào), output của `/usage` trả về `Current session: 0% used` và **không kèm theo** mốc thời gian reset kế tiếp. Do regex parser cũ quá cứng nhắc, việc không khớp chuỗi thời gian dẫn đến lỗi parse, khiến file cache không được cập nhật. Kết hợp với logic vô hiệu hóa cache cũ khi qua giờ reset, UI sẽ bị kẹt vĩnh viễn ở trạng thái "No data - waiting for next session".

**⚠️ Giới hạn quan trọng của `/usage` (xác nhận 2026-06-24):** Lệnh `/usage` **KHÔNG** thực hiện network call đến Anthropic API. Output của nó ghi rõ: *"Approximate, based on local sessions on this machine — does not include other devices or claude.ai"*. Tức là nó **chỉ đọc file JSONL local** (`~/.claude/projects/**/*.jsonl`) rồi tính toán locally — là **họ P2**, không phải P3. Kết quả:
- Chỉ phản ánh session trên chính máy `bien`, không phản ánh tài khoản tổng thể.
- Nếu người dùng khác dùng Claude Code trên máy khác, `/usage` trên `bien` **không thay đổi**.
- `0% used` + không có `resets_at` là **chính xác** khi không có session local nào trong 5 giờ qua, dù tài khoản thực tế có thể đã dùng nhiều trên thiết bị khác.


---

## 3. Kiến trúc Giải pháp: "Hybrid Patching"

Để vượt qua các giới hạn trên, Aki Dev Sync áp dụng giải pháp **Hybrid Patching** (Vá thông minh kết hợp 2 luồng) được xử lý ngay từ tầng Rust Backend bằng Python và Bash script qua SSH.

### Luồng Bị động (Passive Flow - Quá trình Provision)
Bắt trọn mọi hoạt động chat của User và đề phòng Lỗi A.
- Khi người dùng kết nối, lệnh [provision_agent_usage](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/src-tauri/src/agent_usage.rs) sẽ tiêm (inject) một đoạn mã jq + bash thông minh vào script `statusline-command.sh` trên máy chủ từ xa thông qua [provision-claudecode.sh](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/provision-claudecode.sh).
- **Thuật toán thông minh:** Nếu luồng JSON của Claude đẩy ra *KHÔNG CÓ* `rate_limits` (do lỗi 429), script sẽ tự động chui vào file `rate-limits-cache.json` cũ, **COPY** lại mốc thời gian Reset, ép giá trị `% used = 100`, rồi hòa trộn (Merge) vào JSON mới để ghi ra file.
- Mọi dữ liệu xịn xò (session, cwd, tokens, v.v.) được giữ nguyên. UI không bao giờ bị sập thanh Progress.

### Luồng Chủ động (Active Flow - Nút Force Sync)
Cố gắng làm mới thông tin quota bằng cách chạy `/usage` trên remote.
- Rust Backend ([agent_usage.rs](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/src-tauri/src/agent_usage.rs)) kích hoạt script [force-sync-claudecode.sh](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/force-sync-claudecode.sh):
  - **Cơ chế tự động Probe Session (v1.2.9, cải tiến v1.3.2):** Lệnh `/usage` (họ P2) chỉ đọc local JSONL logs. Nếu trong 5h qua không có local session nào, nó sẽ báo `0% used` và **không hiển thị** mốc `resets_at`. Probe được kích hoạt trong **hai trường hợp**: (1) output không có từ khóa `resets` — không có session local nào trong window hiện tại; (2) output có `resets` nhưng mốc thời gian đã qua — `/usage` echo lại `resets_at` cũ từ `rate-limits-cache.json` (xảy ra ngay sau quota reset khi cache chưa được làm mới). Trường hợp (2) là lý do UI bị stuck "No data" sau reset dù STALE_RESET auto-recovery đã fire. Probe chạy `claude --model haiku -p "respond with ok" < /dev/null` → statusLine hook fire → ghi `resets_at` mới vào cache → `/usage` lần 2 hiện mốc future. Kiểm tra thực hiện bằng Python inline ngay trong script.
- **⚠️ Cơ chế thực tế của `/usage`:** Lệnh này **KHÔNG** gọi Anthropic API để lấy quota tài khoản tổng thể, nó **chỉ đọc local JSONL files** (`~/.claude/projects/**/*.jsonl`) rồi tính toán offline. Do đó, nó phản ánh chính xác mốc reset của chu kỳ hiện tại mà thiết bị này tham gia, nhưng không bao gồm hoạt động trên thiết bị khác.
- **Các kịch bản kích hoạt luồng Force Sync:**
  1. **Khởi chạy ứng dụng (App Startup) / Thay đổi Host (Host Change):** Khi người dùng mở app hoặc chuyển sang host khác, hệ thống sẽ đọc cache cục bộ trên remote. Nếu cache chưa tồn tại hoặc chứa `resets_at = 0` (chưa có session nào để tính toán mốc reset), hệ thống sẽ **tự động gọi Force Sync** một lần để chạy probe session và điền mốc reset chuẩn xác.
  2. **Khi mốc reset hiện tại đã qua (Timeout / Reset Time Reached):** Bộ đếm thời gian countdown của UI sẽ tự động kích hoạt Force Sync để cập nhật hạn mức cho chu kỳ mới.
  3. **STALE_RESET Auto-Recovery (v1.3.0):** Khi `get-claudecode-usage.sh` phát hiện cache đã qua `resets_at`, nó trả về `|||STALE_RESET|||` → JS nhận `null` → `data.value = null`. Nếu trước đó `data` đang có giá trị (transition từ data → null), `useAgentUsage.js` tự động kích hoạt Force Sync một lần duy nhất, được bảo vệ bởi cờ `staleResetSyncDone`. Cờ này reset về `false` khi data trở lại, đảm bảo chu kỳ tiếp theo vẫn tự phục hồi.
  4. **Yêu cầu thủ công của người dùng:**
     - Người dùng bấm nút **Reload** trên thanh tiêu đề ứng dụng (App Header).
     - Người dùng bấm nút **Refresh** (icon xoay màu trắng) ở góc phải của thẻ Claude Code card.
     - Người dùng bấm nút **Force Sync** ở trạng thái card trống hoặc ở chân progress bar (khi mốc reset cũ đã qua).
- **`< /dev/null`** là bắt buộc: nếu thiếu, Claude Code chờ stdin 3 giây không cần thiết trước khi xử lý.
- **Blank dir độc lập** (`/tmp/aki-dev-sync-blank-dir`): tránh dùng `/tmp` trực tiếp vì có thể có file bị nhặt làm project context; tạo mới nếu chưa tồn tại.
- **Concurrency guard (v1.3.0/v1.3.1):** `useAgentUsage.js` dùng hai cờ độc lập:
  - `isSyncing` — bảo vệ `forceSync()`: chỉ một lần force sync chạy tại một thời điểm.
  - `isChecking` — bảo vệ `checkUsage()`: ngăn nhiều poll tick hoặc `manualRefreshCount` watch trigger đồng thời. Cả hai cờ reset khi host thay đổi.
- **JSONL cleanup (v1.3.0, rút ngắn v1.3.2):** Cuối mỗi lần chạy, `force-sync-claudecode.sh` dọn dẹp các file JSONL trong BLANK_DIR project folder và thư mục probe orphan (`-tmp-aki-probe-*`) có tuổi trên **1 ngày**. Các file này chỉ cần tồn tại cho đúng một `/usage` call ngay sau khi tạo ra, không cần giữ lại lâu hơn.
- **Shell safety (v1.3.1):** `get-claudecode-usage.sh` có `set -e` — parse Python fail sẽ abort thay vì truyền data rỗng. `force-sync-claudecode.sh` có `set -o pipefail`. `auth-cache.json` được validate JSON qua `python3` trước khi dùng — file bị truncate/corrupt sẽ fallback `{}`.
- Output (Stdout) có dạng: `Current session: 3% used · resets Jun 22, 10:10pm (Asia/Singapore)` (khi có session) hoặc `Current session: 0% used` (khi không có session local trong 5h qua).

### Delimiter Chain trong `get-claudecode-usage.sh`

Script ghép nhiều thông tin vào một stdout stream qua các delimiter riêng biệt để Rust parse:

```
<nội dung rate-limits-cache.json>
|||MTIME|||<unix timestamp của file>
|||SUBTYPE|||<subscriptionType từ .credentials.json>
|||TIER|||<rateLimitTier từ .credentials.json>
|||AUTHINFO|||<nội dung auth-cache.json (JSON: email, orgName, ...)>
```

Rust (`agent_usage.rs`) split tuần tự theo từng delimiter, parse `|||AUTHINFO|||` thành `serde_json::Value`, rồi inject `email` và `orgName` (nếu có, nếu non-empty) vào payload JSON trả về frontend. `orgName` tự động bị suppressed nếu trùng với pattern `"<email>'s Organization"` (Anthropic default) — logic này nằm ở computed `ccOrgName` trong `AgentUsage.vue`.

- Backend nhúng script [force-sync-parse.py](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/force-sync-parse.py) chạy inline:
  1. Dùng Regex cắt lấy số `%` (bắt buộc) và chuỗi ngày giờ (tùy chọn).
  2. Dùng thư viện `datetime` chuyển đổi "Jun 22, 10:10pm" thành Unix Timestamp.
  3. Ghi đè cụm `rate_limits.five_hour` trong cache. Nếu không có `resets_at`, ghi `resets_at: 0`.

> _Cập nhật 2026-06-23: Bổ sung `< /dev/null` để loại 3s delay. Đổi `cd /tmp` thành blank dir riêng để đảm bảo context rỗng._
> _Cập nhật 2026-06-24 (v1.2.9): Sửa lỗi parser regex thất bại khi quota được reset hoàn toàn (0% used). Tích hợp cơ chế tự động chạy "Probe Session" (dummy session haiku) khi `/usage` thiếu mốc thời gian reset. Tài liệu hóa các kịch bản kích hoạt luồng Force Sync._
> _Cập nhật 2026-06-24 (v1.3.0): Bổ sung STALE_RESET auto-recovery (trigger #3 mới), concurrency guard `isSyncing`, và JSONL cleanup cho BLANK_DIR + probe orphan dirs. Bổ sung CC Auth Info Pipeline: `provision-claudecode.sh` → `auth-cache.json` → `|||AUTHINFO|||` delimiter → Rust inject `email`/`orgName` vào payload._
> _Cập nhật 2026-06-24 (v1.3.1): Thêm `isChecking` guard cho `checkUsage()`. Shell safety: `set -e` + `set -o pipefail`, JSON validation của `auth-cache.json`, temp file cleanup trong `provision-claudecode.sh`._
> _Cập nhật 2026-06-25 (v1.3.2): Fix probe bị bypass sau quota reset — `/usage` echo lại `resets_at` cũ (past) từ cache, `grep -q "resets"` pass nên probe không fire, Python ghi lại past timestamp, UI stuck vô thời hạn. Fix bằng Python inline check verify reset time phải là future. Rút ngắn JSONL cleanup từ 7 ngày xuống 1 ngày._

---

## 4. File Code Liên Quan (Related Source Files)

- **Backend / Scripts:**
  - [provision-claudecode.sh](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/provision-claudecode.sh) — Script patch statusline hook của Claude Code.
  - [get-claudecode-usage.sh](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/get-claudecode-usage.sh) — Script đọc file cache và detect stale reset.
  - [force-sync-claudecode.sh](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/force-sync-claudecode.sh) — Script kích hoạt live sync quota qua command `/usage`.
  - [force-sync-parse.py](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/force-sync-parse.py) — Python parsing dữ liệu stdout của `/usage`.
  - [agent_usage.rs](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/src-tauri/src/agent_usage.rs) — Tầng điều phối Tauri commands bên Rust.
- **Frontend:**
  - [useAgentUsage.js](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/src/composables/useAgentUsage.js) — Vue composable theo dõi & đồng bộ agent usage. Guards: `isSyncing` (forceSync), `isChecking` (checkUsage).
  - [AgentUsageSection.vue](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/src/components/AgentUsageSection.vue) — Layout LOCAL/REMOTE split, eye-toggle email per column, pass `selectedSshHost` computed từ `useSsh.js`.
  - [AgentUsage.vue](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/src/components/AgentUsage.vue) — Card hiển thị quota. CC header: tier badge, email, orgName (suppresses Anthropic default). AG header: email full. Dùng `RefreshRing` cho countdown reload.
  - [UsageCircle.vue](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/src/components/UsageCircle.vue) — SVG radial progress circle với tooltip reset time (dùng cho Antigravity buckets).
  - [RefreshRing.vue](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/src/components/RefreshRing.vue) — SVG `stroke-dashoffset` countdown ring tái dùng được. Hai mode: `overlay` (position absolute trên button, dùng trong AgentUsage) và `inline` (16px trong flex row, dùng trong ProjectTable header cho git/diff timer).
  - ~~UsageProgressBar.vue~~ — Đã xóa ở v1.3.0, thay bởi UsageCircle + CC horizontal bars.

---

## 5. Official References

- [StatusLine Documentation](https://code.claude.com/docs/en/statusline)
- [Claude Code Changelog](https://code.claude.com/docs/en/changelog)
- [Rate Limits Documentation](https://platform.claude.com/docs/en/api/rate-limits)
