# Refactor UI/UX: Project Open Menu Hub

**Status:** Done  
**Started:** 2026-06-23  
**Completed:** 2026-06-23

**Progress:**
- [x] Backend: system.rs — add check_ide_availability, open_ide_local, open_ide_remote
- [x] Backend: lib.rs — register new, remove old commands
- [x] Frontend: GitModal.vue — add GitHub URL
- [x] Frontend: ProjectTable.vue — hub UI + cleanup

---

**Mục tiêu:** Cải thiện UX phần danh sách dự án. Chuyển đổi các hành động mở Code/IDE từ các nút bấm phân mảnh hoặc ngầm định (click đường dẫn) thành một "Menu Hub" tập trung, hiện ra khi Hover vào Project Icon. Hỗ trợ đa dạng IDE (VSCode, VSCode Insiders, Antigravity) ở cả môi trường Local và Remote SSH.

## 1. Thiết kế Interaction & UI (Frontend - `ProjectTable.vue`)

### 1.1. Project Open Hub (Hover Menu)
- **Kích hoạt:** Bọc Project Icon bằng một thẻ `div`. Khi người dùng `hover` vào khu vực này, một Popover Menu sẽ nổi lên mượt mà.
- **Trạng thái IDE:** 
  - Khi khởi tạo hoặc lần đầu hover, Vue sẽ gọi API Tauri `check_ide_availability` để kiểm tra trên máy người dùng có những IDE nào.
  - Các IDE chưa cài đặt sẽ bị thêm class CSS làm mờ: `filter: grayscale(1) opacity(0.4);` và vô hiệu hóa click (`cursor: not-allowed`).
  - *Lưu ý:* VSCode Insiders sẽ dùng chung file ảnh icon của VSCode nhưng sử dụng CSS `filter` để đổi màu sang xanh ngọc `#04b597`.
- **Cấu trúc Menu (3 phần chính):**
  - 💻 **LOCAL**
    - Finder
    - Terminal
    - VSCode
    - VSCode Insiders
    - Antigravity IDE
  - ☁️ **REMOTE (SSH)**
    - SSH Terminal
    - VSCode (Remote SSH)
    - VSCode Insiders (Remote SSH)
    - Antigravity IDE (Remote SSH)
  - 🌐 **LINKS**
    - Open Production Site

### 1.2. Dọn dẹp Giao diện cũ
- **Tắt Implicit Click:** Loại bỏ sự kiện `@click` đang gắn trên "Local path" và "Remote path" để chống bấm nhầm.
- **Cột ACTIONS:** Xóa 2 nút độc lập `[>_]` (Terminal) và `[VSCode]`. Cột này giờ chỉ dành cho PUSH/PULL/LOG/CONFIG/GIT.
- **Git Modal:** Nút mở "GitHub URL" (hiện đang nằm cạnh tên dự án) sẽ bị xóa khỏi dòng hiển thị và chuyển vào bên trong giao diện của `GitModal.vue`.

---

## 2. Xử lý Backend (Rust - `src-tauri/src/system.rs`)

Cần bổ sung và sửa đổi các Tauri Commands sau:

### 2.1. `check_ide_availability()`
- Viết hàm kiểm tra xem các app có tồn tại trong `/Applications/` (trên macOS) hay không.
- Paths cần check:
  - `/Applications/Visual Studio Code.app`
  - `/Applications/Visual Studio Code - Insiders.app`
  - `/Applications/Antigravity.app`
- Trả về object: `{ vscode: bool, vscode_insiders: bool, antigravity: bool }`.

### 2.2. `open_ide_local(ide_name: String, path: String)`
Thay thế các hàm mở lẻ tẻ trước đây, gộp chung thành một hàm mở IDE cho thư mục Local.
- Lệnh thực thi trên macOS: `open -a "<App Name>" "<path>"`
- Tùy theo `ide_name` (truyền từ Vue) mà map sang đúng `<App Name>`.

### 2.3. `open_ide_remote(ide_name: String, host: String, path: String)`
Thực thi lệnh kết nối Remote SSH bằng chính IDE thay vì dùng terminal thường.
- Cấu trúc lệnh đối với **VSCode**:
  `open "vscode://vscode-remote/ssh-remote+<host><path>"`
  *(Ví dụ: `open "vscode://vscode-remote/ssh-remote+admin@192.168.1.50/var/www/project"`)*
- Cấu trúc lệnh đối với **VSCode Insiders**:
  `open "vscode-insiders://vscode-remote/ssh-remote+<host><path>"`
- Cấu trúc lệnh đối với **Antigravity IDE**:
  `antigravity-ide --remote ssh-remote+<host> <path>`
  *(Sử dụng `std::process::Command::new("antigravity-ide")` với các đối số tương ứng).*

> *Lưu ý: Không thêm dấu `/` thừa giữa hostname và absolute_path khi ghép chuỗi cho chuẩn vscode protocol.*

---

## 3. Đăng ký IPC (`src-tauri/src/lib.rs`)
- Add các function mới (`check_ide_availability`, `open_ide_local`, `open_ide_remote`) vào `tauri::generate_handler!`.
- Xóa các hàm cũ đã bị thay thế (ví dụ `open_in_vscode`, `open_antigravity_app`).
