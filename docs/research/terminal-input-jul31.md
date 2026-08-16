# Terminal Input — Bản ghi Thực nghiệm & Quyết định Hỗ trợ IME (2026-07-31)

> **Bản ghi thực nghiệm:** Kiểm thử thực tế kiến trúc `useTerminalTextDrain.js` ("xterm owns keys, app owns text") trên macOS và Android (Gboard). Kế hoạch thực thi liên quan: [`docs/plan/done/terminal-input-jul31.md`](../plan/done/terminal-input-jul31.md).

---

## 1. Quyết định Thiết kế & Phạm vi Hỗ trợ (Negative Constraints)

- **macOS Telex dựng sẵn (Out of Scope):** Gõ `ăn gì, công việc ` bị lặp từ tại commit boundary (`ăn ăn gì, gì công công việc việc `), tương tự lỗi trên VS Code do đặc thù WebKit/xterm. **Quyết định: App chỉ hỗ trợ OpenKey / EVKey.** Tuyệt đối không coi hiện tượng lặp từ của bộ gõ macOS Telex mặc định là hồi quy cần sửa.
- **Loại bỏ Escape Hatch & Bracketed Paste:** Toàn bộ chế độ lùi `aki-input-mode='legacy'` và bracketed paste phức tạp đã bị loại bỏ để giữ code tinh giản, không lưu trữ mã dự phòng thừa.

---

## 2. Bằng chứng Thực nghiệm Máy thật (Empirical Evidence)

- **Android Gboard (Input Correction Bug):** Khi gõ `ăn gì đi nhé bây giờ cần một`, chuỗi nhận được tại PTY là `aăn giì dđi nheé baây gioơờ caânần moôtột`. Bằng chứng xác thực Gboard gửi ký tự sửa mới nhưng thao tác xoá ký tự đệm trước đó không tới được PTY $\to$ Căn cứ trực tiếp cho bộ lọc `deleteContentBackward` (gửi byte `\x7f` backspace).
- **Double Space Blocker & Broken Syllable (Chrome Companion/Web):** Gõ tiếng Anh hoặc tiếng Việt đều bị nhân đôi dấu cách (`-> kiểm  tra  những  thay  đổi  lần  này  và  bá o  cáo  ngắn  gọn`). Dấu cách chèn giữa âm tiết (`bá o`) chứng minh lỗi nằm ở composition/input stream chứ không phải phím cách bị lặp (key-repeat) $\to$ Cơ sở mở phiên hội đồng cho [`terminal-vietnamese-ime-root-cause-5.md`](terminal-vietnamese-ime-root-cause-5.md) (xác định `_keyDown` của xterm 5.5.0 không force-cancel phím space `keyCode 32 < 48`).
- **OpenKey Auto-restore & Space Anomaly:** Gõ ra `tôi  cần  exit warning wasm `. Xác nhận OpenKey khôi phục từ khóa tiếng Anh sau dấu cách; đoạn sau auto-restore (`exit warning wasm`) không bị double space dù đoạn trước (`tôi  cần`) bị $\to$ Bằng chứng phân biệt quan trọng về luồng can thiệp của OpenKey.
- **Modifier Latch (Ctrl / Shift):** Latch Shift $\to$ `Tab` trong Claude Code cho backtab đúng; Latch Ctrl $\to$ `x` trong `nano` gửi đúng mã `0x18`; Latch Ctrl $\to$ gõ `c` trong ô compose ngắt tiến trình đúng chuẩn $\to$ Khẳng định tầng logic `ctrlByteFor` và latch object hoạt động chính xác.

---

## 3. Câu hỏi Kỹ thuật Mở & Tài liệu Tham chiếu (Cross References)

- **Điểm bất định Carrier 229:** Phiên test chưa đo `__akiTermInput.tail(20)` nên chưa kết luận carrier của OpenKey có gắn `keyCode 229` hay không; giữ nguyên ghi nhận từ `-4.md`.
- [`docs/research/terminal-vietnamese-ime-root-cause-4.md`](terminal-vietnamese-ime-root-cause-4.md) — Kiến trúc nền tảng "xterm owns keys, app owns text".
- [`docs/research/terminal-vietnamese-ime-root-cause-5.md`](terminal-vietnamese-ime-root-cause-5.md) — Phân tích nguyên nhân gốc rễ lỗi double-space của phím cách trên xterm.
- [`docs/plan/done/terminal-input-surface.md`](../plan/done/terminal-input-surface.md) — Đặc tả bề mặt input và hàng phím điều khiển.
- [`src/composables/useTerminalTextDrain.js`](../../src/composables/useTerminalTextDrain.js) — Module composable thực thi text drain và IME handling.
