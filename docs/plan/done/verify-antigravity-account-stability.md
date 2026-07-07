# Plan — Verify & harden Antigravity account-switch stability

Status: **DONE** — A superseded, B done, C dropped (cập nhật 2026-07-08, xem correction note ngay dưới)

> **⚠️ Correction 2026-07-07:** Mục A dưới đây (kỳ vọng "header phải mất hết data / về trạng thái
> trống, không còn hiện account X hay Y" sau logout) đã bị **PO bác bỏ và sửa lại** — đó không phải
> bug, mà đúng là mục tiêu thật của cache multi-account (xem hiện trạng lần cuối của từng account).
> `resetAccount()`/`clearAgStore()` viết theo kỳ vọng SAI này từng gây ra regression 1.9.3 (xóa sạch
> lịch sử mọi account mỗi lần logout). Nguồn chân lý hiện tại:
> `docs/arch/usage-antigravity.md` § "Log Out behavior & cache retention". **Không dùng mục A bên dưới
> để test/verify nữa** — nó mô tả hành vi cũ đã bị đảo ngược. Mục B đã implement xong (xem ghi chú tại
> mục B).
>
> **⚠️ Quyết định 2026-07-08 (PO): Mục C bị DẸP BỎ, không làm.** Lý do: mục C chỉ là phân loại lý do
> "offline" (IDE chưa mở / mid-restart / signed-out / timeout) cho mục đích **log/diagnostic** — không
> có bug report gốc, không có hành vi sai nào hiện tại phụ thuộc vào nó (UI đã hiển thị đúng trạng thái
> cached/empty state dù không biết lý do chi tiết). Effort trung bình (đổi kiểu trả về IPC dùng chung
> với Claude Code) không xứng đáng với lợi ích thuần logging. Không backlog lại trừ khi có nhu cầu cụ
> thể mới phát sinh.

Cross-refs:
- Fix vừa commit: `a26b8f5` — `src/composables/useAgentUsage.js` (`persistAgAccount`, `resetAccount`, `pendingRecheck`), `src/components/AgentUsage.vue` (`logout-success` emit), `src/components/AgentUsageSlot.vue`
- Doc kiến trúc: `docs/arch/usage-antigravity.md`, `docs/arch/logger.md`
- Code liên quan: `src-tauri/src/agent_usage.rs` (`get_antigravity_usage`, `logout_antigravity`), `scripts/get-antigravity-usage.js`, `src/composables/useAgentUsage.js`

Gồm 3 việc, độc lập nhau:
- ~~**A. Test protocol** cho fix vừa làm (item 1) — confirm hết bug thật, không phải chỉ build pass.~~
  **SUPERSEDED** — xem correction note ở trên.
- **B. `viewingEmail` auto-reset` (item 2) — lỗ hổng cùng loại, chưa vá.** ✅ **Đã implement**
  (commit `b082d0d`: auto-reset `viewingEmail` khi live account đổi sang email hoàn toàn mới).
- **C. Phân loại lý do "offline"** ở tầng Rust/script (item 3) — hiện gộp hết vào `Ok(None)`.
  ❌ **Dẹp bỏ 2026-07-08** (quyết định PO) — thuần diagnostic/logging, không có bug thật phụ thuộc
  vào nó, effort không xứng lợi ích. Xem mục C gốc bên dưới (giữ để tham chiếu lịch sử, không phải
  backlog).

---

## Cách bật debug + đọc log (bắt buộc cho cả 3 phần)

```bash
# Chạy app từ terminal với debug bật (không phải double-click từ Finder)
/Applications/Aki\ Dev\ Sync.app/Contents/MacOS/aki-dev-sync --debug
```

Log file: `~/Library/Application Support/aki.devsync/usage.log` (đường dẫn chính xác cũng in ra ở dòng `[USAGE:init] log_file=...` lúc startup, cả trong terminal stderr lẫn DevTools F12 console).

Tag cần lọc: `USAGE:antigravity` (frontend, qua `ulog`) và `GET_USAGE`/`LOGOUT:antigravity` (Rust, qua `logger::*`).

```bash
tail -f ~/Library/Application\ Support/aki.devsync/usage.log | grep -E "USAGE:antigravity|LOGOUT:antigravity"
```

Mỗi dòng log đã có timestamp `YYYYMMDD.HHMMSS.mmm` — khi báo cáo, cứ paste nguyên đoạn log quanh thời điểm test (không cần tóm tắt), kèm mốc giờ hành động thật (vd "14:32 tôi bấm Log Out", "14:33 tôi login lại bằng account khác").

---

## A. Test protocol — xác nhận fix `a26b8f5` (item 1) — SUPERSEDED, đừng dùng để test

> Kỳ vọng ghi trong A1 bên dưới ("header phải mất hết data... không còn hiện account X hay Y") đã bị
> đảo ngược 2026-07-07 — xem correction note ở đầu file. Giữ lại nguyên văn chỉ để tham chiếu lịch sử.

### A1. Kịch bản gốc (bug report ban đầu)

1. Mở app debug (`--debug`), mở AG đang login account X. Đợi 1 vòng poll thấy đủ 4 số liệu (không N/A).
2. Trong AG, đổi sang account Y (không qua nút Log Out của app này). Quan sát app: có hiện N/A không, bao lâu thì tự hồi phục.
3. Từ app, bấm **Log Out** (dropdown account → Log Out). Xác nhận AG bị đóng.
4. Tự mở lại AG, login bằng account Z (khác X, Y).
5. Quay lại app, bấm **Reload** vài lần liên tiếp, quan sát email hiển thị + 4 số liệu theo từng lần.

**Kỳ vọng sau fix:**
- Ngay sau bước 3 (Log Out xong), header phải mất hết data / về trạng thái trống — **không** còn hiện account X hay Y.
- Log phải có dòng `ag account reset (post-logout)` (từ `resetAccount()`) ngay sau `LOGOUT:antigravity cleared auth rows...`.
- Sau bước 4-5, email hiển thị phải khớp Z trong vòng tối đa 1-2 lần Reload — không quay lại X/Y, không kẹt N/A vĩnh viễn nếu quota đã từng fetch thành công ít nhất 1 lần cho Z.

**Log cần chụp lại:** toàn bộ đoạn từ dòng `ag logout` (lúc bấm nút) tới dòng `ag live fetched email=<Z>` đầu tiên sau đó. Nếu Z vẫn không lên đúng sau nhiều lần reload, tìm dòng `checkUsage queued (isChecking=true)` — nếu liên tục xuất hiện mà không thấy `ag live fetched` theo sau, nghĩa là `get_antigravity_usage` phía Rust liên tục trả `None` (script fail) → xem thêm phần C.

### A2. Kịch bản đầu độc cache (item #1 gốc trong hypothesis)

1. Login account X, đợi có đủ số liệu (quotaSummary hợp lệ).
2. Đổi nhanh sang account Y ngay trong AG (không logout qua app) — cố tình bắt đúng lúc `RetrieveUserQuotaSummary` có thể fail do session chưa ổn định.
3. Theo dõi log: tìm dòng `ag live fetched email=Y` — nếu ngay sau đó widget hiện N/A, kiểm tra log Rust `GET_USAGE` xem `quotaSummary` có null không (grep `quotaSummary` hoặc xem raw JSON trong `content_preview`).
4. Đợi vài poll tiếp theo (mỗi 30s mặc định) — số liệu Y phải tự lấp đầy mà **không cần** Reload/Logout thủ công, và khi quay lại xem account X qua dropdown, cache X vẫn phải giữ số liệu cũ (không bị Y ghi đè do cùng object reference — check riêng).

**Kỳ vọng sau fix:** một khi Y đã từng có `quotaSummary` hợp lệ 1 lần, N/A tạm thời lúc đổi account không được phép "đóng băng" — poll kế tiếp phải ghi đè đúng. Nếu N/A vẫn treo qua >2 poll (>60s) dù AG đang chạy bình thường, đó là dấu hiệu fix chưa đủ, cần xem lại `persistAgAccount`.

### A3. Kịch bản Reload bị nuốt

1. Trong lúc app đang loading (icon reload đang quay / `loading.value=true` — dễ bắt nhất ngay sau khi mở lại AG, probe chậm), bấm thêm Reload 1 lần nữa.
2. Log phải có dòng `checkUsage queued (isChecking=true)` theo sau bởi 1 lần `checkUsage start` mới (không phải im lặng không có gì).

---

## B. `viewingEmail` auto-reset (item 2 — chưa implement, đề xuất)

**Vấn đề:** `viewingEmail` (trong `useAgentUsage.js`) dùng để "pin" xem cache của 1 account cũ qua dropdown. Nó chỉ reset về `null` (theo live) khi: gọi `selectAccount(null)` thủ công, hoặc `resetAccount()` (chỉ chạy sau Log Out). Nếu `viewingEmail` đang trỏ vào 1 email mà sau đó biến mất khỏi `accounts` (bị AG đổi account, hoặc cache bị dọn), UI có thể kẹt ở view cũ vô thời hạn — mọi live fetch mới bị gate ra (`useAgentUsage.js`, điều kiện `viewingEmail.value === null || viewingEmail.value === activeEmail.value` mới cho ghi vào `data.value`).

**Test trước khi vá (để xác nhận có thật hay không, tránh vá nhầm chỗ):**
1. Có ít nhất 2 account trong dropdown (X đã từng fetch, Y đang live).
2. Bấm dropdown, chọn xem X (pin vào X — không phải account live).
3. Trong AG đổi sang account Z hoàn toàn mới (chưa từng có trong list).
4. Quan sát: header có tự động nhảy theo Z không, hay vẫn kẹt hiện X?
5. Log cần xem: `ag select account email=X` rồi sau đó có `ag live fetched email=Z viewing=X` lặp lại nhiều lần mà `data.value` không đổi (không có dòng log riêng cho việc *không* update — cần thêm log debug tạm thời nếu muốn quan sát rõ, hoặc quan sát UI trực tiếp là đủ).

**Đề xuất fix (nếu B được xác nhận là bug thật):**
Trong `checkUsage()`, nhánh `agentName === 'antigravity'` khi có live fetch mới: nếu `viewingEmail.value` không còn khớp bất kỳ `activeEmail` nào từng live trong phiên này (tức người dùng đang pin 1 account "chết", không phải account vừa chọn để so sánh có chủ đích), tự động reset `viewingEmail.value = null`. Cân nhắc thêm điều kiện thời gian (vd pin quá X phút không có hoạt động) để không phá tính năng "ghim xem account cũ" hợp lệ khi người dùng chủ động muốn so sánh lâu dài.

---

## C. Phân loại lý do "offline" ở tầng Rust/script (item 3 — DẸP BỎ 2026-07-08, giữ nguyên văn để tham chiếu lịch sử)

**Vấn đề:** `get_antigravity_usage` (`agent_usage.rs`) gộp **mọi** exit code ≠ 0 của `scripts/get-antigravity-usage.js` thành `Ok(None)` — IDE chưa mở, đang mid-restart, vừa signed-out, hay probe timeout đều trông giống hệt nhau ở tầng Rust/frontend. Điều này làm mất khả năng phân biệt "đang trong quá trình chuyển đổi, sắp có data" với "thực sự offline" — ảnh hưởng tới việc quyết định retry nhanh hay chờ đủ 30s.

**Test để đánh giá mức độ cần thiết:**
1. Trong lúc test A/B ở trên, mỗi lần thấy `soft-miss (offline→cache)` trong log, chép nguyên message stderr đi kèm (đã có sẵn, không cần code thêm — xem `agent_usage.rs:532`, dòng log hiện tại: `soft-miss (offline→cache): <stderr>`).
2. Gom lại xem trong thực tế các nguyên nhân gặp phải là gì (process not running / port not found / Connect API not found / signed out / parse error khác) và tần suất — nếu >90% case chỉ là "IDE not running" hoặc "signed out" (2 loại đã có message phân biệt sẵn trong script, xem `console.error(JSON.stringify({error: ...}))` các dòng khác nhau), có thể **không cần** sửa gì thêm ở Rust — chỉ cần frontend đọc `stderr` reason (hiện đang bị bỏ qua hoàn toàn, `Ok(None)` không mang theo message) nếu muốn hiển thị tinh hơn.

**Đề xuất fix (chỉ làm nếu test C cho thấy cần thiết):**
`get_antigravity_usage` đổi `Ok(None)` thành `Ok(None)` + kèm reason string (đổi kiểu trả về hoặc thêm field), để frontend `checkUsage()` có thể log/hiển thị "đang chờ AG khởi động lại" khác với "chưa đăng nhập" mà không cần đoán qua thời gian. Đây là thay đổi kiểu dữ liệu (`AgentUsageResponse` hoặc kiểu trả `Result`), cần rà lại toàn bộ chỗ gọi `get_agent_usage` (dùng chung với Claude Code) — effort trung bình, không làm nếu test C cho thấy không đáng.

---

## Việc cần user làm

**Cập nhật 2026-07-07: không còn việc nào cần user làm trong file này.** Mục A không cần test nữa
(superseded). Mục B đã implement xong từ `b082d0d`, không cần verify riêng. Mục C là backlog kỹ
thuật thuần túy (không có bug report gốc), chỉ làm khi có nhu cầu — không chủ động yêu cầu user test.

<details><summary>Nội dung gốc (trước 2026-07-07), giữ để tham chiếu lịch sử</summary>

1. Bật `--debug`, chạy 3 test A1/A2/A3 → xác nhận fix `a26b8f5` đã đủ hay chưa.
2. Nếu rảnh, chạy thêm test B (pin account rồi đổi sang account mới hoàn toàn) → xác nhận có bug thật không.
3. Gom log thô (không cần lọc/tóm tắt) theo từng test, kèm mốc thời gian hành động thật, gửi lại để phân tích tiếp.

</details>
