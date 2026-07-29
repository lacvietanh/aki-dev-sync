# Terminal Resize Authority — Mac-only grid, phone view-scale only

## Cốt lõi vấn đề
Một PTY chỉ tồn tại đúng một cặp cols/rows tại một thời điểm — không thể có "hai nguồn sự thật" cùng lúc cho cùng một shell. Khi nhiều client (Mac + phone) cùng attach vào một PTY dùng chung, hành động resize là một thao tác **GHI** (mutate state), không phải thao tác **ĐỌC** (view).

## Triết lý nền tảng
Tách biệt hoàn toàn giữa:
- **Grid truth** (cols/rows thật của PTY) — chỉ Mac (source of truth) có quyền thay đổi.
- **View scaling** (font-size / cell-size / zoom hiển thị) — phone tự do điều chỉnh cục bộ, không ảnh hưởng ngược lại PTY.

## Quy tắc triển khai

1. **Chỉ Mac client được phép gọi resize PTY thật (SIGWINCH)**. One-way authority — không có ngoại lệ.
2. **Mọi client khác (phone) khi nhận cols/rows mới broadcast từ Mac → chỉ tính lại fontSize/cell-size để fit khung hình hiện có**, tuyệt đối không tự tính cols/rows rồi gửi resize ngược lên server/PTY.
3. **Tắt hoàn toàn mọi auto-fit-addon** (vd fit-addon của xterm.js) tự động resize theo kích thước container trên phía phone — đây là nguồn lỗi phổ biến nhất khi port terminal lib từ desktop sang mobile, vì addon mặc định tự ý coi kích thước container là grid mới.
4. **Khi grid (cols/rows do Mac quy định) quá lớn so với màn hình phone** khiến font bị scale nhỏ tới mức khó đọc → cho phép pinch-zoom/pan (transform scale ở tầng render) hoặc scroll ngang/dọc, **không được ép fit bằng cách đổi lại grid**.

## Tham chiếu mô hình tương tự
- Cách **tmux** xử lý window-size mode "manual".
- Cách **mosh/VNC** giữ 1 canvas cố định phía server và mọi client chỉ scale để xem.
