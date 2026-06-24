# Claude Code Usage Flow — Bug Analysis & Improvement Proposals (v1.2.x)

> Phân tích kỹ thuật các vấn đề phát hiện trong flow giám sát quota Claude Code.
> * **Soạn ngày:** 2026-06-23 18:00 ICT
> * **Cập nhật ngày:** 2026-06-24 12:40 ICT (v1.2.9): Đã tích hợp thành công các đề xuất (B, C, D, E) và giải pháp Probe Session (F).

---

## Bối cảnh

Hệ thống giám sát quota Claude Code hiện tại gồm hai lớp:

1. **Passive (P1):** Hook vá vào `~/.claude/statusline-command.sh` trên Remote. Mỗi khi Claude Code chạy, nó đẩy `rate_limits` vào `stdin` của hook — hook ghi đè vào `~/.claude/rate-limits-cache.json`.
2. **Active fallback (P3):** Người dùng bấm Force Sync → chạy `claude --model haiku -p /usage` trên Remote → Python parser trích xuất `% used` + `resets_at` → ghi đè cache.

Vấn đề phát sinh khi **người dùng không sử dụng Claude Code** trong khoảng thời gian dài (qua 1 hoặc nhiều chu kỳ reset 5 giờ): P1 im lặng, cache cũ nằm im, UI hiển thị sai. Active fallback cũng có vấn đề riêng (xem bên dưới).

---

## ✅ Vấn đề 1 — [SẢI LẦM BAN ĐẦU] `/usage` LÀ GÌ?

> **Đính chính sau thực nghiệm 2026-06-24:** Cả hai giả thiết ban đầu (đọc RAM session và gọi Anthropic API) đều **SAI**.

**Thực tế đã xác minh:**
- `/usage` chỉ đọc **local JSONL session files** trên máy (`~/.claude/projects/**/*.jsonl`) rồi tính toán offline.
- Output ghi rõ: *"Approximate, based on local sessions on this machine — does not include other devices or claude.ai"*.
- Đây là **họ P2** (parse JSONL local), **KHÔNG phải P3** (OAuth endpoint gọi Anthropic API).
- Kết quả: chỉ phản ánh session trên chính máy đó. Nếu máy khác dùng cùng tài khoản, **không phản ánh ở đây**.
- `Last 24h · N requests · M sessions` trong output giảm theo thời gian thực vì nó đếm file JSONL local, không phải từ API.

**Nguyên nhân sai lầm trước:** Đã quan sát thấy `Last 24h` request count thay đổi và kết luận sai là đang gọi API. Thực ra do local JSONL files đang có data mới từ session local, dẫn đến count thay đổi.

---

## 🔴 Vấn đề 1b — Force Sync chậm 3 giây do thiếu `< /dev/null` (**ĐÃ FIX**)

**File:** [scripts/force-sync-claudecode.sh](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/force-sync-claudecode.sh)

**Trước khi sửa:**
```sh
claude --model haiku -p /usage 2>/dev/null
```

**Vấn đề:**
- Khi chạy trong subshell `$()` qua SSH, stdin của lệnh không được redirect → Claude Code chờ stdin 3 giây rồi mới tiếp tục, in warning vào stderr.
- Tổng thời gian Force Sync = 3s chờ stdin + ~2s chạy `/usage` + overhead SSH = **>5 giây** thay vì ~2 giây.
- Cũng không có `cd /tmp` → có thể load project context không cần thiết.

**Sau khi sửa (2026-06-23):**
```sh
BLANK_DIR="/tmp/aki-dev-sync-blank-dir"
mkdir -p "$BLANK_DIR"
cd "$BLANK_DIR" && claude --model haiku -p /usage < /dev/null 2>/dev/null
```
- `/tmp` trực tiếp bị loại vì có thể chứa file bị nhặt làm project context.
- Blank dir chuyên dụng, rỗng hoàn toàn, tạo tự động nếu chưa tồn tại.

**Hậu quả đã giải quyết:** Force Sync giờ chạy ~2 giây thay vì >5 giây.

---

## 🔴 Vấn đề 2 — Cache cũ không bao giờ bị xóa hay vô hiệu hóa (**ĐÃ FIX ở v1.2.9**)

**File:** [scripts/force-sync-parse.py](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/force-sync-parse.py), [scripts/get-claudecode-usage.sh](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/get-claudecode-usage.sh)

```python
if not match:
    print(json.dumps({"parsed": False, "raw_preview": raw_preview}))
    sys.exit(0)  # ← thoát mà không xóa hay reset cache
```

**Nguyên nhân:**
- Khi parse thất bại, file `~/.claude/rate-limits-cache.json` giữ nguyên nội dung cũ với `resets_at` của session cuối cùng.
- `get-claudecode-usage.sh` chỉ kiểm tra file có tồn tại hay không (`[ -f "$FILE" ]`), không hề kiểm tra nội dung có còn hợp lệ không.
- Kết quả: UI mãi hiển thị thanh progress bar của session cuối cùng kèm "Reset 4h ago" hoặc "Reset 9h ago" vô thời hạn.

---

## 🔴 Vấn đề 3 — Passive hook (P1) hoàn toàn im lặng khi không có activity

**File:** [scripts/provision-claudecode.sh](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/provision-claudecode.sh)

Cơ chế P1 vá vào `statusline-command.sh`. Hook này chỉ kích hoạt khi Claude Code **thực sự đang chạy** và đẩy dữ liệu qua `stdin`. Khi không ai dùng Claude Code trên remote, hook im lặng hoàn toàn, file cache không được cập nhật dù quota đã reset.

**Đây là đặc điểm by-design của P1, không phải lỗi** — nhưng không có cơ chế bù đắp cho trường hợp người dùng nghỉ lâu.

---

## 🟡 Vấn đề 4 — `stale` detection không nhận biết trường hợp đã qua reset (**ĐÃ FIX ở v1.2.9**)

**File:** [useAgentUsage.js](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/src/composables/useAgentUsage.js)

```js
stale.value = mtime > 0 && (Date.now() / 1000 - mtime) > 600;
```

**Vấn đề:**
- Logic chỉ so sánh **thời gian ghi file** (`mtime`) với hiện tại. Ngưỡng 10 phút (`600s`) đủ để bắt cache cũ trong điều kiện bình thường.
- Nhưng nếu Force Sync được bấm và parse thất bại → file **không được ghi lại** → `mtime` không đổi → sau 10 phút file bị đánh dấu `stale`.
- Tuy nhiên nếu Force Sync không được bấm trong thời gian dài và mtime đã cũ hơn 10 phút từ trước, badge "Stale" xuất hiện — nhưng `resets_at` trong file vẫn không được cập nhật, UI vẫn sai.
- **Không có logic nào** so sánh `data.rate_limits.five_hour.resets_at` với `Date.now()` để phát hiện "cache này đã qua thời điểm reset."

---

## 🟡 Vấn đề 5 — UI không tự trigger khi phát hiện reset đã qua (**ĐÃ FIX ở v1.2.9**)

**File:** [UsageProgressBar.vue](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/src/components/UsageProgressBar.vue)

```js
// Timer chỉ cập nhật currentTime, không trigger event
timer = setInterval(() => {
  currentTime.value = Math.floor(Date.now() / 1000);
}, 10000);
```

**Vấn đề:**
- Khi `currentTime` vượt qua `resetsAt`, component chuyển sang hiển thị "Reset X ago" và hiện nút Force Sync — nhưng **không tự động emit `force-sync`**.
- Người dùng phải tự nhận ra và bấm tay. Trong thực tế, người dùng có thể không chú ý trong khi chờ quota phục hồi.

---

## ✅ Đề xuất cải thiện

### ~~Cải thiện A — Fallback OAuth endpoint~~ ❌ KHÔNG CẦN

> **Loại bỏ sau thực nghiệm 2026-06-23:** `claude -p /usage` đã tự gọi OAuth API nội bộ. Gọi thêm endpoint OAuth bên ngoài là thừa và thêm attack surface không cần thiết.

---

### Cải thiện B — Phát hiện và vô hiệu hóa cache đã qua `resets_at`

Trong `get-claudecode-usage.sh`, bổ sung kiểm tra `resets_at` so với thời gian hiện tại trước khi xuất file:

```sh
FILE="$HOME/.claude/rate-limits-cache.json"
if [ -f "$FILE" ]; then
  # Kiểm tra resets_at của five_hour
  RESETS_AT=$(jq -r '.rate_limits.five_hour.resets_at // 0' "$FILE" 2>/dev/null)
  NOW=$(date +%s)
  if [ "$RESETS_AT" -gt 0 ] && [ "$NOW" -gt "$RESETS_AT" ]; then
    # Cache đã qua reset, vô hiệu hóa used_percentage để UI biết
    echo "STALE_RESET"
    exit 0
  fi
  # ... xuất bình thường
fi
```

Backend Rust nhận `STALE_RESET` và trả về `Ok(None)` để UI hiển thị trạng thái "cần đồng bộ" thay vì dữ liệu sai.

---

### Cải thiện C — `stale` detection nhận biết `resets_at` trong data

Trong [useAgentUsage.js](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/src/composables/useAgentUsage.js):

```js
// Hiện tại:
stale.value = mtime > 0 && (Date.now() / 1000 - mtime) > 600;

// Đề xuất: bổ sung kiểm tra resets_at đã qua chưa
const fiveHour = data?.rate_limits?.five_hour;
const resetIsPast = fiveHour?.resets_at > 0
  && (Date.now() / 1000) > fiveHour.resets_at;
stale.value = resetIsPast || (mtime > 0 && (Date.now() / 1000 - mtime) > 600);
```

---

### Cải thiện D — Auto-trigger force sync khi reset qua

Trong [UsageProgressBar.vue](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/src/components/UsageProgressBar.vue), khi timer phát hiện `isPast` chuyển từ `false` sang `true`, emit event `timeout` để component cha gọi `forceSync` thay vì chỉ `retry`:

```js
let wasPast = false;
timer = setInterval(() => {
  currentTime.value = Math.floor(Date.now() / 1000);
  const nowPast = props.resetsAt > 0 && currentTime.value > props.resetsAt;
  if (nowPast && !wasPast) {
    emit('timeout'); // ← component cha nên map sang forceSync, không chỉ retry
  }
  wasPast = nowPast;
}, 10000);
```

Hiện tại `@timeout` ở `AgentUsage.vue` đang map sang `$emit('retry')` — nên đổi thành `$emit('force-sync')` để thực sự cập nhật cache thay vì chỉ đọc lại file cũ.

---

## 🔴 Vấn đề 6 — Regex Parser bị fail khi Quota trả về 0% (v1.2.9)

**File:** [scripts/force-sync-parse.py](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/force-sync-parse.py)

**Nguyên nhân:**
- Khi Quota của Claude Code đã được reset hoàn toàn (hoặc chưa dùng gì), lệnh `/usage` trả về:
  `Current session: 0% used`
- Chuỗi output này **hoàn toàn không chứa** từ khóa `resets` hay mốc thời gian reset kế tiếp.
- Regex parser cũ bắt buộc phải có đầy đủ phần trăm và thời gian reset:
  `r'(\d+)%\s*used\s*.\s*resets\s*([a-zA-Z]+\s+\d+),\s*(\d+):(\d+)([ap]m)'`
- Do không khớp, parser trả về `parsed: false` và thoát mà không cập nhật `rate-limits-cache.json`. Cache giữ nguyên `resets_at` cũ trong quá khứ, kích hoạt `STALE_RESET` khiến UI bị kẹt ở trạng thái `"No data — waiting for next session"`.

**Giải pháp:**
- Tách Regex thành 2 phần độc lập:
  - Regex 1 (Bắt buộc): `r'(\d+)%\s*used'` để lấy phần trăm.
  - Regex 2 (Tùy chọn): `r'resets\s*([a-zA-Z]+\s+\d+),\s*(\d+):(\d+)([ap]m)'` để lấy thời gian reset. Nếu không tìm thấy, mặc định `resets_at = 0`.

---

## 🛠️ Hướng dẫn Kiểm thử Thủ công (Manual Testing Guide)

Để kiểm tra trực tiếp thông tin quota qua SSH từ terminal máy local mà không phụ thuộc vào GUI:

1. **Lệnh chạy chuẩn xác:**
   ```bash
   ssh <host> "bash -lc 'claude --model haiku -p /usage < /dev/null'"
   ```
2. **Giải thích các tham số:**
   * `<host>`: Tên host cấu hình trong `~/.ssh/config` (ví dụ: `bien`).
   * `bash -lc`: Bắt buộc sử dụng shell đăng nhập (`-l` / login) và chạy lệnh tương tác (`-c`). Điều này đảm bảo PATH được nạp đầy đủ các biến môi trường và đường dẫn đến thư mục cài đặt `claude`.
   * **Cảnh báo về Shell:** Tránh dùng `zsh` cứng trên môi trường remote chưa chắc chắn cài zsh (ví dụ: `bien` sẽ báo lỗi `zsh: command not found`). Dùng `bash` là an toàn nhất.
   * `< /dev/null`: Tránh việc Claude CLI chờ stdin 3 giây gây chậm trễ tiến trình.

---

## 🟢 Vấn đề 7 — Không thể lấy reset time khi không có session local hoạt động trong 5h (Giải pháp: Kích hoạt Probe Session)

Khi người dùng không mở session Claude Code cục bộ nào trên máy hiện tại trong 5 giờ qua, lệnh `claude -p /usage` sẽ báo `0% used` và **không hiển thị** thời gian reset. Điều này khiến parser không thể lấy được `resets_at` để cập nhật cho UI, làm UI bị kẹt ở trạng thái chờ.

**Giải pháp (Probe Session):**
* **Thực nghiệm nghiên cứu (Ngày 2026-06-24 12:28 ICT):** Chạy thử một session Claude cực ngắn (dummy session) sử dụng mô hình rẻ nhất (Haiku) để yêu cầu phản hồi đơn giản (ví dụ: `"respond with ok"`) trong thư mục `/tmp/`.
* Thao tác này kích hoạt ghi nhận log session cục bộ mới (`~/.claude/projects/**/*.jsonl`), đồng thời ép Claude CLI nhận diện một session hoạt động trong chu kỳ hiện tại.
* Ngay sau đó, chạy lệnh `claude -p /usage` sẽ nhận được đầy đủ mốc `resets_at` của chu kỳ đó.
* Chi phí token cực thấp (~100 tokens của Haiku).
* Đã tích hợp tự động vào [scripts/force-sync-claudecode.sh](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/force-sync-claudecode.sh): Nếu lần chạy đầu không tìm thấy chuỗi `resets` trong output, script sẽ tự kích hoạt probe session trong thư mục tạm, sau đó chạy lại lệnh `/usage`.
* **Kịch bản tự động kích hoạt:** Xem chi tiết các sự kiện tự động kích hoạt Probe Session / Force Sync trong [usage-claudecode.md](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/docs/arch/usage-claudecode.md).

---

## 👥 Đặc thù tài khoản dùng chung (Shared Account)

- `/usage` (P2) chỉ phản ánh các session đã thực hiện trên **máy đó**.
- Vì `/usage` parse local JSONL files, nó không bao gồm: activity từ các thiết bị khác, activity trên web (claude.ai), hay các project không nằm trong scope local.
- Dòng chữ trong output *"does not include other devices or claude.ai"* là khẳng định cho tính chất P2 này.
- Khi làm việc trên nhiều máy, `force-sync` trên máy A sẽ không cập nhật quota đã tiêu thụ trên máy B.

---

## 🔮 Wishlist (Tính năng mong muốn)

* **Phân rã Quota cá nhân (Personal vs. Others):** 
  Do sử dụng chung tài khoản với máy khác, chúng ta mong muốn hiển thị chi tiết lượng quota do chính session local của mình tiêu thụ và lượng quota do người khác tiêu thụ.
  * **Giải pháp đề xuất:** Parser cục bộ sẽ tự đếm tổng số tokens tiêu thụ từ các file logs của session hiện tại (`~/.claude/projects/**/*.jsonl`). Sau đó vẽ thanh Progress Bar gồm 2 màu/2 phần rõ rệt: Phần trăm mình đã dùng và Phần trăm người khác đã dùng.

---

## Tình trạng triển khai (v1.2.9)

| # | Cải thiện | Trạng thái | Ghi chú |
|---|---|---|---|
| 1b | `< /dev/null` + `cd /tmp` trong force-sync script | ✅ **ĐÃ FIX** | Giảm từ >5s xuống ~2s |
| B | Vô hiệu hóa cache khi `resets_at` đã qua | ✅ **ĐÃ FIX** | Vô hiệu hóa cache qua tín hiệu STALE_RESET |
| C | `stale` detection nhận biết `resets_at` | ✅ **ĐÃ FIX** | Composable `useAgentUsage.js` tự động đánh dấu `stale` |
| D | Auto-trigger force sync khi reset qua | ✅ **ĐÃ FIX** | `UsageProgressBar` tự động kích hoạt `force-sync` khi timeout |
| E | Hỗ trợ case `0% used` không có resets time | ✅ **ĐÃ FIX** | Tách Regex để parse additive trong Python parser |
| F | Probe Session (dummy session) khi thiếu resets_at | ✅ **ĐÃ FIX** | Tạo dummy session `haiku` rồi lấy mốc reset chính xác |
| ~~A~~ | ~~Fallback OAuth endpoint~~ | ❌ **ĐÃ BỎ** | `/usage` đã dùng OAuth nội bộ |

> **⚠️ Ghi chú 2026-06-24:** Dòng A đặt sai lý do: định `/usage` = OAuth API, thực ra `/usage` là P2 (local JSONL). Cần xem xét lại khả năng dùng P3 thật (đọc `api.anthropic.com/api/oauth/usage`) như các tool cộng đồng (xem `deepresearch-claudecode-antigravity-quota-measurement.md` mục P3) để lấy quota thực tế của tài khoản tổng thể.
