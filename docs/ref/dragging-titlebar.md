# Hướng dẫn thiết lập Kéo Thả (Drag) và Titlebar Custom trong Tauri v2

Để thiết lập một Titlebar tùy biến hoàn toàn (giống Aki Waves Ultra) mà không dính lỗi "âm thầm từ chối" của Tauri v2, cần thực hiện chuẩn xác 4 bước sau:

## 1. Mở Khóa Quyền Window (Capabilities) - QUAN TRỌNG NHẤT
Tauri v2 áp dụng bảo mật IPC cực kỳ khắt khe. Nếu không cấp quyền, mọi lệnh tương tác với cửa sổ từ JS (như `startDragging`, `minimize`, `close`) sẽ bị Rust Backend từ chối trong im lặng (Silent Fail).

Mở file `src-tauri/capabilities/default.json` và thêm vào mảng `"permissions"`:
```json
"core:window:allow-minimize",
"core:window:allow-close",
"core:window:allow-start-dragging"
```

## 2. Cấu Hình Window Trong `tauri.conf.json`
Để ẩn thanh Titlebar mặc định của hệ điều hành và cho phép nền trong suốt:
```json
"app": {
  "windows": [
    {
      "transparent": true,
      "decorations": false
    }
  ],
  "macOSPrivateApi": true
}
```

## 3. Đồng bộ Cargo.toml
Khi dùng tính năng `transparent` trên macOS, tính năng `macos-private-api` phải được bật trong `src-tauri/Cargo.toml`:
```toml
[dependencies]
tauri = { version = "2", features = ["macos-private-api"] }
```

## 4. Xử Lý Giao Diện (Vue / Frontend)
Sử dụng hàm API của Tauri để bắt sự kiện. **Lưu ý: Các API này trả về Promise, nên luôn dùng `.catch()` để biết lý do nếu lỗi.**

```html
<script setup>
import { getCurrentWindow } from "@tauri-apps/api/window";

let appWindow = null;
try {
  appWindow = getCurrentWindow();
} catch (e) {
  console.warn("Chạy ngoài Tauri:", e);
}

function minimize() {
  if (appWindow) appWindow.minimize().catch(e => console.error(e));
}
function closeWin() {
  if (appWindow) appWindow.close().catch(e => console.error(e));
}
function startDragging() {
  if (appWindow) appWindow.startDragging().catch(e => console.error(e));
}
</script>

<template>
  <!-- Gắn thuộc tính @mousedown.prevent và data-tauri-drag-region -->
  <header class="top-header" data-tauri-drag-region @mousedown.prevent="startDragging">
    <!-- Nút bấm cần loại trừ vùng kéo thả -->
    <button class="no-drag" @click="minimize">Minimize</button>
    <button class="no-drag" @click="closeWin">Close</button>
  </header>
</template>

<style>
/* Cho phép macOS native drag nếu cần */
.top-header {
  -webkit-app-region: drag;
}

/* Loại bỏ kéo thả chữ và hình ảnh gốc của web */
.top-header, .top-header * {
  user-select: none;
  -webkit-user-select: none;
  -webkit-user-drag: none;
}

/* Vùng có nút bấm không được kéo */
.no-drag {
  -webkit-app-region: no-drag;
}
</style>
```

---
**💡 Bài học rút ra:** 
Luôn kiểm tra `capabilities/default.json` trước tiên khi bất kỳ API native nào của Tauri v2 gọi từ Frontend không phản hồi. Các lỗi "kéo thả không được" hay "nút bấm vô hiệu" hầu hết đều bắt nguồn từ việc thiếu quyền IPC.
