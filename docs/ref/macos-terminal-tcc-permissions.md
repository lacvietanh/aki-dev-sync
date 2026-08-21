# macOS TCC Permissions & In-App Terminal CLI Architecture

Tài liệu tham chiếu về kiến trúc bảo mật macOS TCC (Transparency, Consent, and Control) áp dụng cho Aki Dev Sync khi khởi tạo tiến trình con trong PTY / Terminal (`/bin/zsh`, `claude`, `agy`, `find`, `fd`, `rg`).

---

## 1. Bản chất kiến trúc macOS TCC đối với ứng dụng Desktop

macOS quản lý quyền truy cập của ứng dụng đối với tài nguyên nhạy cảm (Filesystem, Microphone, Camera, Accessibility, v.v.) thông qua daemon bảo mật `tccd` và cơ sở dữ liệu TCC (`~/Library/Application Support/com.apple.TCC/TCC.db` cho cấp user và `/Library/Application Support/com.apple.TCC/TCC.db` cho cấp system).

Khi Aki Dev Sync (Tauri v2 desktop app, bundle ID `aki.devsync`) mở một tab terminal nhúng hoặc khởi chạy CLI agent:
1. **Spawn PTY Process**: Ứng dụng tạo một pseudo-terminal session và fork/exec `/bin/zsh` (login shell).
2. **Process Tree & Attribution**: Khi người dùng hoặc agent chạy một công cụ tìm kiếm/quét file (`find`, `fd`, `rg`, `ls`) từ terminal tab, tiến trình CLI đó là con (child process) của zsh, và zsh là con của binary Aki Dev Sync.
3. **Responsible Process Attribution**: Khi một tiến trình con thực hiện system call truy cập tệp (`open`, `opendir`, `getattrlist`) vào một thư mục được bảo vệ, kernel và `tccd` không chỉ kiểm tra bản thân binary CLI con mà truy vết ngược cây tiến trình (process tree) để xác định **Responsible Process** (tiến trình chịu trách nhiệm gốc — ở đây là `aki.devsync`).
4. **Trigger Prompt**: Nếu Responsible Process chưa có quyền hợp lệ trong cơ sở dữ liệu TCC cho danh mục tài nguyên đó, `tccd` sẽ tạm dừng luồng thực thi và kích hoạt pop-up hộp thoại hệ thống yêu cầu người dùng phê duyệt quyền.

---

## 2. 3 tầng phân quyền của macOS TCC

macOS phân cấp bảo vệ hệ thống tệp thành các tầng độc lập với cơ chế kiểm soát khác nhau:

| Tầng phân quyền | TCC Service Identifier | Bản chất & Phạm vi truy cập |
|---|---|---|
| **Full Disk Access (FDA)** | `kTCCServiceSystemPolicyAllFiles` | Cho phép ứng dụng đọc/ghi hầu hết hệ thống tệp, bao gồm dữ liệu nhạy cảm của hệ thống và ứng dụng khác (Mail, Messages, Safari history, Time Machine backups). Được quản lý tập trung trong `System Settings > Privacy & Security > Full Disk Access`. |
| **Protected User Folders (Files and Folders)** | `kTCCServiceSystemPolicyDocumentsFolder`<br>`kTCCServiceSystemPolicyDesktopFolder`<br>`kTCCServiceSystemPolicyDownloadsFolder`<br>`kTCCServiceSystemPolicyNetworkVolumes` | Cơ chế bảo vệ danh mục riêng theo thư mục cá nhân người dùng (User-Consent per directory). Mỗi thư mục (`~/Documents`, `~/Desktop`, `~/Downloads`, Network/Removable Volumes) có một cờ kiểm soát riêng trong `System Settings > Privacy & Security > Files and Folders`. |
| **Developer Tools** | `kTCCServiceDeveloperTool` | Quyền chuyên dụng dành cho IDE, Terminal emulators và công cụ phát triển phần mềm. Khi được cấp quyền này, mọi tiến trình con được spawn bởi ứng dụng sẽ tự động kế thừa quyền truy cập môi trường làm việc và filesystem mà không bị `tccd` ngắt quãng bằng pop-up prompt. |

---

## 3. Tại sao chỉ cấp Full Disk Access vẫn có thể bị popup khi quét vào `~/Documents`?

Nhiều trường hợp ứng dụng đã được gán **Full Disk Access** nhưng khi terminal bên trong app quét vào `~/Documents` hoặc `~/Desktop` vẫn xuất hiện dialog hỏi quyền. Nguyên nhân kỹ thuật:

1. **Phân tách cơ chế giữa FDA và Protected Folders**: Trên các phiên bản macOS hiện đại (Sonoma, Sequoia), các danh mục `SystemPolicy*Folders` vận hành như các chốt chặn User-Consent độc lập. Đối với các binary đang trong giai đoạn dev (ad-hoc signed, self-signed hoặc chưa notarized qua Apple Developer ID), `tccd` ưu tiên xác thực chính sách User-Consent trực tiếp nếu chưa có bản ghi tường minh cho bundle ID đó trong `kTCCServiceSystemPolicyDocumentsFolder`.
2. **Pseudoterminal Session Boundary**: Tiến trình con chạy bên trong PTY session đôi khi được gán attribution theo ngữ cảnh shell session. Nếu kernel đánh giá lệnh gọi xuất phát từ interactive shell chưa được gán quyền Developer Tool, yêu cầu truy cập thư mục người dùng vẫn kích hoạt TCC prompt.
3. **Unscoped Wildcard Traversal**: Khi một lệnh quét đệ quy vô tình duyệt qua các thư mục được bảo vệ (ví dụ: quét từ `$HOME`), bất kỳ thư mục con nào chưa có bản ghi `ALLOW` cụ thể sẽ lập tức kích hoạt prompt tương ứng.

---

## 4. Các phương án xử lý dứt điểm

### Phương án 1: Cấp quyền Developer Tools (Khuyên dùng cho IDE / Terminal)
Đây là phương án chuẩn nhất của macOS dành cho các terminal emulator và công cụ phát triển phần mềm:
1. Mở **System Settings** > **Privacy & Security** > **Developer Tools**.
2. Tìm **Aki Dev Sync** trong danh sách và bật toggle sang **ON**.
3. Nếu ứng dụng chưa có trong danh sách, bấm biểu tượng **`+`**, duyệt đến file bundle của ứng dụng (ví dụ `/Applications/Aki Dev Sync.app` hoặc binary dev build) để thêm vào.

> Sau khi bật Developer Tools, toàn bộ CLI tools (`claude`, `agy`, `fd`, `find`, `git`) chạy trong terminal tab sẽ thực thi trơn tru mà không bị TCC can thiệp.

### Phương án 2: Xác nhận Allow trực tiếp 1 lần
Khi macOS hiển thị pop-up `"Aki Dev Sync" would like to access files in your Documents folder`:
- Bấm nút **Allow**.
- macOS sẽ tự động ghi nhận quyền này vào **System Settings** > **Privacy & Security** > **Files and Folders** > **Aki Dev Sync** (`Documents Folder: ON`).
- Quyết định này được lưu vĩnh viễn cho bundle ID hiện tại cho đến khi bị reset.

### Phương án 3: Reset TCC Cache khi Rebuild Binary bị lệch Code Signature
Trong quá trình phát triển (development), mỗi lần rebuild binary với ad-hoc signing mới, mã băm (Code Directory hash / Designated Requirement) của binary có thể thay đổi khiến `tccd` coi đây là một thực thể khác hoặc giữ bản ghi quyền cũ bị xung đột.

Chạy lệnh terminal sau để reset toàn bộ trạng thái TCC của ứng dụng:
```sh
tccutil reset All aki.devsync
```

Hoặc reset chọn lọc từng quyền cụ thể:
```sh
# Reset quyền Full Disk Access
tccutil reset SystemPolicyAllFiles aki.devsync

# Reset quyền truy cập thư mục Documents
tccutil reset SystemPolicyDocumentsFolder aki.devsync

# Reset quyền Developer Tools
tccutil reset DeveloperTool aki.devsync
```
Sau khi reset, khởi động lại ứng dụng và cấp lại quyền trong System Settings.

---

## 5. Kỷ luật Scoping khi thực thi lệnh tìm kiếm trong Terminal & Agent

Việc xuất hiện TCC pop-up thường là triệu chứng của việc chạy lệnh tìm kiếm thiếu scoping rõ ràng. Tuân thủ các nguyên tắc sau:

1. **Luôn gắn với Workspace / Working Directory (`cwd`)**:
   - ❌ **Không quét từ root hoặc thư mục cá nhân**: Tránh các lệnh như `find ~ -name "*.rs"`, `fd keyword /Users/aki/`, `grep -r "pattern" $HOME/`.
   - ✅ **Chỉ quét bên trong project directory**: Chạy lệnh tương đối `./` hoặc tuyệt đối trong thư mục project (ví dụ: `/Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/` hoặc `~/Projects/my-app/`).
2. **Cấu hình Exclude danh mục nhạy cảm**:
   - Khi buộc phải tìm kiếm ở cấp thư mục cha, luôn truyền tham số loại trừ các thư mục macOS đặc thù:
     ```sh
     fd --exclude "Documents" --exclude "Desktop" --exclude "Downloads" --exclude "Library" <pattern> <path>
     ```
3. **Ràng buộc Agent Context (Claude Code / AGY CLI)**:
   - Các subagent hoặc CLI tool phải luôn được cung cấp đường dẫn tuyệt đối hoặc tương đối của workspace hiện tại. Không để agent tự do chạy lệnh discovery đệ quy từ `$HOME` hoặc `/`.
