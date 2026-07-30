# Terminal input — plan chốt lại, 2026-07-31

**Báo cáo test (đối chiếu):** [`docs/research/terminal-input-jul31.md`](../research/terminal-input-jul31.md)
— cùng tên, cùng ngày. File đó là **quan sát**; file này là **quyết định và việc còn phải làm**.
Mọi mục dưới đây trỏ về số mục tương ứng bên đó thay vì chép lại quan sát.

**Trạng thái:** kiến trúc đã đúng hướng và phần lớn bề mặt đã đóng. Còn **1 blocker + 2 defect**.
File này được mở ra để một **phiên hội đồng riêng** nghiên cứu — nó cố ý *không* chứa lời giải.

**Thay thế / thu hẹp các plan trước:**
- [`docs/plan/terminal-input-surface.md`](terminal-input-surface.md) — bảng §6 của nó đã được chạy
  hết một lượt; kết quả nằm ở research §4 (bảng checklist). Plan đó nay chỉ còn hiệu lực cho
  phần **UI feedback** (§2 dưới đây). Thứ tự thi công `#3 → #13 → #2 + #8` của nó đã hoàn tất trên thực tế.
- [`docs/research/terminal-vietnamese-ime-root-cause-4.md`](../research/terminal-vietnamese-ime-root-cause-4.md)
  — vẫn là head của chuỗi nghiên cứu và vẫn mô tả đúng kiến trúc đang chạy. **Một tiền đề của nó hết
  hiệu lực**: §5.3 coi khả năng A/B `aki-input-mode='legacy'` là điều kiện tiên quyết; chủ sở hữu đã
  bác bỏ (§4 dưới đây).

---

## 1. Đã đóng — không mở lại

Xác nhận trên máy thật ngày 2026-07-31 bởi chủ sở hữu (research §4, §5):

- **Kiến trúc "xterm owns keys; the app owns text"** — đúng. Phím chức năng (mũi tên trong
  `vim`/`less`, Ctrl+C, Ctrl+D, Option+Backspace/mũi tên) hoạt động bình thường sau khi trả
  `disableStdin` về `false`. Không có hồi quy nào từ việc gỡ overlay textarea.
- **`"ăn gì" → "ăn g"`** — lỗi cắt ký tự cuối, tức defect gốc của cả chuỗi nghiên cứu, **đã hết**.
- **OpenKey auto-restore** — đúng (research §5.4).
- **Sticky Ctrl/Shift, tầng logic** — đúng, có bằng chứng ở hai chương trình độc lập
  (Shift+Tab trong Claude Code, Ctrl+X trong `nano`, Ctrl+C từ ô compose). Latch object + funnel
  `emitKey` + `ctrlByteFor` + latch áp dụng trong ô compose: tất cả đóng.
- **Shift+Enter** trong ô compose — đúng, "hoàn hảo".
- **Compose row + key row chỉ hiện trên companion** (`showKeyRow`) — đúng.
- **Font ô compose** — nhất quán, "hoàn hảo".

Hệ quả: việc xoá `useTerminalInput.js` và `useWkImeGuard.js` là đúng và không cần đắn đo thêm.

---

## 2. Blocker và defect còn lại — nội dung cho phiên hội đồng

Ba mục, xếp theo mức chặn. Mục 2.1 phải giải trước vì nó che khuất phần còn lại.

### 2.1 — BLOCKER: double space, không liên quan bộ gõ

Quan sát: research §5.2. Tóm tắt điều kiện: mọi dấu cách bị nhân đôi, chữ cái thì không, **không bật
bộ gõ nào cũng bị**, trên **Chrome**. Có trường hợp một dấu cách chen vào giữa âm tiết (`bá o`).

Ràng buộc cho lời giải:
- Phải giải thích được **vì sao chỉ space**, không phải mọi ký tự.
- Phải giải thích được **research §5.4**: cùng phiên gõ, phần sau khi OpenKey auto-restore lại **không**
  bị double space. Bất kỳ giả thuyết nào không khớp quan sát này là sai.
- Không được đánh đổi bằng cách phá lại mục 1 (bracketed paste, phím chức năng, latch).

### 2.2 — Android / Gboard: ký tự gốc và ký tự đã sửa đều tới PTY

Quan sát: research §5.5 (`ăn gì` → `aăn giì`). **Giữ tách khỏi 2.1** — khác nền tảng, khác hình dạng,
nhiều khả năng khác nguyên nhân. Không gộp hai mục này vào một fix chung nếu chưa chứng minh được
chúng cùng gốc.

### 2.3 — Nút Ctrl/Shift không sáng khi đang armed — **ĐÃ SỬA 2026-07-31, chờ xác nhận**

Quan sát: research §5.1. Defect **hiển thị**, không phải input — byte tới PTY đã đúng ngay từ đầu.

**Nguyên nhân:** `const ptyApi = ref(null)` trong `TerminalView.vue`. Một `ref` sâu chạy object được
gán qua `reactive()`, mà `reactive()` **bóc (unwrap) các ref nằm trong thuộc tính**. Nên
`ptyApi.value.pendingModifiers` trả về thẳng object `{ ctrl, shift }`, và mọi
`.pendingModifiers.value` trong file đọc ra `undefined`. Template test
`undefined?.[k.arms]` → `is-armed` không bao giờ bật. Latch vẫn đúng vì nó sống trong closure của
composable, chỗ mà việc bóc ref không với tới được — đúng như triệu chứng "chạy đúng nhưng không
sáng".

**Cách sửa:** `ref` → `shallowRef`. Một từ, không thêm DOM, không thêm state, không đổi CSS
(Extreme Narrow giữ nguyên: armed = tô đặc accent trên chính nút đó).

**Cùng một nguyên nhân, sửa luôn trong cùng lần:** hai chỗ khác cũng đang đọc `undefined`
— `onComposeKeydown` (`api?.pendingModifiers.value.ctrl`, tức Ctrl latch trong ô compose) và
`defineExpose({ alive })` (`ptyApi.value?.alive?.value ?? 'unknown'`, tức tab strip không bao giờ
biết PTY đã chết, luôn hiển thị 'unknown'). Không phải việc ngoài phạm vi: đó là **cùng một dòng
lỗi**, và sửa `shallowRef` làm cả ba chỗ đúng cùng lúc.

**Cần chủ sở hữu xác nhận trên máy thật:** (a) nút Ctrl/Shift sáng khi latch, tắt khi nhả;
(b) tab hiển thị đúng trạng thái chết thay vì luôn 'unknown'.

Ràng buộc đã tuân thủ: `showKeyRow` / `ownsPtySize` phải giữ nguyên là **boolean thuần** trong
`usePtyTerminal.js` — nếu biến chúng thành ref thì `v-if="ptyApi?.showKeyRow"` luôn truthy và hàng
phím phone sẽ hiện trên Mac, tức phá D9 vốn vừa PASS. Ghi lại thành comment ngay tại `shallowRef`.

---

## 3. Chốt ngoài phạm vi — không nhận là bug, không sửa

- **macOS Telex (bộ gõ dựng sẵn của macOS)** — research §5.3. Quyết định của chủ sở hữu: bỏ qua, vì
  VS Code cũng hỏng tương tự nên không phải khiếm khuyết riêng của app. **Chỉ hỗ trợ OpenKey.**
  Một phiên sau bắt gặp `ăn ăn gì, gì` phải đọc mục này trước khi coi đó là hồi quy.
- **Paste nhiều dòng / bracketed paste (C8)** — chủ sở hữu đánh giá quá phức tạp so với giá trị.
  Hình thức gửi hiện tại (các dòng thành các lệnh riêng) giữ nguyên. Câu hỏi bỏ ngỏ trong
  `terminal-input-surface.md` §3.4 về việc đọc trạng thái bracketed-paste của xterm 5.x
  **không cần trả lời nữa**.

---

## 4. Việc phải làm ngay, không cần chờ hội đồng

**Gỡ escape hatch `aki-input-mode='legacy'`.** Chủ sở hữu yêu cầu rõ: đường legacy vốn đầy lỗi, giữ
lại chỉ làm code rộng, rối, thành rác (research §5.6).

Phạm vi gỡ:
- `TerminalView.vue`: hằng `legacyInput` đọc từ `localStorage` lúc setup, và nhánh rẽ dựa trên nó.
- `useTerminalTextDrain.js`: nhắc tới `aki-input-mode` trong comment đầu file, trong `status()`
  (`flags`) và trong `help()`.

Giữ lại: `__akiTermInput` (ring buffer, `status`/`tail`/`dump`/`clear`) và cờ
`aki-term-input-debug` — chúng là công cụ chẩn đoán cho chính mục 2.1, không phải nhánh code thứ hai.

Đây là điều kiện tiên quyết của phiên hội đồng: hội đồng nên đọc **một** đường đi, không phải hai.

---

## 5. Nghĩa vụ đồng bộ tài liệu khi các mục ở §2 được đóng

Kế thừa từ `terminal-input-surface.md` §7, thu hẹp lại theo phạm vi thực tế còn lại:

- `docs/feat/in-app-terminal.md` — hành vi gõ trực tiếp tiếng Việt; xoá mô tả compose row là đường
  chính trên Mac.
- `docs/arch/terminal-stack.md` — thay `useTerminalInput` / `useWkImeGuard` bằng
  `useTerminalTextDrain`; ghi ranh giới "xterm owns keys; the app owns text".
- `README.md` + `src/components/modals/IntroModal.vue` — chỉ khi §2.3 đổi hành vi nhìn thấy được.
- `CHANGELOG.md` mục `[Unreleased]` — **không đánh số phiên bản** (`RULE-release.md` §A5).
- `docs/plan/terminal-input-surface.md` → `docs/plan/done/` sau khi §2.3 đóng.
- File này + file research song sinh → `docs/plan/done/` và giữ nguyên chỗ cho research, khi cả §2
  đóng hết.

---

## 6. Trạng thái commit

Toàn bộ công việc terminal ở trên **chưa commit**: `useTerminalTextDrain.js` (mới),
`TerminalView.vue` (sửa), `useTerminalInput.js` + `useWkImeGuard.js` (đã xoá),
`terminal-vietnamese-ime-root-cause-3.md` (dòng status "superseded"), `-4.md` (mới),
`terminal-input-surface.md` (mới), cùng hai file jul31 này.

Vì §2.1 là blocker, **đây là quyết định của chủ sở hữu** — commit ngay để có mốc lùi cho hội đồng,
hay giữ trong cây làm việc cho tới khi double space được giải. Không tự quyết.
