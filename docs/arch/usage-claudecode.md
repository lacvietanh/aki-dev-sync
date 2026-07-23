# Kiến trúc: theo dõi quota Claude Code

> Mô tả hệ thống **đang chạy**. Vì sao nó thành ra thế này, và những gì đã thử rồi bỏ:
> `docs/research/claudecode-usage-FINAL.md` - **đọc §5 (nhật ký) trước khi đề xuất thay đổi**.
>
> Viết lại 2026-07-20 (v1.14.0), sau khi xoá toàn bộ luồng active. Bản trước dài 388 dòng và
> chứa 5 chỗ tự đính chính inline; những đoạn sai đã bỏ, kết luận đúng giữ lại.

## 1. Nguyên lý - một nguồn dữ liệu duy nhất

Claude Code CLI xuất telemetry qua hook `statusLine` (khai báo ở `~/.claude/settings.json`).
Sau **mỗi turn** của một session **interactive**, CLI đẩy một JSON vào stdin của hook - trong đó
có `rate_limits`, số thật từ server Anthropic, không phải ước lượng.

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

**App không bao giờ tự chạy `claude` để lấy số.** Đây là ràng buộc kiến trúc, không phải chi
tiết cài đặt - xem §5.

Chỉ hoạt động với tài khoản Claude.ai Pro/Max (API key thường không có `rate_limits`).

### Payload của hook

```json
{
  "rate_limits": {
    "five_hour": { "used_percentage": 42, "resets_at": 1782034800 },
    "seven_day": { "used_percentage": 18, "resets_at": 1782288000 }
  },
  "cwd": "/home/user/project",
  "transcript_path": "/home/user/.claude/projects/..."
}
```

`resets_at`: Unix epoch giây, UTC. `used_percentage`: phần trăm đã dùng trong cửa sổ tương ứng.

Pool quota Pro/Max **dùng chung** cho claude.ai web, Desktop, mobile, Cowork và Claude Code  - 
nên con số này đã bao gồm mọi hoạt động, không riêng CC.

### File trên máy được theo dõi

```
~/.claude/settings.json            → trỏ tới statusLine script
~/.claude/statusline-command.sh    → script app vá vào, hứng stdin, ghi cache
~/.claude/rate-limits-cache.json   → cache app đọc. NGUỒN DỮ LIỆU DUY NHẤT
~/.claude/auth-cache.json          → email/org/tier, TTL 300s
~/.claude/.credentials.json        → ⚠️ KHÔNG còn tồn tại trên bản CC mới (keychain)
```

## 2. Local hay remote - cùng một script

`run_interpreter_timeout()` (`agent_usage.rs`) kiểm tra `is_local_host(host)`
(`"local"`/`"localhost"`) rồi rẽ nhánh:

| | Claude Code (`Interpreter::Sh`) | Antigravity (`Interpreter::Node`) |
|---|---|---|
| Local | `sh` | `zsh -lc node` (login shell - resolve `node` qua nvm) |
| Remote | `ssh host sh` | `ssh host node` |

Cùng một script POSIX chạy được cả hai đường vì nó chỉ đụng `$HOME`.

Ở tầng UI, "Claude Code (local)" và "Claude Code (remote)" là hai instance độc lập của
`useAgentUsage()`. Nguồn remote có công tắc riêng (`aki-src-ccremote-enabled`, độc lập với sync
check) - xem `docs/feat/sync-check-and-usage-switches.md`.

### Mọi SSH theo timer đi qua `polling_ssh()`

Kèm `BatchMode=yes`, `ConnectTimeout=10`, `ServerAliveInterval=5`, `ServerAliveCountMax=3`.
Thiếu `ConnectTimeout`, một host bão hoà có thể đốt trọn ngân sách 30s chỉ ở khâu bắt tay TCP.

### Một khoá cho mỗi host

`run_interpreter_timeout()` lấy `host_lock(host)` (registry `HashMap<host, Arc<Mutex<()>>>`) trước
khi spawn. Vì mọi lệnh remote của app đều đi qua funnel này, hai tính năng khác nhau (probe
statusline, Apply, poll usage, git info) không bao giờ bắn SSH chồng lên **cùng một host**; host
khác nhau vẫn chạy song song bình thường. Khoá dùng `unwrap_or_else(|e| e.into_inner())` để một
panic không khoá chết host đó vĩnh viễn.

### Mọi lệnh `claude` chạy xa đều có giới hạn tại chỗ

`CLAUDE_BIN_RESOLVER_PREAMBLE` đặt `AKI_CLAUDE_TMO` - prefix bound 45s cho mọi lệnh `claude`,
**thực thi trên chính máy đó**, không dựa vào việc cắt SSH từ xa (cắt SSH không giết được cháu
qua login shell). Thứ tự tìm: `timeout` → `gtimeout` → `perl -e 'alarm shift; exec @ARGV'`.

macOS không có sẵn `timeout`/`gtimeout` nên nhánh `perl` là nhánh thực tế chạy trên Mac. Dùng
`exec` để tín hiệu bắn thẳng vào `claude`, không vào lớp bọc.

Preamble này cũng resolve `$CLAUDE_BIN` bằng kiểm tra file tĩnh trước khi rơi về PATH - tránh
đua với việc shell rc chưa source xong lúc app vừa khởi động.

## 3. Provision - cài hook

`provision-claudecode.sh` chạy khi cache đọc ra `null` (hook có thể chưa tồn tại). Nó vá
`statusline-command.sh` để ghi `rate-limits-cache.json`, và ghi `auth-cache.json`.

Khi turn của CLI đẩy ra JSON **không có** `rate_limits`, script merge nguyên vẹn cục
`rate_limits` cũ từ cache vào JSON mới - **không fabricate** giá trị.

Từ 1.18.0 (`aki-rlcache v4`), việc merge đó có hai điều kiện bắt buộc - đánh dấu `DESIGN LOCK`
ngay trong script `src-tauri/src/statusline-unified.sh` (khối `# aki-rlcache v4`, chỉ chạy khi
`CLI=CC`; xem `docs/feat/statusline-customizer.md`):
- entry đã qua `resets_at` bị **loại**, không hiển thị (`resets_at: 0` = "không rõ", vẫn giữ);
- cache ghi kèm account đã ghi nó; cache của account khác (hoặc cache cũ 1.10.0-1.17.0 không có
  field `account` trong khi máy đang biết account) bị bỏ, không đọc.

Thiếu hai điều kiện này, một field lọt vào cache sẽ sống vĩnh viễn (merge chỉ thêm/ghi đè key có
trong payload, không bao giờ xoá key vắng mặt) - đúng nguyên nhân của quota ma `7d 45%` cho account
không hề có weekly limit. Chi tiết: `docs/plan/1.18.0-statusline-apply-correctness.md` §P0-5.

Hai điều kiện này được test khoá lại (`cc_drops_a_cached_quota_whose_reset_has_passed`,
`cc_ignores_a_cache_written_by_another_account`), cùng với việc **nhánh AGY không bao giờ đọc/ghi
file cache này** (`agy_never_touches_the_claude_rate_limit_cache`). Cách chạy các test đó trên máy
không build được Tauri: `docs/research/statusline-generator-test-suite.md`.

### Hai gate, hai vai trò khác nhau

Gate ở trên nằm trong `statusLine` hook (Rust generate ra, chạy trên máy được theo dõi) - đó là
phía **ghi**: "dọn tận gốc", loại field trước khi nó kịp ghi xuống `rate-limits-cache.json`. Nhưng
hook đó chỉ chạy trên host **đã** nhận bản script mới; một host chưa được Apply lại vẫn có thể ghi
ra một cache không có field `account`, hoặc một cache có `resets_at` đã qua mà không ai dọn.

Vì vậy `scripts/get-claudecode-usage.sh` (phía **đọc**, chạy mỗi ~30s cả local lẫn qua SSH) áp lại
đúng hai gate đó một lần nữa, ngay trước khi ghi ra stdout - "lọc lúc hiển thị", không đụng tới file:

1. **Account gate**: so `account` trong cache với `.oauthAccount.emailAddress` hiện tại
   (`~/.claude.json`). Hai bên đều có giá trị và khác nhau → toàn bộ cache bị coi là không đáng
   tin, script không in gì ra stdout (giống hệt nhánh "thiếu file cache"). Cache cũ (v2/v3, không
   có field `account`) **không** bị bỏ theo cách này - nếu bỏ, mọi host chưa được vá lại sẽ mất
   trắng quota hiển thị; script chỉ log cảnh báo là host đó nên được Apply lại.
2. **Expiry gate**, áp cho từng entry trong `rate_limits`: bỏ entry có `resets_at` đã qua; bỏ luôn
   entry có `resets_at` bằng 0 hoặc thiếu - một cửa sổ không xác minh được chính là hình dạng của
   quota ma `7d` năm xưa, không hiện còn an toàn hơn hiện sai.
3. Sau khi lọc, `rate_limits` rỗng → cũng không in gì ra stdout.

Script đọc **không bao giờ** ghi lại hay xoá `rate-limits-cache.json` - việc đó vẫn là của
statusLine hook. Nếu `python3` lỗi phân tích JSON, coi như "không có dữ liệu đáng tin", không cho
`set -e` giết cả script và không rơi về in nguyên file thô.

Tier hiển thị (Pro/Max): đọc `.credentials.json` trước; file này không còn trên bản CC mới nên
fallback sang parse `subscriptionType` từ `claude auth status` (đã cache). `rateLimitTier`
(5x/20x) không có nguồn thay thế - để `Unknown`, không ảnh hưởng badge chính.

## 4. Trạng thái hiển thị

| Trạng thái | Khi nào | UI |
|---|---|---|
| `data` | Cache đọc được, mốc reset còn ở tương lai | Vòng tròn % + mốc reset |
| `cached` | Đã qua mốc reset, chưa có turn CC mới | Số đo cuối + nhãn thời điểm + `Waiting for next Claude Code session` |
| `empty` | Chưa từng đọc được cache | Dòng trạng thái |
| `off` | Nguồn bị tắt, hoặc Remote Mode tắt | Dòng trạng thái |

`get-claudecode-usage.sh` phát hiện `now > resets_at` → trả `|||STALE_RESET|||`. Phía JS **giữ
nguyên `data`**, bật `isCached`/`cachedAt` - cùng cơ chế Antigravity dùng. Số cũ đứng lại tới
khi có turn CC mới ghi cache.

Trễ phát hiện reset ≤ một chu kỳ poll (mặc định 30s). Đã cân nhắc đặt timer đúng tại
`resets_at + 2s` và **chủ động không làm**: thêm phức tạp để tiết kiệm vài giây mỗi 5 giờ.

### Circuit breaker

Hỏng 5 lần liên tiếp → `haltPolling()` dừng hẳn poll và báo lý do. Chỉ hành động rõ ràng của
người dùng mới mở lại (Reload, hoặc đổi host) - **không** phải `visibilitychange`/`focus`, vốn
bắn liên tục và sẽ dựng lại vòng lặp qua cửa sau.

Lý do dừng hẳn thay vì giãn dần: log sự cố 2026-07-20 cho thấy probe vốn đã tuần tự
(guard `isChecking`), không có gì để giãn. Dừng vừa đơn giản vừa trung thực hơn.

### WKWebView suspend self-heal

WKWebView bóp/treo `setInterval` khi cửa sổ bị che hoàn toàn hoặc máy ngủ. Hai lớp phục hồi,
cài **một lần** ở module scope, dùng chung cho cả 3 instance:

1. `visibilitychange`/`focus` → check ngay.
2. Nhịp watchdog 7s → nếu khoảng cách giữa hai tick vượt ngưỡng thì coi như vừa resume.

Ngưỡng lấy theo từng subscriber, không phải hằng số chung - nếu không, một nguồn đang halt sẽ bị
watchdog đánh thức liên tục.

## 5. Ràng buộc bất biến

Vi phạm bất kỳ mục nào là tái diễn một bug đã trả giá. Chi tiết:
`docs/research/claudecode-usage-FINAL.md` §6.

1. **App không tự chạy `claude` để lấy usage.** Luồng active (force-sync/probe) đã bị xoá
   2026-07-20 - nó gây tràn RAM một máy remote đến mức phải bỏ máy, và đo thật cho thấy một turn
   headless chỉ trả mốc reset, không có phần trăm.
2. **Không gọi endpoint nội bộ không công bố của Anthropic** (từng có `oauth/usage`, đã gỡ).
3. **Chỉ một nguồn ghi cache**: statusLine hook.
4. **Script gửi qua SSH phải là POSIX `sh` thuần** - remote chạy dash, không phải bash.
   `set -o pipefail` trần sẽ giết dash im lặng; phải dùng
   `( set -o pipefail ) 2>/dev/null && set -o pipefail`. Có `scripts/lint-remote-scripts.js` gác.
5. **Mọi lệnh `claude` chạy xa phải bound tại chỗ**, không dựa vào cắt SSH.
6. **Thao tác nhanh và chậm không dùng chung ngân sách thời gian.**

## 6. Điểm mù đã biết, chấp nhận có chủ đích

Nếu chỉ dùng Claude app/Cowork nhiều giờ mà không mở Claude Code, con số đứng im tới turn CC kế
tiếp. Hook chỉ fire theo turn của CC.

Đây **không phải TODO**. Cách duy nhất vá được là gọi endpoint nội bộ của Anthropic bằng token
của user - đã cân nhắc và bác bỏ, lý do đầy đủ ở research §3.

## 7. Đọc log khi debug

Log: `~/Library/Application Support/aki.devsync/usage.log` (macOS).
Bật chi tiết: chạy app với `--debug` hoặc `AKI_DEBUG=1`.

| Tag | Nguồn |
|---|---|
| `GET_USAGE`, `PROVISION` | Rust - mỗi điểm quyết định IPC |
| `SHELL:*` | stderr của script remote, relay từng dòng |
| `USAGE:claudecode` | JS - chuyển trạng thái, `poll tick`, `halted` |

Format: `[YYYYMMDD.HHMMSS.mmm][TAG] event key=value`.

## 8. File liên quan

| File | Vai trò |
|---|---|
| `src-tauri/src/agent_usage.rs` | IPC, funnel spawn, preamble, timeout |
| `scripts/get-claudecode-usage.sh` | Đọc cache + auth, phát hiện STALE_RESET |
| `scripts/provision-claudecode.sh` | Vá statusLine hook |
| `scripts/lint-remote-scripts.js` | Gác bashism trong script gửi qua SSH |
| `src/composables/useAgentUsage.js` | Poll, circuit breaker, wake self-heal |
| `src/components/AgentUsage.vue` | Trạng thái hiển thị |

## 9. Tham chiếu ngoài

- Claude Code statusLine: https://docs.claude.com/en/docs/claude-code/statusline
- Pool quota dùng chung Pro/Max: https://support.claude.com/en/articles/11145838-use-claude-code-with-your-pro-or-max-plan
