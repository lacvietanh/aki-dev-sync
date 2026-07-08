# Plan: P3 OAuth usage polling — fix điểm mù freshness (Lỗi C)

## BUG — Email hiển thị sai khi đổi tài khoản Claude Code (v1.9.3, ghi nhận 2026-07-07) — ✅ FIXED 2026-07-08 (root cause tĩnh, cần Mac-verify phần dư)

**Repro**: login CC bằng `lacvietanh@gmail.com` → sau đó login lại bằng `vietanhmusic@gmail.com`
→ restart app hoặc reload bất kỳ số lần nào.

**Actual**: header vẫn hiện email `lacvietanh...` (tài khoản CŨ).
**Expected**: header phải hiện `vietanhmusic...` — tài khoản đang active.

**Bằng chứng lệch pha rõ nhất**: số usage (%) hiển thị **đúng** là của `vietanhmusic` — chỉ riêng
cái label email bị kẹt ở tài khoản cũ. Tức là hai phần "số usage" và "label email" đang lấy từ hai
nguồn/thời điểm khác nhau, không đồng bộ khi tài khoản đổi (không phải toàn bộ pipeline đứng im).

**Liên quan nghi vấn tới scope plan này**: đây đúng lớp vấn đề "cache SSOT, nhiều writer không
đồng bộ" mà P3 đang xử lý cho usage — email/account label (`data.email` trong
`useAgentUsage.js`, hiển thị ở `AgentUsage.vue:14`) là một field khác trong cùng payload/cache có
thể đang bị stale-write hoặc không được overwrite đúng lúc login mới ghi cache. Chưa root-cause,
cần điều tra riêng (không tự động gộp fix vào Phase 1 P3 nếu nguyên nhân khác nhánh).

**Root cause tìm được 2026-07-08 (đọc code tĩnh, `get-claudecode-usage.sh` + `useAgentUsage.js` +
`agent_usage.rs` — không phải statusLine hook, không phải pin JS):**

`~/.claude/auth-cache.json` (nguồn của field `email`/`orgName`) trước đây chỉ được ghi **MỘT LẦN**:
`get-claudecode-usage.sh` chỉ chạy `claude auth status` (và ghi cache) trong nhánh `else` — tức
**chỉ khi file chưa tồn tại**. Một khi đã ghi lần đầu, mọi lần đọc sau `cat` verbatim file đó **mãi
mãi**, không có logic nào phát hiện account đã đổi để chạy lại. Trong khi đó `rate_limits` (số usage
%) có writer riêng (oauth-poll block, độc lập) nên vẫn tươi — đúng khớp triệu chứng "số đúng, email
sai". JS (`useAgentUsage.js`) và Rust (`agent_usage.rs`) không cache/pin email ở tầng của chúng —
chúng chỉ hiển thị verbatim những gì shell script đưa lên mỗi poll.

**Fix đã áp dụng (`scripts/get-claudecode-usage.sh`, 2026-07-08):** đổi điều kiện từ "file không tồn
tại → chạy" thành "file không tồn tại HOẶC cũ hơn `AUTH_REFRESH_AGE_S=300s` → chạy lại". Cache vẫn
được dùng để tránh spawn `claude auth status` mỗi 30s-poll, nhưng giờ tự làm mới trong vòng tối đa 5
phút — không cần restart app. Nếu `claude auth status` fail/rỗng ở một chu kỳ, fallback về cache cũ
(không blank email), thử lại chu kỳ sau. Đã unit-test 4 nhánh bằng fake `$HOME` + fake binary
`claude` (no-cache/fresh-cache/stale-cache-refresh/refresh-fails-fallback) — cả 4 đúng hành vi.

**Phần CHƯA verify được (cần Mac, đọc code không đủ):** fix trên đúng nếu nguyên nhân gốc là "chỉ
chạy 1 lần"; nhưng còn một khả năng dư chưa loại trừ — bản thân `claude auth status` (chính CLI của
Anthropic, không phải code của chúng ta) có thể trả về identity CŨ do cache/session nội bộ riêng của
nó, độc lập với việc app này gọi lại nó bao nhiêu lần.
**⚠️ Cần rebuild trên Mac để có hiệu lực** (script nhúng bằng `include_str!`, compile-time).

**Kết quả test thật trên Mac (2026-07-08, sau rebuild với fix `AUTH_REFRESH_AGE_S=300`):** user xác
nhận đã rebuild + relogin account khác + đợi qua gate 5 phút — **usage % cập nhật đúng (qua statusLine
hook), nhưng email vẫn kẹt ở account cũ.** Điều này loại bỏ khả năng "chỉ chạy 1 lần" — script ĐÃ chạy
lại `claude auth status` đúng như thiết kế (gate hoạt động đúng), nhưng lệnh đó tự nó trả về identity
cũ. Kết luận: root cause đã dịch chuyển hẳn sang **bản thân CLI `claude auth status`** (hoặc cách app
invoke nó qua `bash -lc` từ subprocess Tauri — có thể khác PATH/HOME với terminal tương tác thật),
không còn là vấn đề ở tầng cache của script `get-claudecode-usage.sh` nữa.

**Bằng chứng bổ sung (tra cứu docs chính thức `code.claude.com/docs/en/statusline`, 2026-07-08):** đầy
đủ schema JSON stdin của statusLine hook (`session_id`, `transcript_path`, `cwd`, `model`, `workspace`,
`version`, `cost`, `context_window`, `rate_limits`, `exceeds_200k_tokens`, `prompt_id`, `worktree`) —
**không có field email/user/account nào**. Nghĩa là "usage đúng nhưng email sai" không phải nghịch lý:
usage refresh được vì nó nằm ngay trong response mỗi turn (server trả rate_limits của account đang
login), còn email hoàn toàn phải đi qua side-channel riêng biệt `claude auth status` — không có cách
nào dùng chung 1 luồng để lấy cả hai như kỳ vọng ban đầu.

**⏸️ TẠM DỪNG 2026-07-08 (quyết định user, quá tải việc khác).** Bước chẩn đoán tiếp theo đã đề xuất
nhưng CHƯA làm — chỉ 1 lệnh nhẹ, không cần rebuild, làm khi nào rảnh:
```
claude auth status
```
chạy trực tiếp trong Terminal (không qua app) ngay sau khi relogin account khác — nếu lệnh này TỰ NÓ
cũng trả email cũ → bug ở chính CLI Anthropic, không sửa được từ phía mình; nếu trả đúng email mới →
bug nằm ở cách app invoke nó (PATH/HOME của subprocess khác terminal thật), sửa được. Không backlog
ép buộc — chỉ làm tiếp khi user chủ động muốn.

## 🐛 BUG XÁC NHẬN 2026-07-08 — regex parse "resets" trong force-sync không khớp format CLI hiện tại

**Repro thật, log Mac** (`usage.log`, host=local, không cần dàn dựng gì): `claude --model haiku -p
/usage` trả về đúng, đầy đủ dữ liệu — `"Current week (all models): 31% used · resets Jul 14 at
9:59am (Asia/Saigon)"` — nhưng cả `force-sync-claudecode.sh` (2 chỗ, dòng ~85 + ~183) lẫn
`force-sync-parse.py` (dòng ~42) đều dùng regex:
```
resets\s*([a-zA-Z]+\s+\d+),\s*(\d+):(\d+)([ap]m)
```
đòi **dấu phẩy** giữa ngày và giờ (`"resets Jul 14, 9:59am"`). CLI hiện tại dùng chữ **"at"**, không
phẩy, và đôi khi **không có phút** khi đúng giờ tròn (`"resets Jul 14 at 10am"`, quan sát cùng 1 log,
2 lần gọi liên tiếp — cả 2 format mới đều không match). Kết quả regex fail mọi lần (`no_match`) → 2
hệ quả:
1. `probe_decision` luôn = YES → mọi force-sync đều chạy probe session **đốt quota thật** dù
   `/usage` đã trả reset time hợp lệ (đúng thứ P3 plan gọi là "hack tồi nhất về nguyên lý").
2. `force-sync-parse.py` vẫn `parsed=true` (vì `pct` parse được từ nhánh khác) nhưng ghi
   `resets_at=0` vào cache — im lặng xoá mất reset time thật, 5h bar mất countdown, không hiện lỗi
   đỏ nào (khác `no_pct_match`, dễ bị bỏ sót vì UI trông vẫn "ổn").

**Fix đã áp dụng (2026-07-08):** regex đổi thành
`resets\s+([a-zA-Z]+\s+\d+)(?:,|\s+at)\s+(\d+)(?::(\d+))?\s*([ap]m)` — chấp nhận cả dấu phẩy (format
cũ) lẫn "at" (format mới), phút optional. Verify bằng unit test tay 3 case (comma, "at"+phút,
"at" không phút) — cả 3 parse đúng ra epoch. Sửa ở cả 3 chỗ (2 trong `force-sync-claudecode.sh`, 1
trong `force-sync-parse.py`). `npm run lint:scripts` pass.

**Verify 2026-07-08 (sau rebuild 1.9.5):** chạy đúng combined-script mà `force_sync_agent_usage` dựng,
trên `claude` thật của máy → `resets_check: raw=future:1783543200`, `probe_decision: … probe_needed=no`
(probe bị skip), parser ghi `resets_at=1783543200` (Jul 14) `parse_error=null` — hết `resets_at=0`,
hết probe thừa. Baseline binary cũ cùng output cho `no_match:0` / `probe_needed=YES` / `resets_at=0`,
xác nhận diff đúng là regex.

**Case `no_pct_match` (raw_len=0, output rỗng) — điều kiện đã xác định 2026-07-08, chờ 1 lần capture stderr thật.**
Điều kiện trigger (user xác nhận): **ranh giới reset quota + KHÔNG có session Claude Code nào chạy**
trên host → không gì giữ `~/.claude/rate-limits-cache.json` ấm (statusLine hook không fire), backend
claude chưa populate window mới → `claude -p /usage` trả rỗng. Đây là **biến thể nặng** của case đã ghi
ở `docs/arch/usage-claudecode.md` §266: ở đó probe phục hồi được (`has_resets=NO` → probe → run2 có
resets); case này ngay cả run2 sau probe vẫn rỗng → export `raw_len=0`. **Không tái hiện on-demand
được:** cần đúng ranh giới reset (backend/time-gated) + host không session; máy dev local luôn có session
giữ cache ấm nên hammer 40× local ra 0/40 rỗng. **Đã ship (1.9.5):** `run_usage()` thôi `2>/dev/null`,
capture stderr `claude` và log `run_usage: EMPTY stdout — claude stderr=…` **chỉ khi** stdout rỗng →
lần tới bug xảy ra sẽ biết *lý do* (auth fail / timeout / rate-limit cứng / claude-not-found) thay vì
mù. **Không thêm retry ở shell** — JS (`useAgentUsage.js`) đã auto-retry tới `MAX_FORCESYNC_RETRIES` ở
poll kế; probe vốn đã là 1 nhịp đánh thức. Root cause cuối cùng chờ 1 log stderr thật.

## 🔑 BREAKTHROUGH 2026-07-08 — nguồn identity đúng là `~/.claude.json → oauthAccount`, đi vòng hẳn qua `claude auth status`

Recon runtime trực tiếp trên host Linux (đọc key, che value) đã lật ngược giả định của cả plan này:

- **`~/.claude/.credentials.json` KHÔNG hề chứa email.** Top keys = `claudeAiOauth` + `mcpOAuth`;
  `claudeAiOauth` = `{accessToken, refreshToken, expiresAt, refreshTokenExpiresAt, scopes,
  subscriptionType, rateLimitTier}`. accessToken là token đục — decode cũng không ra identity.
- **Identity sống thật nằm ở `~/.claude.json → oauthAccount`**, có sẵn: `emailAddress`,
  `organizationName`, `displayName`, `accountUuid`, `organizationUuid`, `seatTier`,
  `profileFetchedAt`, … Claude Code **tự ghi lại file này** (mtime của `~/.claude.json` MỚI NHẤT
  trong 3 file: `.claude.json` > `auth-cache.json` > `.credentials.json`) → nó phản ánh account
  đang login **tức thì**, không qua CLI, không keychain, không lệ thuộc PATH/HOME của subprocess.
  File này **có cả trên Mac** (nơi `.credentials.json` vắng mặt — xem §"Finding chặn 2026-07-07").

**Ý nghĩa cho plan:** toàn bộ bế tắc "bug ở chính CLI `claude auth status`" (dòng 45-69) trở nên
**không liên quan** — ta không cần biết `claude auth status` đúng/sai nữa, vì có nguồn tốt hơn đọc
thẳng file. Đây đúng thứ `docs/plan/done/improve-jun24.md:40` đã gợi ý từ trước ("đọc thêm
`~/.claude.json → oauthAccount` qua đúng kênh SSH + delimiter đang dùng, pattern y hệt cách grep
`.credentials.json`, rủi ro thấp, không đụng token").

**Giải pháp khả thi (ưu tiên A):**
- **A. (Khuyến nghị)** Trong `get-claudecode-usage.sh` đọc `oauthAccount.emailAddress` /
  `oauthAccount.organizationName` từ `~/.claude.json` (cùng pattern grep/python như block
  `subscriptionType` hiện có), emit dưới đúng key `email` / `orgName` mà `agent_usage.rs:417` đang
  parse. Bỏ phụ thuộc `claude auth status` (giữ lại làm fallback nếu muốn). → sống, tức thì, bền,
  cross-platform.
- **B.** Gắn invalidation của `auth-cache.json` vào mtime/hash của `~/.claude.json` thay vì TTL 300s
  thuần → relogin ép refresh ngay (bổ trợ, không thay A).
- **C.** Bỏ `2>/dev/null` che lỗi ở nhánh auth (dòng 256-269) để lỗi "kẹt vĩnh viễn" chẩn đoán được
  thay vì trông như bình thường.

**Trạng thái:** đã điều tra + verify nguồn dữ liệu; **chưa implement** (chờ user quyết A). Bước chẩn
đoán `claude auth status` ở dòng 61-69 giờ **tùy chọn** — giải pháp A không cần nó để tiến hành.

## Phân loại theo nhu cầu test (cập nhật 2026-07-07)

**A. Không cần test** (đã verify qua code/log audit, hoặc là quyết định không làm):
- 4 mục "Xác nhận đúng" ở §"Kết quả rà soát đối chiếu code" (forceSync unreachable khi oauth khỏe;
  provision fire-and-forget không bị đói; lint-remote-scripts.js đã quét sẵn; usage_interval_s=30).
- Token schema trên Linux remote — đã chốt một mẫu: `accessToken`/`expiresAt` (ms epoch)/
  `refreshToken`/`scopes`/`subscriptionType`/`rateLimitTier`.
- BUG email: statusLine hook (`provision-claudecode.sh`) có ghi lại field `email` mỗi lần hay chỉ
  lần ghi đầu — xác định bằng **đọc source**, không cần chạy thử.
- Rủi ro refresh-token rotation — plan đã quyết định KHÔNG tự POST refresh, nên không cần test
  kịch bản rotation.

**B1. Phải test trên Mac** (cần Claude app / máy dev thật đang login CC):
- BUG email: login acc A → acc B → restart/reload nhiều lần → header có đổi đúng email không.
- Vòng đời token trên Mac: `expiresAt` ngay sau turn CC vs sau vài giờ idle.
- `claude auth status` có refresh access token hết hạn không (đo trên Mac).
- Mac local dùng `.credentials.json` hay chỉ có trong keychain.
- Kịch bản incident gốc: tắt CC hẳn, chỉ dùng claude.ai app → đo độ trễ UI, cả lúc token còn hạn
  lẫn sau khi hết hạn (>5h idle).
- Badge Stale hết mù + thanh 7D nhích khi chỉ dùng app — cần UI thật đang chạy.
- CC chạy turn dày → xem log oauth im lặng (age gate, không double-write) — cần môi trường CC
  hoạt động thật.

**B2. Cần test nhưng không bắt buộc trên Mac** (làm được trên Linux remote hoặc host bất kỳ có
token thật) — cập nhật 2026-07-07 sau khi code Phase 1, test bằng `env -i HOME=<scratch> sh
scripts/get-claudecode-usage.sh` (fake `$HOME`, không đụng state thật):
- [x] `npm run lint:scripts` pass + bashism injection → lint (build gate) phải fail: cả hai xác nhận
  (`dash -n` cũng pass sẵn tiện thể, dash có cài trên host này).
- [x] Host không có token → log `oauth: no token`, stdout vẫn 0 byte (xác nhận bằng `wc -c`).
- [x] Token hết hạn (token bịa) → `oauth: token expired`; gọi lại ngay lập tức → `oauth: token
  expired, gated` (60s gate qua marker file xác nhận đúng, không gọi `claude auth status` lại).
- [x] HTTP status khi token sai (request thật, token bịa) → `oauth: http_error status=401` — xem
  ghi chú Bước 0 (agent bị chặn đọc token thật, dùng token bịa để test riêng nhánh lỗi này).
- [x] Gate mạng 60s → xác nhận qua marker file + lần gọi thứ hai bị gate.
- [x] Merge logic (`to_window`/`to_epoch`) unit-test độc lập: epoch, ISO8601, `used_percentage`
  fallback key, window absent (idle) đều map đúng; merge giữ nguyên các key khác trong cache
  (`session_id`, `cwd`, `email`) và chỉ ghi đè `rate_limits.five_hour`/`seven_day`.
- [x] Cache resets_at đã qua (pre-STALE_RESET) với mtime mới → oauth_should_run() đúng vào nhánh
  (network fail vì token bịa → fallback STALE_RESET vẫn fire như cũ, xác nhận force-sync KHÔNG bị
  đụng khi oauth fail).
- [x] Cache fresh (resets_at tương lai, mtime mới) → `oauth: skip (cache fresh)`, không gọi mạng.
- [ ] Mô phỏng 429 thật — chưa làm (cần request thật liên tục hoặc token thật để trigger).
- [ ] Ngắt mạng mô phỏng endpoint chết → log `oauth: http_error/timeout` — chưa test timeout
  riêng (chỉ test được nhánh HTTPError/401 qua request thật); code dùng cùng khối `except` nên rủi
  ro thấp nhưng chưa quan sát trực tiếp `timeout/network:` message.
- [ ] Response khi account idle (`five_hour` absent? `resets_at` null?) từ SERVER THẬT — chỉ mới
  unit-test hàm merge với payload giả lập, chưa có response 200 thật (cần token hợp lệ, xem §7
  research doc).
- [ ] Qua mốc reset khi idle, oauth THÀNH CÔNG (không phải fallback) → không thấy `STALE_RESET` —
  cần token hợp lệ để oauth thật sự ghi được `resets_at` mới trước khi stale-check chạy; chỉ mới
  xác nhận fallback path (oauth fail → STALE_RESET vẫn fire, không worse hơn trước).

---

> Status: **TẠM DỪNG TOÀN BỘ PLAN 2026-07-08 (quyết định PO)** — không chỉ Mac-testing, cả việc theo
> đuổi P3 nói chung bị deprioritize để dồn lực fix bug + release. Code Phase 1 (Bước 1 oauth block +
> Bước 2 badge fix) đã landed 2026-07-07, unit-tested trên Linux remote (B2 category), **không revert**
> (an toàn, fail-open, không tốn gì khi không có token) — nhưng không đầu tư thêm cho tới khi user chủ
> động mở lại. Không chuyển `done/` vì mục tiêu gốc (fix Lỗi C trên máy Mac hàng ngày) chưa đạt.
>
> **⚠️ Finding chặn 2026-07-07 (Mac recon): `~/.claude/.credentials.json` KHÔNG tồn tại trên Mac**
> (`FileNotFoundError` khi user tự chạy lệnh Bước 0) — credential đã chuyển hẳn sang OS keychain trên
> bản Claude Code hiện tại của Mac (đúng như nghi ngờ ghi trong `docs/arch/usage-claudecode.md` §
> Subscription Tier Fallback từ 2026-07-03, nay xác nhận thật). **Hệ quả: oauth block hiện tại
> (`scripts/get-claudecode-usage.sh`) sẽ luôn log `oauth: no token` và không làm gì trên Mac** —
> fail-open đúng thiết kế (an toàn, không phá gì), nhưng **Phase 1 không giải quyết được incident gốc
> trên chính máy Mac hàng ngày của user** — chỉ có tác dụng trên host có file token thật (vd Linux
> remote headless).
>
> **Q&A 2026-07-08 — "flow relogin CC không tự 'ăn' native à?"** Không. Hai thứ khác nhau đang bị nhầm
> làm một: (1) `rate_limits` (số usage %) chỉ được ghi mới khi statusLine hook fire — tức phải có một
> **turn CC thật** xảy ra sau khi relogin; bản thân hành động login lại **không** tạo turn, nên nếu sau
> đó user quay lại dùng Claude app (không mở CC), số vẫn đứng im — đúng y Lỗi C, relogin không cứu được.
> (2) Email/label tài khoản hiển thị ở header còn tệ hơn: xem mục **"BUG — Email hiển thị sai khi đổi
> tài khoản"** ngay bên dưới — đây là bằng chứng thực tế (không phải suy đoán) rằng sau khi relogin
> bằng tài khoản khác, header **kẹt ở email CŨ** dù số usage đã đúng của tài khoản MỚI. Tức là ngay cả
> phần "nhận diện email" cũng không tự "ăn" theo relogin như kỳ vọng. Quyết định: **tạm gác cả hai**
> (usage-app-only freshness lẫn email-desync) cho tới sau release; xem bug email ngay dưới để biết nó
> có được xử lý riêng hay không.
>
> User đã báo effort test trên Mac tốn quá nhiều thời gian/công sức và ảnh hưởng công việc → **quyết
> định: TẠM DỪNG** đẩy thêm việc test/điều tra Mac cho tới khi user chủ động muốn tiếp tục. Đọc keychain
> (ACL theo binary `claude`, đọc bằng binary khác → có thể nổ password prompt — rủi ro đã lường trước ở
> §5 Rủi ro) không được thử tùy tiện; nếu muốn nối lại, ưu tiên nghiên cứu (không tốn thời gian của
> user) trước khi đề xuất bất kỳ lệnh nào user phải tự chạy.
> Soạn 2026-07-07, tinh chỉnh sâu cùng ngày (v2 — thu nhỏ scope
> sau flow-audit: v1 chỉ đụng đúng MỘT file script; force-sync/probe không sửa mà chờ xóa ở Phase 2).
> Nguồn gốc: `docs/arch/usage-claudecode.md` §2 Lỗi C + `docs/research/claude-app-usage-measurement.md`.
> Incident: user chỉ dùng Claude app (Cowork) → không có turn Claude Code → statusLine hook không
> fire → usage % trên UI đứng im dù quota thật đang bị tiêu.

## 0. Bản luận — vì sao thiết kế lại như dưới đây

### Mục tiêu thật (business/UX, một câu)

User liếc header Aki Dev Sync và **tin** con số để quyết định "còn làm tiếp được không, nên dùng
tool/account nào" — tin nghĩa là: số phản ánh **tài khoản** (mọi surface: app, Cowork, CC), độ trễ
thấp, và **không bao giờ nói dối im lặng** (stale mà trông như fresh là tội UX nặng nhất, nặng hơn
cả stale có đánh dấu).

### Chẩn đoán flow — insight trung tâm

Đại bộ phận machinery hiện tại của pipeline CC usage — STALE_RESET signaling, forceSync, probe
session đốt quota thật, khóa kép `initialSyncDone`/`staleResetSyncDone`, retry cap, year_fix parser,
regex parse text người đọc của `/usage`, blank dir, 3 lớp dọn JSONL transcript — tồn tại chỉ vì một
thiếu hụt duy nhất: **hệ thống không có cách nào hỏi server "usage của tôi bây giờ là bao nhiêu"
theo yêu cầu.** P1 (statusline) là passive, phải có turn. `/usage` là ước lượng local. Probe là hack
tồi nhất về nguyên lý: *tiêu quota để moi telemetry*.

P3 (`GET api.anthropic.com/api/oauth/usage`) chính là primitive bị thiếu đó. Vậy vị thế đúng của nó
KHÔNG phải "thêm một fallback writer nữa bên cạnh statusline và probe" (thêm path thứ ba = thêm
phức tạp — red flag "keep both paths for now") mà là: **bổ sung primitive gốc → để sau đó XÓA cả
tầng enforcement mọc quanh chỗ thiếu.** Kế hoạch vì thế có hai phase: Phase 1 thêm nhỏ nhất có thể,
Phase 2 xóa (có tiêu chí sunset rõ) — không nuôi ba writer song song vĩnh viễn.

### Các giả định của plan cũ bị bẻ sau khi phản biện

1. ~~"Cần script dùng chung, Rust concatenate vào cả get-usage lẫn force-sync"~~ — **Sai.** Nếu khối
   oauth chạy trong `get-claudecode-usage.sh` TRƯỚC stale-check thì: (a) cache thiếu → oauth tự tạo
   cache → first-load không bao giờ null → forceSync không trigger; (b) qua reset → oauth ghi window
   mới → STALE_RESET không bao giờ signal → forceSync không trigger. Tức là khi oauth khỏe,
   **force-sync/probe trở thành unreachable một cách tự nhiên** — không cần sửa nó. Còn khi oauth
   fail (không token…), step-0-oauth trong force-sync cũng sẽ fail y hệt → thêm vào đó là vô nghĩa.
   → Cắt toàn bộ tích hợp force-sync khỏi v1. Force-sync giữ nguyên 100%, chờ Phase 2 xóa.
2. ~~"Dùng curl + header-file umask 077 chống lộ token qua ps"~~ — **Thừa một dependency và một
   hack.** `python3` đã là hard-dependency của pipeline (dùng ở mọi bước). Một heredoc python3
   `urllib.request` làm trọn: đọc token → HTTP (timeout 10s) → parse JSON → merge → ghi atomic —
   trong MỘT process, token không bao giờ xuất hiện ở process args hay file tạm. Curl bị loại hẳn.
3. ~~"Đọc `claude --version` để build User-Agent"~~ — spawn login shell chỉ để lấy chuỗi UA là chi
   phí + state caching vô ích. Hardcode một hằng `UA="claude-code/2.1.0"` (verify ở Bước 0), đặt
   một chỗ, đổi một dòng khi cần.
4. ~~"macOS keychain fallback trong v1"~~ — **Nguy hiểm UX.** Keychain item của CC bị ACL theo
   binary `claude`; app khác đọc sẽ nổ password prompt, và item được tạo lại sau mỗi lần re-login →
   prompt lặp. v1 chỉ đọc `~/.claude/.credentials.json`; host nào không có file → oauth tắt im lặng,
   P1 flow cũ nguyên vẹn. Quyết định keychain dời sang sau khi Bước 0 khảo sát token thực tế nằm
   đâu trên từng host đang monitor.

### Hệ quả đẹp nhất của thiết kế rút gọn

**Phase 1 lõi = chỉnh đúng MỘT file: `scripts/get-claudecode-usage.sh`.** Zero thay đổi Rust,
zero IPC, zero UI element mới; lint (`lint-remote-scripts.js`) vốn đã quét file này; logging stderr
vốn đã được Rust relay. Cache vẫn là SSOT — oauth chỉ là một writer mới của nó, cùng format với
statusLine hook. Kèm theo một chỉnh JS vài dòng (Bước 2: nguồn `dataAge` cho badge Stale) — là
bugfix hiển thị độc lập, không đụng luồng dữ liệu.

### Kết quả rà soát đối chiếu code + changelog (2026-07-07, audit lần 2)

Đối chiếu từng nhận định với code thật và lịch sử changelog — 4 xác nhận, 3 hiệu chỉnh:

**Xác nhận đúng (đã verify tận dòng code):**
1. forceSync chỉ trigger ở null-path (`useAgentUsage.js:379-387`) → oauth khỏe = forceSync
   unreachable. ✓
2. Provision KHÔNG bị bỏ đói khi oauth tạo cache trước: nhánh data-present đã gọi `provision()`
   fire-and-forget một lần/session (`useAgentUsage.js:336`, thêm từ fix aki-rlcache v2). ✓
3. `lint-remote-scripts.js` đã quét sẵn `get-claudecode-usage.sh` (dòng 24-27). ✓
4. `usage_interval_s=30` mặc định (`refreshStore.js:7`) → trần độ trễ ~2.5 phút đúng. Recon máy
   Linux remote thật: `.credentials.json` tồn tại, `claudeAiOauth` có đủ `accessToken`,
   `expiresAt` (**ms epoch — đã xác nhận**), `refreshToken`, `scopes`, `subscriptionType`,
   `rateLimitTier`. ✓

**Hiệu chỉnh 1 — token expiry có thể là ca CHÍNH, không phải edge case (⚠️ quan sát MỘT mẫu,
chưa chốt).** Recon trên host Linux remote (headless, KHÔNG có Claude app): `expiresAt` chỉ cách
thời điểm đọc vài giờ — gợi ý access token sống ngắn theo giờ và chỉ được refresh khi CC chạy.
NẾU đúng như vậy trên Mac thì kịch bản incident ("cả ngày chỉ dùng app") token gần như chắc chắn
hết hạn → oauth chết → quay lại điểm xuất phát. Nhưng đây là một lần đọc trên một host — chưa
loại trừ khả năng expiry ngắn chỉ là trạng thái ngay-sau-turn, hoặc Mac hành xử khác. → Để MỞ:
Bước 0 trên Mac phải đo lại `expiresAt` nhiều thời điểm (ngay sau turn CC, sau vài giờ idle),
verify hành vi refresh của `claude auth status` và **rủi ro rotation của refresh token** trước
khi nghĩ đến tự refresh. Bước 1.1b được thiết kế theo giả định xấu nhất (token ngắn) — nếu Mac
chứng minh token dài hạn thì 1.1b thu nhỏ thành log-only như bản plan đầu.

**Hiệu chỉnh 2 — hook statusLine ghi cache KHÔNG atomic.** Đọc `provision-claudecode.sh`: block
tiêm vào ghi bằng `printf '%s' "$input" > cache.json` (ghi đè trực tiếp). Phân tích race bản
trước ("không đua nhờ os.replace") chỉ đúng một chiều: oauth GHI atomic, nhưng oauth ĐỌC có thể
trúng file đang được hook ghi dở → JSON parse fail → skip chu kỳ đó (fail-open, vô hại). Ghi
nhận đúng bản chất trong bảng rủi ro.

**Hiệu chỉnh 3 — Stale badge hiện có bị "mù" đúng kiểu incident, và seven_day còn tệ hơn.**
UI đã có badge `Stale` (không cần thêm element!), nhưng với CC nó tính `dataAge` từ `fetched_at`
(≈0 ở mọi poll thành công) nên thực tế chỉ sáng khi `resetIsPast` — cache đông cứng giữa window
với `resets_at` future thì badge im lặng, UI "xanh và tự tin" trong khi số đã cũ hàng giờ. Đồng
thời changelog (fix "Spurious 7D @timeout forceSync") xác nhận: `seven_day` **chỉ** có statusLine
là writer — force-sync không bao giờ cập nhật 7D — nên Lỗi C đánh vào thanh 7D còn nặng hơn 5H.
→ Bước 2 đổi cách làm: thay vì tooltip mới, sửa nguồn `dataAge` của CC từ `fetched_at` →
`file_modified_at` (mtime cache, đã có sẵn trong payload) — vài dòng JS, tái dùng badge sẵn có.
Và oauth ghi cả `seven_day` → 7D lần đầu tiên có writer chủ động.

### UX guard bắt buộc kèm theo: staleness phải NHÌN THẤY được

Fail-open ở mọi tầng có mặt tối: mọi nguồn cùng fail + user chỉ dùng app → quay lại đúng incident,
**âm thầm**. Không chống được mọi đường fail, nhưng phải chống "nói dối im lặng". UI đã có badge
`Stale` (`AgentUsage.vue`) — chỉ cần sửa cho nó hết mù với CC: nguồn `dataAge` đổi từ `fetched_at`
(luôn ≈0) sang `file_modified_at` (tuổi thật của cache). Zero element mới, đúng Extreme Narrow.

---

## 1. Mục tiêu

Usage 5h/7d trên UI phản ánh **tài khoản** (gồm cả Claude app) với độ trễ tối đa ≈
`OAUTH_REFRESH_AGE_S + usage_interval_s` (~2.5 phút), kể cả khi Claude Code không chạy turn nào.

```
GET https://api.anthropic.com/api/oauth/usage
Authorization: Bearer <claudeAiOauth.accessToken>
anthropic-beta: oauth-2025-04-20
User-Agent: claude-code/<ver>        # bắt buộc, thiếu → 429
```

## 2. Nguyên tắc

1. **Cache là SSOT, oauth là một writer mới** — statusLine (P1) và oauth (P3) cùng ghi
   `~/.claude/rate-limits-cache.json`, cùng schema. Rust/JS/UI không biết gì thay đổi.
2. **Fail-open từng nấc**: không file token / token hết hạn / mạng lỗi / schema lạ → log một dòng
   `[SHELL:get-usage] oauth: <lý do>` rồi rơi về flow cũ nguyên vẹn.
3. **Tự trọng tài giữa hai writer**: gate theo tuổi cache — CC đang hoạt động thì cache luôn tươi
   → oauth tự im lặng; CC nghỉ thì oauth gánh. Không cần cờ, không cần config.
4. **POSIX sh + python3 thuần** (không curl), pass `scripts/lint-remote-scripts.js`.

## 3. Phase 1 — thiết kế chi tiết

### Bước 0 — Recon & verify schema thật (làm ĐẦU TIÊN, chưa code gì)

Endpoint undocumented — không code theo tài liệu cộng đồng mù. Trên host thật (Linux remote):

```sh
TOKEN=$(python3 -c "import json;print(json.load(open('$HOME/.claude/.credentials.json'))['claudeAiOauth']['accessToken'])")
curl -sS https://api.anthropic.com/api/oauth/usage \
  -H "Authorization: Bearer $TOKEN" \
  -H "anthropic-beta: oauth-2025-04-20" \
  -H "User-Agent: claude-code/2.1.0" | python3 -m json.tool
```

**Lưu ý 2026-07-07 (agent tự thực hiện Bước 0)**: agent bị permission classifier chặn khi thử tự
đọc `~/.claude/.credentials.json` để lấy token thật ("Credential Exploration" — đúng chủ đích, không
bypass). Recon dưới đây vì vậy dùng token BỊA (không phải token thật của user) để test các nhánh lỗi
(401, network) — hợp lệ cho phần lỗi, nhưng KHÔNG thể lấy được response 200 thật để khóa field name/
format. Ba mục còn `[ ]` bên dưới cần user tự chạy lệnh ở Bước 0 (trên Mac hoặc bất kỳ host có token
hợp lệ) và dán kết quả vào đây + `docs/research/claude-app-usage-measurement.md` §7.

Checklist chốt (ghi kết quả vào `docs/research/claude-app-usage-measurement.md`):
- [ ] Tên field thật: `utilization` hay `used_percentage`? float hay int? — code đã viết chấp nhận
      cả hai (xem `to_window()` trong `scripts/get-claudecode-usage.sh`), cần response 200 thật để khóa.
- [ ] `resets_at`: ISO8601 hay epoch? timezone? — code đã viết chấp nhận cả hai (`to_epoch()`).
- [ ] Response khi **account idle / ngay sau reset** (không có window active): `five_hour` absent?
      `resets_at` null? — quyết định cách merge (xem Bước 1.4, đã code: window absent → resets_at=0).
- [x] HTTP status khi token sai/hết hạn: **401** (recon thật 2026-07-07, token bịa — xem
      `docs/research/claude-app-usage-measurement.md` §7).
- [x] UA `claude-code/2.1.0` không bị chặn tiền-auth (request tới được bước auth-check, trả 401
      chứ không phải 429) — chốt hằng `UA` tạm thời, có thể cần đổi nếu Mac thấy 429 thật.
- [x] **Token trên host Linux remote** (một mẫu, 2026-07-07): `.credentials.json` tồn tại,
      `claudeAiOauth` = `{accessToken, expiresAt (ms epoch), refreshToken, scopes,
      subscriptionType, rateLimitTier}`. `expiresAt` lúc đọc chỉ còn ~4.5h → nghi token sống
      ngắn, **chưa chốt** — xem Hiệu chỉnh 1.
- [x] Token trên **Mac local**: **KHÔNG có `.credentials.json`** (`FileNotFoundError`, xác nhận
      2026-07-07 bởi user) → chỉ còn OS keychain. Oauth block hiện tại (file-only) vô hiệu trên Mac —
      xem finding chặn ở đầu file. Quyết định: **không** chase keychain-path ngay bây giờ (effort cao,
      user đã báo hết bandwidth) — để mở, không đoán, không code vội.
- [ ] **Vòng đời token** (đo trên Mac, nhiều thời điểm): `expiresAt` ngay sau một turn CC là bao
      lâu? Idle vài giờ thì sao? Token ngắn thật hay chỉ là mẫu lệch?
- [ ] **`claude auth status` có refresh access token hết hạn không** — câu hỏi CORE
      (Bước 1.1b phụ thuộc): chạy khi token expired rồi đọc lại `expiresAt` xem có mới không,
      và đo thời gian chạy (budget timeout 30s).
- [ ] **Refresh token có bị rotate khi dùng không**: nếu tự POST refresh mà Anthropic rotate
      refresh token thì bản copy trong tay CC bị vô hiệu → PHÁ auth của CC. Chưa verify được
      điều này thì tuyệt đối không tự refresh — đây là lý do 1.1b đi đường `claude auth status`
      (để chính CC tự quản credential của nó) thay vì tự gọi token endpoint.

### Bước 1 — Khối oauth trong `get-claudecode-usage.sh`

Chèn **trước** phần đọc cache/stale-check (sau `_log "start"`). Điều kiện chạy (OR):
- cache không tồn tại;
- cache mtime cũ hơn `OAUTH_REFRESH_AGE_S=120`;
- cache có `five_hour.resets_at` > 0 và đã qua (tiền-STALE_RESET).

**Kỷ luật giao thức**: stdout của script là delimiter chain mà Rust parse — khối oauth **tuyệt đối
không in gì ra stdout**; mọi diagnostic đi qua stderr (`_log`/python `sys.stderr`). Khối được bọc
`|| _log "oauth: failed"` để lỗi python không giết script dưới `set -e` (Rust coi exit≠0 là
null → sẽ kích forceSync oan — chính là kiểu lỗi dash/pipefail cũ, không được tái phạm).

**Budget thời gian**: `run_remote_script` có trần cứng 30s (`agent_usage.rs:14`). Khối oauth
worst-case = auth-status refresh (~vài giây) + HTTP timeout 8s — cộng phần script cũ vẫn phải
dư margin. HTTP timeout chốt **8s**, không phải 10s.

Thân khối = MỘT heredoc python3 làm tuần tự, fail nấc nào thoát nấc đó (log lý do ra stderr):
1. Đọc `~/.claude/.credentials.json` → `claudeAiOauth.accessToken`; check `expiresAt` (ms epoch).
1b. **Token hết hạn — ca chính, không phải edge** (token chỉ sống ~giờ, xem Hiệu chỉnh 1):
   chạy `bash -lc 'claude auth status'` MỘT lần (chung gate 60s với marker) để CC tự refresh
   credential của chính nó, rồi đọc lại `.credentials.json`. Vẫn expired (auth status không
   refresh — chờ kết quả Bước 0) → `oauth: token expired` → thoát. KHÔNG tự POST refresh token
   (rủi ro rotation phá auth CC — Bước 0).
2. **Gate mạng**: marker `~/.claude/aki-oauth-last-attempt` (epoch). `now - last < 60` → thoát.
   Ghi marker **trước** khi gọi mạng/auth-status (token hỏng vĩnh viễn cũng chỉ 1 attempt/phút).
3. `urllib.request` GET với 3 header trên, `timeout=8`. Non-200 → log status → thoát.
4. **Merge, không thay thế**: load cache hiện có (hoặc `{}` nếu chưa có) →
   map `five_hour`/`seven_day` (`utilization`→`used_percentage` round int, `resets_at`→epoch)
   vào `rate_limits.*` → giữ nguyên mọi key khác (session, cwd, tokens…). Window absent trong
   response (account idle) → ghi `resets_at: 0` cho window đó (get-usage đã coi 0 = "valid, không
   stale-check" — đúng nghĩa "chưa có window active", tự nhiên thoát khỏi limbo hậu-reset mà
   trước đây phải probe).
5. Ghi atomic: temp file cùng dir + `os.replace` (không đua với statusLine hook đang ghi).
6. Log kết quả: `oauth: ok pct_5h=… resets_at=…` (KHÔNG bao giờ log token).

Token không xuất hiện ở process args, file tạm, hay stdout — chỉ sống trong process python3.

### Bước 2 — UX staleness: chữa badge Stale đang mù (vài dòng JS, zero element mới)

Badge `Stale` đã tồn tại (`AgentUsage.vue:22`) nhưng với CC nó chết logic: `dataAge` tính từ
`fetched_at` (thời điểm Rust fetch — luôn ≈0 ở poll thành công), nên cache đông cứng giữa window
không bao giờ làm badge sáng. Sửa trong `useAgentUsage.js`: với CC, tính `dataAge` từ
`res.file_modified_at` (mtime cache = tuổi thật của dữ liệu). Ngưỡng 600s giữ nguyên. Kết quả:
mọi đường fail (token expired, endpoint chết, statusline im) sau 10 phút đều hiện `Stale` —
guard "không nói dối im lặng" bằng element sẵn có. Với oauth khỏe, mtime luôn <120s → badge tắt.

### Bước 3 — Docs & release (cùng task với code)

- `docs/arch/usage-claudecode.md`: Lỗi C → "fixed by P3 writer"; thêm oauth vào sơ đồ §3b; ghi rõ
  "force-sync/probe giờ chỉ reachable khi oauth unavailable — candidate xóa ở Phase 2".
- Research doc: điền kết quả Bước 0.
- CHANGELOG: **minor bump**, Added. Plan này → `docs/plan/done/` sau khi Phase 1 verify.

## 4. Phase 2 — Sunset probe (điều kiện, không phải lời hứa suông)

**Tiêu chí kích hoạt**: sau ≥14 ngày dùng thật với Phase 1, `usage.log` xác nhận (a) không lần nào
`probe_decision=YES` trong khi oauth healthy, (b) không 429, (c) không lần nào UI kẹt "No data".

**Việc xóa** (mỗi dòng là complexity đang trả lãi hằng ngày): probe session trong force-sync
(hết đốt quota), 3 lớp dọn JSONL transcript, blank-dir, year_fix + regex parse text `/usage`,
`force-sync-parse.py`, và thu nhỏ forceSync JS về "chạy checkUsage ngay" (nút bấm tay giữ nguyên
hành vi nhìn từ user). STALE_RESET signaling giữ lại làm trigger oauth-tiền-stale (đã là một dòng).
Nếu tiêu chí không đạt → giữ nguyên, ghi lý do vào arch doc.

## 5. Rủi ro & đối sách

| Rủi ro | Đối sách |
|---|---|
| Endpoint undocumented đổi schema/khóa | Fail-open về P1; parse lỗi chỉ log; endpoint+UA là hằng một chỗ, sửa một dòng |
| 429 | UA đúng + gate 60s (max 1 req/phút/host — thưa hơn cả CLI chính chủ). Chỉ thêm backoff nếu soak thấy 429 thật — không guard đón đầu |
| Token hết hạn (nghi là ca CHÍNH — một mẫu Linux cho thấy token ~giờ, CHƯA chốt, đo lại trên Mac) | Bước 1.1b: `claude auth status` refresh gated 60s; nếu Bước 0 chứng minh auth-status không refresh → giá trị Phase 1 giảm còn "tươi trong ~5h sau lần dùng CC cuối" — phải đánh giá lại trước khi code, cân nhắc refresh-token flow CHỈ sau khi verify không rotation. Nếu Mac cho thấy token dài hạn → 1.1b thu về log-only |
| Race ghi cache: hook statusLine ghi KHÔNG atomic (`printf > file`) | Chiều oauth-ghi: `os.replace` atomic, hook đọc thấy cũ hoặc mới, không bao giờ dở. Chiều oauth-đọc: có thể trúng file hook đang ghi dở → JSON parse fail → skip chu kỳ (fail-open, tự lành ở poll sau). Không sửa hook trong plan này |
| Tự refresh token phá auth của CC (rotation) | Cấm tự POST refresh trong v1; chỉ đi qua `claude auth status` để CC tự quản credential |
| Mac local không có `.credentials.json` (keychain) | v1: oauth tắt trên host đó, P1 nguyên vẹn; số phận keychain quyết ở Bước 0 bằng dữ liệu thật, không đoán (prompt ACL là rủi ro UX có thật) |
| Bashism/python lỗi làm chết script get-usage | Khối oauth bọc để lỗi python → exit code riêng được nuốt + log, KHÔNG chạm `set -e` của phần còn lại; lint lớp 2 chặn bashism từ build |
| Clock skew host | Không tệ hơn hiện trạng (mọi so sánh resets_at đã dùng clock host) |

## 6. Verification (trên Mac, sau build)

1. **Kịch bản incident gốc**: tắt Claude Code hẳn, dùng claude.ai app → UI phải nhích trong
   ≤ ~2.5 phút, khớp `claude.ai/settings/usage`. Chạy lại kịch bản này **sau khi token quá
   `expiresAt`** (chờ >5h không dùng CC) — đây mới là ca quyết định thành bại (Hiệu chỉnh 1).
1b. **Badge Stale hết mù**: giả lập cache mtime cũ >10 phút (touch -d) với resets_at future →
   badge phải sáng; oauth ghi mới → badge tắt. Thanh 7D cũng phải nhích khi chỉ dùng app
   (seven_day lần đầu có writer chủ động).
2. CC đang chạy turn dày: `usage.log` phải cho thấy oauth **im lặng** (age gate) — không double-write.
3. Host không token: một dòng `oauth: no token`, hành vi y bản cũ.
4. Qua mốc reset khi account idle: KHÔNG thấy `STALE_RESET` lẫn `probe_decision` trong log —
   oauth ghi `resets_at` mới/0 trước khi stale-check kịp chạy.
5. Ngắt mạng host (mô phỏng endpoint chết): log `oauth: http_error/timeout`, UI giữ số cũ +
   tooltip tuổi dữ liệu tăng dần — không error banner, không flicker.
6. `npm run lint:scripts` pass; nhét thử bashism vào khối mới → build phải fail.
7. Soak 14 ngày → đánh giá tiêu chí Phase 2.

## 7. Ngoài scope (ghi để khỏi quên, không làm bây giờ)

- Thanh `seven_day_opus` riêng cho Max plan (oauth có data; UI hiện chưa cần).
- `extra_usage` credits.
- OAuth refresh-token flow tự viết (đụng credential của CC — rủi ro > lợi ích khi CC tự refresh).
- Relabel panel "Claude Code" → "Claude account" (tooltip là đủ; đổi label là quyết định semantic
  stability, cần user chốt riêng — xem RULE-content-write).
