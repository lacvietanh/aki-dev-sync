# Drag & Drop Live Sorting

Tài liệu chi tiết về tính năng kéo thả trực quan (Live Sorting) để sắp xếp thứ tự dự án trong Aki Dev Sync. Tính năng này được xây dựng thuần tuý bằng HTML5 Drag & Drop API kết hợp Vue 3 Transition Group và tối ưu hoá hệ thống ở lớp Tauri/WebKit mà không sử dụng bất kỳ thư viện bên thứ ba nào.

---

## Chức năng chính

### 1. Live Sắp xếp và Persist Cấu hình
- Người dùng rê chuột vào grip handle (Project Icon) ở cột đầu tiên và kéo để thay đổi vị trí.
- Các hàng dự án xung quanh tự động trượt ra nhường chỗ một cách trực quan nhờ hiệu ứng chuyển động FLIP của `<transition-group>` trong Vue 3.
- Khi người dùng thả chuột (kết thúc hành động drag), vị trí mới được ghi đè trực tiếp xuống file cấu hình `projects.json` trên đĩa thông qua Rust Command `save_projects` để lưu lại vĩnh viễn.

### 2. Thiết kế Spacing Nhất quán và Khoa học (Đồng bộ Grid bằng CSS Variables)
- **Đồng bộ cột bằng CSS Variables:** Để tránh lệch cột giữa `.grid-header` và các `.grid-row` khi nội dung thay đổi, ứng dụng sử dụng CSS Variables `--grid-cols: 13.5rem 5rem 3.8rem 1fr;` định nghĩa một lần duy nhất tại container cha. Cả header và row đều kế thừa biến này để áp dụng cho `grid-template-columns`, đảm bảo tất cả các cột được xếp thẳng hàng tăm tắp với pixel-perfect.
- **Dùng Grid Gap theo rem:** Sử dụng `--grid-gap: 0.5rem` (Desktop) và `--grid-gap: 0.25rem` (Dưới 800px) kết hợp với `column-gap` giúp quản lý khoảng cách giữa các cột một cách hệ thống, nhất quán và khoa học mà không cần chắp vá padding hay margin cục bộ.
- **Căn lề trục dọc nhất quán:** Tất cả tiêu đề cột và nội dung hiển thị đều được căn lề trái (`text-align: left`) tạo nên trục dọc gọn gàng và khoa học.

---

## Các Pitfalls trên macOS & Tauri (Giải pháp Native)

### 1. Tauri Native File Drop Conflict
- **Vấn đề:** Tauri V2 mặc định kích hoạt bộ bắt sự kiện kéo thả file (`dragDropEnabled: true`) ở lớp cửa sổ Rust của hệ điều hành. Lớp này chặn bắt và nuốt chửng toàn bộ sự kiện kéo thả chuột, khiến WebView (Vue/JS) hoàn toàn không nhận được bất kỳ sự kiện HTML5 nào.
- **Giải pháp:** Thiết lập `"dragDropEnabled": false` trong file cấu hình cửa sổ `src-tauri/tauri.conf.json`. Điều này tắt bộ chặn native của Tauri và trả lại toàn bộ quyền kiểm soát drag-and-drop cho WebView.

### 2. Sự hạn chế của WebKit Frameless Div
- **Vấn đề:** Trình duyệt WebKit (Safari/macOS WebView) không cho phép kéo thả custom elements (thẻ `div` thông thường) một cách native chỉ bằng thuộc tính `draggable="true"`.
- **Giải pháp:** Thêm thuộc tính CSS `-webkit-user-drag: element !important;` vào class `.grid-row` để kích hoạt hoàn toàn khả năng kéo thả native của WebKit cho hàng.

### 3. Tránh việc kéo nhầm File Ảnh (Image Dragging) & Tối ưu hiển thị tay nắm kéo thả
- **Tránh kéo nhầm ảnh:** Mặc định, trình duyệt luôn ưu tiên kéo native hình ảnh (`<img>`) có trong grip handle (Project Icon) để copy file ảnh thay vì kéo hàng cha. Giải quyết bằng cách đặt thuộc tính HTML `draggable="false"` lên thẻ `<img>`, đồng thời cấu hình CSS `pointer-events: none;` và `-webkit-user-drag: none !important;` trên các phần tử con của grip handle.
- **Nổi bật chỉ thị kéo thả (Overlay Grip Handle):** 
  - Đặt thuộc tính `z-index: 1` trên phần tử giả `.project-drag-handle::before` để đưa lớp chỉ thị chấm bi nổi lên phía trên ảnh icon dự án, thay vì bị chìm bên dưới.
  - Phủ thêm lớp nền tối mờ `background-color: rgba(0, 0, 0, 0.45)` kết hợp với lưới chấm trắng có độ tương phản cao `rgba(255, 255, 255, 0.8)` trên sự kiện hover để người dùng dễ dàng nhận thấy vùng tay nắm kéo thả một cách trực quan trên mọi tông màu của icon.

### 4. Loại bỏ bất đồng bộ Vue (Reactivity Delay)
- **Vấn đề:** Nếu cập nhật thuộc tính `:draggable` động bằng JS trên sự kiện `@mousedown`, độ trễ bất đồng bộ (next tick) của Vue khiến WebView không bắt kịp cử chỉ kéo chuột của người dùng, dẫn đến kéo thả chập chờn.
- **Giải pháp:** 
  - Đặt `draggable="true"` tĩnh trực tiếp trên DOM của `.grid-row`.
  - Quản lý trạng thái click chuột của grip handle bằng biến đồng bộ `isHandleMouseDown`.
  - Khi sự kiện `@dragstart` phát ra, nếu việc kéo không bắt nguồn từ grip handle, ta gọi `event.preventDefault()` để **huỷ kéo đồng bộ ngay lập tức**. Việc click chọn văn bản hoặc bấm nút trên hàng vẫn hoạt động bình thường.
  - Sử dụng `@mousedown` trên hàng dự án để tự động reset trạng thái nếu người dùng click ra ngoài grip handle.

---

## Giải thuật triệt tiêu nháy giật (Midpoint Geometric Threshold)

### Vấn đề Jittering (Nhấp nháy / Nhảy hàng liên tục)
Khi người dùng kéo một hàng dự án và con trỏ chuột vừa chạm biên của hàng đích, hành động hoán đổi vị trí trong mảng xảy ra ngay lập tức. DOM re-render khiến hàng đích lập tức hoán đổi vị trí với hàng đang kéo. Lúc này, con trỏ chuột (đang giữ nguyên toạ độ màn hình) đột ngột nằm trên một hàng mới, kích hoạt sự kiện `dragover` của hàng đó và hoán đổi ngược lại. Điều này tạo ra một vòng lặp phản hồi nháy giật liên tục (feedback loop).

### Giải pháp Hình học Trung điểm (Hysteresis Dead-zone)
Để giải quyết triệt để, hệ thống tính toán toạ độ hình học của hàng đích thời điểm rê chuột qua:

```javascript
function onRowDragOver(index, event) {
  if (dragFromIndex.value === null || dragFromIndex.value === index) return;
  
  const rect = event.currentTarget.getBoundingClientRect();
  const threshold = rect.top + rect.height / 2; // Đường ranh giới trung tâm trục dọc
  const fromIndex = dragFromIndex.value;
  
  // 1. Kéo xuống dưới (Dragging Down):
  // Chỉ swap khi con trỏ chuột đi qua nửa dưới (trung điểm) của hàng đích
  if (fromIndex < index && event.clientY < threshold) return;
  
  // 2. Kéo lên trên (Dragging Up):
  // Chỉ swap khi con trỏ chuột đi qua nửa trên (trung điểm) của hàng đích
  if (fromIndex > index && event.clientY > threshold) return;
  
  // Thực hiện hoán đổi phần tử
  const arr = [...projects.value];
  const [movedItem] = arr.splice(fromIndex, 1);
  arr.splice(index, 0, movedItem);
  
  projects.value = arr;
  dragFromIndex.value = index;
}
```

Giải pháp này tạo ra một vùng đệm trễ (hystersis) tương đương **50% chiều cao của hàng**. Hàng đích chỉ thực sự trượt đi khi con trỏ chuột đã đi qua điểm chính giữa của nó, đảm bảo hiệu ứng dịch chuyển cực kỳ êm ái, dứt khoát và mượt mà.
