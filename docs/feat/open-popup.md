# Open Popup

Một menu popup tập trung giúp hợp nhất các thao tác mở dự án Local và Remote SSH trên nhiều IDE khác nhau. Được tái cấu trúc từ các nút bấm phân tán (xem thêm chi tiết kiến trúc macOS open consolidation).

## Chức năng chính

### 1. Unified Trigger (Nút OPEN)
- Menu được kích hoạt khi người dùng **hover** (rê chuột) hoặc **click** vào vùng chứa nút `OPEN` ở cột ACTIONS của danh sách dự án.
- Sử dụng delay timer 150ms để chống chớp tắt liên tục khi rê chuột, giữ trải nghiệm mượt mà.
- Popup có logic tính toán vị trí động:
  - Mặc định thả xuống dưới.
  - Tự động lật lên trên (bottom-up) nếu vị trí hiện tại gần cạnh dưới màn hình (cách đáy màn hình ít hơn 350px).

### 2. Local IDE Targets
Hiển thị danh sách các lối tắt mở code tại thư mục máy Local:
- **Finder:** Mở folder gốc.
- **Terminal:** Mở tab Terminal native macOS.
- **VSCode & VSCode Insiders:** Mở bằng text editor phổ biến.
- **Antigravity IDE:** Editor mặc định của hệ sinh thái Aki.

### 3. Remote SSH Targets
Với các project có cấu hình Remote, popup hiển thị thêm cột kết nối từ xa:
- **SSH Terminal:** Mở Terminal native, tự tạo script `osascript` kết nối SSH thẳng vào Server và cd vào thư mục project (`~` sẽ được tự động resolve thành `/home/user`).
- **VSCode Remote (và Insiders):** Dùng URL Scheme `vscode://vscode-remote/ssh-remote+...` để điều hướng VSCode mở Remote Extension. Logic JS luôn xử lý ghép chuẩn xác URL (thêm `/` ở đầu absolute path nếu cần).
- **Antigravity Remote:** Chạy CLI `antigravity-ide --remote` kết nối tới Server.

### 4. Dynamic IDE Availability
- Bằng cơ chế IPC, ứng dụng tự động kiểm tra sự tồn tại của các app (`.app`) trong thư mục `/Applications` trên macOS (như Visual Studio Code, Antigravity IDE).
- Các App/IDE chưa được cài đặt trên máy người dùng sẽ tự động bị chuyển sang trạng thái làm mờ (grayscale, độ trong suốt thấp) và khóa click `cursor: not-allowed` mà không báo lỗi câm (silent fail) khi gọi Command.

---

## Technical Details (Refactor Insights)

Trước bản cập nhật refactor: 
- Có nhiều nút rải rác: `[>_]` Terminal, `[VSCode]`. 
- Nhiều Implicit click (Click ngầm vào đường dẫn thư mục).
- Rất nhiều API dư thừa phía Rust: `open_in_terminal`, `open_ide_local`, v.v... chỉ thuần tuý dùng `open -a`.

**Sau refactor (MacOS Open Consolidation):**
- **JS-side (Frontend):** 
  - Các cấu hình URL URL Schemes (`vscode://`), app params (`-a 'Terminal'`) được chuyển về và quản lý ở `ProjectTable.vue`. 
  - Chỉ gọi duy nhất 1 handler dùng chung `macos_open(args)`.
- **Rust-side (Backend):** 
  - Bỏ đi logic thin-wrapper.
  - `system.rs` giờ đây chỉ có `macos_open(args)`, lệnh chuyên sâu SSH (`open_remote_subprocess`), và check file system (`check_ide_availability`).
  - Gọn nhẹ, giảm rủi ro bảo mật (như String injection trong Command args).
