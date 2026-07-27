# Tham chiếu: Danh mục Đường dẫn & Quy trình Dọn dẹp Claude Code CLI

> Tài liệu tổng hợp danh mục đường dẫn, mục đích và tác động khi dọn dẹp các tàn dư của **Claude Code CLI** trên hệ thống (macOS / POSIX).
> Dùng làm tài liệu chuẩn cho việc thiết kế tính năng dọn dẹp (Clean / Reset) trên Aki-Dev-Sync.

> **Hai đính chính sau khi đối chiếu với cây thư mục thật (2026-07-27, Claude Code bản đang chạy):**
> 1. **`~/.claude/projects/` không được xử lý như một khối.** Mỗi `projects/<slug>/memory/` chứa file memory do agent ghi - là nội dung, không phải transcript. Bảng ở §2 nói nhóm này "chỉ mất lịch sử chat" là sai. Trong app, hai thứ này là hai mục riêng ở hai nhóm riêng: xoá transcript thì xoá quanh `memory/`, còn memory có mục của chính nó (vẫn xoá được, chỉ là không bao giờ bị xoá kèm).
> 2. **Danh mục dưới đây không đủ.** Cây thật còn `sessions/`, `daemon/`, `daemon.lock`, `daemon.status.json`, `chrome/`, `ide/`, `jobs/`, `debug/`, `downloads/`, `config.json`, `settings.local.json`, `rate-limits-cache.json` - CLI sinh thêm state nhanh hơn tốc độ cập nhật một danh sách viết tay. Vì vậy tính năng trong app **mặc định giữ lại** mọi thứ không nằm trong danh mục cứng của nó, chứ không suy đoán.
>
> Triển khai: `docs/feat/claudecode-cleanup.md`.

---

## 1. Nhóm A: Thông tin Tài khoản & Phiên đăng nhập (Account & Auth Credentials)

Các file lưu trữ định danh tài khoản, OAuth token và thông tin gói cước. Xóa nhóm này sẽ đưa Claude Code về trạng thái **chưa đăng nhập** (như vừa mới cài đặt).

| Đường dẫn | Loại | Mục đích & Nội dung | Tác động khi xóa |
|---|---|---|---|
| `~/.claude.json` | File JSON | Chứa OAuth Token (`accessToken`, `refreshToken`), `accountUuid`, `email`, `organizationRateLimitTier` | Đăng xuất tài khoản ngay lập tức. Cần chạy `claude auth login` để dùng lại. |
| `~/.claude/auth-cache.json` | File JSON | Cache thông tin gói cước (`email`, `orgId`, `subscriptionType`, TTL 300s) | Mất cache auth, hệ thống sẽ probe lại khi có lượt dùng mới. |
| `~/.claude/.credentials.json` | File JSON | Credentials cũ (Legacy auth token trên các bản Claude Code trước) | Không ảnh hưởng đến bản Claude Code mới (vốn dùng OS Keychain). |
| `~/.claude/stats-cache.json` | File JSON | Cache thống kê hạn ngạch và lượt sử dụng tài khoản | Reset thống kê hiển thị tạm thời trên CLI. |
| `~/.claude/daemon-auth-status.json` | File JSON | Trạng thái đăng nhập của tiến trình background daemon | Daemon sẽ kiểm tra lại trạng thái auth ở lần khởi chạy tới. |
| `~/.claude/daemon-auth-cooldown` | File | File đánh dấu cooldown khi daemon gặp lỗi auth | Reset thời gian chờ của daemon. |

---

## 2. Nhóm B: Lịch sử Hội thoại & Nhật ký Dự án (Transcripts & History)

Chứa toàn bộ dữ liệu làm việc cũ, nhật ký trao đổi, bản lưu file sửa đổi và lệnh đã nhập.

| Đường dẫn | Loại | Dung lượng thường gặp | Mục đích & Nội dung | Tác động khi xóa |
|---|---|---|---|---|
| `~/.claude/projects/` | Thư mục | Rất lớn (hàng trăm MB) | Lưu chi tiết từng hội thoại (transcript), log tool calls theo từng thư mục dự án. **Kèm `<slug>/memory/` - file memory do agent ghi, thuộc nhóm bảo vệ ở §4** | Xóa lịch sử chat cũ của tất cả dự án. Giải phóng dung lượng lớn nhất. **Phải chừa `memory/` lại** - xoá cả thư mục là mất luôn memory. |
| `~/.claude/history.jsonl` | File JSONL | 1 MB - 50 MB | Nhật ký danh sách các câu lệnh prompt người dùng đã nhập | Xóa lịch sử gợi ý lệnh cũ (prompt history). |
| `~/.claude/file-history/` | Thư mục | 50 MB - 500 MB | Bản lưu snapshot các file trước khi Claude Code sửa đổi | Mất khả năng undo / restore file về trạng thái trước khi Claude sửa. |
| `~/.claude/paste-cache/` | Thư mục | < 1 MB | Cache nội dung văn bản / code người dùng dán (paste) vào CLI | Không ảnh hưởng hoạt động. |
| `~/.claude/plans/` | Thư mục | < 1 MB | Lưu trữ các file plan (`.md`) đã được khởi tạo trong các phiên | Mất các file plan tạm thời đã lưu. |
| `~/.claude/session-env/` | Thư mục | < 1 MB | Lưu biến môi trường (environment variables) theo từng session | Reset trạng thái môi trường phiên cũ. |
| `~/.claude/tasks/` | Thư mục | < 1 MB | Dữ liệu và output của các background task | Clears background task histories. |
| `~/.claude/shell-snapshots/` | Thư mục | < 1 MB | Snapshot môi trường subshell khi chạy terminal command | Reset snapshot shell. |

---

## 3. Nhóm C: Cache, Telemetry & Plugin (Caches & Ephemeral Data)

Các dữ liệu tạm thời sinh ra trong quá trình chạy ứng dụng.

| Đường dẫn | Loại | Mục đích & Nội dung | Tác động khi xóa |
|---|---|---|---|
| `~/.claude/cache/` | Thư mục | Cache tạm của ứng dụng CLI | An toàn. Tự sinh lại khi cần. |
| `~/.claude/backups/` | Thư mục | Các bản sao lưu tự động của hệ thống | An toàn. |
| `~/.claude/plugins/` | Thư mục | Cache thực thi của plugin (như `rust-analyzer-lsp`) | Tự động tải/khởi tạo lại plugin khi cần. |
| `~/.claude/telemetry/` | Thư mục | Log chẩn đoán và telemetry gửi về Anthropic | An toàn. |
| `~/.claude/daemon.log` | File text | Log ghi chép hoạt động daemon | An toàn. |
| `~/Library/Caches/claude-code/` | Thư mục | Cache cấp OS hệ điều hành | An toàn. |

---

## 4. 🛡️ Danh mục BẢO VỆ TUYỆT ĐỐI (Protected System Paths)

Các file/thư mục thuộc hệ sinh thái quy tắc (Aki Rules / Claude Extensions). **KHÔNG ĐƯỢC XÓA** khi dọn dẹp tàn dư Claude Code.

| Đường dẫn | Loại | Lý do bảo vệ |
|---|---|---|
| `~/.claude/CLAUDE.local.md` | File Markdown | File chứa cấu hình/hướng dẫn dành riêng cho máy local. |
| `~/.claude/skills/` | Thư mục | Chứa bộ kịch bản/kỹ năng mở rộng (skills) của hệ thống. |
| `~/.claude/CLAUDE.md` | File Markdown | File liên kết quy tắc hệ sinh thái (`akirule`). |
| `~/.claude/*.aki*` | Files | Các bản backup quy tắc hệ thống (`.akiclaudedoc-backup`, `.akidevrule-backup`). |
| `~/.claude/settings.json` | File JSON | Chứa danh sách quyền (permissions), cài đặt theme và khai báo statusLine hook. |
| `~/.claude/statusline-command.sh` | Shell script | Script thu thập và ghi cache quota rate limit cho Aki-Dev-Sync. |
| `~/.claude/hooks/` | Thư mục | Chứa các hook tự động (như `aki-update-check.py`). |
| `~/.claude/projects/*/memory/` | Thư mục | Memory bền vững do agent ghi - là nội dung, không phải transcript. Nằm bên trong `projects/` nên rất dễ bị xoá nhầm theo cả nhóm B. **Không thuộc nhóm cấm xoá** - vẫn xoá được, nhưng phải là một thao tác riêng, không đi kèm việc xoá lịch sử chat. |
| `~/.claude/settings.local.json` | File JSON | Cài đặt riêng theo máy, không đồng bộ. |
| `~/.claude/config.json` | File JSON | Cấu hình CLI (theme, trạng thái onboarding). |

---

## 5. Quy trình Dọn dẹp Chuẩn (Standard Cleanup Procedure)

### Kịch bản A: Reset như mới (Full Clean)
* **Mục tiêu**: Xóa sạch tài khoản + lịch sử + cache, giữ nguyên bộ Skill & Rule.
* **Các file/thư mục cần xóa**:
  ```bash
  rm -f ~/.claude.json
  rm -f ~/.claude/.credentials.json
  rm -f ~/.claude/auth-cache.json
  rm -f ~/.claude/stats-cache.json
  rm -f ~/.claude/daemon-auth-*
  # KHÔNG dùng `rm -rf ~/.claude/projects` - xoá quanh memory/:
  find ~/.claude/projects -mindepth 2 -maxdepth 2 ! -name memory -exec rm -rf {} +
  rm -f ~/.claude/history.jsonl
  rm -rf ~/.claude/file-history
  rm -rf ~/.claude/paste-cache
  rm -rf ~/.claude/plans
  rm -rf ~/.claude/session-env
  rm -rf ~/.claude/tasks
  rm -rf ~/.claude/shell-snapshots
  rm -rf ~/.claude/cache
  rm -rf ~/.claude/backups
  rm -rf ~/.claude/telemetry
  rm -f ~/.claude/daemon.log
  ```

### Kịch bản B: Chỉ xóa Lịch sử & Dọn dẹp Dung lượng (Keep Auth)
* **Mục tiêu**: Giải phóng dung lượng ổ đĩa, giữ nguyên trạng thái đã đăng nhập tài khoản.
* **Chỉ xóa**: `projects/` (chừa `memory/`), `history.jsonl`, `file-history/`, `paste-cache/`, `cache/`.
