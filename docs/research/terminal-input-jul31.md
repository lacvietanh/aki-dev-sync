# Terminal Input — Bản ghi Thực nghiệm & Quyết định Hỗ trợ IME (2026-07-31)

> **Bản ghi thực nghiệm:** Kiểm thử thực tế kiến trúc `useTerminalTextDrain.js` ("xterm owns keys, app owns text") trên macOS và Android (Gboard). Kế hoạch thực thi liên quan: [`docs/plan/done/terminal-input-jul31.md`](../plan/done/terminal-input-jul31.md).

---

## 1. Quyết định Thiết kế & Phạm vi Hỗ trợ (Negative Constraints)

- **macOS Telex dựng sẵn (Out of Scope):** Gõ bằng bộ gõ Telex mặc định của macOS bị lặp từ tại commit boundary (`ăn ăn gì, gì công công việc việc `). Hiện tượng này xảy ra tương tự trên VS Code do đặc thù WebKit/xterm. **Quyết định: App chỉ hỗ trợ OpenKey / EVKey.** Tuyệt đối không coi hiện tượng lặp từ của bộ gõ macOS Telex mặc định là hồi quy cần sửa.
- **Loại bỏ Escape Hatch:** Toàn bộ chế độ lùi `aki-input-mode='legacy'` và bracketed paste phức tạp đã bị loại bỏ để giữ code tinh giản, không lưu trữ mã dự phòng thừa.

---

## 2. Bằng chứng Thực nghiệm Máy thật (Empirical Evidence)

- **Android Gboard (Input Correction Bug):** Khi gõ `ăn gì đi nhé bây giờ cần một`, chuỗi nhận được tại PTY là `aăn giì dđi nheé baây gioơờ caânần moôtột`. Bằng chứng xác thực Gboard gửi ký tự sửa mới nhưng thao tác xoá ký tự đệm trước đó không tới được PTY $\to$ Căn cứ trực tiếp cho bộ lọc `deleteContentBackward` (gửi byte `\x7f` backspace).
- **OpenKey Auto-restore:** Gõ `tôi cần exit warning wasm` xác nhận OpenKey tự khôi phục từ khóa tiếng Anh sau khi nhấn phím cách mà không bị nuốt ký tự.
- **Modifier Latch (Ctrl / Shift):** Latch Ctrl + gõ `x` trong `nano` gửi đúng mã `0x18`, Latch Ctrl + gõ `c` trong ô compose ngắt tiến trình đúng chuẩn $\to$ Khẳng định tầng logic `ctrlByteFor` và latch object hoạt động chính xác.

---

## 3. Tài liệu Liên quan (Cross References)

- [`docs/research/terminal-vietnamese-ime-root-cause-4.md`](terminal-vietnamese-ime-root-cause-4.md) — Kiến trúc nền tảng "xterm owns keys, app owns text".
- [`docs/plan/done/terminal-input-surface.md`](../plan/done/terminal-input-surface.md) — Đặc tả bề mặt input và hàng phím điều khiển.
- [`src/composables/useTerminalTextDrain.js`](../../src/composables/useTerminalTextDrain.js) — Module composable thực thi text drain và IME handling.
