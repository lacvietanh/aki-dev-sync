# Research: `rate_limit_event` từ `claude -p --output-format json` — nguồn reset-time native, mọi máy, một turn

**Ngày:** 2026-07-09 · **Máy:** Mac (aki, account `lacvietanh@gmail.com`, Pro) · **Phương pháp:** thực nghiệm trực tiếp, không suy đoán.

Doc này đính chính một giả định sai đã tồn tại lâu trong `docs/arch/usage-claudecode.md` và ghi lại nguồn dữ liệu mới phát hiện. Xem thêm memory `headless-p-no-statusline-hook`.

---

## 1. Bối cảnh — vì sao điều tra

Hiện tượng khởi phát: để app chạy qua đêm, sáng ra thanh 5h và 7d **cùng một mốc reset**. Log:

```
rl 5h=[pct=0 resets_at=1783998000 ...] 7d=[pct=35 resets_at=1783998000]
```

Truy ra hai chuyện tách biệt:
1. **Artifact hiển thị (đã fix):** khi 5h idle ở 0% không có traffic mới, Claude báo `five_hour.resets_at == seven_day.resets_at`. Đã null-hoá ở tầng hiển thị (`AgentUsage.vue`) → rơi vào state N/A thay vì vẽ mốc "5 ngày" vô lý.
2. **Câu hỏi lõi:** cơ chế force-sync sau reset (`claude -p /usage` + probe) — tại sao "kích một turn haiku" mà thông số không được cập nhật? Đây là bug kéo dài (`docs/plan/claudecode-oauth-usage-p3.md`, case `no_pct_match`/`raw_len=0`).

---

## 2. Ba sự thật chứng minh bằng thực nghiệm

### 2.1 Headless `claude -p` KHÔNG fire statusLine hook

```sh
CACHE="$HOME/.claude/rate-limits-cache.json"
B=$(stat -f %m "$CACHE")
zsh -lc "cd /tmp/blank && claude --model haiku -p 'respond with ok' < /dev/null"   # exit=0, stdout="ok"
A=$(stat -f %m "$CACHE")
# KẾT QUẢ: A == B — cache mtime KHÔNG đổi.
```

statusLine hook chỉ fire trong session **interactive** (nó render status line mỗi turn). `-p` không có status line để render → hook không chạy → **probe không hề ghi cache**.

→ **Đính chính:** `usage-claudecode.md` §143 + dòng 106 + mermaid dòng 240 nói *"probe → statusLine hook fire → ghi resets_at mới vào cache"* là **SAI**.

### 2.2 Cơ chế THẬT của probe

Probe (`claude -p 'respond with ok'`) có đúng một tác dụng: tạo một **transcript JSONL local** chứa `rate_limit_event` mới. `/usage` (đọc JSONL local — "Approximate, based on local sessions on this machine") sau đó đọc lại transcript ấy và echo mốc reset. Hook không tham gia. Đây là lý do phải chạy `/usage` **lần 2** sau probe.

### 2.3 Turn probe MANG SẴN thông số — trong response của chính nó

`claude --model haiku -p 'respond with ok' --output-format json` trả về mảng event, trong đó có:

```json
{
  "type": "rate_limit_event",
  "rate_limit_info": {
    "status": "allowed",
    "resetsAt": 1783570200,          // mốc reset 5h — server-truth, khớp cache đúng
    "rateLimitType": "five_hour",
    "overageStatus": "rejected",
    "overageDisabledReason": "org_level_disabled",
    "isUsingOverage": false
  }
}
```

**Ổn định:** 2/2 run đều đúng 1 `rate_limit_event`, `five_hour`, cùng `resetsAt`. Đây chính là dữ liệu server mà statusLine hook lấy — chỉ khác ta bắt thẳng từ stream `-p` thay vì chờ hook.

---

## 3. Giới hạn đã đo (không suy đoán)

| Field | Có trong `rate_limit_info`? |
|-------|------------------------------|
| `resetsAt` (5h) | ✅ luôn có |
| `rateLimitType` | ✅ (`five_hour`) |
| `status` (allowed / …) | ✅ |
| `used_percentage` (%) | ❌ không có |
| window **seven_day** | ❌ chưa bao giờ thấy (chỉ window đang bind) |

**Chưa verify được (time-gated, không ép on-demand):** event này có xuất hiện đúng ngay **0% sau mốc reset** khi không có session hay không. Khả năng cao có (API trả rate-limit info ở mọi response), nhưng cần một lần bắt thật ở ranh giới.

---

## 4. Giải pháp đề xuất — native, mọi máy, không keychain

Thay vũ điệu `run1 /usage → regex → probe → run2 /usage → regex` bằng:

> **1 turn `claude --model haiku -p 'ok' --output-format json` → parse `rate_limit_info.resetsAt`.**

Ưu điểm:
- **Mọi máy** (Mac/Linux): output của chính Claude Code, không đọc `.credentials.json`, không keychain, không OS-specific.
- **Một turn** duy nhất.
- Miễn nhiễm bug regex "resets" (JSON có cấu trúc) **và** bug `raw_len=0` (envelope JSON thật, không phải stdout `/usage` rỗng).
- `status != "allowed"` → biết có bị chặn thật không, thay cho suy đoán từ %.

Cần bù:
- **%**: giữ `/usage` text (chạy tốt khi có session — mà probe tạo ra session), hoặc chấp nhận hiển thị theo `status`.
- **7-day**: không có trong event → vẫn dựa statusLine hook / oauth-poll cho 7d (7d reset xa, hiếm là điểm đau).

Khớp mục tiêu Phase 2 của `claudecode-oauth-usage-p3.md` ("bỏ machinery probe/text fragile") **nhưng không cần oauth/keychain** — chỉ đọc thứ probe vốn đã trả về.

---

## 5. Trạng thái & việc còn lại

**Đã ship code (chưa commit) trong phiên này:**
- `AgentUsage.vue` — 5h dính 7d → hiển thị N/A.
- `useAgentUsage.js` — modal debug force-sync giveup chỉ còn dưới `--debug`.

**Còn lại:**
- [ ] Quyết: implement `--output-format json` vào `force-sync-claudecode.sh`, hay chờ Mac-verify qua đúng mốc reset trước.
- [ ] Verify `rate_limit_event` ở ranh giới 0%-sau-reset (time-gated).
- [ ] Chốt nguồn `%` và `seven_day` khi bỏ đường `/usage` text.
