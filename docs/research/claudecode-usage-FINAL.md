# Claude Code usage monitoring — nghiên cứu FINAL

> **Trạng thái:** đây là tài liệu nghiên cứu **duy nhất** về theo dõi quota Claude Code.
> Thay thế 6 file research + 4 file plan (~2.400 dòng) viết rải rác 2026-06-23 → 2026-07-20.
> Kiến trúc đang chạy: `docs/arch/usage-claudecode.md`. Kế hoạch dọn: `docs/plan/claudecode-usage-cleanup-FINAL.md`.
>
> Cập nhật cuối: 2026-07-20.

Đọc §5 trước nếu bạn đang định "cải tiến" cơ chế này. Phần lớn ý tưởng nghe hay đã được thử và
loại rồi — §5 là nhật ký để không ai phải trả giá lần hai.

---

## 1. Sự thật nền tảng (đã kiểm chứng, không suy đoán)

| # | Sự thật | Cách kiểm chứng |
|---|---|---|
| 1.1 | Pool quota Pro/Max **dùng chung** cho claude.ai web, Desktop, mobile, Cowork và Claude Code | Anthropic công bố chính thức ([support.claude.com](https://support.claude.com/en/articles/11145838-use-claude-code-with-your-pro-or-max-plan)) |
| 1.2 | Claude Code đẩy `rate_limits` thật từ server ra qua hook `statusLine`, **mỗi turn** | Đọc stdin của hook |
| 1.3 | Hook chỉ chạy trong session **interactive**. Headless `claude -p` **KHÔNG** fire hook | Thực nghiệm 2026-07-09 trên Mac: chạy probe → mtime của `rate-limits-cache.json` bất động |
| 1.4 | `/usage` **không** gọi API. Nó chỉ đọc JSONL local rồi tính offline | Output tự ghi: *"Approximate, based on local sessions on this machine"* |
| 1.5 | `claude -p --output-format json` trả về **một danh sách sự kiện**; phần tử `type="rate_limit_event"` chứa `rate_limit_info` = `{status, resetsAt, rateLimitType}`. **Chỉ có mốc reset, KHÔNG có phần trăm** | Thực nghiệm 2026-07-20 trên Mac, CC `2.1.215` |
| 1.6 | Bản Claude Code mới không còn `~/.claude/.credentials.json` — credential chuyển vào OS keychain | Quan sát trên Mac |
| 1.7 | Script giao qua `ssh host sh` chạy bằng **dash** trên đa số Linux, không phải bash | Post-mortem 2026-06-25 |

**Hệ quả quan trọng nhất của 1.1 + 1.2:** con số hook nhận được **đã bao gồm** usage của Claude
app. Vấn đề chưa bao giờ là *phạm vi*, mà là *độ tươi* — nó chỉ được ghi mới khi có turn CC.

**Hệ quả của 1.3 + 1.4 + 1.5:** không có cách nào lấy **phần trăm** mà không có một turn CC
interactive. Turn headless chỉ cho mốc reset. `/usage` chỉ đọc file local. Đây là lý do luồng
active bị xoá hẳn — xem §2.

---

## 2. Kiến trúc chốt — một đường duy nhất

```
Người dùng chạy Claude Code (interactive)
        └─> statusLine hook ──> ~/.claude/rate-limits-cache.json
                                        │
                          app đọc file này mỗi 30s
                                        │
                            qua mốc reset, chưa có turn mới?
                                        └─> giữ số cũ, đánh dấu "cached",
                                            hiện 1 dòng chờ. KHÔNG tự gọi claude.
```

**Không có luồng active.** App không bao giờ tự chạy `claude`. Đây là quyết định chốt
2026-07-20 sau khi force-sync bị xoá (§5.13).

Lý do quyết định được: **một turn headless không thể khôi phục phần trăm** (§1.5) — nó chỉ cho
mốc reset. Mà mốc reset không kèm phần trăm thì gần như vô dụng cho người dùng. Toàn bộ luồng
active vì thế đổi lấy rất ít, trong khi nó là nguồn gốc của mọi bug nặng nhất trong lịch sử
tính năng này (§4).

Hành vi khi qua mốc reset mà chưa có session CC mới: hiện số đo cuối cùng kèm nhãn thời điểm,
cộng một dòng "Waiting for next Claude Code session". Cơ chế này AG đã dùng sẵn
(`isCached`/`cachedAt`) — CC dùng lại, không viết mới.

Điểm mù đã biết và **chấp nhận**: số liệu đứng im cho tới turn CC kế tiếp. Xem §3 vì sao không
vá bằng endpoint nội bộ.

## 3. Vì sao KHÔNG dùng endpoint `oauth/usage`

Từng có `GET https://api.anthropic.com/api/oauth/usage` trong `get-claudecode-usage.sh`
(Phase 1, 2026-07-07) — server-side, account-level, realtime, 0 token. Trên giấy nó xử lý trọn
điểm mù ở §2. **Đã gỡ 2026-07-20.** Bốn lý do, xếp theo mức nặng:

1. **Nhạy cảm.** Endpoint không được công bố, không có tài liệu, không có cam kết tương thích.
   Gọi nó cần token OAuth của Claude Code **và** giả `User-Agent: claude-code/2.1.0` để server
   chấp nhận. Giả danh client chính chủ để chạm API nội bộ bằng credential của user là thứ
   không nên có trong một dev tool.
2. **Không doc nào từng thẩm định điểm này.** Kế hoạch P3 dài 485 dòng có hẳn bảng rủi ro —
   nhưng chỉ bàn rủi ro *kỹ thuật* (token rotation, ACL keychain, password prompt). Không một
   dòng nào hỏi "có nên gọi endpoint này không". Trớ trêu, doc đối ngoại của chính dự án
   (`aki-dev-sync-ag-cc-usage-flow.md`) lại quảng cáo tiêu chí **"An toàn — không vi phạm ToS"**.
   Hai thứ này mâu thuẫn nhau và không ai bắt được.
3. **Trên Mac nó vốn no-op.** Cần `.credentials.json` để lấy token; Mac để token trong Keychain
   (§1.6). Lợi ích thực tế trên máy chính = 0 kể từ ngày code.
4. **Nó bắt mọi lượt poll phải gọi mạng.** Poll lẽ ra chỉ là đọc một file local.

**Kết luận:** điểm mù ở §2 là **đánh đổi có chủ đích**, không phải thiếu sót chờ vá. Bất kỳ đề
xuất nào định vá nó bằng endpoint nội bộ của Anthropic đều phải đọc lại mục này trước.

---

## 4. Sự cố tràn RAM remote (2026-07-20) — nguyên nhân & bài học

**Triệu chứng:** remote `bien` giữ **19 phiên `claude` nhiều ngày tuổi**, ~6GB RAM + 4GB swap.
Đây là số đo thật duy nhất; mọi ước lượng khác trong bản điều tra đầu đã bị xoá vì là suy diễn.

**Ba lỗi cộng dồn, tất cả nằm ở force-sync:**

1. **Force-sync dùng ngân sách của thao tác nhanh.** Nó chạy 3 lượt `claude -p` nối tiếp
   (`/usage` → probe → `/usage`) nhưng đi qua cùng hàm, cùng hạn **30s** với một lần poll đọc
   cache. Không thể nào kịp → bị cắt giữa chừng gần như mọi lần.
   *Gốc:* hạn 30s đặt cho poll (chống treo UI, v1.3.3); force-sync nối vào cùng helper **vì
   tiện**, không ai tính lại ngân sách cho thao tác khác hẳn bản chất.
2. **Không lệnh `claude` nào bị giới hạn phía remote.** Cắt SSH ở local **không** giết được
   `claude` ở remote: chuỗi là `sshd → sh → zsh -lc → claude`, SIGHUP không truyền đáng tin qua
   login shell tới cháu. `claude` treo ở API thì chạy vĩnh viễn.
3. **Lệnh dọn rác chưa từng khớp.** `pkill -f "claude -p"` với cmdline thật là
   `claude --model haiku -p /usage` — chuỗi con đó không tồn tại. Cleanup im lặng không làm gì
   suốt từ ngày viết, mà vẫn mở thêm 1 SSH mỗi lần.
   *Gốc:* pattern dọn được **chép tay** từ lệnh gọi thành hai bản sao ở hai file (Rust vs
   shell), trôi khỏi nhau không ai phát hiện.

**Vì sao tích luỹ nhiều ngày:** force-sync chạy khi đọc cache trả `null`. Host hỏng → lần nào
cũng `null` → mỗi lần mở app đẻ một lượt. Riêng 20/07 app khởi động 15 lần.

**Vì sao poll KHÔNG phải thủ phạm** (bản điều tra đầu quy sai cho nó):

| Giả thuyết | Bằng chứng phản bác |
|---|---|
| Poll spawn SSH chồng lấn | 12 timeout cuối cách nhau **đúng 30.0s**, đều tăm tắp. Chồng lấn thì nhịp phải loạn. Guard `isChecking` vốn đã tuần tự hoá. |
| Force-sync retry cascade | Cả ngày chỉ **5** dòng `fs err`, kết bằng `giveup n=3`. Cap hoạt động đúng. |
| Poll gây tràn RAM | Poll chỉ để lại `claude auth status` (nhẹ). Không giải thích được 6GB, càng không giải thích được process nhiều ngày tuổi. |

**Bài học về phương pháp điều tra:**

1. **Đọc flow trước, log sau.** Log cho biết cái gì *ồn ào*, không cho biết cái gì *tốn tài nguyên*.
   Bản điều tra đầu đọc log thấy đầy timeout poll → kết luận poll là thủ phạm → rồi vẽ sơ đồ và
   bảng số để minh hoạ cho kết luận đó.
2. **Tách số đo khỏi số suy diễn.** Bản cũ trộn lẫn, khiến ước lượng đọc như đo đạc.
3. **Đối chiếu tuổi đời hiện vật.** Process nhiều ngày tuổi loại trừ mọi nguyên nhân 24 phút.
   Chỉ một câu hỏi này đã đủ phá toàn bộ giả thuyết sai.
4. **Process tự kết thúc thì không cần ai dọn.** Mọi cơ chế cleanup đều là dấu hiệu có chỗ đang
   đẻ rác. Sửa chỗ đẻ, đừng nuôi chổi.

---

## 5. Nhật ký: đã thử gì, bỏ gì, vì sao

Bảng này thay cho 10 file đã xoá. **Đọc trước khi đề xuất bất cứ thay đổi nào.**

| # | Ý tưởng / giả định | Kết cục | Vì sao |
|---|---|---|---|
| 5.1 | `/usage` đọc session trong RAM | ❌ Sai | Thực nghiệm 2026-06-24: nó đọc JSONL local |
| 5.2 | `/usage` gọi Anthropic API | ❌ Sai | Cùng thực nghiệm. Output tự ghi là ước lượng local |
| 5.3 | Probe headless fire statusLine hook → ghi cache | ❌ Sai, tồn tại rất lâu | Thực nghiệm 2026-07-09: mtime cache bất động. **Đây là hiểu lầm đẻ ra toàn bộ vũ điệu 3 lượt gọi** |
| 5.4 | Chạy `/usage` lần 2 để đọc lại transcript probe vừa ghi | ⚠️ Đúng nhưng thừa | Có `rate_limit_info.resetsAt` ngay trong response turn đầu (§1.5). Lượt 1 và 3 vô ích |
| 5.5 | `set -o pipefail` trong script gửi qua SSH | ❌ Chết im lặng | `ssh host sh` = **dash**; đây là special built-in, dash thoát ngay exit 2 **trước** cả `\|\| true`. Phải dùng `( set -o pipefail ) 2>/dev/null && set -o pipefail` |
| 5.6 | Đọc tier từ `.credentials.json` | ⚠️ Hỏng trên bản mới | Keychain (§1.6). Fallback: parse `subscriptionType` từ `claude auth status` |
| 5.7 | Endpoint `oauth/usage` vá điểm mù Claude app | ❌ Gỡ 2026-07-20 | §3. Nhạy cảm + no-op trên Mac |
| 5.8 | `pkill -f "claude -p"` dọn orphan | ❌ Chưa từng khớp | §4.3 |
| 5.9 | Nới hạn force-sync lên 180s cho vừa 3 lượt gọi | ❌ Gỡ cùng ngày khi vừa thêm | Chống đỡ cho code lẽ ra phải xoá. Đúng: rút còn 1 lượt thì lọt 30s sẵn có |
| 5.10 | Backoff mũ khi host chết | ❌ Không làm | Log chứng minh probe vốn đã tuần tự, không có gì để giãn. Dừng hẳn vừa đơn giản vừa trung thực hơn |
| 5.11 | Đặt timer đúng tại `resets_at + 2s` để bắt reset tức thì | ❌ Chủ động không làm | Thêm phức tạp để tiết kiệm vài giây mỗi 5h. Trễ ≤1 chu kỳ poll là chấp nhận được |
| 5.13 | Toàn bộ luồng force-sync (app tự chạy `claude` để lấy số) | ❌ **Xoá hẳn 2026-07-20** | Đo thật cho thấy turn headless chỉ trả mốc reset, **không có phần trăm** (§1.5) → đổi lấy quá ít. Đây là nguồn gốc của cả 3 lỗi gây tràn RAM (§4) và của ~300 dòng script. Thay bằng: hiện cache cũ + 1 dòng chờ, như AG |
| 5.14 | Nút "Force Sync" trên UI | ❌ Xoá cùng | Nó chỉ hiện ở trạng thái `empty` — đúng lúc force-sync **đã tự chạy rồi**. Là nút bấm tay cho việc đã tự động |
| 5.12 | Nuôi song song nhiều nguồn ghi cache | ❌ Nguyên tắc cấm | Chính kế hoạch P3 đã cảnh báo: *"không nuôi ba writer song song vĩnh viễn"*. Rồi vẫn nuôi. Đó là gốc của mớ hỗn độn này |

---

## 6. Ràng buộc bất biến

Vi phạm bất kỳ mục nào dưới đây là tái diễn một bug đã trả giá:

1. **Script gửi qua SSH phải là POSIX `sh` thuần** — remote dùng dash (§1.7, §5.5).
2. **Mọi lệnh `claude` chạy xa phải có giới hạn thời gian tại chỗ**, không dựa vào việc cắt SSH
   (§4.2). macOS không có `timeout`/`gtimeout`; fallback là
   `perl -e 'alarm shift; exec @ARGV'` — `exec` để tín hiệu bắn thẳng vào `claude`, không vào
   lớp bọc.
3. **Chuỗi lệnh dùng để dọn phải sinh ra từ cùng một hằng số với lệnh gọi** — nếu còn cơ chế dọn
   nào tồn tại (§4.3).
4. **Thao tác nhanh và thao tác chậm không dùng chung ngân sách thời gian** (§4.1).
5. **Không thêm nguồn ghi cache thứ ba** (§5.12). Sau 2026-07-20 chỉ còn **một** nguồn:
   statusLine hook. App không tự gọi `claude` bao giờ.
6. **Không gọi endpoint nội bộ không công bố của Anthropic** (§3).

---

## 7. Bản đồ 8 file đã xoá

Xoá ngày 2026-07-20. Mục này thay chúng: ai cần biết "chuyện gì đã được điều tra rồi" đọc đây,
không phải lục git. Mỗi mục ghi: file đó làm gì, phần nào còn đúng, phần nào đã bị bác.

**`research/claude-usage-1.2.x-analyze.md`** (281 dòng, 06-23→06-24) — phân tích bug đầu tiên của
flow. Công lớn nhất: bác bỏ bằng thực nghiệm hai giả thiết sai về `/usage` (đọc RAM session, gọi
API) → chốt nó chỉ đọc JSONL local. Kết luận đó thành §1.4. Phần còn lại là đề xuất B–F đã
implement rồi bị plan này xoá cùng force-sync.

**`research/claude-usage-dash-pipefail-regression.md`** (166 dòng, 06-25) — post-mortem vụ
force-sync chết im lặng trên mọi remote. Nguyên nhân: `ssh host sh` = dash, `set -o pipefail` là
special built-in nên dash thoát ngay exit 2 **trước** cả `|| true`, `2>/dev/null` nuốt luôn lỗi.
Kết luận thành §1.7 + ràng buộc §6.1 — vẫn áp dụng cho mọi script gửi qua SSH.

**`research/claude-headless-rate-limit-event-2026-07-09.md`** (108 dòng) — file quan trọng nhất
trong 8 file. Thực nghiệm chứng minh headless `claude -p` **không** fire statusLine hook, bác bỏ
giả định nền tảng đã tồn tại nhiều tháng. Đồng thời phát hiện `rate_limit_event` trong output
JSON. Cả hai thành §1.3 và §1.5. Trớ trêu: phát hiện này lẽ ra đã đủ để xoá force-sync từ
2026-07-09; nó chỉ được ghi vào doc chứ không ai hành động, và 11 ngày sau force-sync giết một
máy remote.

**`research/claude-app-usage-measurement.md`** (172 dòng, 07-07) — khảo sát cách đo quota tiêu
bởi Claude app. Kết luận còn đúng: pool là chung toàn tài khoản, nên vấn đề là *độ tươi* chứ
không phải *phạm vi* (§1.1). Đề xuất của nó — dùng endpoint `oauth/usage` — bị bác ở §3.

**`research/ssh-process-leak-remote-ram-overflow.md`** (128 dòng, 07-20) — điều tra vụ tràn RAM.
Bản đầu ~380 dòng quy sai thủ phạm cho vòng lặp poll, kèm sơ đồ và bảng ước lượng process không
có cơ sở đo đạc; đã viết lại rồi gộp vào §4. Bài học phương pháp ở §4 lấy từ đây.

**`plan/done/claudecode-oauth-usage-p3.md`** (485 dòng, 07-07) — kế hoạch 2 phase cho endpoint
`oauth/usage`. Phase 1 landed nhưng no-op trên Mac (keychain). Phase 2 (xoá force-sync) có tiêu
chí sunset viết sẵn, **không ai thực hiện** — plan hiện tại chính là Phase 2, muộn 13 ngày. Câu
đáng nhớ nhất của nó: *"không nuôi ba writer song song vĩnh viễn"* — cảnh báo đúng, không ai thi
hành (§5.12).

**`plan/done/fix-claude-flow-24jun.md`** (318 dòng, 06-24) — sửa flow force-sync: probe session,
STALE_RESET auto-recovery, concurrency guard `isSyncing`, JSONL cleanup. Toàn bộ thuộc về
force-sync nên bị xoá cùng. Riêng `isChecking` guard sống sót và về sau chính nó là bằng chứng
bác bỏ giả thuyết "poll chồng lấn" (§4).

**`plan/done/fix-usage-monitor-freeze.md`** (287 dòng, 07-18) — P1–P5 chống treo monitor. Phần
còn sống: wake self-heal (`visibilitychange`/`focus`/watchdog), timeout cho probe AG, tổng quát
hoá `run_remote_script_timeout` → `run_interpreter_timeout`. Đáng chú ý: đánh dấu "chờ VERIFY
trên Mac" và **chưa session nào verify** — trạng thái đó tồn tại tới khi file bị xoá.

**Mẫu hình lặp lại xuyên suốt 8 file:** phát hiện đúng được ghi lại tử tế, rồi không ai hành
động theo. Ba lần liên tiếp (§5.3 → §5.12 → Phase 2). Doc không tự thi hành; nếu một phát hiện
đủ sức xoá code, hãy xoá ngay lúc đó.
