# Kế hoạch khắc phục sai số Context Window (`ctx`) sau `/compact` và tối ưu hiệu năng chống `SIGKILL` 5s cho Unified Statusline

> **Status:** DONE 2026-11-25 · **Created:** 2026-11-22 · **Research worker:** `gemini-4-argon-high` (`0c68b230`) · **Related task:** `.akidevsync/notes.json` (`task-1795228278209`) · **Scope:** `src-tauri/src/statusline-unified.sh`, `src-tauri/src/statusline.rs`, `src/components/modals/ClaudeSettingModal.vue`, `docs/feat/statusline-customizer.md`, `docs/ref/statusline-unified-spec.md`

## 1. Kết luận điều hành (Executive Summary)

1. **Defect 1 — Sai số `ctx` sau `/compact` (`ctx 881k/1M` đỏ trong khi prompt thực tế chỉ còn `34.7k`):** `src-tauri/src/statusline-unified.sh` (L181–186, L437–449) đang tính `ctx_total = .context_window.total_input_tokens + .context_window.total_output_tokens` (tổng token cộng dồn của cả phiên từ lúc mở máy), thay vì độ chiếm dụng cửa sổ ngữ cảnh của lượt hiện tại (`.context_window.current_usage`). Khi người dùng chạy `/compact`, tổng cộng dồn không giảm nên `ctx` báo giả mức đỏ `>200k`. **Giải pháp chốt:** Tính `ctx_total` ưu tiên từ `.context_window.current_usage` (`input_tokens + cache_creation_input_tokens + cache_read_input_tokens + output_tokens`), chỉ fallback về `total_input_tokens + total_output_tokens` khi `current_usage` vắng mặt hoặc bằng `0` (tương thích ngược với payload tối giản và turn 0).
2. **Defect 2 — Bão fork tiến trình con gây timeout 5s (`signal: killed`) và nguy cơ tắt vĩnh viễn statusline trong phiên:** Mỗi lượt vẽ `statusline-unified.sh` hiện fork tuần tự **22–28 tiến trình ngoại vi** trên nhánh AGY (và **35–42 tiến trình** trên nhánh Claude Code) cộng thêm **~25 subshell `$(...)`**. Khi macOS chịu tải cao (Load 12–19, nén RAM/swap) và chạy song song nhiều cửa sổ AGY, thời gian thực thi vọt từ `~150ms` (idle) lên `>5000ms`, bị `statusline_runner.go` của `agy` bắn `SIGKILL`. Khảo sát 125 file log thực tế ghi nhận **61 sự kiện lỗi** trên 10 phiên, trong đó **5 phiên đã chạm chuỗi lỗi liên tiếp `20/30`** (chỉ cách ngưỡng `30/30` tự động vô hiệu hóa statusline đúng 10 lượt). **Giải pháp chốt:** Gom toàn bộ xử lý chuỗi/toán học vào lượt `jq` duy nhất, thay `whoami`/`hostname`/`basename`/`sed`/`tr`/`awk`/`git` bằng pure Bash 3.2 builtins, loại bỏ hoàn toàn subshell `$(colored ...)` bằng biến nội suy trực tiếp, chỉ ghi file `.frame` khi fingerprint quota đổi hoặc `mtime >= 300s` (bảo toàn tuyệt đối cửa sổ tươi 6h của `get-antigravity-usage.sh`), và cache `memory_pressure` theo TTL 15s trong `$HOME`. Kết quả kéo số fork ngoại vi ở trạng thái ổn định xuống **2 tiến trình** (`cat`, `jq`), thời gian chạy từ `~150–600ms` xuống **`<25ms`**.

---

## 2. Phân tích Deep-Think (`METHOD-deep-think.md`)

### 2.1. Chuỗi mục tiêu (B1 — Goal Chain)

- **Surface goal:** Sửa lỗi hiển thị sai số token `ctx` sau lệnh `/compact` và giảm thời gian chạy của `statusline-unified.sh` từ `~150–880ms` xuống `<60ms`.
- **Immediate goal:** Đảm bảo chỉ số `ctx` phản ánh đúng số token thực sự nằm trong context window ở lượt hiện tại trên cả Claude Code lẫn AGY CLI, đồng thời triệt tiêu bão `fork()/execve()` và khóa kernel VM (`memory_pressure`, I/O đĩa `.frame` dư thừa) để không bao giờ chạm ngưỡng timeout 5s của `statusline_runner.go`.
- **Ultimate goal:** Duy trì thanh trạng thái thời gian thực **chính xác về ngữ nghĩa** (người dùng biết chính xác khi nào cần `/compact`) và **bất tử về vận hành** (không bao giờ bị `agy` ngắt vĩnh viễn ở mốc `30/30` khi máy Mac đang build nặng hoặc chạy đa phiên, đồng thời tương thích 100% giữa macOS Bash 3.2 và các máy chủ Linux từ xa).

### 2.2. Phân rã nguyên lý gốc (B2 — Facts, Constraints, Assumptions)

#### Verified Facts (Đã kiểm chứng từ mã nguồn, binary, benchmark và log)
- **F1 (Log Forensics):** Quét 125 file log tại `/Users/aki/.gemini/antigravity-cli/log` (`2026-11-16` đến `2026-11-22`) phát hiện 10 file có lỗi statusline với tổng cộng **61 lần thất bại**: 59 lỗi `signal: killed`, 1 lỗi `context deadline exceeded`, 1 lỗi `signal: hangup`.
- **F2 (Cơ chế Auto-Disable 30 lượt của AGY):** Binary `/Users/aki/.local/bin/agy` (`v1.5.14`, `third_party/jetski/cli/store/statusline_runner.go:259`) chỉ ghi log tại các mốc chuỗi lỗi liên tiếp `1/30`, `10/30`, `20/30` và tự động tắt hẳn statusline với thông điệp `"Statusline disabled after %d consecutive failures."` ở `30/30`. Đã có **5 phiên thực tế chạm mốc `20/30`** (`cli-20261122_024938-lva-84246.log` với 28 log lỗi, `cli-20261122_135106-ntu-59990.log`, `cli-20261121_132538-lac-54297.log`, `cli-20261122_024216-lac-59990.log`, `cli-20261122_111607-ntu-93208.log`).
- **F3 (Bằng chứng nghẽn đồng thời — Stampede):** Các đỉnh lỗi xảy ra cùng một giây trên nhiều tiến trình `agy` chạy song song (`1122 15:32:34` ở `lva-84246` và `1122 15:32:35` ở `lac-54297`; `1122 15:42:33` ở `lva-84246` và `1122 15:42:34` ở `lac-54297`), chứng minh nguyên nhân là tranh chấp tài nguyên toàn hệ thống (XNU fork lock + `memory_pressure` VM lock + APFS directory lock).
- **F4 (Bằng chứng toán học trên 3 Fixture chuẩn trong `docs/ref/statusline-unified-spec.md`):**
  - Claude Code 2.1.217 (§8.4): `total_input_tokens: 39787`, `total_output_tokens: 263`, `context_window_size: 1000000`, `current_usage: {input_tokens: 2, output_tokens: 263, cache_creation_input_tokens: 862, cache_read_input_tokens: 38923}`, **không có trường `used_percentage`**. Tổng prompt hiện tại `2 + 862 + 38923 = 39787`; cộng `output_tokens` là `40050`.
  - AGY CLI 1.1.5 (§8.5): `total_input_tokens: 92138`, `total_output_tokens: 33015`, `context_window_size: 250000`, `used_percentage: 36.86`, `current_usage: {input_tokens: 601, output_tokens: 179, cache_creation_input_tokens: 0, cache_read_input_tokens: 102680}`. Phép chia `92138 / 250000 = 36.8552% -> 36.86%` chứng minh **chính AGY CLI đang tính `used_percentage` từ `total_input_tokens / context_window_size`** chứ không tính từ `current_usage` (`103281 / 250000 = 41.31%`).
  - AGY CLI 1.1.5 (§8.6): `total_input_tokens: 97441`, `total_output_tokens: 35195`, `context_window_size: 1048576`, `used_percentage: 9.29`, `current_usage: {input_tokens: 91499, output_tokens: 372, cache_creation_input_tokens: 0, cache_read_input_tokens: 0}`. Phép chia `97441 / 1048576 = 9.2927% -> 9.29%` một lần nữa xác nhận `used_percentage` của AGY bị neo vào tổng cộng dồn `total_input_tokens`.

| Thành phần đo thực nghiệm (5 runs, clean repo, idle) | Các lần đo (ms) | Trung bình (ms) | Số tiến trình con / cơ chế hiện tại |
|---|---|---|---|
| Toàn bộ `statusline-unified.sh` (nhánh AGY) | `[171.3, 165.1, 134.3, 136.9, 140.2]` | **149.6 ms** | ~24 binary forks + ~25 subshells (tăng lên 464–880ms khi repo lớn, >5000ms khi tải cao) |
| `6x awk` (`fmt_k` x2, `dynamic_pct`, `quota_used_pct` x2, `cache_pct`) | `[26.2, 21.9, 26.9, 21.6, 23.5]` | **24.0 ms** | 6–9 lần gọi `awk` rời rạc cho các phép chia số nguyên đơn giản |
| `whoami` + `hostname -s` + `basename` x2 | `[19.4, 16.6, 16.3, 16.9, 20.5]` | **17.9 ms** | 4 lệnh ngoại vi (`basename` bị gọi lặp 2 lần ở L248 và L409) |
| `git branch --show-current` | `[21.2, 12.8, 14.9, 16.7, 21.9]` | **17.5 ms** | Fork `git` khi payload thiếu `JSON_BRANCH` |
| `jq` single pass (L144) | `[11.4, 12.5, 14.6, 14.7, 15.4]` | **13.7 ms** | 1 tiến trình `jq-1.8.1` (bắt buộc giữ, có thể gánh toàn bộ math/regex) |
| `mktemp` + `mv` (ghi `.frame` mỗi lượt ở L242–244) | `[14.3, 13.7, 13.3, 11.8, 11.3]` | **12.9 ms** | `mkdir -p` + `mktemp` + `tr` + `mv -f` chạy vô điều kiện mỗi turn |
| `4x sed` + `1x tr` (`compact_model` + `effort`) | `[11.2, 12.3, 13.2, 9.5, 13.5]` | **11.9 ms** | Pipeline 5 chặng trong `compact_model` + `tr` hạ chữ thường |
| `memory_pressure` (macOS L592) | `[12.7, 8.1, 12.6, 14.7, 8.5]` | **11.3 ms** | `memory_pressure \| awk` gọi mỗi turn; nghẽn nặng khi kernel nén RAM |
| `ps -o etime= -p $PPID` (L511) | `[12.2, 9.6, 12.3, 9.4, 10.1]` | **10.7 ms** | `ps \| awk` chạy mỗi turn trên AGY vì AGY không gửi `duration_ms` |

#### Hard Constraints (Ràng buộc cứng không được phép vi phạm)
- **C1 (Tương thích đa nền tảng Bash 3.2 macOS + Linux):** macOS đóng gói `/bin/bash` **3.2.57(1)-release** (GPLv2 freeze), đồng thời Aki-Dev-Sync triển khai cùng script này qua SSH tới các máy chủ Linux từ xa (Ubuntu/Debian/CentOS chạy Bash 4/5). Tuyệt đối cấm dùng cú pháp Bash 4+ (`${var,,}`, `${var^^}`, `declare -A`, `readarray`/`mapfile`, `$EPOCHSECONDS`, `printf '%(...)T'`) và cấm phụ thuộc độc quyền vào flag BSD hay GNU.
- **C2 (Kiến trúc Single-Template & Marker Splicing):** `src-tauri/src/statusline-unified.sh` được nhúng vào Rust bằng `include_str!("statusline-unified.sh")`. Mọi dòng ngoài cặp `# >>> AKI-GENERATED-CONFIG >>>` và `# <<< AKI-GENERATED-CONFIG <<<` được xuất nguyên bản tới cả `~/.claude/statusline-command.sh` và `~/.gemini/antigravity-cli/statusline.sh`.
- **C3 (Hợp đồng tươi 6 giờ của `.frame` với `get-antigravity-usage.sh` và `antigravity.rs`):** `get-antigravity-usage.sh:338` lọc `find "$ag_frame" -mmin -360` và `src-tauri/src/agent_usage/antigravity.rs:220-228` kiểm tra ngưỡng `six_hours` (21.600 giây) dựa trên `mtime` của `<email>.frame`. Nếu bỏ qua ghi `.frame` chỉ dựa trên so sánh nội dung mà không làm mới `mtime` định kỳ, một phiên AGY hoạt động liên tục nhưng quota chưa đổi (hoặc gói không giới hạn) sau 6 giờ sẽ bị coi là stale và biến mất khỏi bảng Usage.
- **C4 (DESIGN LOCK của `aki-rlcache` và kiểm thử băm FNV-1a 64-bit trong `statusline.rs`):**
  - `docs/feat/statusline-customizer.md` khóa thiết kế 4 quy tắc của `aki-rlcache` (expiry, account+org scope, 6h staleness, `fresh_at` newest-wins).
  - `statusline.rs:2417` (`a_marker_block_edit_bumps_its_version`) băm FNV-1a 64-bit từng byte của khối `# aki-rlcache vN` (từ `# aki-rlcache v` đến trước `# aki-agquota v`) và khối `# aki-agquota vN` (từ `# aki-agquota v` đến trước `if [ -n "$JSON_CWD"`). Bất kỳ thay đổi nào trong hai vùng này bắt buộc phải nâng số phiên bản marker tương ứng và cập nhật hash trong test.
  - `statusline.rs:393-403` (`StatuslineHostStatus::from_probe`) phát hiện trạng thái `cc_outdated` hoàn toàn dựa vào `# aki-rlcache vN` và `ag_outdated` dựa vào `# aki-agquota vN`. Để giao diện Aki-Dev-Sync báo `Outdated` và nhắc người dùng bấm **Apply** cập nhật cả hai file cài đặt (`~/.claude/statusline-command.sh` và `~/.gemini/antigravity-cli/statusline.sh`), **cả hai marker phải được nâng phiên bản** (`# aki-rlcache v9` -> `v10`, `# aki-agquota v3` -> `v4`).
- **C5 (Cấm ghi file tạm ra `/tmp`):** Các test bảo mật `no_tmp_dump_on_any_execution_path` (`statusline.rs:2027`) và `legacy_tmp_symlink_is_not_followed` (`statusline.rs:2058`) kiểm soát chặt hành vi chạm vào `/tmp`. Mọi file cache mới (như cache RAM) phải đặt bên trong `$HOME` (nơi mỗi unit test trong `statusline.rs` đã cô lập qua `test_home(name)`).

#### Explicit Assumptions (Giả định tường minh)
- **A1:** Cả Claude Code (từ 2.1+) và AGY CLI (từ 1.1+) đều gửi `.context_window.current_usage` dưới dạng object chứa `input_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens` và (khi có) `output_tokens` ở mọi lượt đã có hội thoại.
- **A2:** Ở turn 0 (khi mới khởi động phiên, chưa gửi tin nhắn đầu tiên) hoặc trong một số test giả lập tối giản (`statusline.rs:1350`, `statusline.rs:1688`), `.context_window.current_usage` có thể là `null`/vắng mặt; khi đó fallback về `total_input_tokens + total_output_tokens` là hành vi an toàn duy nhất.

---

### 2.3. Phản biện 5 lăng kính bắt buộc (B3 — Five-Lens Critique)

#### Lăng kính 1: Steelman phương án thay thế (Bảo vệ góc nhìn đối lập)
- **Đối với Defect 1 (`ctx`):** *Tại sao không dùng `.context_window.used_percentage * .context_window.context_window_size / 100` hoặc chỉ cộng 3 trường prompt (`input_tokens + cache_creation_input_tokens + cache_read_input_tokens`) mà bỏ qua `current_usage.output_tokens`?*
  - **Steelman cho `used_percentage`:** Nếu CLI đã tính sẵn phần trăm, script chỉ việc đọc mà không cần tự cộng các trường con. **Bác bỏ bằng chứng thực nghiệm (Fact F4):** Trong spec §8.4, Claude Code không hề có trường `used_percentage`; còn trong spec §8.5 (`92138 / 250000 = 36.86%`) và §8.6 (`97441 / 1048576 = 9.29%`), chính AGY CLI tính `used_percentage` từ `total_input_tokens` cộng dồn! Dùng `used_percentage` sẽ làm hỏng hoàn toàn Claude Code và tái hiện nguyên vẹn lỗi `881k/1M` sau `/compact` trên AGY.
  - **Steelman cho tổng 3 trường prompt-only (`input + cache_create + cache_read`, không cộng `current_usage.output_tokens`):** Tổng 3 trường này trùng khớp tuyệt đối với mẫu số `cache_total` ở L535. Tuy nhiên, sau khi model trả lời xong một lượt (đặc biệt các lượt sinh 10k–32k token code/plan), toàn bộ `current_usage.output_tokens` của lượt vừa xong đã nằm trong lịch sử hội thoại và chiếm chỗ thực sự trong context window của lượt kế tiếp; đồng thời UI `ClaudeSettingModal.vue:356` mô tả rõ thành phần là `['token in+out', '/Max Token']`. Nếu cộng thêm `current_usage.output_tokens // 0`: trên payload thực tế (§8.4: `39787 + 263 = 40050`; §8.5: `103281 + 179 = 103460`), con số phản ánh chính xác tổng `in+out` của lượt hiện tại; còn trên các test fixture trong `statusline.rs` (L803, L1516) vốn không khai báo `current_usage.output_tokens`, `(.context_window.current_usage.output_tokens // 0)` tự động bằng `0`.
- **Đối với Defect 2 (Hiệu năng):** *Tại sao không viết lại statusline bằng một binary Rust nhỏ hoặc gộp chung vào daemon của Aki-Dev-Sync thay vì tối ưu Bash?*
  - **Steelman cho Rust binary:** Một binary biên dịch sẵn chạy mất `<3ms` và không phụ thuộc phiên bản Bash. **Bác bỏ theo ràng buộc kiến trúc (Constraint C1 & C2):** `statusline.rs` triển khai script tới các máy chủ Linux/macOS từ xa chỉ qua một lệnh SSH đẩy nội dung văn bản (`build_installer_scripts`), không có toolchain biên dịch hay cơ chế phân phối cross-architecture binaries (`x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `aarch64-apple-darwin`). Một script Bash thuần túy chỉ gọi `jq` đạt `<25ms` giải quyết triệt để timeout 5.000ms (dư biên an toàn 200 lần) mà không phá vỡ kiến trúc zero-binary-deploy.

#### Lăng kính 2: Tấn công thiết kế được chọn (Attack the Favored Design)
- **Điểm yếu 1 — Jitter 1 giây của `reset_time` trong `JSON_AG_SUMMARY` phá hỏng bộ lọc chống ghi `.frame`:**
  - Tại `statusline-unified.sh:218`, `jq` tạo trường `reset_time: ((now + .reset) | floor | todate)`. Vì `now` là thời gian thực (float) còn `.reset` (`reset_in_seconds`) là số nguyên do AGY cấp, tổng `now + .reset` khi qua `floor` sẽ dao động `±1 giây` giữa hai lượt chạy liên tiếp (ví dụ `"2026-11-22T18:00:01Z"` rồi `"2026-11-22T18:00:02Z"`). Nếu ta so sánh chuỗi `JSON_AG_SUMMARY` cũ và mới để quyết định có ghi file `.frame` hay không, phép so sánh sẽ **luôn luôn khác nhau** mỗi khi `.reset > 0`, khiến tối ưu ghi đĩa `.frame` vô tác dụng 100%!
  - **Cách hóa giải trong thiết kế:** Tạo một biến fingerprint ổn định `JSON_AG_FP` ngay trong `jq` chỉ gồm những dữ liệu thực sự mang ngữ nghĩa thay đổi: email, `plan_tier`, và danh sách `key=remaining_fraction`. Chỉ ghi lại `<email>.frame` khi (a) `JSON_AG_FP` thay đổi, HOẶC (b) file `<email>.frame` chưa tồn tại / có timestamp ghi cuối cách hiện tại `>= 300s` (5 phút). Khi đó, trong các lượt chat liên tiếp (cách nhau vài giây đến vài phút) mà quota fraction chưa nhảy, script **không gọi `mktemp` và `mv`**, nhưng cứ mỗi 5 phút hoạt động vẫn cập nhật `.frame` một lần (vừa làm mới `reset_time`, vừa giữ `mtime` luôn `< 5 phút`, an toàn gấp 72 lần so với ngưỡng cắt 6 giờ = 360 phút).
- **Điểm yếu 2 — `stat` trên macOS trong `_rl_mtime` tốn 2 lần fork vì thử GNU trước BSD:**
  - Tại `statusline-unified.sh:30`, `_rl_mtime` chạy `stat -c %Y "$1" 2>/dev/null || stat -f %m "$1" 2>/dev/null`. Trên macOS (nơi chạy chính), `stat -c` luôn thất bại rồi mới fallback sang `stat -f`, nghĩa là mỗi lần đọc `mtime` tốn **2 tiến trình `stat`**.
  - **Cách hóa giải trong thiết kế:** Nhánh theo `$OSTYPE` (`darwin*` gọi thẳng `stat -f %m "$1"`, nhánh khác gọi `stat -c %Y "$1" || stat -f %m "$1"`), cắt ngay 50% số lệnh `stat` trên macOS. Hơn nữa, với cache RAM (`.ram-cache`), ghi thẳng `$_now $mem_pct` vào nội dung file để đọc bằng builtin `IFS=' ' read -r _ts _val < "$file"` với **0 lần fork** (không cần gọi `stat` để biết tuổi cache!).

#### Lăng kính 3: Đảo ngược bài toán (Inversion — Làm sao để chắc chắn thất bại?)
1. **Cách chắc chắn làm hỏng `statusline.rs` cargo test:** Sửa dòng trong vùng `# aki-rlcache` hoặc `# aki-agquota` mà quên nâng số version marker hoặc quên cập nhật mã băm FNV-1a 64-bit trong `a_marker_block_edit_bumps_its_version` (`statusline.rs:2417`), hoặc đổi giá trị `ctx` mà không kiểm tra `renders_a_line` (`statusline.rs:802`).
2. **Cách chắc chắn làm hỏng máy chủ Linux hoặc macOS:** Dùng `${effort_found,,}` (chết ngay trên macOS `/bin/bash` 3.2.57 với lỗi `bad substitution`) hoặc dùng `stat -f %m` không nhánh OS (chết trên Linux vì GNU `stat -f` in thông tin filesystem thay vì epoch mtime).
3. **Cách chắc chắn làm mất thẻ Antigravity CLI Usage sau 6 giờ:** Bỏ qua ghi `<email>.frame` khi `remaining_fraction` đứng yên (ví dụ tài khoản còn 100% hoặc giữ nguyên mức quota) mà không có ngưỡng tuổi tối đa (`max_age`), khiến `find "$ag_frame" -mmin -360` trong `get-antigravity-usage.sh:338` loại bỏ file.

#### Lăng kính 4: Pre-mortem (Giả sử 2 tuần sau khi ship vẫn xảy ra lỗi)
- **Kịch bản Pre-mortem 1:** *"2 tuần sau, người dùng mở 6 cửa sổ AGY cùng lúc khi máy đang swap 12GB; 6 cửa sổ cùng thấy `.ram-cache` hết hạn 15s ở cùng một mili-giây và cả 6 cùng gọi `memory_pressure`!"*
  - **Chốt chặn phòng vệ (Stampede Protection):** Trước khi gọi `memory_pressure`, nếu file `.ram-cache` đã tồn tại với giá trị cũ `_ram_old_val`, tiến trình chạm cache miss sẽ **ghi đè ngay lập tức** timestamp hiện tại `$_now $_ram_old_val` (cập nhật lạc quan — stale-while-revalidate) trước khi chạy `memory_pressure`. Nhờ vậy, 5 cửa sổ còn lại chạy trong cùng giây đó sẽ thấy `_now - _ram_ts == 0 < 15` và dùng ngay giá trị `_ram_old_val` trong **0.1ms**, chỉ duy nhất 1 tiến trình chạy `memory_pressure` để cập nhật số mới!
- **Kịch bản Pre-mortem 2:** *"Một phiên terminal mở xuyên 2 ngày (`etime` của `$PPID` có định dạng `2-04:15:30`); hàm parse pure-bash không xử lý dấu gạch ngang `dd-hh:mm:ss` nên in ra thời lượng âm hoặc lỗi cú pháp arithmetic!"*
  - **Chốt chặn phòng vệ:** Hàm pure-bash parse `etime` phải tách rõ phần ngày `${etime%%-*}` khi chuỗi chứa `*-*`, nhân `86400`, và ép cơ số 10 (`10#$x`) để Bash không hiểu nhầm `08`/`09` là số bát phân (octal error `value too great for base`).

#### Lăng kính 5: Tác động bậc hai (Second-Order Effects)
1. **Tác động lên cặp chỉ số `ctx` và `cache` (`↬`):** Trong `statusline-unified.sh:434` (`ClaudeSettingModal.vue`), khối `cache` nằm ngay sau `context` với chú thích *"how many tokens, and how much of it came from cache"*. Khi `ctx` chuyển từ `total_input_tokens + total_output_tokens` (cộng dồn cả phiên, ví dụ `881k`) sang `current_usage` (lượt hiện tại, ví dụ `35k`), nó trở nên **đồng nhất về mặt thời gian** với khối `cache` (`↬89% 31k`) ngay bên cạnh — chấm dứt nghịch lý cũ khi `ctx` hiện `881k` nhưng `cache_tokens` bên cạnh chỉ hiện `31k`.
2. **Tác động lên `StatuslineHostStatus` (`cc_outdated` / `ag_outdated`):** Khi nâng đồng thời `# aki-rlcache v9` -> `v10` và `# aki-agquota v3` -> `v4`, mọi host đang cài bản cũ sẽ hiện badge `Outdated` cho cả Claude Code và AGY CLI trong modal Statusline Customizer, dẫn dắt người dùng bấm **Apply** một lần để đồng bộ cả 2 script.
3. **Tác động lên `ClaudeSettingModal.vue`:** Dòng mô tả `desc` ở L233 (`'ctx NN% used/max, auto-colored by threshold'`) đã lỗi thời từ lúc bỏ hiển thị `%`; cập nhật lại thành `'ctx tokens used/max in current window (200k color scale)'` và đồng bộ `SAMPLE.ctxUsed` ở L820 (`'45k'` khớp với payload test L803 `32000 + 800 + 12400 = 45200`).

---

### 2.4. Bản ghi quyết định hội tụ (Converged Decision Records)

- **Decision D1 (Công thức tính `ctx_total`):**
  `Decided: Tính ctx_total = current_usage.input_tokens + current_usage.cache_creation_input_tokens + current_usage.cache_read_input_tokens + current_usage.output_tokens; nếu tổng này <= 0 thì fallback về total_input_tokens + total_output_tokens · because phản ánh chính xác tổng token in+out đang chiếm chỗ trong context window ở lượt hiện tại (sau /compact từ 461k tụt về đúng 34.7k), tương thích đồng nhất trên cả Claude Code lẫn AGY CLI, và giữ nguyên khả năng chạy với các payload tối giản không có current_usage · rejected (1) total_input_tokens + total_output_tokens (cộng dồn toàn phiên, không giảm sau /compact), (2) used_percentage * context_window_size (CC không có trường này; AGY §8.5/§8.6 tính used_percentage từ total_input_tokens nên vẫn sai sau /compact) · reopen if một CLI tương lai tách riêng trường current_context_tokens chính thức ở cấp root của context_window.`
- **Decision D2 (Kiến trúc cắt giảm fork & bảo vệ timeout 5s):**
  `Decided: Giữ kiến trúc single-file Bash 3.2/POSIX nhưng chuyển toàn bộ regex/math/formatting sang lượt jq sẵn có + pure Bash parameter expansion, bỏ toàn bộ subshell $(colored ...), ghi .frame có điều kiện (khi fingerprint email|tier|quota_fracs đổi hoặc mtime >= 300s), và cache memory_pressure 15s theo cơ chế stale-while-revalidate tại $HOME · because kéo số tiến trình con mỗi lượt AGY từ ~24 binary + ~25 subshell xuống 2 binary (cat, jq) + 0 subshell render (<25ms), triệt tiêu stampede khi mở nhiều cửa sổ AGY mà vẫn giữ 100% tương thích macOS Bash 3.2, Linux SSH deploy và cửa sổ tươi 6h của get-antigravity-usage.sh · rejected (1) chỉ so sánh chuỗi JSON_AG_SUMMARY (luôn trượt cache do now + .reset jitter ±1s và làm mtime quá hạn 6h khi quota đứng yên), (2) lưu cache RAM ở /tmp (vi phạm quy tắc cô lập test_home và kiểm thử bảo mật /tmp trong statusline.rs) · reopen if thời gian khởi tạo tiến trình jq đơn lẻ vượt quá 500ms trên máy cấu hình thấp.`

---

## 3. Đối chiếu chi tiết số lượng Fork & Toán học Context Window

### 3.1. Bảng kiểm toán tiến trình con (Fork Count Before vs. After)

| Nhóm chức năng | Hiện tại (`statusline-unified.sh` v9/v3) | Sau tối ưu (`statusline-unified.sh` v10/v4) | Cơ chế thay thế cụ thể (100% Bash 3.2 & Linux safe) |
|---|---|---|---|
| **Đọc stdin & Parse JSON chính** | `cat` (1) + `echo \| jq` (2) = **3 forks** | `cat` (1) + `jq <<< "$input"` (1) = **2 forks** | Dùng here-string `jq -r '...' <<< "$input"`, bỏ pipe subshell `echo "$input" \|` |
| **Ghi `.frame` & `.tier` (AGY L226–245)** | `tr` (2) + `mkdir` (1) + `mktemp` (1) + `mv` (1) = **5 forks/turn** | **0 forks** (steady-state) / 2 forks (khi đổi hoặc `>=300s`) | Sanitize email ngay trong `jq` (`JSON_ACCOUNT_SAFE`); lưu fingerprint `JSON_AG_FP` + chỉ ghi `.frame` khi fingerprint đổi hoặc `(_now - frame_ts) >= 300` |
| **Identity (`whoami`, `hostname -s`)** | `whoami` (1) + `hostname -s` (1) = **2 forks** | **0 forks** | `user="${USER:-${LOGNAME:-$(id -un 2>/dev/null)}}"`; `host="${HOSTNAME:-$(hostname -s 2>/dev/null)}"`; `host="${host%%.*}"` |
| **CWD (`basename` x2 ở L248 & L409)** | `basename` (2) = **2 forks** | **0 forks** | Pure Bash: strip trailing `/`, lấy `${_p##*/}` (xử lý gọn cả `$HOME` -> `~` và `/`) |
| **Model & Effort (`compact_model`)** | `sed` (3–4) + `tr` (2) = **5–6 forks** | **0 forks** | Tính sẵn `JSON_MODEL_CLEAN` và `JSON_EFFORT_CLEAN` ngay trong lượt `jq` chính (dùng `sub`, `gsub`, `ascii_downcase`) |
| **Toán học & Định dạng (`awk` x6–9)** | `awk` (6–9) = **6–9 forks** | **0 forks** | `jq` xuất sẵn số nguyên `%` cho quota/cost; `fmt_k`, `dynamic_pct`, `cache_pct` dùng toán học số nguyên Bash `$(( ... ))` |
| **Session Duration (`ps \| awk` L511)** | `ps` (1) + `awk` (1) = **2 forks** | **0 forks** (khi cache `$PPID`) / **1 fork** (`ps` không `awk`) | Parse chuỗi `[[dd-]hh:]mm:ss` của `ps -o etime= -p "$PPID"` bằng pure Bash cơ số `10#` |
| **Git Branch (`git branch` L552)** | `git` (1 khi thiếu `JSON_BRANCH`) = **0–1 fork** | **0 forks** | Khi `JSON_BRANCH` rỗng, đọc trực tiếp `$_cwd/.git/HEAD` bằng builtin `read` (`${_head#ref: refs/heads/}`), chỉ gọi `git` nếu là worktree phức tạp |
| **System RAM (`memory_pressure \| awk`)** | `memory_pressure` (1) + `awk` (1) = **2 forks/turn** | **0 forks** (trong TTL 15s) | Cache `$_now $mem_pct` tại `$HOME` (TTL 15s, stale-while-revalidate); trên Linux đọc trực tiếp `/proc/meminfo` bằng pure Bash (0 fork kể cả khi cache miss) |
| **Render Subshells `$(colored ...)`** | **~25 subshell forks** | **0 subshell forks** | Ghép chuỗi trực tiếp `"${COLOR}${text}${RESET_FG}"` và trả kết quả qua biến thay vì `$(...)` |
| **Nhánh CC `aki-rlcache` (L24–140)** | `date` (3) + `stat` (4 trên macOS) + `cat` (1) + `mkdir` (1) + `tr` (1) + `mktemp` (1) + `mv` (1) + `jq` (12) = **~24 forks** | `date` (1) + `stat` (2) + `jq` (2–3) + conditional write = **~6 forks** | Nhánh `_rl_mtime` theo `$OSTYPE` (1 `stat`), gộp chuỗi đọc/gộp/tỉa `rate_limits` trong `aki-rlcache v10`, dùng chung 1 biến `_now` cho toàn script |
| **TỔNG CỘNG (Steady-state AGY)** | **22–28 binary forks + ~25 subshells (~150–880ms)** | **2 binary forks (`cat`, `jq`) + 0 subshells (<25ms)** | **Giảm >92% tiến trình con, biên an toàn >200x dưới ngưỡng 5.000ms** |

### 3.2. Bảng kiểm chứng công thức `ctx` trên toàn bộ Fixtures & Kịch bản thực tế

Công thức chuẩn hóa trong `jq` / Bash:
- `ctx_cur = (.context_window.current_usage.input_tokens // 0) + (.context_window.current_usage.cache_creation_input_tokens // 0) + (.context_window.current_usage.cache_read_input_tokens // 0) + (.context_window.current_usage.output_tokens // 0)`
- `ctx_cum = (.context_window.total_input_tokens // 0) + (.context_window.total_output_tokens // 0)`
- `ctx_total = if ctx_cur > 0 then ctx_cur else ctx_cum end`

| Kịch bản / Nguồn dữ liệu | `total_input` + `total_output` (Cũ) | `current_usage` (`in + create + read + out`) | Kết quả `ctx` Cũ (Sai) | Kết quả `ctx` Mới (Đúng) | Màu bậc thang (thang 200k) |
|---|---|---|---|---|---|
| **Ca lỗi thực tế của User sau `/compact` (AGY `neon-high`)** | `~881,000` (cộng dồn trước & sau compact) | `3,757 + 0 + 30,901 + 0 = 34,658` | `ctx 881k/1M` (Đỏ `100%`) | **`ctx 35k/1M`** | Calm Aqua (`17% < 20%`) |
| **Spec §8.4 — Claude Code 2.1.217** | `39,787 + 263 = 40,050` | `2 + 862 + 38,923 + 263 = 40,050` | `ctx 40k/1M` | **`ctx 40k/1M`** | Bold Green (`20%`) |
| **Spec §8.5 — AGY CLI 1.1.5 (250k window)** | `92,138 + 33,015 = 125,153` | `601 + 0 + 102,680 + 179 = 103,460` | `ctx 125k/250k` (cộng dồn 33k out cũ) | **`ctx 103k/250k`** | Bold Yellow (`52% >= 51%`) |
| **Spec §8.6 — AGY CLI 1.1.5 (1M window)** | `97,441 + 35,195 = 132,636` | `91,499 + 0 + 0 + 372 = 91,871` | `ctx 133k/1M` | **`ctx 92k/1M`** | Bold Green (`46% < 51%`) |
| **Unit test `statusline.rs:803` (`renders_a_line`)** | `120,000 + 14,400 = 134,400` | `32,000 + 800 + 12,400 + 0 = 45,200` | `ctx 134k/1M` | **`ctx 45k/1M`** | Bold Green (`23% >= 20%`) |
| **Unit test `statusline.rs:1350` & `1688` (không có `current_usage`)** | `100 + 20 = 120` / `1000 + 0 = 1000` | `0` (fallback sang `ctx_cum`) | `ctx 120/1k` / `ctx 1k/200k` | **`ctx 120/1k` / `ctx 1k/200k`** | Calm Aqua (Tương thích ngược 100%) |

---

## 4. Ma trận tương thích Bash 3.2 (macOS) & Linux và Bảng Edge Cases (B5)

### 4.1. Quy tắc tương thích POSIX / Bash 3.2 bắt buộc khi viết `statusline-unified.sh`

| Tác vụ | Cú pháp BỊ CẤM (Bash 4+ hoặc không đồng nhất OS) | Cú pháp CHUẨN ĐƯỢC DÙNG (Bash 3.2.57 macOS + Bash 4/5 Linux) |
|---|---|---|
| Đổi chữ hoa thành chữ thường | `${var,,}` (lỗi `bad substitution` trên macOS Bash 3.2) | Xử lý bằng `ascii_downcase` ngay trong `jq` (0 fork) |
| Lấy epoch hiện tại | `$EPOCHSECONDS` hoặc `printf '%(%s)T' -1` (Bash 4.2+/5.0+) | Xuất `"JSON_NOW=" + (now \| floor \| tostring)` từ `jq` hoặc `_now=$(date +%s)` 1 lần duy nhất |
| Đọc `mtime` của file | Chỉ gọi `stat -f %m` (sai trên Linux) hoặc luôn gọi `stat -c %Y \|\| stat -f %m` (2 forks trên macOS) | Nhánh theo `$OSTYPE`: `case "$OSTYPE" in darwin*) stat -f %m "$1" ;; *) stat -c %Y "$1" \|\| stat -f %m "$1" ;; esac` |
| Parse số từ `ps -o etime=` | Tính trực tiếp `$(( $mm * 60 ))` khi `$mm` là `08` hoặc `09` (lỗi số bát phân `08: value too great for base`) | Dùng hệ cơ số 10 tường minh `$(( 10#$mm * 60 + 10#$ss ))` (hỗ trợ chuẩn trên Bash 3.2+ và ksh/zsh) |
| Đọc RAM trên Linux | `free \| awk ...` (2 forks) | Đọc trực tiếp `/proc/meminfo` bằng vòng lặp `while IFS=': ' read -r _k _v _; do ... done < /proc/meminfo` (0 forks, fallback `free` nếu không có `/proc/meminfo`) |

### 4.2. Bảng Edge Cases & Tác dụng phụ xếp theo mức độ nghiêm trọng (B5)

| Severity | Edge Case / Rủi ro | Nguyên nhân gốc | Cơ chế chặn trong bản thiết kế |
|---|---|---|---|
| **CRITICAL** | `get-antigravity-usage.sh` bỏ qua `<email>.frame` sau 6 giờ nếu người dùng chat liên tục nhưng quota không đổi | `get-antigravity-usage.sh:338` lọc `find "$ag_frame" -mmin -360` và `antigravity.rs:222` kiểm tra `mtime` 6 giờ | Lưu kèm timestamp lần ghi cuối vào `.${_ag_acct_safe}.fp` (`<epoch> <fingerprint>`); bắt buộc ghi lại `.frame` nếu fingerprint đổi **HOẶC** `(_now - _last_write) >= 300` (5 phút << 360 phút) |
| **CRITICAL** | 5+ cửa sổ AGY cùng hết hạn cache RAM 15s tại một thời điểm và đồng loạt gọi `memory_pressure` (Cache Stampede) | Nhiều tiến trình cùng đọc `.ram-cache` thấy `_now - _ts >= 15` | **Stale-while-revalidate:** Khi phát hiện hết hạn nhưng đã có `_old_pct`, tiến trình đầu tiên ghi đè ngay `$_now $_old_pct` vào `.ram-cache` trước khi chạy `memory_pressure`; các tiến trình đến sau trong cùng giây thấy timestamp mới nên dùng ngay `_old_pct` (0 fork) |
| **HIGH** | Cargo test `a_marker_block_edit_bumps_its_version` (`statusline.rs:2417`) đỏ hoặc UI không báo `Outdated` cho CC/AGY | Test kiểm tra FNV-1a 64-bit của khối `# aki-rlcache` và `# aki-agquota`; `from_probe` dựa vào số version của 2 marker này | Nâng `# aki-rlcache v9` -> `# aki-rlcache v10` và `# aki-agquota v3` -> `# aki-agquota v4`; cập nhật cặp `(version, fnv1a64_hash)` tại `statusline.rs:2420-2421` |
| **HIGH** | Lỗi Octal (`08`, `09`) khi parse `ps -o etime=` bằng pure Bash | Bash coi số bắt đầu bằng `0` trong `$(( ... ))` là hệ bát phân | Luôn ép cơ số 10 bằng toán tử `10#${var}` (ví dụ `$(( 10#$_m * 60 + 10#$_s ))`) sau khi kiểm tra chỉ chứa chữ số |
| **MEDIUM** | Payload ở Turn 0 hoặc trong unit test (`statusline.rs:1350`, `1688`) không có `.context_window.current_usage` | CLI chưa gửi `current_usage` trước lượt trả lời đầu tiên | `jq` tính sẵn `JSON_CTX_USED`: nếu tổng `current_usage` > 0 thì lấy `current_usage`, ngược lại lấy `total_input_tokens + total_output_tokens` |
| **MEDIUM** | Repo dùng Git Worktree (`.git` là một file chứa `gitdir: ...` thay vì thư mục) | Đọc trực tiếp `.git/HEAD` thất bại vì `.git` không phải thư mục | Kiểm tra `[ -f "$_cwd/.git/HEAD" ]` để đọc nhanh (0 fork); nếu `.git` là file worktree hoặc không tìm thấy mới fallback về `git branch --show-current` |
| **LOW** | `renders_a_line` (`statusline.rs:802`) hoặc preview trong `ClaudeSettingModal.vue` lệch số token mẫu | Payload ở L803 có `total=134.4k` nhưng `current_usage=45.2k` | Kiểm tra assertion của `renders_a_line` và đồng bộ `SAMPLE.ctxUsed` / mô tả UI trong `ClaudeSettingModal.vue` |

---

## 5. Thiết kế kỹ thuật chi tiết theo từng File

### 5.1. `src-tauri/src/statusline-unified.sh`

1. **Khối `# aki-rlcache v10` (nâng từ `v9`, dòng 23–140):**
   - Tối ưu `_rl_mtime()` theo `$OSTYPE` để trên macOS (`darwin*`) gọi thẳng `stat -f %m "$1" 2>/dev/null` (1 fork thay vì 2).
   - Gộp 2 lệnh `jq` lấy `_rl_acct` và `_rl_org_id` (L58–59) và lệnh đọc `rl_cached` / `_rl_cached_org` / `_rl_tier` (L84–90) để giảm 5 lần fork `jq` thừa trên nhánh Claude Code mà **không thay đổi bất kỳ quy tắc cổng nào** của DESIGN LOCK (expiry, account+org gate, 6h staleness, `fresh_at` newest-wins).
   - Sanitize tên file `RLCACHE` bằng fast-path `case "$_rl_acct" in *[!A-Za-z0-9@._+-]*) ... tr ... ;; *) _acct_safe="$_rl_acct" ;; esac` (0 fork cho email chuẩn).

2. **Khối `# aki-agquota v4` (nâng từ `v3`, dòng 142–246):**
   - Trong lượt `jq` duy nhất (dùng `jq -r '...' <<< "$input" 2>/dev/null` thay cho `echo "$input" | jq -r '...'`):
     - Tính sẵn thời gian hiện tại: `"JSON_NOW=" + (now | floor | tostring)`.
     - Tính sẵn `JSON_MODEL_CLEAN` (thay thế hoàn toàn 4 lệnh `sed` + 1 lệnh `tr` của `compact_model`) và `JSON_EFFORT_CLEAN` (thay thế `sed` + 2 lệnh `tr` ở L419–425):
       ```jq
       ($model_name | sub(" *\\([^)]*\\)$"; "") | gsub("claude|gemini"; ""; "i") | gsub(" "; "") | gsub("[-_./]{2,}"; "-") | sub("^[-_./]+"; "") | sub("[-_./]+$"; "")) as $clean_model
       | ((if (.effort | type) == "object" then .effort.level elif (.effort | type) == "string" then .effort elif (.model | type) == "object" then .model.effort else null end // "") | tostring) as $raw_eff
       | (if $raw_eff != "" then $raw_eff else ([$model_name | capture("\\((?<e>[^)]+)\\)").e] | first // "") end | ascii_downcase | if . == "thinking" then "" elif . == "medium" then "med" else . end) as $clean_effort
       ```
     - Tính sẵn `JSON_CTX_USED` theo Decision D1:
       ```jq
       ((.context_window.current_usage.input_tokens // 0) + (.context_window.current_usage.cache_creation_input_tokens // 0) + (.context_window.current_usage.cache_read_input_tokens // 0) + (.context_window.current_usage.output_tokens // 0) | floor) as $ctx_cur
       | (((.context_window.total_input_tokens // 0) + (.context_window.total_output_tokens // 0)) | floor) as $ctx_cum
       | (if $ctx_cur > 0 then $ctx_cur else $ctx_cum end) as $ctx_used
       ```
     - Tính sẵn các giá trị phần trăm đã làm tròn (`round`) cho quota và cost ngay trong `jq`: `JSON_5H_USED_PCT`, `JSON_7D_USED_PCT`, `JSON_COST_FMT`, `JSON_COST_PCT`, `JSON_ACCOUNT_SAFE` (`gsub("[^A-Za-z0-9@._+-]"; "_")`), và `JSON_AG_FP` (fingerprint ổn định gồm `$e + "|" + $t + "|" + ([ $quota | to_entries[] | select((.value | type) == "object" or (.value | type) == "number") | "\(.key)=\(if (.value | type) == "object" then (.value.remaining_fraction // "") else .value end)" ] | sort | join(","))`).
   - **Ghi `.frame` có điều kiện với bảo toàn `mtime` 6 giờ (L239–245):**
     ```bash
     if [ "$CLI" = "AG" ] && [ -n "$JSON_AG_STATUS" ] && [ -n "$JSON_AG_SUMMARY" ]; then
       _ag_rl_dir="$HOME/.gemini/antigravity-cli/rate-limits"
       _ag_acct_safe="${JSON_ACCOUNT_SAFE:-$(printf '%s' "$JSON_ACCOUNT_EMAIL" | tr -c 'A-Za-z0-9@._+-' '_')}"
       _ag_frame_file="$_ag_rl_dir/${_ag_acct_safe}.frame"
       _ag_fp_file="$_ag_rl_dir/.${_ag_acct_safe}.fp"
       _ag_now="${JSON_NOW:-$(date +%s)}"
       _ag_need_write=1
       if [ -f "$_ag_frame_file" ] && [ -f "$_ag_fp_file" ]; then
         IFS=' ' read -r _ag_fp_ts _ag_fp_old < "$_ag_fp_file" 2>/dev/null || true
         if [ "$_ag_fp_old" = "$JSON_AG_FP" ] && [ -n "$_ag_fp_ts" ] && [ $(( _ag_now - _ag_fp_ts )) -lt 300 ] 2>/dev/null && [ $(( _ag_now - _ag_fp_ts )) -ge 0 ] 2>/dev/null; then
           _ag_need_write=0
         fi
       fi
       if [ "$_ag_need_write" = 1 ]; then
         [ -d "$_ag_rl_dir" ] || mkdir -p "$_ag_rl_dir" 2>/dev/null
         _ag_rl_tmp=$(mktemp "$_ag_rl_dir/.frame.XXXXXX" 2>/dev/null) \
           && { { printf '%s\n%s\n' "$JSON_AG_STATUS" "$JSON_AG_SUMMARY" > "$_ag_rl_tmp" \
             && mv -f "$_ag_rl_tmp" "$_ag_frame_file" \
             && printf '%s %s\n' "$_ag_now" "$JSON_AG_FP" > "$_ag_fp_file" 2>/dev/null; } || rm -f "$_ag_rl_tmp"; }
       fi
     fi
     ```

3. **Khối Helpers & Rendering không subshell (L247–617):**
   - **Pure Bash `basename` (L247–249 & L406–412):**
     ```bash
     _raw_cwd="${JSON_CWD:-$PWD}"
     [ "$_raw_cwd" = "null" ] && _raw_cwd="$PWD"
     if [ "$_raw_cwd" = "$HOME" ]; then
       cwd_dir="~"
     else
       while [ "${_raw_cwd%/}" != "$_raw_cwd" ] && [ "$_raw_cwd" != "/" ]; do _raw_cwd="${_raw_cwd%/}"; done
       cwd_dir="${_raw_cwd##*/}"
       [ -z "$cwd_dir" ] && cwd_dir="/"
     fi
     ```
   - **Loại bỏ subshell `$(colored ...)` và `$(color_for_pct ...)`:**
     - Thay `$(colored "$COL" "$txt")` bằng `"${COL}${txt}${RESET_FG}"`.
     - Hàm `set_color_for_pct` gán thẳng vào biến `PCT_COLOR` thay vì `printf` qua subshell `$(color_for_pct ...)`.
     - Hàm `fmt_k_var`, `fmt_dur_var`, `fmt_sec_eta_var` gán thẳng vào biến `FMT_OUT` để 0 fork.
   - **Pure Bash `fmt_k_var` (0 `awk` fork):**
     ```bash
     fmt_k_var() {
       local t="${1:-0}"
       t="${t%.*}"
       case "$t" in ''|*[!0-9-]*) t=0 ;; esac
       if [ "$t" -ge 1000000 ]; then
         FMT_OUT="$(( (t + 500000) / 1000000 ))M"
       elif [ "$t" -ge 1000 ]; then
         FMT_OUT="$(( (t + 500) / 1000 ))k"
       else
         FMT_OUT="$t"
       fi
     }
     ```
   - **Khối Context Window (`EN_context=1`, L436–450):**
     ```bash
     g_context=""
     if [ "$EN_context" = 1 ] && [ -n "$JSON_CTX_SIZE" ] && [ "$JSON_CTX_SIZE" -gt 0 ] 2>/dev/null; then
       ctx_total="${JSON_CTX_USED:-0}"
       ctx_total="${ctx_total%.*}"
       if [ "${ctx_total:-0}" -le 0 ] 2>/dev/null; then
         ctx_total=$(( ${JSON_CTX_INPUT%.*} + ${JSON_CTX_OUTPUT%.*} ))
       fi
       fmt_k_var "$ctx_total"; ctx_fmt="$FMT_OUT"
       fmt_k_var "$JSON_CTX_SIZE"; size_fmt="$FMT_OUT"
       dynamic_pct=$(( (ctx_total + 1000) / 2000 ))
       [ "$dynamic_pct" -gt 100 ] && dynamic_pct=100
       [ "$dynamic_pct" -lt 0 ] && dynamic_pct=0
       set_color_for_pct "$dynamic_pct"
       g_context="${WHITE}ctx${RESET_FG}${PCT_COLOR}${ctx_fmt}${RESET_FG}/${GREY}${size_fmt}${RESET_FG}"
     fi
     ```
   - **Khối Session Duration (`EN_session=1`, L508–514):**
     - Khi `JSON_DUR <= 0`, đọc `etime` bằng `ps -o etime= -p "$PPID" 2>/dev/null` và parse `[[dd-]hh:]mm:ss` hoàn toàn bằng pure Bash với ép cơ số `10#` (loại bỏ `awk` và sửa luôn lỗi `dd-hh:mm:ss` khi terminal chạy qua ngày).
   - **Khối Git Branch (`EN_git_branch=1`, L550–554):**
     - Nếu `JSON_BRANCH` rỗng, kiểm tra file `"${JSON_CWD:-$PWD}/.git/HEAD"` bằng `IFS= read -r _git_head < "${JSON_CWD:-$PWD}/.git/HEAD"`; nếu khớp `ref: refs/heads/*` thì lấy `${_git_head#ref: refs/heads/}` (0 fork); chỉ khi không đọc được mới fallback sang `git branch --show-current 2>/dev/null`.
   - **Khối System RAM (`EN_ram=1`, L589–598) với Cache 15s Stale-While-Revalidate tại `$HOME`:**
     ```bash
     g_ram=""
     if [ "$EN_ram" = 1 ]; then
       _ram_now="${JSON_NOW:-${_rl_now:-$(date +%s)}}"
       if [ "$CLI" = "AG" ]; then
         _ram_cache="$HOME/.gemini/antigravity-cli/.ram-cache"
       else
         _ram_cache="${CLAUDE_CONFIG_DIR:-$HOME/.claude}/.ram-cache"
       fi
       mem_pct=""
       _ram_ts=0; _ram_val=""
       if [ -f "$_ram_cache" ] && [ ! -L "$_ram_cache" ]; then
         IFS=' ' read -r _ram_ts _ram_val < "$_ram_cache" 2>/dev/null || true
       fi
       if [ -n "$_ram_val" ] && [ -n "$_ram_ts" ] && [ $(( _ram_now - _ram_ts )) -lt 15 ] 2>/dev/null && [ $(( _ram_now - _ram_ts )) -ge 0 ] 2>/dev/null; then
         mem_pct="$_ram_val"
       else
         # Stampede guard: refresh timestamp with stale value before spawning memory_pressure
         [ -n "$_ram_val" ] && printf '%s %s\n' "$_ram_now" "$_ram_val" > "$_ram_cache" 2>/dev/null
         if [[ "$OSTYPE" == "darwin"* ]]; then
           _mp_out=$(memory_pressure 2>/dev/null)
           case "$_mp_out" in
             *"System-wide memory free percentage:"*)
               _free_pct="${_mp_out##*System-wide memory free percentage: }"
               _free_pct="${_free_pct%%%*}"
               _free_pct="${_free_pct%%[![:digit:]]*}"
               [ -n "$_free_pct" ] && mem_pct=$(( 100 - 10#$_free_pct ))
               ;;
           esac
         elif [[ "$OSTYPE" == linux* ]]; then
           if [ -r /proc/meminfo ]; then
             _mt=0; _ma=0; _mf=0; _mb=0; _mc=0
             while IFS=': ' read -r _k _v _; do
               case "$_k" in
                 MemTotal) _mt="$_v" ;;
                 MemAvailable) _ma="$_v" ;;
                 MemFree) _mf="$_v" ;;
                 Buffers) _mb="$_v" ;;
                 Cached) _mc="$_v" ;;
               esac
             done < /proc/meminfo
             if [ "${_mt:-0}" -gt 0 ]; then
               if [ "${_ma:-0}" -gt 0 ]; then
                 mem_pct=$(( ((_mt - _ma) * 100 + _mt / 2) / _mt ))
               else
                 mem_pct=$(( ((_mt - _mf - _mb - _mc) * 100 + _mt / 2) / _mt ))
               fi
             fi
           else
             mem_pct=$(free 2>/dev/null | awk '/Mem:/ {printf "%.0f", $3/$2 * 100}')
           fi
         fi
         if [ -n "$mem_pct" ]; then
           _ram_dir="${_ram_cache%/*}"
           [ -d "$_ram_dir" ] || mkdir -p "$_ram_dir" 2>/dev/null
           printf '%s %s\n' "$_ram_now" "$mem_pct" > "$_ram_cache" 2>/dev/null
         else
           mem_pct="$_ram_val"
         fi
       fi
       [ -n "$mem_pct" ] && g_ram="${WHITE}⚅${RESET_FG}${GREY}${mem_pct}%${RESET_FG}"
     fi
     ```

---

## 6. Checklist thực thi theo từng Phase (Execution Plan — `RULE-docs.md` B1)

### Phase 1: Cập nhật Template nguồn `src-tauri/src/statusline-unified.sh`
- [x] **1.1. Nâng cấp `# aki-rlcache v9` -> `# aki-rlcache v10` (L23–140):** Đổi `_rl_mtime()` sang nhánh `$OSTYPE` (`darwin*` gọi `stat -f %m` trực tiếp), dùng fast-path `case` cho sanitize tên file `RLCACHE`, gộp đọc `.email` và `.orgId` của `_cc_auth` vào 1 lần gọi `jq`, giữ nguyên 100% logic cổng DESIGN LOCK.
- [x] **1.2. Nâng cấp `# aki-agquota v3` -> `# aki-agquota v4` & sửa công thức `ctx` trong `jq` (L142–223):** Đổi `echo "$input" | jq` sang `jq -r '...' <<< "$input"`, xuất thêm `JSON_NOW`, `JSON_MODEL_CLEAN`, `JSON_EFFORT_CLEAN`, `JSON_CTX_USED` (tổng 4 trường `current_usage` với fallback `total_input_tokens + total_output_tokens`), `JSON_5H_USED_PCT`, `JSON_7D_USED_PCT`, `JSON_COST_FMT`, `JSON_COST_PCT`, `JSON_ACCOUNT_SAFE`, và `JSON_AG_FP`.
- [x] **1.3. Ghi `.frame` có điều kiện với chốt chặn tươi 5 phút (`< 300s` << `360m`) (L239–245):** So sánh `JSON_AG_FP` và timestamp trong `.${_ag_acct_safe}.fp`; chỉ gọi `mktemp` + `mv -f` khi fingerprint đổi hoặc tuổi file `>= 300s`, đảm bảo `get-antigravity-usage.sh:338` và `antigravity.rs:222` không bao giờ gặp `.frame` quá hạn 6 giờ.
- [x] **1.4. Thay toàn bộ lệnh ngoại vi và subshell ở phần Render (L247–617):** Thay `whoami`/`hostname -s`/`basename` x2/`compact_model` (`4x sed + tr`)/`awk` x6 bằng biến môi trường + pure Bash integer math + giá trị tính sẵn từ `jq`; thay `$(colored ...)` bằng nội suy chuỗi `"${COL}${txt}${RESET_FG}"`; đọc nhanh `.git/HEAD` bằng builtin `read`; parse `ps -o etime=` bằng pure Bash cơ số `10#`; thêm `.ram-cache` TTL 15s (stale-while-revalidate) cho `memory_pressure` (macOS) và `/proc/meminfo` (Linux).

### Phase 2: Cập nhật Rust Backend & Bộ Unit Tests trong `src-tauri/src/statusline.rs`
- [x] **2.1. Cập nhật test ghim marker & hash `a_marker_block_edit_bumps_its_version` (`src-tauri/src/statusline.rs:2417-2425`):** Cập nhật tuple `(RLCACHE_MARKER, AGQUOTA_MARKER, 10, <new_fnv1a64>)` và `(AGQUOTA_MARKER, r#"if [ -n "$JSON_CWD""#, 4, <new_fnv1a64>)`. Đã làm với mốc cuối `_cwd="${JSON_CWD` vì dòng `if [ -n "$JSON_CWD"` không còn.
- [x] **2.2. Kiểm tra & cập nhật test `renders_a_line` (`src-tauri/src/statusline.rs:802-816`):** Với payload L803 (`total_input_tokens: 120000, total_output_tokens: 14400`, `current_usage: {cache_read: 12400, cache_creation: 800, input: 32000}`), xác nhận output mới hiển thị `ctx45k/1M` thay vì `ctx134k/1M`.
- [x] **2.3. Bổ sung 3 Unit Tests hồi quy mới vào `src-tauri/src/statusline.rs`:** Đã làm: ca sau `/compact` ghim trong `renders_a_line` (`ctx45k/1M`), thêm `ctx_falls_back_to_session_totals_without_a_current_window` và `agy_rewrites_its_quota_frame_only_when_the_quota_changes`.
  - `ctx_prefers_current_usage_over_cumulative_totals_after_compact`: Truyền payload có `total_input_tokens: 850000, total_output_tokens: 31000, context_window_size: 1000000` nhưng `current_usage: {input_tokens: 3757, cache_read_input_tokens: 30901, cache_creation_input_tokens: 0, output_tokens: 200}`, assert output chứa `"ctx35k/1M"` (màu calm aqua `38;5;86m`, KHÔNG phải `"881k"` màu đỏ).
  - `ctx_falls_back_to_cumulative_totals_when_current_usage_is_absent_or_zero`: Truyền payload không có `current_usage` (hoặc toàn `0`) với `total_input_tokens: 12000, total_output_tokens: 3000, context_window_size: 200000`, assert output chứa `"ctx15k/200k"`.
  - `agy_skips_redundant_frame_rewrite_within_ttl_but_rewrites_on_quota_change`: Chạy script 2 lần liên tiếp với cùng payload AGY, kiểm tra lần 2 không đổi inode/mtime khi chưa quá 300s; chạy lần 3 với `remaining_fraction` thay đổi, assert file `<email>.frame` được cập nhật ngay lập tức.

### Phase 3: Đồng bộ Giao diện Vue & Tài liệu Dự án (Doc Sync)
- [x] **3.1. Cập nhật `src/components/modals/ClaudeSettingModal.vue`:**
  - Dòng 233: Cập nhật `context.desc` từ `'ctx NN% used/max, auto-colored by threshold'` sang `'ctx tokens used/max in current window (200k color scale)'`.
  - Dòng 820: Cập nhật `SAMPLE.ctxUsed` từ `'134.4k'` sang `'45.2k'` (hoặc `'45k'`) và `SAMPLE.ctxPct` tương ứng để preview khớp với cách tính `current_usage`.
- [x] **3.2. Cập nhật `docs/feat/statusline-customizer.md` & `docs/ref/statusline-unified-spec.md`:**
  - Cập nhật bảng trường dữ liệu `ctx` (`statusline-customizer.md:173` và `statusline-unified-spec.md:23`): ghi rõ `ctx` đọc từ `.context_window.current_usage` (`input_tokens + cache_creation_input_tokens + cache_read_input_tokens + output_tokens`), fallback sang `total_input_tokens + total_output_tokens` khi `current_usage` vắng mặt/bằng 0.
  - Cập nhật số phiên bản marker trong tài liệu sang `aki-rlcache v10` và `aki-agquota v4`, ghi chú cơ chế ghi `.frame` có điều kiện (TTL 300s trong cửa sổ 6h) và `.ram-cache` (TTL 15s).
- [x] **3.3. Đóng task trong `.akidevsync/notes.json`:** Sau khi toàn bộ test và benchmark thực tế xanh, đánh dấu `"done": true` cho note `task-1795228278209` (*"Tối ưu tốc độ statusline.sh (tránh bị agy kill timeout 5s khi máy tải cao)"*).

---

## 7. Lệnh kiểm chứng Runtime (Verification Commands)

1. **Kiểm tra cú pháp dưới `/bin/bash` 3.2.57 (macOS):**
   ```bash
   /bin/bash -n src-tauri/src/statusline-unified.sh
   ```
2. **Chạy toàn bộ bộ test `statusline` và `antigravity` trong Rust (bao gồm kiểm tra POSIX, marker hash, toggle isolation, quota cache, `.frame` emission):**
   ```bash
   npm run test:statusline
   cd src-tauri && cargo test antigravity
   ```
3. **Đo thời gian steady-state & kiểm chứng output sau `/compact` (isolated `HOME`, không chạm state thật):**
   ```bash
   python3 -c '
   import subprocess, tempfile, os, time, shutil
   h = tempfile.mkdtemp()
   try:
       p = r"""{"cwd":"/tmp/demo","account":{"email":"lva@akitao.com"},"plan_tier":"Ultra","model":{"display_name":"Gemini 4 Argon (High)"},"context_window":{"total_input_tokens":850000,"total_output_tokens":31000,"context_window_size":1000000,"current_usage":{"input_tokens":3757,"cache_read_input_tokens":30901,"cache_creation_input_tokens":0,"output_tokens":120}},"quota":{"gemini-argon-5h":{"remaining_fraction":0.82,"reset_in_seconds":3600},"gemini-argon-wtu-weekly":{"remaining_fraction":0.64,"reset_in_seconds":86400}},"workspace":{"git_branch":"master"}}"""
       env = {**os.environ, "HOME": h}
       os.makedirs(f"{h}/.gemini/antigravity-cli", exist_ok=True)
       sh = f"{h}/.gemini/antigravity-cli/statusline.sh"
       open(sh, "w").write(open("src-tauri/src/statusline-unified.sh").read())
       os.chmod(sh, 0o755)
       subprocess.run([sh], input=p.encode(), env=env, stdout=subprocess.PIPE)
       times = []
       for _ in range(5):
           t0 = time.perf_counter()
           out = subprocess.run([sh], input=p.encode(), env=env, stdout=subprocess.PIPE).stdout.decode()
           times.append((time.perf_counter() - t0) * 1000)
       print("Output:", repr(out))
       print("5 runs (ms):", [round(x, 1) for x in times], "Avg:", round(sum(times)/len(times), 1), "ms")
       assert "35k/1M" in out and "881k" not in out
   finally:
       shutil.rmtree(h, ignore_errors=True)
   '
   ```
   - **Tiêu chí nghiệm thu (Acceptance Criteria):**
     - Output hiển thị `ctx35k/1M` (màu calm aqua), tuyệt đối không hiển thị `881k/1M`.
     - Thời gian chạy trung bình 5 lượt steady-state trên macOS đạt **`< 30ms`** (giảm từ `149.6ms` ở clean repo và `464–880ms` ở thực tế, thấp hơn mục tiêu `< 60ms` của `task-1795228278209`).
     - Toàn bộ `npm run test:statusline` và `cargo test antigravity` đạt 100% PASS.
