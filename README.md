# Aki Remote Dev Sync 🚀

**Aki Remote Dev Sync** là ứng dụng desktop (Command Center) chuyên biệt, tối ưu cho quy trình lập trình **Lạc Việt Anh Workflow** (Local ↔ Remote với AI).

![Aki Sync Dashboard](./src-tauri/icons/icon.png)

## 📖 Triết Lý: Lạc Việt Anh Workflow

Ứng dụng này giải quyết bài toán chia tách môi trường phát triển để tối đa hóa hiệu suất của AI (Claude) và giữ an toàn tuyệt đối cho source code:

1. **Máy Local (Source of Truth):**
   - Nơi lưu trữ bộ code gốc, chuẩn nhất.
   - Bạn chỉ thực hiện **code nhẹ** và **commit Git** tại đây.

2. **Máy Remote (Engine / AI Workspace):**
   - Nơi chạy AI Assistant (Claude Max) với cấu hình và sức mạnh tính toán cực lớn.
   - Bạn đẩy code lên đây để AI đọc, refactor, tạo file hàng loạt.

3. **Tại sao lại cần đồng bộ cả `.git/` lên Remote?**
   - Mặc dù Local là Source of Truth, ta vẫn đẩy file `.git/` sang Remote. Việc này giúp AI (Claude) trên Remote có thể chạy lệnh Git, đọc diffs, và hiểu chính xác tiến trình công việc của dự án.
   - Quan trọng hơn, khi bạn mở **VSCode Remote SSH** trên máy Remote, cây Git sẽ hiển thị chính xác các trạng thái (Changes/Staged) so với máy Local của bạn, tạo cảm giác mượt mà và liền mạch như đang code 100% trên một máy.

## 🔥 Tính Năng Nổi Bật

### 1. Nút PUSH Thông Minh (Có Checkbox `.git`)
- Nút **PUSH** nay đi kèm một Checkbox `[.git]`.
- **Mặc định (ON):** Đẩy toàn bộ code VÀ thư mục `.git/` lên Remote để Claude có đầy đủ context.
- **Tắt (OFF):** Chỉ đẩy code lên Remote, bỏ qua `.git/` (dành cho những trường hợp đặc biệt không muốn chép đè lịch sử Git trên Remote).

### 2. PUSH SPECIAL (Chỉ đẩy file thay đổi)
- Mở danh sách **những file bị thay đổi** ở máy Local (Modified, Untracked, Deleted).
- "Những file bên đó có rồi thì cần gì, đúng ko?" - Chính xác! Tính năng này cho phép bạn chọn nhanh (multi-select) một vài file vừa sửa để cập nhật sang Remote cho Claude xử lý, tiết kiệm tối đa thời gian quét và đồng bộ.

### 3. PULL Siêu Tốc
- Lấy lại đoạn code tuyệt vời mà Claude vừa viết trên Remote về thẳng máy Local.
- Bạn review nhanh và **Commit trực tiếp** tại Local, hoàn thành chu trình.

### 4. SSH Config Editor (Tích hợp & An Toàn)
- Chỉnh sửa trực tiếp file `~/.ssh/config` ngay trên giao diện (Raw Text Code Editor).
- Tính năng **UNDO (Khôi phục Backup nội bộ)** cứu nguy ngay lập tức nếu lỡ gõ sai cú pháp làm mất kết nối.
- Auto-load danh sách Host tự động.

### 5. Open-Source Ready (Zero Hardcode)
- Codebase được thiết kế chuẩn mực Native Flow, loại bỏ mọi đường dẫn (paths) và biến môi trường cá nhân bị gài cứng.
- Global Logging ghi nhận chi tiết từng Real-time Event (Load, SSH, Git) phục vụ quá trình debug và giám sát.

### 6. Quản Lý Trạng Thái Git Hợp Nhất (Single Flow)
- Gộp toàn bộ lệnh check Git (Clean/Dirty/Ahead), lấy URL Remote và trích xuất lịch sử Commit Log vào một luồng quét native duy nhất.
- Tối ưu hiệu năng tối đa cho mỗi project, giúp mở Modal xem chi tiết tình trạng Git cực nhanh mà không phải chờ đợi hay gọi các luồng phụ chắp vá.

### 7. Giám Sát Quota & Force Sync (AI Agents)
- Theo dõi Real-time % hạn mức sử dụng của Claude Code trên Remote và Antigravity ở Local với thời gian đếm ngược (Relative Time) được tự động quy đổi cực kỳ trực quan dựa theo Absolute Time.
- **Force Sync Quota (Phá băng Cache):** Bổ sung nút (↻) tự động xuất hiện khi qua chu kỳ reset. Click để chạy ngầm lệnh `claude -m haiku` ở thư mục rỗng `/tmp`. Kỹ thuật này ép server Anthropic trả về Rate Limit Headers chuẩn nhất mà không hề tốn Token đọc Context.

### 8. Cơ Chế An Toàn Khác
- **DRY RUN Toggle:** Công tắc xem trước. Kích hoạt sẽ hiển thị chi tiết lệnh rsync sẽ làm gì mà không chép đè bất cứ byte nào xuống ổ cứng.

---

## 🛠 Tech Stack
- **Frontend:** Vue 3 + Vite, Vanilla CSS.
- **Backend:** Rust + Tauri v2.
- **Core Engine:** `rsync` và `ssh` native.

## 💻 Development

### Prerequisites (macOS)

Cài Xcode Command Line Tools và Rust:

```bash
xcode-select --install
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Prerequisites (Linux — Ubuntu 22.04 / 24.04)

**1. Cài Rust:**
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

**2. Cài system dependencies cho Tauri v2:**
```bash
sudo apt install -y \
  libwebkit2gtk-4.1-dev \
  libjavascriptcoregtk-4.1-dev \
  libsoup-3.0-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  build-essential \
  libssl-dev \
  pkg-config
```

> `build-essential`, `libssl-dev`, `pkg-config` thường đã có sẵn trên máy dev — giữ lại để đảm bảo đủ khi cài fresh.

### Run & Build

```bash
npm install
npm run tauri dev   # Dev (lần đầu build Rust ~5–10 phút)
```
```bash
npm run tauri build # Production build
```

*Thiết kế dành riêng cho tốc độ và quy trình Lạc Việt Anh Workflow.*
