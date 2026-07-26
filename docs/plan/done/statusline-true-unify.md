# Plan: Statusline — hoàn thiện script unified làm chuẩn (Phase 1), rồi mới port vào `statusline.rs` (Phase 2)

**Status:** **XONG TOÀN BỘ (2026-07-23)** — Phase 1 (bản chuẩn + spec §8), Phase 2.1 (Vue Customizer),
Phase 2.2 (generator Rust). Bản chuẩn `src-tauri/src/statusline-unified.sh` giờ **chính là template** mà
`statusline.rs` nhúng bằng `include_str!`; hợp đồng generator ở spec §8.3b. **Đã nghiệm thu trên Mac
(2026-07-23):** `npm run test:statusline` pass 29/29, Apply thật (AGY → host `bien`) ra statusline
đúng sau khi khởi động lại `agy`. Còn lại đúng một việc: bump version + chốt mục `[Unreleased]` khi
phát hành. Phase 1 chạy trên máy remote khác (không phải máy Mac dev đang chứa
`src-tauri`), **không đụng code Rust** — mục tiêu là tự tay hoàn thiện 1 script chuẩn (nội dung +
default color) bằng cách thử nghiệm trực tiếp với payload thật của cả 2 CLI, làm chuẩn để sau này
đối chiếu khi port vào generator. Phase 2 (port vào `statusline.rs`) là việc **riêng, sau này**, chỉ
bắt đầu khi Phase 1 đã chốt và người dùng xác nhận.
**Background:** `docs/ref/statusline-unified-spec.md` §7-8 (điều tra thực địa 2026-07-23),
`docs/plan/1.18.0-statusline-apply-correctness.md` (ma trận bug gốc của kiến trúc generator hiện tại).

---

## 0. Vì sao tách 2 phase

Bài học từ `1.18.0-statusline-apply-correctness.md` §0: một lần sửa trực tiếp trong `statusline.rs`
mà không có bản tham chiếu đã verify bằng tay đã tạo ra "3 bản script khác nhau" và các fix đi vào
bản chết. Lần này làm ngược lại: **trước tiên hoàn thiện 1 script chuẩn bằng tay, verify bằng payload
thật trên máy remote**, chốt xong nội dung + màu mặc định rồi mới đưa vào code sinh tự động. Không
làm tắt hai bước gộp lại.

Phase 1 **không** đụng `src-tauri/`, không bump version, không cần `cargo test` — đây là nghiên cứu
độc lập với app.

---

## 1. Phase 1 — Nghiên cứu & hoàn thiện script chuẩn trên máy remote

### 1.1. Thu thập payload thật của cả 2 CLI trên máy đó

Bản thân script hiện có (dù là bản cũ, sửa tay) đã có sẵn dòng debug hữu ích:
```sh
echo "$input" > /tmp/statusline_stdin_dump.json
```
Tận dụng file này (hoặc thêm dòng tương tự vào bản đang thử nghiệm) để bắt payload thật từ:
- 1 phiên Claude Code thật (ít nhất 1 turn có `rate_limits`, ít nhất 1 turn KHÔNG có, để verify rlcache).
- 1 phiên AGY CLI thật (model Gemini, model 3P/Claude nếu có để verify nhánh quota).

Lưu các payload mẫu này vào file cục bộ (ví dụ `docs/ref/statusline-sample-payloads/` hoặc tương
đương) để làm test fixture cho Phase 2 sau này — không cần đoán field name, verify bằng dữ liệu thật.

### 1.2. Viết/chỉnh script chuẩn trực tiếp, không qua generator

Làm việc trên **1 file duy nhất** (đúng đề xuất §8 của `statusline-unified-spec.md`), cài trực tiếp
vào cả 2 đường dẫn thật để test sống:
```
~/.claude/statusline-command.sh
~/.gemini/antigravity-cli/statusline.sh
```
Bám theo bảng ánh xạ jq đã có ở `docs/ref/statusline-unified-spec.md` mục 1-5 làm điểm khởi đầu.

**~~Gate `aki-rlcache` bằng `.model | type == "object"`~~ — ĐÃ BÁC BỎ.** Payload thật cho thấy
`.model` của CC **cũng là object**, gate này nhận nhầm CC thành AGY. Chốt lại: gate bằng đường dẫn
gọi `$0` (`*/.gemini/*` → AG). Xem spec §8.1.

### 1.3. Thử nghiệm cùng người dùng — chốt default color

Đây là phần cần làm **tương tác trực tiếp** trên máy đó (không phải việc tự động hoá được): chạy thử
statusline sống trong terminal thật, cùng người dùng xem và tinh chỉnh:
- 5-tier color ladder (thresholds mặc định) — có còn hợp với 2 CLI cùng lúc trên terminal đó không.
- Bảng "Color doctrine" hiện có ở `docs/feat/statusline-customizer.md` (white=label, cyan=info,
  grey=qualifier, dynamic=ladder, magenta=machine/git) — verify còn đọc tốt trên theme terminal thật
  của máy đó, đặc biệt 2 badge `CC`/`AG` cạnh nhau.
- Bất kỳ field nào cần thêm/bớt cho unified mà bản 2-target hiện tại chưa có.

### 1.4. Chốt lại thành tài liệu chuẩn

Khi đã hài lòng với kết quả sống trên terminal, cập nhật `docs/ref/statusline-unified-spec.md`:
- Dán nội dung script chuẩn cuối cùng (hoặc trỏ tới file đã lưu) làm "reference implementation".
- Chốt bảng default color (giá trị hex/ANSI thật đã dùng, không phải giá trị cũ trong
  `statusline-customizer.md` nếu có đổi).
- Đánh dấu §8 chuyển từ "đề xuất" → "đã chốt làm chuẩn, chờ port vào generator (Phase 2)".

**Không sửa `statusline.rs` ở Phase 1** dù có chạy trên máy nào — kể cả khi máy đó cũng compile được
Rust. Việc port là Phase 2, có plan riêng, chỉ bắt đầu sau khi người dùng xác nhận chuẩn đã chốt.

---

## 2. Phase 2 — Đưa bản chuẩn vào app (XONG)

**Trình tự bắt buộc: 2.1 (Vue/UI) TRƯỚC, 2.2 (Rust) SAU.** Chốt xong cách trình bày trên UI rồi mới
đụng generator — làm ngược lại thì generator phải sửa hai lần khi UI đổi ý.

### 2.1. Vue/UI trước — chốt cách trình bày (XONG)

File: `src/components/modals/*` của Statusline Customizer (xem `docs/feat/statusline-customizer.md`).

- **Ô nhập độ dài truncate cho từng field**, sàn `3`, trần theo field (cwd/branch `15`, còn lại `12`),
  default theo spec §8.2: account **4**, user **5**, host **6**, cwd **12**, branch **10**.
  Account: UI phải nói rõ là cắt **sau khi bỏ domain** (`lva@akitao.com` → `lva`), không phải cắt thô.
- Thứ tự field mặc định đổi: `cache` lên ngay sau `context` (spec §8.2).
- Model và effort dính liền, không khoảng trắng.
- Nền tên account: `48;5;252`.
- **Bỏ ô chọn ký tự/màu phân cách** — không còn dấu `|`. Thay bằng 2 ô chọn nền zebra
  (`BG_ZEBRA_A`=16, `BG_ZEBRA_B`=235), picker **giới hạn trong dải xám trung tính 232..255**, kèm ô
  tick "separate" (`SEPARATE_BLOCKS`, **mặc định bật**) đệm 1 khoảng trắng hai bên mỗi khối.
- **Gated fieldset — cổng KHÔNG được ghi đè con.** Toggle cha (`cache`, `5h`, `7d`) chỉ quyết định
  nhóm có hiển thị/tương tác được hay không; state `enabled` của các field con **giữ nguyên** khi tắt
  cổng, bật lại là khôi phục đúng lựa chọn cũ. Quan hệ khai báo một chỗ duy nhất (map `DEPENDS`), cả
  checkbox bị khoá lẫn preview đều đọc từ đó.
- **Không migration.** Default đổi nghĩa thì bump `CONFIG_VERSION`, config cũ bị vứt thẳng, không dịch
  ngược — nếu không người dùng phải bấm Reset mới thấy chuẩn mới.
- Bảng màu + 5-tier ladder giữ nguyên hiện trạng — người dùng đã chốt, UI không được đổi default.

### 2.2. Rust — XONG

**Cách giải quyết cuối cùng khác với dự kiến ban đầu, và tốt hơn:** thay vì port từng khối shell vào
code sinh chuỗi của Rust (đúng cái đã đẻ ra "3 bản script khác nhau"), `src-tauri/src/statusline-unified.sh`
được `include_str!` thẳng vào binary và generator **chỉ thay vùng giữa 2 marker** `AKI-GENERATED-CONFIG`.
Toàn bộ 4 bug dưới đây biến mất theo cách cấu trúc — không còn bản thứ hai của script để lệch. Hợp đồng
đầy đủ: spec §8.3b.

Bốn bug đã phát hiện khi đọc `statusline.rs` ở Phase 1 (giữ lại để đối chiếu lịch sử):

Đọc code thật ngày 2026-07-23. Hai bug đầu **không** nằm trong ma trận của
`1.18.0-statusline-apply-correctness.md` — Apply lại mà không sửa thì lỗi vẫn còn / còn làm mất tính năng.

1. **Reset time CC không bao giờ hiển thị.** Generator có đọc `.rate_limits.*.resets_at` nhưng chỉ để
   tính hạn cho `aki-rlcache` (`statusline.rs:348`); nó **không export biến nào ra cho khối hiển thị**,
   nên statusline CC không có ETA reset. Phải thêm `JSON_CLAUDE_5H_RESET` / `JSON_CLAUDE_7D_RESET` và
   quy về giây tương đối (`resets_at - now`) — xem spec §2 và `src-tauri/src/statusline-unified.sh`.
2. **Mất nhánh quota `3p-*`.** `statusline.rs:692-695` chỉ đọc `gemini-5h` / `gemini-weekly`. Chạy
   model Claude/GPT trong AGY sẽ **không hiện quota nào cả**. Bản chuẩn có đủ cả 2 nhánh, chọn theo
   `.model` có chứa `"gemini"` hay không (spec §3). Đây là **regression khi Apply**: bản sửa tay đang
   cài trên máy có tính năng này, generator thì không.
3. **Gate rlcache + nhãn CLI.** Chuyển sang `$0` như spec §8.1; bỏ hẳn lối suy luận từ `$JSON_MODEL`.
4. **`COLOR_cwd` là code chết.** Script sinh ra có biến `COLOR_cwd` nhưng khối `g_cwd` nhúng cứng
   `MAGENTA_BOLD` và không đọc biến đó. Hệ quả: ô chọn màu CWD trên UI là **nút chết** — generator
   có ghi đúng màu vào biến thì màn hình vẫn ra magenta. Cùng lớp bug với P0-2 (toggle không nối
   được xuống script). Khi port phải kiểm **từng** `COLOR_*` là thật sự được khối tương ứng đọc,
   không chỉ được khai báo.

*Đã đúng sẵn ở bản Rust cũ, đã được mang sang template:* `aki-rlcache v4` (2 gate account + expiry) và
fallback account CC về `~/.claude.json` → `.oauthAccount.emailAddress`. Bản tham chiếu Phase 1 mới chỉ
là v3 — nếu port ngược hướng thì đây đã là một regression.

Phần còn lại của 2.2, đã làm:

- [x] Gộp 2 builder thành **một body duy nhất** dùng chung cho cả 2 đích, mỗi đích chỉ khác ở khâu cài:
      đường dẫn file + patch `settings.json` của CLI đó. Test `both_targets_receive_the_same_body` khoá
      phần body, `each_target_registers_its_script_in_the_cli_settings` khoá phần cài.
      **Sửa sau khi Apply thật (2026-07-23):** đích AGY ban đầu chỉ ghi file, không patch settings —
      `statusLine.command` của AGY để rỗng nên script nằm đó không ai gọi, Apply vẫn báo thành công.
- [x] `default_config()` và command `get_default_statusline_config` **đã xoá** (Vue là SSOT). Vue cũng
      đã bỏ lời gọi nạp default từ backend — chính nó là nguyên nhân bảng UI hiện màu khác script.
- [x] Sinh `EN_*` / `COLOR_*` / `THRESH_*` / `TRUNC_*` / `BG_ZEBRA_*` / `SEPARATE_BLOCKS` / `BLOCK_ORDER`
      từ config UI; khối clamp truncate vẫn nằm trong script (generator không clamp trùng).
- [x] `DEPENDS` resolve ở generator: cha tắt → con ra `0`, config của con không bị sửa.
- [x] Test bất biến: mọi `EN_`/`COLOR_` thân script đọc phải có trong `EN_KEYS`/`COLOR_KEYS` và ngược
      lại → bug "picker chết" như `COLOR_cwd` không thể tái diễn.
- [x] Fixture payload thật (spec §8.4-8.6) chạy qua bộ test tạm trên máy remote: 60 check, all pass —
      từng toggle, từng màu, từng độ dài cắt, zebra/separate, thứ tự khối, ladder, rlcache v4.
- [x] **Bộ test thay hẳn việc tick từng ô rồi Apply rồi soi file bằng tay:** `npm run test:statusline`
      (= `cargo test` trên crate thật, toàn bộ nằm trong `#[cfg(test)]` của `statusline.rs`, không
      file phụ). Phủ **cả 18 công tắc**, có guard: thêm gate vào template mà quên test là fail.
      Xem `docs/research/statusline-generator-test-suite.md`. Lần chạy đầu bắt 1 bug thật:
      `.model` dạng string làm chết cả chương trình jq → statusline trắng trơn (đã sửa + dồn 4 chỗ
      lặp biểu thức chọn quota pool về một mối). Hiện 26/26 pass.
- [x] **Nghiệm thu trên Mac (2026-07-23):** `npm run test:statusline` → 29/29 pass trên crate thật;
      Apply thật (AGY → host `bien`) ra đúng statusline. Hai thiếu sót chỉ lộ khi Apply thật, đã sửa:
      (1) đích AGY không patch `settings.json` nên script không ai gọi; (2) probe trạng thái host chỉ
      biết Claude Code nên chip host không phản ánh AGY — nay mỗi CLI có tag riêng.
- [ ] **Còn lại:** bump version + chốt mục `[Unreleased]` trong CHANGELOG khi phát hành.

---

## 3. Tiêu chí nghiệm thu Phase 1

- [x] Payload mẫu CC thật (có + thiếu `rate_limits`) — spec §8.4.
- [x] Payload AGY thật, cả model 3P lẫn Gemini — spec §8.5-8.6, chạy đúng cả 2 nhánh quota.
- [x] Script chuẩn chạy sống trên terminal thật, người dùng đã xem và duyệt (spec §8.3).
- [x] `docs/ref/statusline-unified-spec.md` §8 đã chốt: script chuẩn (`src-tauri/src/statusline-unified.sh`),
      bảng default color + thứ tự field. Chưa đụng `statusline.rs`.

### 3.1. Đã đóng

Điểm hở trước đây (máy remote không cài AGY CLI nên payload AGY chỉ dựng theo docs) **đã được đóng**:
người dùng cung cấp payload `agy` 1.1.5 thật cho cả model 3P lẫn Gemini (spec §8.5-8.6), chạy lại
đúng cả 2 nhánh quota. Bug `3p-*` ở §2.1 giờ có fixture thật để bảo chứng khi port.

**Về default color:** người dùng đã chốt giữ nguyên toàn bộ màu và thứ tự field hiện tại làm default
(spec §8.2). Phase 2 phải sinh ra đúng bảng đó, không được đổi.
