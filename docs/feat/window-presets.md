# Window Presets (AppWindow)

> updated 2026-08-17 · v1.26.0

Các preset kích thước/vị trí cửa sổ nằm trong menu ☰ ở titlebar, mục `AppWindow:`, kèm tuỳ chọn ghi nhớ để lần mở app sau tự áp lại. Phím tắt là F-key trần (`F1`/`F2`/`F3`/`F12`), không kèm modifier nào.

Code: `src/composables/useAppWindow.js` (toàn bộ state + logic) và `src/components/AppHeader.vue` (markup menu, phím tắt, CSS).

## 1. Hai trục độc lập

Preset **không** phải một danh sách phẳng mà là hai trục riêng, vì chúng thay đổi hai thứ khác nhau và phải cùng tồn tại được:

| Trục | Preset | Tác dụng |
|---|---|---|
| `width` | `narrow` (440px) / `wide` (768px) / `ultrawide` (1400px) | Đổi chiều rộng, giữ nguyên chiều cao và vị trí |
| `place` | `stick` / `center` | `stick`: dán vào góc trên-trái của màn hình app **đang ở** (`currentMonitor()`, fallback về màn hình trên-trái nhất nếu không xác định được), chiều cao luôn bằng trọn work area của màn hình đó (đã trừ menu bar). `center`: căn giữa màn hình chính, không đổi kích thước |

Nếu gộp thành một trục (chỉ nhớ "nút bấm cuối cùng") thì chọn Narrow rồi chọn Center Primary sẽ xoá mất Narrow đã nhớ.

`applyView(axis, name)` là cửa duy nhất để áp một preset — vừa gọi hàm tương ứng, vừa ghi nhớ khi tuỳ chọn remember đang bật.

## 2. Remember

- Tắt mặc định. `remember` **không còn lưu preset đã chọn** — nó lưu kích thước/vị trí cửa sổ thật tại thời điểm chụp: `{ width, height, x, y }` (`captureBounds()`), dưới key `aki-devsync-window-bounds-pt`. Cờ bật/tắt riêng ở `aki-devsync-remember-view`.
- Cả 4 trường đều là logical point (trước đây `x`/`y` lưu physical pixel), vì đó là đơn vị duy nhất nhất quán trên máy nhiều màn hình khác scale factor để restore đúng chỗ.
- Chụp chỉ xảy ra ở hai thời điểm: lúc bật checkbox `remember` lên, và mỗi lần một preset được áp trong khi `remember` đang bật. Kéo/resize cửa sổ bằng tay **không** kích hoạt chụp lại.
- Tắt `remember` **không xoá** dữ liệu đã lưu — giá trị lưu là một phép đo chính xác, không phải một preset có thể chọn lại, nên gạt tắt không được phép huỷ nó.
- Khi app khởi động với `remember` đang bật nhưng chưa có gì lưu (profile mới nâng cấp, hoặc localStorage vừa bị xoá), `restoreView()` tự chụp ngay lúc đó thay vì bỏ qua, để lần khởi động sau có cái để khôi phục.
- Preset được tô sáng (`is-active`) trong menu là preset **được bấm gần nhất trong phiên hiện tại** (`savedView`, chỉ tồn tại trong bộ nhớ session) — không liên quan tới dữ liệu đã lưu ở `remember`, nên nó không phản ánh layout sẽ được khôi phục ở lần mở tiếp theo.
- Cả hai key cũ, `aki-devsync-window-view` (định dạng preset cũ) và `aki-devsync-window-bounds` (định dạng physical pixel cũ), bị xoá khỏi `localStorage` ngay khi app khởi động, không phụ thuộc `remember` đang bật hay tắt — ai đã bật remember trước bản này mất bounds đã nhớ đúng một lần và phải bật lại.

## 3. Phím tắt F1 / F2 / F3 / F12

- `F1` = tổ hợp cả một cột của lưới 2x2 (`applyViewCombo(1)`): Narrow + Stick Top-Left, tức snap vào màn hình app **đang ở**, không phải màn hình trên-trái nhất của cả hệ thống.
- `F2` = toggle riêng trục `width` (`toggleUltrawide()`): `ultrawide` (1400px) ↔ `narrow`, đọc trạng thái hiện tại từ `savedView.width` chứ không đo bề rộng cửa sổ thật — bề rộng có thể đã bị kéo tay, đo nó sẽ khiến toggle nhảy lung tung. Không đụng trục `place`.
- `F3` = chỉ áp preset `place: center` (`applyView('place', 'center')`), không đụng trục `width`.
- `F12` = `togglePin()`, tương đương bấm nút pin trên titlebar — không thuộc hai trục `width`/`place`, không được `remember` ghi nhớ.

F1/F2/F3 đi qua đúng `applyView`/`applyViewCombo` như khi bấm chuột, nên vẫn được ghi nhớ nếu remember đang bật.

Listener gắn ở `window` (global trong app) vì mục đích của phím tắt chính là không phải mở menu ra trước. Chỉ nhận phím chức năng trơn: có thêm Shift/Alt/Ctrl/Cmd, hoặc auto-repeat khi giữ phím, đều bị bỏ qua để không giành tổ hợp của thứ khác.

Không có thẻ nhãn phím tắt nào phủ lên menu — phím tắt được tra cứu qua modal **Keyboard Shortcuts** riêng (nút ⌨ trên titlebar), không thêm hàng/nhãn nào vào lưới preset (luật Extreme Narrow trong `CLAUDE.md`).

## 4. Khôi phục lúc khởi động

`restoreView()` chạy trong `onMounted` của AppHeader và khôi phục thẳng bounds đã lưu bằng Tauri window API (`setSize`/`setPosition`) — không còn đi qua `applyView`/preset, không còn khái niệm "width trước, place sau". Nếu vị trí đã lưu không còn nằm trên màn hình nào đang kết nối (màn hình rời đi hoặc đổi độ phân giải), toạ độ được nudge về trong work area của màn hình gần nhất thay vì để cửa sổ trôi ra ngoài tầm với.

Giới hạn cũ về `stick` đo chiều cao qua DOM (`measureRequiredContentHeight()`) không còn tồn tại — hàm đo đã bị xoá, `stick` giờ luôn lấy trọn chiều cao work area của màn hình đích nên không phụ thuộc project list đã render xong hay chưa.
