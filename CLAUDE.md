# CLAUDE.md

## Aki Rules

Shared rules live at `~/.aki/claudedoc/`. Read `~/.aki/claudedoc/index.md` for the full rule index and loading policy.
Claude Code loads these automatically via the `akirule` skill. Gemini reads them directly from that path.

SRP, SOLID, DRY

---

## THIS PROJECT

### 1. Project
- **Name**: aki-sync-gui
- **Description**: A Tauri-based Desktop App for managing and executing rsync-based deployment workflows with remote pre/post hooks.
- **Stack**: Vue 3 (Vite), Tauri v2, Rust.

### 2. Specific Config
- **Tauri Architecture**: Cửa sổ chạy ở chế độ `"decorations": false` và `"transparent": true` (với cờ `macos-private-api` trên Mac) để tắt hoàn toàn Native macOS OS Titlebar.
- **Window API**: Tất cả lệnh liên quan đến cửa sổ như Kéo Thả (Drag), Thu nhỏ (Minimize), Đóng (Close) được thực hiện bằng JavaScript thông qua `@tauri-apps/api/window`.
- **Bảo mật (Tauri v2)**: BẮT BUỘC phải cấp các quyền `core:window:allow-minimize`, `core:window:allow-close`, `core:window:allow-start-dragging` trong file `src-tauri/capabilities/default.json`. Nếu không có, IPC sẽ âm thầm từ chối (Silent Fail) và giao diện không phản hồi.

### 3. Vibe / Styling
- **Vibe**: Dark mode, functional developer tool, minimal footprint.
- Phản hồi nhanh lỗi hệ thống bằng hệ thống Notification (dùng SweetAlert2) khi API từ Rust bị từ chối hoặc Promise bị catch.
