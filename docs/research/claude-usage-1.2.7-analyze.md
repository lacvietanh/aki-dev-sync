# Claude Code Usage Flow — Bug Analysis & Improvement Proposals (v1.2.7 → v1.2.8)

> Phân tích kỹ thuật các vấn đề phát hiện trong flow giám sát quota Claude Code.
> Soạn ngày 2026-06-23. Cập nhật ngày 2026-06-24 (v1.2.8): Toàn bộ các đề xuất cải thiện (B, C, D) đã được triển khai thành công.

---

## Bối cảnh

Hệ thống giám sát quota Claude Code hiện tại gồm hai lớp:

1. **Passive (P1):** Hook vá vào `~/.claude/statusline-command.sh` trên Remote. Mỗi khi Claude Code chạy, nó đẩy `rate_limits` vào `stdin` của hook — hook ghi đè vào `~/.claude/rate-limits-cache.json`.
2. **Active fallback (P3):** Người dùng bấm Force Sync → chạy `claude --model haiku -p /usage` trên Remote → Python parser trích xuất `% used` + `resets_at` → ghi đè cache.

Vấn đề phát sinh khi **người dùng không sử dụng Claude Code** trong khoảng thời gian dài (qua 1 hoặc nhiều chu kỳ reset 5 giờ): P1 im lặng, cache cũ nằm im, UI hiển thị sai. Active fallback cũng có vấn đề riêng (xem bên dưới).

---

## ✅ Vấn đề 1 — [ĐÃ SỬA SAI] `/usage` KHÔNG cần session đang chạy

> **Cập nhật sau thực nghiệm SSH 2026-06-23:** Phân tích ban đầu sai.

**Thực tế đã xác minh:**
- `claude --model haiku -p /usage < /dev/null` hoạt động hoàn toàn **không cần session nào đang chạy** trên remote.
- Lệnh này thực hiện **network call thực sự** đến Anthropic API, xác thực bằng OAuth token từ `~/.claude/.credentials.json`. Thời gian thực thi ~2 giây.
- Output (ví dụ): `Current session: 38% used · resets Jun 24, 3:20am (Asia/Singapore)` — là **live data từ server**, không phải từ RAM hay cache.
- `Last 24h · N requests` trong output giảm theo thời gian thực, xác nhận đây là API call.
- Data từ `/usage` khác với `rate-limits-cache.json` cục bộ (chứng minh nguồn độc lập).

**Nguyên nhân thực sự khi Force Sync fail:** xem Vấn đề 1b bên dưới.

---

## 🔴 Vấn đề 1b — Force Sync chậm 3 giây do thiếu `< /dev/null` (**ĐÃ FIX**)

**File:** [scripts/force-sync-claudecode.sh](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/force-sync-claudecode.sh)

**Trước khi sửa:**
```sh
claude --model haiku -p /usage 2>/dev/null
```

**Vấn đề:**
- Khi chạy trong subshell `$()` qua SSH, stdin của lệnh không được redirect → Claude Code chờ stdin 3 giây rồi mới tiếp tục, in warning vào stderr.
- Tổng thời gian Force Sync = 3s chờ stdin + ~2s network call + overhead SSH = **>5 giây** thay vì ~2 giây.
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

## 🔴 Vấn đề 2 — Cache cũ không bao giờ bị xóa hay vô hiệu hóa (**ĐÃ FIX ở v1.2.8**)

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

## 🟡 Vấn đề 4 — `stale` detection không nhận biết trường hợp đã qua reset (**ĐÃ FIX ở v1.2.8**)

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

## 🟡 Vấn đề 5 — UI không tự trigger khi phát hiện reset đã qua (**ĐÃ FIX ở v1.2.8**)

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

## Tình trạng triển khai (v1.2.8)

| # | Cải thiện | Trạng thái | Ghi chú |
|---|---|---|---|
| 1b | `< /dev/null` + `cd /tmp` trong force-sync script | ✅ **ĐÃ FIX** | Giảm từ >5s xuống ~2s |
| B | Vô hiệu hóa cache khi `resets_at` đã qua | ✅ **ĐÃ FIX** | Vô hiệu hóa cache qua tín hiệu STALE_RESET |
| C | `stale` detection nhận biết `resets_at` | ✅ **ĐÃ FIX** | Composable `useAgentUsage.js` tự động đánh dấu `stale` |
| D | Auto-trigger force sync khi reset qua | ✅ **ĐÃ FIX** | `UsageProgressBar` tự động kích hoạt `force-sync` khi timeout |
| ~~A~~ | ~~Fallback OAuth endpoint~~ | ❌ **ĐÃ BỎ** | `/usage` đã dùng OAuth nội bộ |
