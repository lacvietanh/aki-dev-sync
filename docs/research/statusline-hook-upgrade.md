# Nâng hook statusline đã cài khi app đổi định dạng cache

## Start time

2026-10-20, sau khi owner thấy thẻ Claude Code chỉ hiện `lacvietanh@gmail.com` dù vừa chạy hai phiên `claude` và `cl-tom` bằng hai account khác nhau.

## Initial purpose

Hook `aki-rlcache v6` (một file quota cho mỗi account) đã có trong `[Unreleased]` trên 1.33.1, nhưng không host nào chạy nó. Câu hỏi: vì sao hook mới không tới được host, nên nâng hook ở đâu và lúc nào, và hook v6 có đúng trên Linux, Windows, và với các thư mục cấu hình khác không. Ràng buộc lúc viết: app chỉ ship macOS (`CLAUDE.md`); các host remote là máy SSH trong `~/.aki/devsync/projects.json` (`akicloud`, `bien`, `grokvm`); cấu hình statusline nằm trong `localStorage` của component `ClaudeSettingModal.vue` (`docs/feat/statusline-customizer.md` § SSOT).

## Strategy

Đọc flow probe → auto-repair → Apply. Kiểm tra trên từng máy chỉ bằng lệnh đọc: phiên bản hook đã cài, nơi lưu login, các lệnh hook cần. Sau đó so các điểm có thể đặt việc nâng hook, và chạy phản biện (`METHOD-deep-think.md` B3) với phương án được chọn.

## Checklist

1. Đọc `check_statusline_status` / `PROBE` (`src-tauri/src/statusline.rs`) và `checkAndAutoInstall` trong modal.
2. Trên Mac: phiên bản hook đã cài, thư mục `rate-limits/`, `claude auth status` khi có và không có `CLAUDE_CONFIG_DIR`.
3. Trên từng host remote, qua SSH: hệ điều hành, phiên bản hook, `.credentials.json`, có `shasum`/`sha256sum`/`python3`/`jq`/`claude` không, các thư mục `~/.claude*`.
4. Đọc nhánh hook v6 cho trường hợp `claude auth status` không trả email.
5. So các trigger: chỉ khi mở modal, khi poll usage, lúc mở app.

## Result

- **Nguyên nhân gốc.** Probe chỉ trả "có hook và `settings.json` trỏ tới nó", còn auto-repair chỉ ghi khi hook *thiếu*. Đổi định dạng cache vì vậy không bao giờ tới được host đã có hook: app cũ vẫn đọc file một-account cũ mà không có dấu hiệu gì. Hook trên Mac là `# aki-rlcache v4` (sửa lần cuối 2026-09-27), và chưa có thư mục `~/.claude/rate-limits/`.
- **Mac.** `env -u CLAUDE_CONFIG_DIR claude auth status` cho `lacvietanh@gmail.com`, `CLAUDE_CONFIG_DIR=/Users/aki/.claude` cho `tdlinh01@gmail.com`. Keychain có mục `Claude Code-credentials`, và sha256 của `/Users/aki/.claude` bắt đầu bằng `05a20467`.
- **`akicloud`** (Linux): hook v4, không có `.credentials.json` trong `~/.claude` lẫn `~/.claude-2`. `claude auth status` cho `authMethod: oauth_token` và không có email, vì phiên dùng `ANTHROPIC_AUTH_TOKEN` + `ANTHROPIC_BASE_URL`, tức là qua gateway. Dưới v4 không có `rate-limits-cache.json`, nghĩa là host này chưa từng nhận `rate_limits`. Có đủ `shasum`, `sha256sum`, `python3`, `jq`.
- **`grokvm`** (Linux): hook v4, có `~/.claude` nhưng không có `claude` trên PATH. Có đủ các lệnh trên.
- **`bien`**: không vào được (`ssh: connect … Operation timed out`).
- **Không có email.** Hook v6 để `RLCACHE` trống, không ghi cũng không đọc file quota nào. Dưới v4 những phiên này cũng không có gì để ghi (xem `akicloud`), nên không có gì bị mất.
- **Thư mục cấu hình khác.** Hook luôn ghi vào `$HOME/.claude/rate-limits/` và script đọc cũng đọc đúng chỗ đó, nên phía đọc vẫn đúng. Phần còn hở nằm ở phía cài: Apply chỉ sửa `~/.claude/settings.json`, nên phiên chạy với `~/.claude-2` không bao giờ chạy hook.
- **Verification:** mọi kết quả trên lấy từ output lệnh trong phiên này. Phần chưa kiểm: nhiều account OAuth trên Linux (không có host nào đăng nhập kiểu đó), và lần nâng hook lúc mở app trên bản build thật. Phần sau phụ thuộc runtime: test `the_probe_flags_an_older_rlcache_hook_as_outdated` khoá phần probe, còn phần ghi file dùng lại `apply_statusline_config`, vốn đã có test riêng.
- **Corroborating links:** `docs/arch/usage-claudecode.md` § Nhiều account trên một máy; `docs/ref/multiple-account-config-dir.md` mục 3; `docs/feat/statusline-customizer.md` § Rollout to hosts.

## Decision

**Action.** Probe đọc marker `# aki-rlcache vN` cao nhất trong hook đã cài và bật `cc_outdated` khi số này nhỏ hơn số của template. Script không có marker thì không phải do app sinh, nên không bao giờ bị ghi lại. Lúc mở app, mỗi host được probe một lần trong phiên (Mac này cộng host remote của từng project, theo thứ tự project được nạp), và chỉ hook *cũ* mới được ghi lại, bằng cùng generator và config đã lưu như khi bấm Apply. Mở modal thì vẫn cài thêm ở host chưa có hook. Lần chạy lúc mở app chỉ diễn ra trong cửa sổ host của bản release. Ghi thất bại thì hiện toast cảnh báo có tên host. Code: `src-tauri/src/statusline.rs` (`rlcache_version`, `StatuslineHostStatus::from_probe`), `src/components/modals/ClaudeSettingModal.vue` (`repairCcStatusline`).

**Rejected / closed:**
- *Chỉ nâng khi mở modal* (thiết kế cũ): chính là lỗi đang sửa. Không ai mở Customizer sau khi cập nhật app.
- *Nâng ở lần poll usage đầu tiên của host*: chỉ phủ được host đang được theo dõi usage, và phải tách config statusline ra khỏi component, tức thêm một module chỉ vì một trigger.
- *Ghép vùng config của hook cũ vào template mới, không cần config đã lưu*: thành một bộ sinh script thứ hai, và một biến config mà template mới cần nhưng vùng cũ không có sẽ âm thầm bị bỏ trống.
- *Chỉ báo, không tự ghi*: lại phải chờ người dùng nhìn thấy rồi bấm, đúng cái bẫy cũ.
- *Chạy lúc mở app trong bản dev*: bản dev có `localStorage` riêng, sẽ ghi đè các lựa chọn hiển thị của owner bằng giá trị mặc định lên hook thật.

**No action:**
- *Linux*: không đổi code. Mọi lệnh hook cần đều có sẵn trên hai host đã kiểm, và không có login thì không có quota, giống v4.
- *Windows*: không hỗ trợ. App chỉ ship macOS, script remote là POSIX `sh` qua SSH.
- *Thư mục cấu hình khác*: đưa vào backlog #9. Chưa có ai dùng thư mục khác với login có quota.

**Phản biện đã chạy.** Steelman cho "chỉ khi mở modal": ghi file lúc mở app là ghi vào máy người khác mà người dùng không hề bấm gì. Trả lời: chỉ ghi lại file do chính app sinh ra, kèm backup có timestamp và toast nêu tên host; không cài mới thứ gì. Pre-mortem: hook bị sửa tay sau lần Apply sẽ bị ghi đè lúc mở app. Mức chấp nhận được: Apply vốn đã như vậy, có backup, và toast báo ngay. Tác động thứ hai: mở app thêm một lần SSH probe cho mỗi host, chạy nền, có giới hạn thời gian và khoá theo host; một host không vào được (`bien`) chỉ bị đánh dấu unreachable.

**Reopen if:** owner mất chỉnh sửa tay trên một hook sau khi mở app; toast "Could not write" hiện mỗi lần mở app với cùng một host; có host Linux đăng nhập OAuth nhiều account mà menu email chỉ hiện một account; có người dùng thật chạy Claude Code với thư mục cấu hình khác.

**Cross-references:** `docs/feat/statusline-customizer.md` § Rollout to hosts, `docs/arch/usage-claudecode.md` § Nhiều account trên một máy, `CHANGELOG.md` `[1.34.0]` › Fixed.
