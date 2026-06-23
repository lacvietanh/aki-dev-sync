# Claude Code Usage Flow — Bug Analysis & Improvement Proposals (v1.2.7)

> Phân tích kỹ thuật các vấn đề phát hiện trong flow giám sát quota Claude Code.
> Soạn ngày 2026-06-23. Dựa hoàn toàn trên mã nguồn thực tế.

---

## Bối cảnh

Hệ thống giám sát quota Claude Code hiện tại gồm hai lớp:

1. **Passive (P1):** Hook vá vào `~/.claude/statusline-command.sh` trên Remote. Mỗi khi Claude Code chạy, nó đẩy `rate_limits` vào `stdin` của hook — hook ghi đè vào `~/.claude/rate-limits-cache.json`.
2. **Active fallback (P3):** Người dùng bấm Force Sync → chạy `claude --model haiku -p /usage` trên Remote → Python parser trích xuất `% used` + `resets_at` → ghi đè cache.

Vấn đề phát sinh khi **người dùng không sử dụng Claude Code** trong khoảng thời gian dài (qua 1 hoặc nhiều chu kỳ reset 5 giờ): cả hai lớp đều không hoạt động, file cache cũ nằm im, UI hiển thị sai.

---

## 🔴 Vấn đề 1 — Force Sync vô dụng khi không có session hoạt động

**File:** [`scripts/force-sync-claudecode.sh`](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/force-sync-claudecode.sh)

```sh
claude --model haiku -p /usage 2>/dev/null
```

**Nguyên nhân:**
- `/usage` là một slash command nội bộ của Claude Code, có ý nghĩa trong session tương tác đang mở.
- Khi gọi qua `claude -p` (pipe mode) từ shell trần, output phụ thuộc vào việc server Anthropic có trả về Rate-Limit header chứa thông tin dạng `XX% used · resets Jun 24, 10:00am` trong response body hay không.
- Sau khi quota reset mà user không hoạt động, Anthropic đã xóa counter phía server nhưng không có lý do gì để trả về text chứa thông số `/usage` nếu không có prompt thực sự.
- Thực tế: `claude -p /usage` thường trả về `""` (rỗng) hoặc text không match regex.

**Hậu quả:**
- [`force-sync-parse.py`](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/force-sync-parse.py) không parse được → in `{"parsed": false}` → **không ghi gì vào cache** → cache cũ giữ nguyên.

---

## 🔴 Vấn đề 2 — Cache cũ không bao giờ bị xóa hay vô hiệu hóa

**File:** [`scripts/force-sync-parse.py`](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/force-sync-parse.py), [`scripts/get-claudecode-usage.sh`](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/get-claudecode-usage.sh)

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

**File:** [`scripts/provision-claudecode.sh`](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/scripts/provision-claudecode.sh)

Cơ chế P1 vá vào `statusline-command.sh`. Hook này chỉ kích hoạt khi Claude Code **thực sự đang chạy** và đẩy dữ liệu qua `stdin`. Khi không ai dùng Claude Code trên remote, hook im lặng hoàn toàn, file cache không được cập nhật dù quota đã reset.

**Đây là đặc điểm by-design của P1, không phải lỗi** — nhưng không có cơ chế bù đắp cho trường hợp người dùng nghỉ lâu.

---

## 🟡 Vấn đề 4 — `stale` detection không nhận biết trường hợp đã qua reset

**File:** [`src/composables/useAgentUsage.js`](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/src/composables/useAgentUsage.js#L42)

```js
stale.value = mtime > 0 && (Date.now() / 1000 - mtime) > 600;
```

**Vấn đề:**
- Logic chỉ so sánh **thời gian ghi file** (`mtime`) với hiện tại. Ngưỡng 10 phút (`600s`) đủ để bắt cache cũ trong điều kiện bình thường.
- Nhưng nếu Force Sync được bấm và parse thất bại → file **không được ghi lại** → `mtime` không đổi → sau 10 phút file bị đánh dấu `stale`.
- Tuy nhiên nếu Force Sync không được bấm trong thời gian dài và mtime đã cũ hơn 10 phút từ trước, badge "Stale" xuất hiện — nhưng `resets_at` trong file vẫn không được cập nhật, UI vẫn sai.
- **Không có logic nào** so sánh `data.rate_limits.five_hour.resets_at` với `Date.now()` để phát hiện "cache này đã qua thời điểm reset."

---

## 🟡 Vấn đề 5 — UI không tự trigger khi phát hiện reset đã qua

**File:** [`src/components/UsageProgressBar.vue`](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/src/components/UsageProgressBar.vue)

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

### Cải thiện A — Thêm fallback P3 thực sự (OAuth endpoint)

Thay vì chỉ dùng `claude -p /usage` vốn không đáng tin khi không có session, bổ sung một phương án đọc OAuth Usage endpoint:

```sh
# Đọc token từ ~/.claude/.credentials.json
TOKEN=$(jq -r '.claudeAiOauth.accessToken // empty' ~/.claude/.credentials.json 2>/dev/null)
if [ -n "$TOKEN" ]; then
  curl -sf -H "Authorization: Bearer $TOKEN" \
       -H "anthropic-beta: oauth-2025-04-20" \
       -H "User-Agent: claude-code/1.0" \
       "https://api.anthropic.com/api/oauth/usage" 2>/dev/null
fi
```

Endpoint này trả về `five_hour` và `seven_day` dạng structured JSON — nguồn dữ liệu chính thức nhất (xác nhận bởi 3 nguồn độc lập trong nghiên cứu: `aiedwardyi`, `jens-duttke`, `Maciek #202`). Dùng làm **fallback khi parse thất bại**, không phải thay thế P1 hoàn toàn.

> ⚠ Endpoint không có tài liệu chính thức, có thể thay đổi. Dùng làm fallback, không phải primary.

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

Trong [`useAgentUsage.js`](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/src/composables/useAgentUsage.js):

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

Trong [`UsageProgressBar.vue`](file:///Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync/src/components/UsageProgressBar.vue), khi timer phát hiện `isPast` chuyển từ `false` sang `true`, emit event `timeout` để component cha gọi `forceSync` thay vì chỉ `retry`:

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

## Ưu tiên triển khai

| # | Cải thiện | Độ ưu tiên | Ghi chú |
|---|---|---|---|
| B | Vô hiệu hóa cache khi `resets_at` đã qua | 🔴 Cao nhất | Fix trực tiếp bug hiển thị sai |
| C | `stale` detection nhận biết `resets_at` | 🔴 Cao | Cần đi kèm với B |
| D | Auto-trigger force sync khi reset qua | 🟡 Trung bình | UX improvement |
| A | Fallback OAuth endpoint | 🟢 Thấp | Bổ sung độ tin cậy nhưng thêm dependency mạng |
