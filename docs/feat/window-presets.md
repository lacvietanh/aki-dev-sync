# Window Presets (AppWindow)

Bốn preset kích thước/vị trí cửa sổ nằm trong menu ☰ ở titlebar, mục `AppWindow:`, kèm tuỳ chọn ghi nhớ để lần mở app sau tự áp lại. App chỉ ship cho macOS nên phím tắt in thẳng ký hiệu `⌘`, không có nhánh dò hệ điều hành.

Code: `src/composables/useAppWindow.js` (toàn bộ state + logic) và `src/components/AppHeader.vue` (markup menu, phím tắt, CSS).

## 1. Hai trục độc lập

Bốn preset **không** phải một danh sách phẳng mà là hai trục riêng, vì chúng thay đổi hai thứ khác nhau và phải cùng tồn tại được:

| Trục | Preset | Tác dụng |
|---|---|---|
| `width` | `narrow` (420px) / `wide` (768px) | Đổi chiều rộng, giữ nguyên chiều cao và vị trí |
| `place` | `stick` / `center` | `stick`: dán vào góc trên-trái của màn hình trên-trái nhất, đồng thời co chiều cao vừa đúng danh sách project. `center`: căn giữa màn hình chính, không đổi kích thước |

Nếu gộp thành một trục (chỉ nhớ "nút bấm cuối cùng") thì chọn Narrow rồi chọn Center Primary sẽ xoá mất Narrow đã nhớ.

`applyView(axis, name)` là cửa duy nhất để áp một preset — vừa gọi hàm tương ứng, vừa ghi nhớ khi tuỳ chọn remember đang bật.

## 2. Remember

- Checkbox `remember` nằm bên phải nhãn `AppWindow:` (cùng hàng, `justify-content: space-between`).
- Tắt mặc định. Khi bật, mỗi lần bấm preset sẽ ghi lại lựa chọn của **trục đó**.
- Tắt remember sẽ **xoá luôn** dữ liệu đã lưu — nếu giữ lại, lần bật sau sẽ khôi phục một layout người dùng chưa hề chọn trong phiên đó.
- Preset đang được nhớ trên mỗi trục được tô sáng (`is-active`) trong menu, nên không cần thêm dòng chữ nào để mô tả trạng thái.
- Lưu ở `localStorage`: `aki-devsync-remember-view` (cờ bật/tắt) và `aki-devsync-window-view` (`{width, place}`).

## 3. Phím tắt ⌘1 / ⌘2

Mỗi tổ hợp áp **cả một cột** của lưới 2x2, tức cả hai trục cùng lúc:

- `⌘1` = Narrow + Stick Top-Left
- `⌘2` = Wide + Center Primary

Chúng đi qua đúng `applyView` như khi bấm chuột, nên vẫn được ghi nhớ nếu remember đang bật.

Listener gắn ở `window` (global trong app) vì mục đích của phím tắt chính là không phải mở menu ra trước. Chỉ nhận `⌘` + phím số trơn: có thêm Shift/Alt/Ctrl, hoặc auto-repeat khi giữ phím, đều bị bỏ qua để không giành tổ hợp của thứ khác.

Nhãn `⌘1`/`⌘2` là hai thẻ `position: absolute` phủ lên khe giữa hai nút của mỗi cột — cố ý không thêm hàng mới nào cho menu (luật Extreme Narrow trong `CLAUDE.md`). Thẻ này bấm được, không chỉ để đọc.

## 4. Khôi phục lúc khởi động

`restoreView()` chạy trong `onMounted` của AppHeader, áp **width trước, place sau**. Thứ tự này bắt buộc: `stick` đo chiều cao nội dung theo chiều rộng **hiện tại**, nếu chạy trước khi đổi chiều rộng thì sẽ vừa khít với chiều rộng cũ.

**Hạn chế đã biết**: `stick` đo chiều cao qua DOM (`measureRequiredContentHeight()`), mà lúc app vừa mở danh sách project có thể chưa load xong, nên chiều cao khôi phục có thể thấp hơn thực tế (sàn dưới là `minHeight` 500px trong `tauri.conf.json`). Nếu thấy khó chịu thì hoãn `restoreView()` tới sau khi danh sách render xong, thay vì gọi ngay ở `onMounted`.
