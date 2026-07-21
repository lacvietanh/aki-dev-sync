# Plan: Open Popup & macOS Open Consolidation

**Status:** Done  
**Started:** 2026-06-23  
**Completed:** 2026-06-23

*Lưu ý: File này gộp 2 bản plan cũ (`project-open-menu-hub.md` và `macos-open-consolidation.md`) để làm tài liệu lưu trữ lịch sử cho tính năng Open Popup. Tên gốc của tính năng trong lúc thiết kế là "Project Hub", sau đó đã được đổi thành "Open Popup".*

---

## PHẦN 1: Refactor UI/UX: Project Open Menu Hub (Bản nháp gốc)

**Mục tiêu:** Cải thiện UX phần danh sách dự án. Chuyển đổi các hành động mở Code/IDE từ các nút bấm phân mảnh hoặc ngầm định (click đường dẫn) thành một "Menu Hub" tập trung, hiện ra khi Hover vào Project Icon. Hỗ trợ đa dạng IDE (VSCode, VSCode Insiders, Antigravity) ở cả môi trường Local và Remote SSH.

### 1. Thiết kế Interaction & UI (Frontend - `ProjectTable.vue`)

#### 1.1. Project Open Hub (Hover Menu)
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

#### 1.2. Dọn dẹp Giao diện cũ
- **Tắt Implicit Click:** Loại bỏ sự kiện `@click` đang gắn trên "Local path" và "Remote path" để chống bấm nhầm.
- **Cột ACTIONS:** Xóa 2 nút độc lập `[>_]` (Terminal) và `[VSCode]`. Cột này giờ chỉ dành cho PUSH/PULL/LOG/CONFIG/GIT.
- **Git Modal:** Nút mở "GitHub URL" (hiện đang nằm cạnh tên dự án) sẽ bị xóa khỏi dòng hiển thị và chuyển vào bên trong giao diện của `GitModal.vue`.

### 2. Xử lý Backend (Rust - `src-tauri/src/system.rs`)

Cần bổ sung và sửa đổi các Tauri Commands sau:

#### 2.1. `check_ide_availability()`
- Viết hàm kiểm tra xem các app có tồn tại trong `/Applications/` (trên macOS) hay không.
- Paths cần check:
  - `/Applications/Visual Studio Code.app`
  - `/Applications/Visual Studio Code - Insiders.app`
  - `/Applications/Antigravity.app`
- Trả về object: `{ vscode: bool, vscode_insiders: bool, antigravity: bool }`.

#### 2.2. `open_ide_local(ide_name: String, path: String)`
Thay thế các hàm mở lẻ tẻ trước đây, gộp chung thành một hàm mở IDE cho thư mục Local.
- Lệnh thực thi trên macOS: `open -a "<App Name>" "<path>"`
- Tùy theo `ide_name` (truyền từ Vue) mà map sang đúng `<App Name>`.

#### 2.3. `open_ide_remote(ide_name: String, host: String, path: String)`
Thực thi lệnh kết nối Remote SSH bằng chính IDE thay vì dùng terminal thường.
- Cấu trúc lệnh đối với **VSCode**:
  `open "vscode://vscode-remote/ssh-remote+<host><path>"`
- Cấu trúc lệnh đối với **VSCode Insiders**:
  `open "vscode-insiders://vscode-remote/ssh-remote+<host><path>"`
- Cấu trúc lệnh đối với **Antigravity IDE**:
  `antigravity-ide --remote ssh-remote+<host> <path>`

---

## PHẦN 2: Plan: macOS Open Consolidation (Tinh giản Backend)

### 1. Vấn đề

`system.rs` hiện có nhiều Tauri IPC commands chỉ là thin wrapper quanh lệnh `open` của macOS:

| Command | Thực ra làm gì |
|---|---|
| `open_url(url)` | `open <url>` |
| `open_local_dir(path)` | `open -R <path>` |
| `open_in_terminal(path)` | `open -a Terminal <path>` |
| `open_antigravity_app()` | `open -a Antigravity` |
| `open_ide_local("finder", path)` | `open <path>` |
| `open_ide_local("terminal", path)` | `open -a Terminal <path>` |
| `open_ide_local("vscode", path)` | `open -a "Visual Studio Code" <path>` |
| `open_ide_local("vscode_insiders", path)` | `open -a "Visual Studio Code - Insiders" <path>` |
| `open_ide_local("antigravity", path)` | `open -a Antigravity <path>` |
| `open_ide_remote("vscode", host, path)` | `open "vscode://vscode-remote/ssh-remote+..."` |
| `open_ide_remote("vscode_insiders", host, path)` | `open "vscode-insiders://vscode-remote/ssh-remote+..."` |

Mỗi case chỉ build arguments khác nhau cho cùng một binary `open`. Đây là logic thuần JS, không cần ở Rust.

### 2. Thiết kế

#### 2.1. Nguyên tắc phân loại

**Giữ ở Rust** - khi có complexity thực sự cần Rust:
- AppleScript string construction + escaping (injection risk)
- Subprocess CLI không phải `open` (`antigravity-ide --remote`)
- Filesystem check (`check_ide_availability`)

**Chuyển về JS** - khi chỉ là build args cho `open`:
- Tất cả `open -a <app> <path>`, `open <url>`, `open <path>`
- VSCode Remote URL scheme (`vscode://...`) - chỉ là string building

#### 2.2. Rust sau refactor

**Giữ nguyên:**
- `check_ide_availability()` - filesystem check
- `open_ide_remote("terminal", ...)` - AppleScript SSH
- `open_ide_remote("antigravity", ...)` - `antigravity-ide --remote` CLI

**Thêm mới:**
```rust
#[tauri::command]
pub fn macos_open(args: Vec<String>) -> Result<(), String> {
    Command::new("open")
        .args(&args)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}
```

**Xóa:**
- `open_url`, `open_local_dir`, `open_in_terminal`, `open_antigravity_app`, `open_ide_local` (toàn bộ)
- `open_ide_remote` arms: `vscode`, `vscode_insiders` (chuyển về JS)
- Hàm remote còn lại sẽ đổi tên thành `open_remote_subprocess` cho rõ nghĩa.

#### 2.3. JS sau refactor

`ProjectTable.vue` - thay `openIdeLocal` và `openIdeRemote`:

```js
// Toàn bộ LOCAL cases - chỉ build args
const IDE_LOCAL_ARGS = {
  finder:          path => [path],
  terminal:        path => ['-a', 'Terminal', path],
  vscode:          path => ['-a', 'Visual Studio Code', path],
  vscode_insiders: path => ['-a', 'Visual Studio Code - Insiders', path],
  antigravity:     path => ['-a', 'Antigravity', path],
}

async function openIdeLocal(ideName, path) {
  const args = IDE_LOCAL_ARGS[ideName]?.(path)
  if (args) await invoke('macos_open', { args })
}

// REMOTE - split: URL scheme về macos_open, subprocess về Rust
async function openIdeRemote(ideName, host, path) {
  if (ideName === 'vscode') {
    await invoke('macos_open', { args: [`vscode://vscode-remote/ssh-remote+${host}${path}`] })
  } else if (ideName === 'vscode_insiders') {
    await invoke('macos_open', { args: [`vscode-insiders://vscode-remote/ssh-remote+${host}${path}`] })
  } else {
    await invoke('open_remote_subprocess', { ideName, host, path })
  }
}
```
