# Statusline Customizer (Claude Code)

**Status:** Planned, not started
**Ngày:** 2026-07-09

---

## Bối cảnh

Qua nhiều vòng chỉnh tay trong 1 phiên chat, đã chốt được 1 bộ "quy chuẩn" statusline cho
`~/.claude/statusline-command.sh` (dùng cho Claude Code CLI, không phải app này) - xem file mẫu đã
hoàn thiện tại `/home/guest/.claude/statusline-command.sh` trên máy dev hiện tại. Việc chỉnh tay qua
chat rất chậm (phải mô tả bằng lời từng màu/từng field/từng dấu phân cách). User muốn đưa việc này
vào UI của Aki Dev Sync để tự chọn field, tự sắp thứ tự, tự đổi màu, rồi bấm 1 nút để "xả" cấu hình
ra tất cả các máy (local + các `remote_host` đã cấu hình trong app).

## Đã có sẵn trong codebase (tái dùng, không viết lại)

- **`src-tauri/src/claude_profile.rs`** - đã đọc/ghi `~/.claude/settings.json` (local). Statusline
  config cũng nằm trong file này (`statusLine.command`), nên field mới có thể theo đúng pattern
  `get_claude_mode()` / `set_claude_profile()` đã có.
- **`scripts/provision-claudecode.sh`** - **đã là cơ chế "patch file statusline" trên remote host**:
  patch `~/.claude/statusline-command.sh` để tiêm block `aki-rlcache v2`, **idempotent** (check marker
  `aki-rlcache v2` trước khi patch, tự dọn block v1 cũ nếu có). Gọi từ `provision()` trong
  `src/composables/useAgentUsage.js` (chạy 1 lần/host/session, fire-and-forget).
- **`src-tauri/src/sync.rs`** - có `rsync_pull_file()` và hạ tầng chạy lệnh qua SSH tới
  `project.remote_host`, dùng được để **push** file thay vì chỉ pull.
- **`src/components/modals/ClaudeProfileModal.vue`** - mẫu modal Claude Code hiện có (proxy config),
  dùng làm tham chiếu style/pattern cho modal mới. **`ClaudeSettingModal` chưa tồn tại - cần tạo mới**,
  đặt cạnh `ClaudeProfileModal.vue` trong `src/components/modals/`.

## Field catalog đã chốt (từ statusLine JSON schema chính thức)

41 field khả dụng, đã liệt kê đầy đủ trong lịch sử chat phiên này (`cwd`, `session_id`, `model.*`,
`workspace.*` - bao gồm `repo.host/owner/name` và `git_worktree` cho ai muốn thêm ô "git"  - ,
`cost.*`, `context_window.*`, `effort.level`, `thinking.enabled`, `rate_limits.*`, `pr.*`,
`worktree.*`). Không phải field nào cũng nên có mặt mặc định - nhưng UI phải cho phép bật bất kỳ field
nào trong danh sách này, kể cả các ô ít dùng như git branch/PR mà user đã chủ động nhắc tới ("trừ hao
cho người thích dùng thêm vài ô của git").

## Bản mẫu hiện tại (default preset), tham khảo file thật

```
guest@roscy | Aki-Dev-Sync | sonnet 5 med | ctx 72% 134.4k/1M | 5h:42%  7d:92% | 12m +122/-52 $1.23
```

Quy tắc màu đã chốt:
- Thang 4 bậc cho mọi số % có ngưỡng: green (<50%) / yellow (50-70%) / orange (70-85%, mã 256-màu
  `\033[01;38;5;208m`) / red (≥85%).
- Label tĩnh (không đổi theo giá trị) dùng màu cố định riêng: `@` trắng, `ctx`/`5h`/`7d` trắng,
  user/model cyan, host green, cwd blue, effort/dấu phân cách xám.
- Mọi cụm thông tin (nhóm) ngăn nhau bằng `" | "` màu xám - không dùng khoảng trắng đơn/đôi tùy
  tiện. Ngoại lệ: bên trong nhóm `5h`/`7d` (rate limits) chỉ dùng 2 space thường, không `" | "`.
- `%` context lấy thẳng từ `context_window.used_percentage` (không tự tính lại); breakdown `x/max`
  hiển thị **tổng input+output cộng gộp** - khớp với ý nghĩa của `used_percentage`, không tách riêng
  input/output ra 2 số như bản nháp trước đó (gây hiểu lầm vì output riêng lẻ không có ý nghĩa tích
  lũy - chỉ phản ánh lượt gần nhất). Không bọc ngoặc vuông `[...]` quanh breakdown - số đã khác màu
  (cyan) so với dấu `/` (xám) nên ngoặc là thừa.
- Mọi số `%` phải làm tròn qua `awk '{printf "%.0f", p}'` trước khi hiển thị - tránh lỗi nhiễu dấu
  phẩy động kiểu `41.999999998%` lọt ra màn hình.
- `1000000` token trở lên hiển thị `1M`/`1.5M` thay vì `1000.0k`.
- `effort.level` hiển thị viết tắt: 5 giá trị khả dĩ là `low` / `medium` / `high` / `xhigh` / `max`
  - chỉ `medium` cần rút gọn thành `med`, còn lại đã đủ ngắn.
- Nhóm cuối cùng trên dòng là "session": gộp session duration + lines `+/-` + cost làm 1 nhóm, theo
  thứ tự `12m +122/-52 $1.23` (duration trước, rồi lines, rồi cost) - không còn là 3 nhóm tách rời
  ngăn bởi `" | "` như bản cũ.

## Việc cần làm (session sau)

1. **Data model**: JSON config lưu trong `~/.claude/aki-statusline-config.json` (hoặc trong
   `settings.json` của app) mô tả: danh sách field đã bật (theo key trong catalog), thứ tự hiển thị
   (mảng có thứ tự), màu cho từng field/label, ngưỡng % (mặc định 50/70/85, cho sửa).
2. **Rust**: hàm `generate_statusline_script(config) -> String` sinh nội dung `.sh` từ config (không
   phải chạy `sed`/patch chuỗi thô như `provision-claudecode.sh` đang làm cho riêng block rate-limit  - 
   ở đây sinh toàn bộ file từ template Rust, ghi đè sạch). Giữ nguyên khối `aki-rlcache v2` bắt buộc
   luôn có mặt (không cho user tắt) vì usage-tracking của app phụ thuộc vào nó.
3. **Rust**: lệnh Tauri mới `apply_statusline_config(config, target_hosts: Vec<String>)` - ghi local
   `~/.claude/statusline-command.sh` + patch `statusLine.command` trong `settings.json` (dùng `$HOME`,
   không hardcode path - bài học từ phiên chat: máy khác nhau có home dir khác nhau), rồi với mỗi host
   trong `target_hosts`, rsync/scp file sinh ra lên `~/.claude/statusline-command.sh` trên remote (tái
   dùng cơ chế ssh hiện có trong `sync.rs`, không phải viết lại).
4. **Vue**: `src/components/modals/ClaudeSettingModal.vue` (mới) - theo pattern
   `ClaudeProfileModal.vue`:
   - Danh sách field dạng checkbox, có thể kéo-thả để đổi thứ tự (hoặc up/down đơn giản nếu drag-drop
     không đáng công).
   - Color picker/preset cho từng field hoặc nhóm field.
   - Input ngưỡng % (3 số: yellow/orange/red).
   - **Live preview** - render đúng màu ANSI thật trong 1 khối `<pre>` giả lập terminal (map mã ANSI
     sang CSS color, không cần lib ngoài - bảng màu ANSI cơ bản 16 màu + 256-màu index 208 là đủ).
   - Danh sách checkbox chọn host để "xả" cấu hình (local luôn có, + các `remote_host` đã cấu hình
     trong project list hiện tại của app).
   - Nút "Apply" gọi `apply_statusline_config`.
5. **Idempotency/an toàn khi patch remote**: theo đúng tinh thần `provision-claudecode.sh` - không
   ghi đè mù, phải backup file cũ (`.bak`) hoặc ít nhất log rõ trước/sau, để không phá statusline thủ
   công mà user có thể đã tự chỉnh trên 1 máy nào đó ngoài ý muốn.

## Việc KHÔNG làm (out of scope)

- Không hỗ trợ Windows native (không có `bash`/`jq`/`awk` mặc định) - chỉ macOS/Linux, đã xác nhận
  trong chat.
- Không đổi cơ chế `aki-rlcache v2` hiện có - chỉ tái sử dụng, không refactor lại
  `provision-claudecode.sh`.

## Câu hỏi mở cho session triển khai

- Field "git" (workspace.repo, git_worktree, pr.*) - cần xác nhận với user format hiển thị mong muốn
  (chưa được chốt màu/vị trí trong phiên này, chỉ mới được nhắc tên).
- Config lưu ở đâu là chuẩn: file riêng trong `~/.claude/` hay trong app's own local storage (rồi mỗi
  lần Apply mới ghi ra `.sh`)? Đề xuất: lưu trong app (Tauri store/local file của Aki Dev Sync), vì
  đây là "nguồn sự thật" để tái sinh script - không nên parse ngược lại từ `.sh` đã sinh.
