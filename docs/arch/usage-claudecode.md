# Kiến trúc: theo dõi quota Claude Code

> Mô tả hệ thống **đang chạy**. Vì sao nó thành ra thế này, và những gì đã thử rồi bỏ: `docs/research/claudecode-usage-FINAL.md` - **đọc §5 (nhật ký) trước khi đề xuất thay đổi**.
>
> Viết lại 2026-07-20 (v1.14.0), sau khi xoá toàn bộ luồng active. Bản trước dài 388 dòng và chứa 5 chỗ tự đính chính inline; những đoạn sai đã bỏ, kết luận đúng giữ lại.

## 1. Nguyên lý - một nguồn dữ liệu duy nhất

Claude Code CLI xuất telemetry qua hook `statusLine` (khai báo ở `~/.claude/settings.json`). Sau **mỗi turn** của một session **interactive**, CLI đẩy một JSON vào stdin của hook - trong đó có `rate_limits`, số thật từ server Anthropic, không phải ước lượng.

App vá một script vào hook đó để persist JSON ấy ra file, rồi định kỳ đọc file.

```
Người dùng chạy Claude Code (interactive)
        └─> statusLine hook ──> ~/.claude/rate-limits-cache.json
                                        │
                          app đọc file này mỗi 30s (poll)
                                        │
                            qua mốc reset, chưa có turn mới?
                                        └─> giữ số cũ, đánh dấu "cached",
                                            hiện một dòng chờ
```

**App không bao giờ tự chạy `claude` để lấy số.** Đây là ràng buộc kiến trúc, không phải chi tiết cài đặt - xem §5.

Chỉ hoạt động với tài khoản Claude.ai Pro/Max (API key thường không có `rate_limits`).

### Payload của hook

```json
{
  "rate_limits": {
    "five_hour": { "used_percentage": 42, "resets_at": 1782034800 },
    "seven_day": { "used_percentage": 18, "resets_at": 1782288000 },
    "seven_day_opus": { "used_percentage": 61, "resets_at": 1782288000 }
  },
  "cwd": "/home/user/project",
  "transcript_path": "/home/user/.claude/projects/..."
}
```

`resets_at`: Unix epoch giây, UTC. `used_percentage`: phần trăm đã dùng trong cửa sổ tương ứng.
(Từ v5 mỗi entry còn được stamp `seen_at` phía hook - xem §3.)

**`rate_limits` là một map MỞ, không phải một cặp cố định.** Anthropic đã thêm các bucket theo model (`seven_day_opus`, `seven_day_sonnet`, `seven_day_oauth_apps`) và có thể thêm nữa bất cứ lúc nào. Danh sách bucket cộng đồng biết tới, kèm phần nào là chính thức phần nào không: `docs/ref/claude-quota-buckets.md`. Không chỗ nào trong pipeline được hardcode số lượng bucket  - xem §4.1.

Pool quota Pro/Max **dùng chung** cho claude.ai web, Desktop, mobile, Cowork và Claude Code  - nên con số này đã bao gồm mọi hoạt động, không riêng CC.

### File trên máy được theo dõi

```
~/.claude/settings.json            → trỏ tới statusLine script
~/.claude/statusline-command.sh    → script app vá vào, hứng stdin, ghi cache
~/.claude/rate-limits-cache.json   → cache quota app đọc. NGUỒN QUOTA DUY NHẤT
~/.claude/.claude.json → .oauthAccount → chỉ còn dùng cho tier/orgType + accountUuid (KHÔNG PHẢI danh tính hiển thị - xem đoạn dưới)
~/.claude/auth-cache.json          → snapshot `claude auth status`, viết lại mỗi lần gọi live thành công - NGUỒN DANH TÍNH DUY NHẤT khi hiển thị
~/.claude/.credentials.json        → ⚠️ KHÔNG còn tồn tại trên bản CC mới (keychain)
```

**Đường dẫn chính xác của `.claude.json` là `${CLAUDE_CONFIG_DIR:-$HOME/.claude}/.claude.json`, KHÔNG PHẢI `$HOME/.claude.json`.** Đây là bug thứ hai phát hiện ngày 2026-07-30 (chi tiết: `docs/plan/cc-account-identity-ssot.md` §14).

**`.claude.json` KHÔNG phải nguồn danh tính đáng tin cho email/org (sửa lần 3, cùng ngày 2026-07-30 - xem SSOT §15).** Nó do chính CLI ghi, nhưng theo lịch refresh nội bộ của CLI, không phải theo lượt lệnh - và khi hai tiến trình `claude` cùng dùng một `CLAUDE_CONFIG_DIR` (hai account thật, cùng đăng nhập, một workflow bình thường của owner - xem `docs/ref/multiple-account-config-dir.md`), tiến trình nào flush sau cùng thắng ở file này, bất kể tiến trình nào vừa đăng nhập gần nhất. Danh tính hiển thị (email/org) giờ luôn đọc từ `claude auth status` chạy **live** (script) hoặc cache TTL 15s của cùng lệnh đó (statusline) - lệnh này resolve đúng danh tính của **phiên gọi nó**, không đụng tới `.claude.json` nên không dính race chia sẻ file. `.claude.json` chỉ còn dùng cho `organizationRateLimitTier`/`organizationType` (tier/gói) và `accountUuid` (so khớp chủ sở hữu cache quota) - không dùng cho email/orgName hiển thị nữa.

**Quota và danh tính là hai luồng tách rời, không cái nào gate cái nào.** Trước 2026-07-30 toàn bộ khối đọc danh tính nằm **bên trong** phép thử "có file cache quota không" - nên đổi account xong: cache cũ bị gate account loại bỏ → script không in gì → app không có gì để cập nhật → thẻ giữ nguyên email cũ cho tới khi khởi động lại app và xoá tay cache. Giờ script luôn phát khung danh tính, kể cả khi không có một byte quota nào đáng tin: khi đó nó in `{}` cộng `|||MTIME|||0` và đủ 3 khung `SUBTYPE`/`TIER`/`AUTHINFO`, thẻ hiện đúng account với 0 thanh quota thay vì hiện sai account.

## 2. Local hay remote - cùng một script

`run_remote_shell()` (`remote_shell.rs`) kiểm tra `is_local_host(host)` (`"local"`/`"localhost"`) rồi rẽ nhánh:

| | Claude Code (`Shell::Plain`) | Antigravity (`Shell::Plain`) |
|---|---|---|
| Local | `sh` | `sh` |
| Remote | `ssh host sh` | `ssh host sh` |

Cùng một script POSIX running over `sh` / `ssh host sh` cho cả hai agent.

Ở tầng UI, mỗi cặp `(agent, máy)` là một **UsageMonitor** riêng, định danh `agentId@host` và tạo qua `usageMonitorRegistry.getMonitor()`. Vì máy là một nửa định danh chứ không phải một tham số đổi được, "Claude Code trên host A" và "Claude Code trên host B" là hai thực thể tồn tại song song - theo dõi hai host cùng lúc, mỗi host một tài khoản. Mỗi monitor có công tắc riêng, lưu theo id trong `store/usageMonitorStore.js`, độc lập với sync check - xem `docs/feat/sync-check-and-usage-switches.md` và `docs/plan/done/usage-monitor-entity-refactor.md`.

### Mọi SSH theo timer đi qua `polling_ssh()`

Kèm `BatchMode=yes`, `ConnectTimeout=10`, `ServerAliveInterval=5`, `ServerAliveCountMax=3`. Thiếu `ConnectTimeout`, một host bão hoà có thể đốt trọn ngân sách 30s chỉ ở khâu bắt tay TCP.

### Một khoá cho mỗi host

`run_interpreter_timeout()` lấy `host_lock(host)` (registry `HashMap<host, Arc<Mutex<()>>>`) trước khi spawn. Vì mọi lệnh remote của app đều đi qua funnel này, hai tính năng khác nhau (probe statusline, Apply, poll usage, git info) không bao giờ bắn SSH chồng lên **cùng một host**; host khác nhau vẫn chạy song song bình thường. Khoá dùng `unwrap_or_else(|e| e.into_inner())` để một panic không khoá chết host đó vĩnh viễn.

### Mọi lệnh `claude` chạy xa đều có giới hạn tại chỗ

`CLAUDE_BIN_RESOLVER_PREAMBLE` đặt `AKI_CLAUDE_TMO` - prefix bound 45s cho mọi lệnh `claude`, **thực thi trên chính máy đó**, không dựa vào việc cắt SSH từ xa (cắt SSH không giết được cháu qua login shell). Thứ tự tìm: `timeout` → `gtimeout` → `perl -e 'alarm shift; exec @ARGV'`.

macOS không có sẵn `timeout`/`gtimeout` nên nhánh `perl` là nhánh thực tế chạy trên Mac. Dùng `exec` để tín hiệu bắn thẳng vào `claude`, không vào lớp bọc.

Preamble này cũng resolve `$CLAUDE_BIN` bằng kiểm tra file tĩnh trước khi rơi về PATH - tránh đua với việc shell rc chưa source xong lúc app vừa khởi động.

## 3. Provision - cài hook

`provision-claudecode.sh` chạy khi cache đọc ra `null` (hook có thể chưa tồn tại). Nó vá `statusline-command.sh` để ghi `rate-limits-cache.json`, và ghi `auth-cache.json`.

**Provision đọc số phiên bản đã cài, không tìm một chuỗi cố định (sửa 2026-07-30).** Guard cũ là `grep -q "aki-rlcache v3"` - nó chỉ biết phiên bản của chính nó, nên một file **đã mang bản mới hơn** (v4/v5, do Statusline Apply cài) không khớp và rơi xuống nhánh gỡ-rồi-cài-lại. Nhánh gỡ đó neo vào `^rl_input=`, trong khi template hợp nhất **thụt lề** dòng đó, nên lệnh xoá là no-op và v3 được chèn **lên trên** khối mới: provision **hạ cấp** một bản cài tốt và để lại **hai** writer chồng nhau, v3 chạy trước và ghi payload **không có** `account_uuid`, **không có** `seen_at`. Đó chính là hai field mà gate toàn vẹn ở §3 dựa vào - nên gate `accountUuid` của v5 **chưa từng chạy** trên host đã Apply, và nó vô hiệu vì bị chính provision phá lại chứ không phải vì host chưa được Apply. Xác minh tại chỗ trên máy dev: `~/.claude/statusline-command.sh` mang cả marker v3 (dòng 16) lẫn v4 (dòng 40), và `rate-limits-cache.json` chỉ có `account`, không có `account_uuid`/`seen_at`.

Hành vi hiện tại: đọc số N lớn nhất trong mọi marker `# aki-rlcache vN`; N ≥ 3 → **không ghi đè bất cứ thứ gì**. Nếu đúng trạng thái chồng lấn (có cả `v3` lẫn một bản > 3), provision **gỡ riêng khối v3** bằng `awk` neo hai đầu, và chỉ ghi file khi tìm được neo đóng - một sed-range trượt neo cuối sẽ xoá tới hết file, đó là cách một statusline bị phá huỷ.

Khi turn của CLI đẩy ra JSON **không có** `rate_limits`, script merge nguyên vẹn cục `rate_limits` cũ từ cache vào JSON mới - **không fabricate** giá trị.

Từ 1.18.0 (`aki-rlcache v4`), việc merge đó có điều kiện bắt buộc - đánh dấu `DESIGN LOCK` ngay trong script `src-tauri/src/statusline-unified.sh` (khối `# aki-rlcache v4/v5`, chỉ chạy khi `CLI=CC`; xem `docs/feat/statusline-customizer.md`):
- entry đã qua `resets_at` bị **loại**, không hiển thị (`resets_at: 0` = "không rõ", vẫn giữ);
- cache ghi kèm account đã ghi nó; cache của account khác bị bỏ, không đọc.

Thiếu điều kiện này, một field lọt vào cache sẽ sống vĩnh viễn (merge chỉ thêm/ghi đè key có trong payload, không bao giờ xoá key vắng mặt) - đúng nguyên nhân của quota ma `7d 45%` cho account không hề có weekly limit. Chi tiết: `docs/plan/done/1.18.0-statusline-apply-correctness.md` §P0-5.

Từ v5, gate account không còn so bằng email: một account bị xoá rồi tạo lại **dưới cùng email** (cùng tổ chức, `accountUuid` mới, gói khác) vẫn lọt qua gate email nguyên vẹn - đúng cách bug này tái xuất hiện sau khi v4 đã đóng nửa `resets_at`. v5 so `account_uuid` trong cache với `.oauthAccount.accountUuid` hiện tại (`~/.claude/.claude.json`); chỉ fallback về so email khi một trong hai bên không có `accountUuid` (cache ghi trước v5, hoặc client cũ chưa từng có field này). v5 cũng stamp `seen_at` lên mỗi entry còn sống sau merge - một entry không còn được CLI gửi trong ≥6 giờ (`maxAge=21600`) bị loại dù `resets_at` vẫn ở tương lai, để "gói mới không có weekly limit" không còn phải chờ tới `resets_at` (có thể là ngày khác) mới hết dính số cũ.

Hai điều kiện này được test khoá lại (`cc_drops_a_cached_quota_whose_reset_has_passed`, `cc_ignores_a_cache_written_by_another_account`), cùng với việc **nhánh AGY không bao giờ đọc/ghi file cache này** (`agy_never_touches_the_claude_rate_limit_cache`). Cách chạy các test đó trên máy không build được Tauri: `docs/research/statusline-generator-test-suite.md`.

### Hai gate, hai vai trò khác nhau

Gate ở trên nằm trong `statusLine` hook (Rust generate ra, chạy trên máy được theo dõi) - đó là phía **ghi**: "dọn tận gốc", loại field trước khi nó kịp ghi xuống `rate-limits-cache.json`. Nhưng hook đó chỉ chạy trên host **đã** nhận bản script mới; một host chưa được Apply lại vẫn có thể ghi ra một cache không có field `account`, hoặc một cache có `resets_at` đã qua mà không ai dọn.

Vì vậy `scripts/get-claudecode-usage.sh` (phía **đọc**, chạy mỗi ~30s cả local lẫn qua SSH) áp lại đúng hai gate đó một lần nữa, ngay trước khi ghi ra stdout - "lọc lúc hiển thị", không đụng tới file:

1. **Account gate**: so `account_uuid` trong cache với `.oauthAccount.accountUuid` hiện tại (`~/.claude/.claude.json`); nếu một trong hai bên thiếu `accountUuid` (cache pre-v5, hoặc client cũ), fallback so `account` (email) như trước. Hai bên đều có giá trị và khác nhau → toàn bộ cache bị coi là không đáng tin, script không in gì ra stdout (giống hệt nhánh "thiếu file cache"). Cache cũ (v2/v3, không có field `account` lẫn `account_uuid`) **không** bị bỏ theo cách này - nếu bỏ, mọi host chưa được vá lại sẽ mất trắng quota hiển thị; script chỉ log cảnh báo là host đó nên được Apply lại.
2. **Expiry gate**, áp cho từng entry trong `rate_limits`: bỏ entry có `resets_at` đã qua; bỏ luôn entry có `resets_at` bằng 0 hoặc thiếu - một cửa sổ không xác minh được chính là hình dạng của quota ma `7d` năm xưa, không hiện còn an toàn hơn hiện sai.
3. Sau khi lọc, `rate_limits` rỗng → không in **quota**, nhưng vẫn in danh tính (§1). "Không có quota đáng tin" chưa bao giờ đồng nghĩa với "không biết đây là account nào".

Script đọc **không bao giờ** ghi lại hay xoá `rate-limits-cache.json` - việc đó vẫn là của statusLine hook. Nếu `python3` lỗi phân tích JSON, coi như "không có dữ liệu đáng tin", không cho `set -e` giết cả script và không rơi về in nguyên file thô.

**`STALE_RESET` chỉ áp cho cache của chính account đang đăng nhập.** Tín hiệu này nghĩa là "cửa sổ 5h của account NÀY vừa lăn qua", và app trả lời bằng cách **giữ nguyên số đang hiện** rồi gắn nhãn cached. Câu trả lời đó sai với một cache do account khác để lại: ở đó `resets_at` gần như luôn đọc ra quá hạn, nên `STALE_RESET` bắn **trước** khi gate account kịp chạy, và email của account cũ nằm lại trên thẻ vĩnh viễn. Vì vậy script so chủ sở hữu cache (`account_uuid`, fallback `account`) **trước** phép thử stale; không khớp → không phát `STALE_RESET`, rơi xuống gate account để cache bị bỏ và danh tính sống được in ra.

Tier hiển thị (Pro/Max/20x...): từ v5, thứ tự ưu tiên đảo theo độ sống của nguồn, không còn ưu tiên `.credentials.json`. `TIER` đọc `organizationRateLimitTier` (hoặc `userRateLimitTier`) từ `~/.claude/.claude.json` trước - file này do CLI ghi lại ngay khi đăng nhập/đổi account nên gần như không bao giờ trễ; **từ 2026-07-30 `SUB_TYPE` cũng vậy**: đọc `organizationType` từ cùng file đó trước (`claude_max` → `max`, `claude_pro` → `pro`), rồi mới tới `subscriptionType` trong `auth-cache.json`/`claude auth status` (đã cache, `AUTH_REFRESH_AGE_S`). Trước đó `auth-cache` luôn thắng, nên ngay sau khi đổi account, gói của account cũ còn dán trên thẻ suốt TTL. `.credentials.json` giờ chỉ là **fallback cuối cùng** khi cả hai nguồn trên đều rỗng - đây là nguồn cũ nhất trong ba: nó không còn trên bản CC mới (đã chuyển vào OS keychain), và khi còn tồn tại chỉ được ghi lúc login/refresh nên có thể mang tier/subscription của account đã đổi từ vài giờ trước (đúng nguyên nhân badge "Pro" dính lại sau khi account mới đã là "Max 20x"). Khi `rateLimitTier` mang giá trị `default_claude_ai` (cắt ra `ai`), UI tự động fallback sang `subscriptionType` thay vì hiển thị badge `Ai`.

## 4. Trạng thái hiển thị

| Trạng thái | Khi nào | UI |
|---|---|---|
| `data` | Cache đọc được, mốc reset còn ở tương lai | Một thanh % + mốc reset **cho mỗi bucket** |
| `cached` | Đã qua mốc reset, chưa có turn CC mới | Số đo cuối + nhãn thời điểm + `Waiting for next Claude Code session` |
| `empty` | Chưa từng đọc được cache | Dòng trạng thái |
| `off` | Monitor bị tắt (per-monitor, theo `agentId@host`) | Dòng trạng thái |

`get-claudecode-usage.sh` phát hiện `now > resets_at` → trả `|||STALE_RESET|||`. Phía JS **giữ nguyên `data`**, bật `isCached`/`cachedAt` - cùng cơ chế Antigravity dùng. Số cũ đứng lại tới khi có turn CC mới ghi cache.

Trễ phát hiện reset ≤ một chu kỳ poll (mặc định 30s). Đã cân nhắc đặt timer đúng tại `resets_at + 2s` và **chủ động không làm**: thêm phức tạp để tiết kiệm vài giây mỗi 5 giờ.

### 4.1 Vẽ bucket theo kiểu tổng quát (quy tắc bất biến)

`AgentUsage.vue` **không** liệt kê tên bucket trong template. Nó `v-for` qua computed `ccBuckets`, dựng từ chính `data.rate_limits`:

- **Lọc**: chỉ nhận entry có `used_percentage` là số hữu hạn. `null`/thiếu/hỏng bị bỏ im lặng - một bucket `null` nghĩa là "gói này không có giới hạn đó", không phải lỗi, nên vẽ một thanh `N/A` cho nó là sai thông tin.
- **Thứ tự**: `five_hour` → `seven_day` → các weekly theo model đã biết theo đúng thứ tự `seven_day_opus`, `seven_day_sonnet`, `seven_day_fable`, `seven_day_mythos`, `seven_day_oauth_apps` → key lạ, sắp theo alphabet (để một bucket chưa từng thấy luôn rơi xuống đáy một cách tất định, không nhảy chỗ giữa hai lần poll).
- **Nhãn** (bảng alias `CC_BUCKET_LABELS`):

  | key | nhãn |
  |---|---|
  | `five_hour` | `5-Hour` |
  | `seven_day` | `7-Day` |
  | `seven_day_opus` | `7-Day Opus` |
  | `seven_day_sonnet` | `7-Day Sonnet` |
  | `seven_day_fable` | `7-Day Fable` |
  | `seven_day_mythos` | `7-Day Mythos` |
  | `seven_day_oauth_apps` | `7-Day OAuth apps` |

  Key lạ có tiền tố `seven_day_`/`five_hour_` được viết lại thành `7-Day X`/`5-Hour X` (X Title Case); còn lại là Title Case cả key (`foo_bar` → `Foo Bar`). Không bao giờ in key thô.
- **Hai đặc thù chỉ áp cho `five_hour`, giữ nguyên**:
  1. `five_hour.resets_at == seven_day.resets_at` → coi mốc reset của **5h** là không rõ (`null`), rơi về trạng thái `N/A` sẵn có. Claude báo trùng khi cửa sổ 5h nằm im ở 0%.
  2. Pool weekly **dùng chung** (`seven_day`) đạt 100% → làm mờ thanh 5h (bỏ khỏi thang màu, giữ tooltip). Bucket theo model **không** làm mờ ai và **không** bị ai làm mờ: chúng là pool riêng, một tuần Opus cạn không nói gì về cửa sổ 5h. Đây là quy tắc bán kính vụ nổ trong `CLAUDE.md`.
- **Không thêm DOM nào ngoài cấu trúc thanh sẵn có** (`.cc-bar-label` + `.cc-progress-track`) - với dữ liệu 2 bucket hôm nay, card render y hệt trước refactor (nguyên tắc Extreme Narrow).

Timer dò qua mốc reset (`@retry` mỗi 60s) vẫn chỉ bám `five_hour`: đó là cửa sổ duy nhất quay vòng đủ dày để một lần refetch phía client có giá trị, và cũng đúng bucket mà hợp đồng `STALE_RESET` phía script được viết theo. `usageMonitor.js` cũng vì vậy chỉ lấy `five_hour` làm mốc stale (log thì enumerate mọi bucket).

### 4.2 Quota lớp Fable 5 / Mythos 5 (ảnh chụp 07/2026)

- **Max**: Fable 5 rút từ chính pool `seven_day` **dùng chung**, trần 50% của pool đó. **Chưa có bucket riêng** - không có `seven_day_fable` trong payload hôm nay.
- **Pro**: chỉ dùng qua credit mua thêm, không nằm trong gói.
- Nếu/khi `seven_day_fable` xuất hiện, **UI không cần sửa gì** - nhãn đã có sẵn trong bảng alias và §4.1 tự nhận bucket mới. Đó chính là lý do tồn tại của refactor này.

Nguồn: https://support.claude.com/en/articles/15424964-claude-fable-5-on-your-plan · https://www.anthropic.com/news/claude-fable-5-mythos-5

### Circuit breaker

Hỏng 5 lần liên tiếp → `haltPolling()` dừng hẳn poll và báo lý do. Chỉ hành động rõ ràng của người dùng mới mở lại (Reload, hoặc đổi host) - **không** phải `visibilitychange`/`focus`, vốn bắn liên tục và sẽ dựng lại vòng lặp qua cửa sau.

Lý do dừng hẳn thay vì giãn dần: log sự cố 2026-07-20 cho thấy probe vốn đã tuần tự (guard `isChecking`), không có gì để giãn. Dừng vừa đơn giản vừa trung thực hơn.

### WKWebView suspend self-heal

WKWebView bóp/treo `setInterval` khi cửa sổ bị che hoàn toàn hoặc máy ngủ. Hai lớp phục hồi, cài **một lần** ở module scope, dùng chung cho mọi monitor instance (từ 1.20.0 monitor được tạo theo nhu cầu cho từng `agentId@host`, không còn số lượng cố định):

1. `visibilitychange`/`focus` → check ngay.
2. Nhịp watchdog 7s → nếu khoảng cách giữa hai tick vượt ngưỡng thì coi như vừa resume.

Ngưỡng lấy theo từng subscriber, không phải hằng số chung - nếu không, một nguồn đang halt sẽ bị watchdog đánh thức liên tục.

## 5. Ràng buộc bất biến

Vi phạm bất kỳ mục nào là tái diễn một bug đã trả giá. Chi tiết: `docs/research/claudecode-usage-FINAL.md` §6.

1. **App không tự chạy `claude` để lấy usage.** Luồng active (force-sync/probe) đã bị xoá 2026-07-20 - nó gây tràn RAM một máy remote đến mức phải bỏ máy, và đo thật cho thấy một turn headless chỉ trả mốc reset, không có phần trăm.
2. **Không gọi endpoint nội bộ không công bố của Anthropic** (từng có `oauth/usage`, đã gỡ).
3. **Chỉ một nguồn ghi cache**: statusLine hook.
4. **Script gửi qua SSH phải là POSIX `sh` thuần** - remote chạy dash, không phải bash. `set -o pipefail` trần sẽ giết dash im lặng; phải dùng `( set -o pipefail ) 2>/dev/null && set -o pipefail`. Có `scripts/lint-remote-scripts.js` gác.
5. **Mọi lệnh `claude` chạy xa phải bound tại chỗ**, không dựa vào cắt SSH.
6. **Thao tác nhanh và chậm không dùng chung ngân sách thời gian.**
7. **Không hardcode số lượng/tên bucket `rate_limits` ở bất kỳ tầng nào.** Parse như map mở, vẽ như danh sách (§4.1). Schema này đã đổi hình 4 lần trong 7 tuần; mọi chỗ đếm cứng "2 bucket" là một lần mất dữ liệu im lặng đang chờ xảy ra.
8. **Danh tính hiển thị (email/org) chỉ có một nguồn: `claude auth status` chạy live (hoặc cache TTL ≤15s của cùng lệnh đó), và nó không bao giờ bị gate bởi quota.** `.claude.json` KHÔNG dùng cho email/orgName nữa (sửa 2026-07-30, xem SSOT §15) - nó là file CLI flush theo lịch nội bộ, không theo phiên, nên khi hai tiến trình `claude` chia sẻ một `CLAUDE_CONFIG_DIR` (workflow đa-account bình thường của owner) nó phản ánh "tiến trình nào flush sau", không phải "phiên này đang đăng nhập account nào". `.claude.json` chỉ còn dùng cho `organizationRateLimitTier`/`organizationType`/`accountUuid`. Có ba chỗ giữ email của một host - `auth-cache.json` (nguồn), field `account` trong `rate-limits-cache.json`, và tag account trên statusline. Hai chỗ sau đều **dẫn xuất** từ cùng lệnh `claude auth status`. Thêm một chỗ giữ email thứ tư, hoặc để một trong hai chỗ dẫn xuất tự quyết theo nguồn khác, là tái diễn đúng bug "đổi account xong vẫn hiện email cũ".
9. **Script đọc không bao giờ ghi/xoá `~/.claude/.claude.json`.** Nó là input read-only. Nhóm Account của `claude_cleanup.rs` có xoá file này - một helper "dọn cache danh tính" mà chạm tới nó là xoá đúng nguồn sự thật để chữa một bản sao bị cũ, cùng loại bán kính với bug 1.9.3.

## 6. Điểm mù đã biết, chấp nhận có chủ đích

Nếu chỉ dùng Claude app/Cowork nhiều giờ mà không mở Claude Code, con số đứng im tới turn CC kế tiếp. Hook chỉ fire theo turn của CC.

Đây **không phải TODO**. Cách duy nhất vá được là gọi endpoint nội bộ của Anthropic bằng token của user - đã cân nhắc và bác bỏ, lý do đầy đủ ở research §3.

## 7. Đọc log khi debug

Log: `~/Library/Application Support/aki.devsync/usage.log` (macOS).
Bật chi tiết: chạy app với `--debug` hoặc `AKI_DEBUG=1`.

| Tag | Nguồn |
|---|---|
| `GET_USAGE`, `PROVISION` | Rust - mỗi điểm quyết định IPC |
| `SHELL:*` | stderr của script remote, relay từng dòng |
| `USAGE:claudecode@<host>` | JS - chuyển trạng thái, `poll tick`, `halted`; hậu tố `@<host>` để tách log của từng monitor |

Format: `[YYYYMMDD.HHMMSS.mmm][TAG] event key=value`.

## 8. File liên quan

| File | Vai trò |
|---|---|
| `src-tauri/src/remote_shell.rs` | Script transport funnel (`run_remote_shell`), SSH lock, timeouts |
| `src-tauri/src/agent_usage/` | Submodules for IPC commands, probe result types, and agent probes |
| `scripts/get-claudecode-usage.sh` | Đọc cache + auth, phát hiện STALE_RESET |
| `scripts/provision-claudecode.sh` | Vá statusLine hook |
| `scripts/lint-remote-scripts.js` | Gác bashism trong script gửi qua SSH |
| `src/composables/usageMonitor.js` | Một monitor: poll, circuit breaker, wake self-heal |
| `src/composables/usageMonitorRegistry.js` | Multiton `agentId@host` - định danh monitor |
| `src/components/AgentUsage.vue` | Trạng thái hiển thị, `ccBuckets` (§4.1) |
| `docs/ref/claude-quota-buckets.md` | Danh sách bucket đã biết của schema OAuth usage |

## 9. Tham chiếu ngoài

- Claude Code statusLine: https://docs.claude.com/en/docs/claude-code/statusline
- Pool quota dùng chung Pro/Max: https://support.claude.com/en/articles/11145838-use-claude-code-with-your-pro-or-max-plan

