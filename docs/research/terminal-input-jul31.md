# Terminal input — bản ghi phiên test trên máy thật

**Plan song sinh:** [`docs/plan/terminal-input-jul31.md`](../plan/terminal-input-jul31.md) — cùng tên,
cùng ngày. File này là **bản ghi sự kiện, không sửa về sau** (`docs.B2`); mọi quyết định và việc còn
phải làm nằm ở plan.

---

## 1. Start time

2026-07-31. Lần đầu tiên toàn bộ bề mặt input của terminal được test trên máy thật kể từ khi kiến
trúc mới được viết.

## 2. Initial purpose

**Câu hỏi:** kiến trúc "xterm owns keys; the app owns text" — đã chốt ở
[`terminal-vietnamese-ime-root-cause-4.md`](terminal-vietnamese-ime-root-cause-4.md) §7 và hiện thực
hoá thành `src/composables/useTerminalTextDrain.js` — có thực sự làm việc trên máy thật không, và
những gì trong bảng "chưa xác nhận" đóng được bao nhiêu?

**Bối cảnh và ràng buộc tại thời điểm test** (cần thiết để một người đọc sau này biết kết quả còn giá
trị hay không):

- Baseline: cây làm việc **chưa commit**. `useTerminalTextDrain.js` (mới, ~290 dòng),
  `src/components/TerminalView.vue` (sửa), `src/composables/useTerminalInput.js` +
  `useWkImeGuard.js` (đã xoá — tổng 750 dòng). HEAD tại `95a540f`.
- Chưa có mục nào trong bảng 8 quan sát của
  [`terminal-input-surface.md`](../plan/terminal-input-surface.md) §6 từng được chạy. Toàn bộ nhánh
  này cho tới lúc đó là suy luận từ mã nguồn xterm 5.5.0 và WebKit, không có dữ liệu máy thật.
- `-4.md` tự ghi nhận một điểm bất định còn lại: không rõ carrier của OpenKey có được gắn
  `keyCode 229` hay không. Thiết kế được cho là bền với cả hai chiều.

## 3. Strategy

Chạy hết một lượt danh sách test tối thiểu do agent đề xuất và chủ sở hữu thực hiện — 11 mục xếp
theo mức nghiêm trọng (`A` sticky modifier → `B` gõ tiếng Việt trực tiếp → `C` compose row →
`D` hiển thị → `E` đường lùi), trên **hai nền tảng thật**: máy Mac (mặt host + trình duyệt Chrome) và
một máy Android dùng Gboard. Quan sát được ghi nguyên văn chuỗi ký tự nhận được, không diễn giải tại
chỗ.

## 4. Checklist — các bước đã chạy

| # | Bước | Kết quả |
|---|------|---------|
| A1 | Latch Ctrl/Shift, đọc trạng thái nút | PASS hành vi / FAIL hiển thị (§5.1) |
| A2 | Latch Ctrl → gõ `c` trong ô compose, tiến trình phải chết | PASS (§5.1) |
| A3 | Latch Shift + gõ tiếng Việt trong ô compose | Không kết luận được — bị §5.2 che khuất |
| B4 | Gõ trực tiếp tiếng Việt bằng OpenKey | Ký tự đúng, nhưng double space (§5.2) |
| B5 | OpenKey auto-restore (`exit`, `warning`, `wasm`) | PASS, và không dính double space (§5.4) |
| B6 | Phím chức năng sau khi trả `disableStdin` về `false` | PASS |
| C7 | Shift+Enter trong ô compose | PASS — "hoàn hảo" |
| C8 | Paste nhiều dòng / bracketed paste | Bỏ qua theo quyết định chủ sở hữu |
| D9 | Compose row + key row chỉ hiện trên companion | PASS |
| D10 | Font ô compose nhất quán | PASS — "hoàn hảo, nhất quán" |
| E11 | Escape hatch `aki-input-mode='legacy'` | Không chạy — chủ sở hữu yêu cầu gỡ bỏ (§5.6) |
| — | Bổ sung: gõ bằng macOS Telex (bộ gõ dựng sẵn) | Hỏng, chốt ngoài phạm vi (§5.3) |
| — | Bổ sung: gõ bằng Gboard trên Android thật | Sai chữ, không double space (§5.5) |

## 5. Result

Defect gốc của cả chuỗi nghiên cứu — `"ăn gì" → "ăn g"`, tức xterm 5.5.0 cắt `event.key` nhiều ký tự
xuống một đơn vị UTF-16 — **đã hết**. Kiến trúc đúng. Nhưng lần test phơi ra **ba defect mới**, trong
đó một cái là blocker và **không nằm trong bất kỳ giả thuyết nào của chuỗi nghiên cứu trước đó**.

### 5.1 A1/A2 — latch modifier chạy đúng, chỉ không sáng nút

Bằng chứng hành vi ở hai chương trình độc lập: latch Shift → `Tab` trong Claude Code cho backtab
đúng chiều; latch Ctrl → `x` trong `nano` mở prompt thoát, tức `0x18` thật sự tới PTY. A2 (latch Ctrl
rồi gõ `c` **trong ô compose**, không phải trên hàng phím) giết được tiến trình.

Nghĩa là toàn bộ tầng logic — latch object `pending`, funnel `emitKey`, `ctrlByteFor`, và việc latch
áp dụng cả trong ô compose (`terminal-input-surface.md` §4.3) — đúng.

Sai duy nhất: nút `Ctrl` / `Shift` **không sáng** khi đang armed, nên người dùng không biết mình đã
bật hay chưa.

### 5.2 Double space — blocker, không liên quan bộ gõ

Gõ vào terminal trong app, nhận được nguyên văn:

```
-> kiểm  tra  những  thay  đổi  lần  này  và  bá o  cáo  ngắn  gọn
```

- Mọi **dấu cách** bị nhân đôi; **chữ cái thì không**, kể cả chữ có dấu.
- Xảy ra **kể cả khi không bật bộ gõ nào**, kể cả khi gõ thuần tiếng Anh.
- Quan sát trên **trình duyệt Chrome** (mặt companion / web).
- Chuỗi `bá o` cho thấy còn một dấu cách **chen vào giữa một âm tiết** — không đơn thuần là "mỗi lần
  nhấn space gửi hai lần".

Đây là blocker: nó làm mọi kết quả tiếng Việt khác không đọc được, và nó che khuất A3.

### 5.3 macOS Telex (bộ gõ dựng sẵn) — hỏng, chốt ngoài phạm vi

Gõ `ăn gì, công việc ` (có dấu cách cuối) ra:

```
ăn ăn gì, gì công công việc việc 
```

Mỗi **từ** bị nhân đôi tại thời điểm commit của bộ gõ — khác hẳn hình dạng của §5.2 (vốn nhân đôi ký
tự cách).

### 5.4 B5 — OpenKey auto-restore đúng, và **không** dính double space

Gõ, nhận được:

```
tôi  cần  exit warning wasm 
```

OpenKey tự khôi phục `exit` / `warning` / `wasm` — đúng. Điểm đắt giá: **phần sau khi auto-restore
không bị double space**, trong khi phần đầu (`tôi  cần`) thì có. Cùng một phím space, cùng một phiên
gõ. Cơ chế: OpenKey tự sửa **sau khi nhấn phím cách**.

### 5.5 Android / Gboard — không double space, nhưng sai chữ

Gõ `ăn gì đi nhé bây giờ cần một`, nhận:

```
aăn giì dđi nheé baây gioơờ caânần moôtột
```

Mỗi ký tự bị bộ gõ sửa đều xuất hiện **cả dạng gốc lẫn dạng đã sửa**, kề nhau: `a`+`ă`, `d`+`đ`,
`câ`+`ân`+`ần`. Tức bản gốc được gửi trước, bản đã sửa gửi tiếp, còn **thao tác xoá ở giữa không tới
được PTY**.

### 5.6 E11 — chủ sở hữu yêu cầu gỡ escape hatch

Không chạy. Lý do nêu ra: đường legacy vốn đầy lỗi, giữ lại chỉ làm code rộng, rối, thành rác.

### Verification

- **Đã xác minh trên máy thật, bởi chính chủ sở hữu**, hai nền tảng (macOS + Android/Gboard). Toàn bộ
  §5.1–§5.6 là quan sát trực tiếp, không suy luận.
- §5.1 có **bằng chứng chéo**: hai chương trình khác nhau (Claude Code, `nano`) cùng xác nhận byte
  tới PTY đúng, nên kết luận "lỗi ở hiển thị chứ không ở input" là chắc, không phải phỏng đoán.
- §5.2 **chưa truy nguyên**. Nguyên nhân chưa biết tại thời điểm ghi. §5.4 là quan sát phân biệt
  (discriminating observation) đắt nhất để truy nó về sau: bất kỳ giả thuyết nào không giải thích
  được "vì sao đoạn sau auto-restore lại không bị" đều sai.
- §5.5 **chưa truy nguyên**, và **chưa chứng minh cùng gốc với §5.2** — khác nền tảng, khác hình dạng.
- Điểm bất định mà `-4.md` nêu (carrier của OpenKey có 229 hay không) **vẫn chưa được đo**: lần test
  này không đọc `__akiTermInput.tail(20)`, nên câu hỏi đó còn nguyên.

### Corroborating links

- [`terminal-vietnamese-ime-root-cause-4.md`](terminal-vietnamese-ime-root-cause-4.md) — kiến trúc
  đang được test; §7 mô tả đúng cái đang chạy, §5.3 có một tiền đề bị lần test này bác bỏ (§5.6).
- [`terminal-input-surface.md`](../plan/terminal-input-surface.md) §6 — bảng 8 quan sát chưa xác nhận
  mà lần test này chạy hết.
- `src/composables/useTerminalTextDrain.js` — mã được test.

## 6. Decision

**Action** — mở [`docs/plan/terminal-input-jul31.md`](../plan/terminal-input-jul31.md) để sắp xếp
phần còn lại: một blocker (§5.2), một defect Android (§5.5), một defect hiển thị (§5.1), và một việc
gỡ bỏ (§5.6). Blocker được giao cho một phiên hội đồng riêng.

**No action, có lý do** — hai mục bị loại bỏ có chủ ý, ghi ra để không bị đọc nhầm thành sơ suất:

- **macOS Telex (§5.3)**: VS Code cũng hỏng tương tự với bộ gõ này, nên đây không phải khiếm khuyết
  riêng của app và không đáng đầu tư. **Chỉ hỗ trợ OpenKey.** Một phiên sau gặp lại `ăn ăn gì, gì`
  phải đọc mục này trước khi coi đó là hồi quy chưa ai biết.
- **Paste nhiều dòng / bracketed paste (C8)**: chủ sở hữu đánh giá quá phức tạp so với giá trị. Câu
  hỏi bỏ ngỏ ở `terminal-input-surface.md` §3.4 (xterm 5.x có phơi ra trạng thái bracketed-paste đọc
  được không) **không cần trả lời nữa**.

**Follow-up research** — chưa mở. Phiên hội đồng xử lý §5.2 sẽ mở doc kế tiếp trong chuỗi
`terminal-vietnamese-ime-root-cause-*` nếu kết luận của nó làm `-4.md` hết hiệu lực.

**Cross-references** — các doc chịu ảnh hưởng của quyết định này, ngoài plan song sinh:
`docs/index.md` (mục lục), `CHANGELOG.md` (mục `[Unreleased]` mô tả text drain phải nói rõ hai defect
còn mở), `docs/plan/terminal-input-surface.md` (nay chỉ còn hiệu lực cho §5.1).
