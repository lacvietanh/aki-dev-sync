# Rò rỉ phiên `claude` trên remote — tràn RAM

> Điều tra 2026-07-20, phiên bản `1.14.0`. Remote: `bien`.
>
> **Lịch sử:** bản đầu của tài liệu này dài ~380 dòng và **quy sai thủ phạm** cho vòng lặp poll
> 30s, kèm sơ đồ ASCII và bảng ước lượng process không có cơ sở đo đạc. Đã xoá toàn bộ phần sai.
> Giữ lại đúng phần kiểm chứng được. Bài học ghi ở §5.

## 1. Triệu chứng

Remote `bien` giữ **19 phiên `claude` nhiều ngày tuổi**, chiếm ~6GB RAM + 4GB swap. Đây là số
duy nhất được đo thật (user quan sát trực tiếp trên máy). Mọi con số khác trong bản cũ
("2–8 orphan mỗi session", "96–768 process") là suy diễn — đã xoá.

Log app cùng thời điểm cho thấy chuỗi timeout poll liên tục, nhưng xem §3: đó là triệu chứng
đi kèm, không phải nguyên nhân.

## 2. Nguyên nhân

Ba lỗi cộng dồn, đều nằm ở **force-sync**, không phải poll.

### 2a. Force-sync dùng ngân sách của một thao tác nhanh

`force-sync-claudecode.sh` chạy **3 lượt `claude -p` nối tiếp** (`/usage` → probe → `/usage`),
mỗi lượt là một round-trip API thật. Nhưng nó gọi qua `run_remote_script` — cùng hàm, cùng hạn
**30 giây** với một lần poll đọc cache. Force-sync **không thể nào kịp**, nên bị cắt giữa chừng
gần như mọi lần chạy.

Nguyên nhân sâu hơn: hạn 30s được đặt ra cho poll (Layer 4 chống treo UI, v1.3.3). Force-sync
sau đó được nối vào cùng helper vì tiện, không ai tính lại ngân sách cho một thao tác khác hẳn
về bản chất.

### 2b. Không lệnh `claude` nào bị giới hạn phía remote

`grep timeout scripts/force-sync-claudecode.sh` → 0 kết quả.

Cắt SSH phía local **không** giết được `claude` phía remote: chuỗi là `sshd → sh → zsh -lc →
claude`, SIGHUP không truyền đáng tin qua login shell tới cháu. Một `claude` đang treo ở API
sẽ chạy vĩnh viễn, giữ vài trăm MB.

### 2c. Lệnh dọn rác chưa từng khớp

```
pkill -f "claude -p"
```
Cmdline thật: `claude --model haiku -p /usage`. Chuỗi con `"claude -p"` **không tồn tại** trong
đó. Kiểm chứng: pattern khớp `claude -p foo`, không khớp `claude --model haiku -p /usage`.

Cleanup im lặng không làm gì từ ngày viết, mà vẫn mở thêm 1 SSH mỗi lần chạy.

Gốc rễ là lỗi clean-code: pattern dọn được **chép tay** từ lệnh gọi, hai bản sao ở hai file
(Rust vs shell) trôi khỏi nhau mà không có gì phát hiện.

### 2d. Tích luỹ theo số lần app khởi động

Force-sync chạy khi `get_agent_usage` trả `null` (trigger #1 "First load, no cache" —
`docs/arch/usage-claudecode.md` §Luồng Chủ động). Trên host hỏng, lần nào đọc cũng `null` → mỗi
lần mở app lại đẻ một lượt force-sync. Riêng 20/07 app khởi động 15 lần.

Đây là lý do có process **nhiều ngày tuổi**: sự cố poll 24 phút không thể sinh ra chúng.

## 3. Vì sao poll KHÔNG phải thủ phạm

| Giả thuyết bản cũ | Bằng chứng phản bác |
|---|---|
| Poll spawn SSH chồng lấn lên nhau | 12 timeout cuối cách nhau **đúng 30.0s**, đều tăm tắp. Chồng lấn thì nhịp phải loạn. Guard `isChecking` bên JS vốn đã tuần tự hoá probe. |
| Force-sync retry cascade | Cả ngày chỉ **5** dòng `fs err`, kết thúc bằng `fs finally outcome="giveup" n=3`. Cap `MAX_FORCESYNC_RETRIES` hoạt động đúng. |
| Vòng lặp poll gây tràn RAM | Poll chỉ để lại `claude auth status` (nhẹ, thoát nhanh). Không giải thích được 6GB, cũng không giải thích được process nhiều ngày tuổi. |

Poll ồn ào trong log nên **trông giống** thủ phạm. Nó có góp phần, nhưng không phải cái ăn RAM.

## 4. Đã sửa (1.14.0)

| Sửa | Chỗ |
|---|---|
| Mọi lệnh `claude` tự giới hạn 45s **trên remote** | `AKI_CLAUDE_TMO` trong preamble → 6 call site |
| Fallback `perl -e 'alarm shift; exec @ARGV'` | macOS không có `timeout`/`gtimeout` (đã verify) |
| Force-sync có ngân sách riêng 180s | `FORCE_SYNC_TIMEOUT_SECS` — **xem §6, đây là chỗ chống chế** |
| Sửa pattern `pkill` | `ORPHAN_PATTERNS` |
| Cleanup có bound 8s, gộp 1 SSH | `wait_with_timeout` |
| Dừng poll sau 5 lần hỏng liên tiếp | `haltPolling()` |
| `ConnectTimeout`/`ServerAlive`/`BatchMode` | `polling_ssh()` |

Nguyên tắc: **process tự kết thúc thì không cần ai dọn.** `pkill` giờ chỉ là lưới an toàn cho
host không có cả `timeout` lẫn `perl`.

Kiểm chứng cơ chế `perl alarm`: lệnh treo 60s trong `zsh -lc`, bound 5s → chết đúng 5s, **0
process sót**. Đây là test thật, không phải suy luận.

## 5. Bài học về chính tài liệu này

Bản đầu sai vì **suy luận từ log thay vì từ flow**. Log đầy timeout poll → kết luận poll là thủ
phạm, rồi dựng sơ đồ và bảng số minh hoạ cho kết luận đó. Không có bước nào kiểm tra "process
nhiều ngày tuổi thì đến từ đâu" — dữ kiện phá vỡ toàn bộ giả thuyết.

Ba quy tắc rút ra:
1. **Đọc flow trước, log sau.** Log cho biết cái gì ồn ào, không cho biết cái gì tốn tài nguyên.
2. **Tách số đo khỏi số suy diễn.** Bản cũ trộn lẫn, khiến ước lượng đọc như đo đạc.
3. **Đối chiếu tuổi đời hiện vật.** Process nhiều ngày tuổi loại trừ mọi nguyên nhân 24 phút.

## 6. Còn nợ — force-sync đáng bị xoá, không phải vá

Câu hỏi đúng không phải "cho force-sync bao nhiêu giây" mà **"tại sao nó vẫn tồn tại"**.

Theo `docs/arch/usage-claudecode.md` §Lỗi C và `docs/plan/done/claudecode-oauth-usage-p3.md`:

- **3 lượt gọi là di sản của một hiểu lầm đã được đính chính.** Thiết kế ban đầu tin rằng probe
  làm statusLine hook fire → ghi cache. Đính chính 2026-07-09 (thực nghiệm): headless `-p`
  **không** fire hook. Cơ chế thật là probe ghi transcript JSONL, nên phải chạy `/usage` **lần
  thứ 2** để đọc lại. Ba lượt gọi tồn tại chỉ để đi vòng qua một cơ chế không hoạt động như tưởng.
- **Cùng lúc đó phát hiện đường một-lượt:** `claude -p '…' --output-format json` trả thẳng
  `rate_limit_info.resetsAt` trong response. Một turn, mọi máy. Lượt 1 và 3 là thừa.
- **Và đường không-lượt-nào:** `GET https://api.anthropic.com/api/oauth/usage` — server-side,
  account-level, realtime, không tốn quota, không cần turn CC. Code đã có sẵn trong
  `get-claudecode-usage.sh` (Phase 1, 2026-07-07). Doc ghi rõ: *"Khi oauth khỏe, force-sync/probe
  trở thành unreachable tự nhiên — candidate xoá ở Phase 2"*.
- **Phase 2 đã có tiêu chí sunset viết sẵn, chưa ai thực hiện.** Chặn chính: Phase 1 no-op trên
  Mac (credentials nằm trong Keychain, không có `.credentials.json` để lấy token).

Nghĩa là `FORCE_SYNC_TIMEOUT_SECS = 180` đang **chống đỡ cho code lẽ ra phải xoá**. Hướng đúng,
theo thứ tự ưu tiên:

1. Rút force-sync còn **một** lượt `claude -p --output-format json` → vừa khít hạn 30s sẵn có,
   xoá luôn `FORCE_SYNC_TIMEOUT_SECS` và `run_remote_script_long`.
2. Giải quyết token trên Mac (Keychain) → oauth path chạy được → thực hiện Phase 2, xoá hẳn
   force-sync + probe + toàn bộ `ORPHAN_PATTERNS`/`cleanup_orphan`.

Không đẻ rác thì không cần dọn rác.
