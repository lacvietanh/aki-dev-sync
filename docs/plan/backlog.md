# Backlog aug17 — các note còn mở trong `.akidevsync/notes.json`

Chốt từ 10 task chưa `done` trong `.akidevsync/notes.json` (đọc 2026-08-17). Hai note pin cụ thể nhất đã tách sang `docs/plan/1.26.1-improve.md`; file này giữ phần còn lại, xếp theo thứ tự cần-nghiên-cứu-trước → làm-được-ngay.

| # | Note | Loại | Trạng thái |
|---|---|---|---|
| B1 | app data dir → `~/.aki/devsync/` | research trước | code shipped 2026-08-19, chờ verify trên Mac |
| B2 | project config vào `.akidevsync/` trong repo | research trước | giữ nguyên 2026-09-09 |
| B3 | GPU cao khi app idle | research trước | shipped 2026-08-19, số đo GPU chưa đo lại trên Mac |
| B9 | Claude Code: xem được nhiều account | code | tạm bỏ qua (owner) |
| B4 | Task Notes: nới giới hạn text, dùng chung pattern | code | done 2026-09-08 |
| B5 | AGY pre-allow list: bổ sung mục | code | done 2026-09-08 |
| B6 | REPORT: định nghĩa "file mới" theo chu kỳ check | code | rejected 2026-09-09 (owner) |
| B7 | Link Google Search Console cho project web | code | done 2026-09-09 |
| B8 | `task-1786949417624` rỗng | rác | xoá |
| B11 | Tàn dư đổi tên: AkiClaudeDoc → AkiDevRule | code | done 2026-09-09 |
| B12 | Terminal tab: nới maxwidth + cuộn ngang | code | done 2026-09-09 |
| B13 | Thêm Cursor vào IDE list của popup OPEN | code | done 2026-09-09 |
| B14 | Default excludes: thêm `__pycache__/` | code | done 2026-09-09 |
| B15 | Tàn dư: "ô host" thừa trong SSH config | code | done 2026-09-10 |
| B16 | CRITICAL: reload làm sai toàn bộ trạng thái terminal | code | done 2026-09-09 |
| B17 | Project Icon: nút reload + helptext, và cache icon không bao giờ quét lại | code | done 2026-09-10 |
| B18 | Audit state nạp một lần lúc startup | audit | báo cáo xong 2026-09-10, 3 mục chưa làm |

---

## B1 — Chuyển app data dir vào `~/.aki/devsync/`

Note `task-1786953511448`. Mục tiêu: chuẩn hoá theo hệ sinh thái `~/.aki/`, kèm migration một chiều — bản mới thấy nơi cũ có file mà nơi mới chưa có thì chuyển hết rồi xoá nơi cũ.

Hiện trạng: mọi thứ nằm ở `app_data_dir()` của Tauri (`~/Library/Application Support/aki.devsync/` trên macOS) — `projects.json`, `usage.log`, baseline sync, cache SSH. Call site: `src-tauri/src/projects.rs:127,137` (`get_app_data_dir`, là funnel chính), `src-tauri/src/logger.rs:39`, `src-tauri/src/sync.rs:25-27,422-447,538` (đã có sẵn một migration cũ từ `~/.aki/devsync-baselines` — đọc trước khi viết cái mới, đây chính là tiền lệ đúng để copy), `src-tauri/src/ssh.rs:67,84,95`.

Hành vi đã chốt trong note, không bàn lại: bản mới thấy nơi cũ có file mà nơi mới chưa có → chuyển hết các file cần thiết sang nơi mới → xoá khỏi nơi cũ.

Việc:
1. Chỉ sửa **một** funnel `get_app_data_dir` — không rải path mới ra từng call site (`pattern.A1`).
2. Migration đặt ở đầu vòng đời khởi động, trước mọi lệnh đọc/ghi appdata; copy xong mới xoá nguồn để crash giữa chừng không mất dữ liệu. Copy pattern từ migration baseline sẵn có ở `src-tauri/src/sync.rs:538`.
3. Migration là hành động lên hệ thống thật, không phải chỉ viết code (`coding.B3`): phải chạy trên máy Mac có dữ liệu thật mới được đóng.

**Code shipped 2026-08-19**: `src-tauri/src/app_paths.rs` là funnel duy nhất (`app_data_dir()` + `migrate_legacy_app_data()`), chạy ở đầu `setup()` trong `lib.rs`, trước cả `logger::init`. Chi tiết migration, checklist verify: `docs/plan/done/appdata-dir-to-aki-devsync.md`. Chưa đóng theo mục 3 ở trên — chưa chạy trên Mac có dữ liệu thật.

## B2 — Đưa project config vào chính repo, ở `.akidevsync/`

Note `task-1785676763350`. Đã có tiền lệ: `notes.json` của tính năng task list đã nằm trong `.akidevsync/` của từng repo từ 1.22.0 (`docs/feat/project-task-list.md`).

Ràng buộc mà chính note đã nêu: config chứa **đường dẫn máy cụ thể**. Repo được mirror từ Mac lên remote thì đường dẫn hai bên khác nhau — commit path của Mac vào repo là làm hỏng phía remote, mà repo lại là thứ dùng chung.

Note đã chỉ định đúng quy trình: research + phân tích ưu nhược trước, rồi hoặc ra giải pháp tốt nhất, hoặc giữ nguyên sau khi phân tích kỹ. Research doc (`docs/research/`, schema `docs.B2`) phải phân tách rõ: trường nào là *thuộc tính của project* (mang theo repo được — tên, thứ tự, toggle tính năng) và trường nào là *thuộc tính của máy* (path local, host SSH, credential — không bao giờ vào repo).

**Giữ nguyên 2026-09-09**: phân loại đầy đủ toàn bộ field của `SyncProject` ở `docs/research/akidevsync-project-config-scope.md`. Đa số field (`local_path`, `remote_host`, `remote_path`, `disabled`, `dry_run`, `delete_on_pull`, `delete_on_push`) thuộc về máy/cặp máy-remote, không thể tách. Nhóm còn lại đủ điều kiện "thuộc project" (`name`, `production_url`, `pull_excludes`, `push_excludes`, `hooks.*`, `dev_cmd_override`, `build_cmd_override`) nhỏ, ít sửa, và hai trong số đó (`hooks.*`, `*_cmd_override`) vẫn có thể chứa path máy cụ thể trong nội dung lệnh — tách container ra repo không làm nội dung portable hơn. Không có nhu cầu cụ thể nào (chia sẻ giữa máy/owner) được nêu ra để bù chi phí vi phạm SSoT (`pattern.A1`) của việc tách config một project ra hai file. Reopen khi có nhu cầu cụ thể.

## B3 — GPU cao khi app chỉ ngồi idle

Note `task-1786650061322`, pin. Câu hỏi của note: vì sao app ăn GPU nhiều dù không làm gì — có hiệu ứng CSS/JS nào chạy sai logic, hay flow rác nào lặp vô ích không.

Đây là việc đo trước, sửa sau. Nghi phạm phải kiểm tra bằng số liệu chứ không đoán: animation/transition CSS chạy vĩnh viễn (`animation: … infinite`, thanh progress, con trỏ nhấp nháy của terminal khi tab không hiển thị), timer/interval của usage monitor và refresh controller vẫn quay khi cửa sổ bị che, xterm render liên tục, và mọi `requestAnimationFrame` không có điều kiện dừng.

Ràng buộc: chỉ đo được trên Mac với app chạy thật (`coding.B3`) — Activity Monitor/`powermetrics` cho số nền, Safari Web Inspector cho phần web (nhớ chọn đúng target `localhost`, không phải `Main.html` của chính inspector). Kết quả vào một research doc (`docs/research/`, schema `docs.B2`) trước khi mở việc sửa.

**Shipped 2026-08-19**: research đã có ở `docs/research/perf-idle-gpu-cpu.md` (đợt trước) và plan `docs/plan/done/fix-idle-gpu-webkit-compositor.md` (đợt này, phần "Implemented"). `backdrop-filter` gỡ khỏi chrome thường trực, đưa vào công tắc "Glass Effect" (mặc định tắt); con trỏ terminal chỉ nhấp nháy ở tab đang active + đang focus; `RefreshRing` đổi sang bước ~1 giây thay vì animate mỗi frame. Số đo GPU/CPU thật chưa được đo lại trên Mac — mục tiêu `< 2%`/`< 5%` của plan gốc vẫn còn để ngỏ.

## B4 — Task Notes: nới giới hạn text, dùng chung pattern

Note `task-1786650384549`. Giới hạn hiện tại rải rác, không có nguồn duy nhất:

| Nơi | Giới hạn |
|---|---|
| `src/components/tasks/NotesField.vue:27` | `maxlength` mặc định 1500 |
| `src/components/modals/ProjectTasksModal.vue:28` | 1500 (truyền tay, trùng giá trị trên) |
| `src/components/tasks/TaskListPanel.vue:26,77` | 200 (title) |
| `src/components/tasks/TaskListPanel.vue:86` | 500 (detail) |
| `src/components/modals/GlobalNoteModal.vue:13` | 100000 |

Yêu cầu của note: xử lý cả global task notes, **dùng chung một pattern tái sử dụng** chứ không sửa lẻ từng chỗ. Việc: gom các hằng số về một nơi (`pattern.A1`), đặt tên theo vai trò (`TASK_TITLE_MAX`, `TASK_DETAIL_MAX`, `NOTES_MAX` — `pattern.A7`), rồi nâng giá trị. Nâng bao nhiêu là câu hỏi mở; ràng buộc thật là `notes.json` nằm trong repo người dùng nên không được phình vô hạn.

**Done 2026-09-08**: đính chính một chỗ lệch trong bảng trên — `TaskListPanel.vue:86` (detail textarea) thực tế đã là 1500, không phải 500 như bảng ghi. Gom về `src/constants/taskLimits.js` (`TASK_TITLE_MAX`, `TASK_DETAIL_MAX`, `NOTES_MAX`, `GLOBAL_NOTE_MAX`), 5 call site còn lại tham chiếu hằng số này thay vì literal. Đã nâng `TASK_DETAIL_MAX` 1500→3000 và `NOTES_MAX` 1500→5000 (giá trị cụ thể là lựa chọn hợp lý của agent, không phải con số owner chốt — có thể chỉnh lại dễ dàng vì giờ chỉ có một nơi cần sửa); `TASK_TITLE_MAX` và `GLOBAL_NOTE_MAX` giữ nguyên. `npm run build` xanh.

## B5 — Bổ sung AGY pre-allow list

Note `task-1785805753407`. Tính năng đã có: `src-tauri/src/gemini_allowlist.rs` + `src/components/modals/GeminiAllowlistModal.vue`, mô tả ở `docs/feat/agy-command-allowlist.md`.

Thêm vào bộ lệnh khuyến nghị được check sẵn:
- mọi đường dẫn script mà skill trong `~/.aki/akidevrule` gọi tới;
- mọi lệnh READONLY mà project `aki-mcp-sv` dùng.

Cả hai danh sách phải liệt kê từ nguồn thật trước khi sửa, không đoán. Đây chỉ là dữ liệu, không đụng logic merge/backup.

**Done 2026-09-08**: `share/gemini_allowlist_unified.json` lên 100 mục — thêm 4 script akiflow còn thiếu (`council-cost.sh`, `council-read.sh`, `council-verify.sh`, `scythe.sh`, cùng thư mục với `council-open.sh` đã có sẵn) + `notes_cli.py`, cộng các lệnh readonly `aki-mcp-sv` đã coi là an toàn mà seed này còn thiếu (`pwd`, `tree`, `whoami`, `uniq`, `cut`, `top`, `nproc`, `lsblk`, `ip addr`, và git `describe`/`ls-remote`/`merge-base`/`shortlog`). Chi tiết: `docs/feat/agy-command-allowlist.md`.

## B6 — REPORT: "mới" tính theo chu kỳ check, không theo mốc thời gian cứng

Note `task-1783626118506`. Khi check sync thấy file REPORT mới thì bật swal xác nhận mở (Enter = mở). Vấn đề: chưa định nghĩa được thế nào là "mới".

Định nghĩa note đưa ra: trong khoảng ~2 phút **hoặc ~2 chu kỳ fetch** — và vì chu kỳ này người dùng chỉnh được trong settings, ngưỡng phải suy ra từ chu kỳ hiện hành chứ không hardcode phút. Nguồn chu kỳ: `docs/arch/refresh-controller.md`.

**Rejected 2026-09-09 (owner)**: owner được trình bày cả hai phương án — edge-detection (so sánh mtime của REPORT giữa hai lần fetch liên tiếp, không cần ngưỡng thời gian nào cả) và phương án đúng như note gốc (cửa sổ ~2 chu kỳ) — và trả lời là bỏ hẳn tính năng, không chọn phương án nào. Note `task-1783626118506` đã xoá khỏi `.akidevsync/notes.json`. Đây là *từ chối*, khác với B9 (chỉ hoãn); không mở lại trừ khi owner tự nêu lại.

## B7 — Link Google Search Console cho project dạng web

Note `task-1782727844849`, wish. Thêm lối mở nhanh Search Console cho project web, dạng `https://search.google.com/search-console/inspect?resource_id=sc-domain%3A<domain>`.

Hai điều kiện chặn đều tự giải được bằng dữ liệu sẵn có, không thêm trường nào: (a) `production_url` đã tồn tại trong project config và luôn có scheme nhờ chuẩn hoá lúc lưu, nên `new URL(...).hostname` (bỏ `www.`) cho ra domain; (b) chỗ đặt là hàng header sẵn có của popup OPEN, cạnh nút REPORT, tái dùng đúng class nút cũ — không thêm hàng, không thêm phần tử.

**Done 2026-09-09**: nút `GSC` trong popup OPEN, chỉ hiện khi project có Production URL (`src/components/ProjectTable.vue`, hàm `openSearchConsole`). Verify: `npm run build` xanh. Note `task-1782727844849` vẫn còn trong app — owner xoá trong UI khi tiện, giống B8.

## B8 — Note rỗng

`task-1786949417624` không có title lẫn detail. Xoá trong app, không phải việc code.

## B9 — Claude Code: xem được nhiều account (tạm bỏ qua)

Yêu cầu ban đầu của owner trong đợt 1.28: theo dõi được nhiều account Claude Code, giống pattern app đã làm cho Antigravity — nhưng cách detect account khác nhau giữa hai CLI (`docs/arch/usage-claudecode.md` §1 vs `docs/arch/usage-antigravity.md`).

Owner tự hoãn nguyên văn: "mất thời gian cho cái này vì cần tôi giúp debug -> tạm bỏ qua tính năng này". Không có code nào cho mục này trong đợt 1.28. Không lên lịch lại cho tới khi owner chủ động mở lại.

## B10 — SSH terminal: copy được text (đóng, đã chuyển thành plan)

**Kết luận cũ ở mục này là sai và đã bị thay thế.** Vòng điều tra 2026-08-19 kết luận "không sửa được bằng code của app này" dựa trên việc Terminal.app cũng lỗi giống hệt — nhưng triệu chứng giống nhau không có nghĩa là cơ chế giống nhau, và vòng 2026-08-20 tìm ra một lỗi nằm hoàn toàn trong tầm tay app: `⌘C` **chưa bao giờ** copy được trong terminal này, kể cả khi không có TUI nào bật mouse-mode. xterm chỉ đổ vùng chọn vào textarea cho cơ chế primary-selection của Linux, nên trong WKWebView sự kiện `copy` không bao giờ có gì để bám vào.

Chi tiết cơ chế + bằng chứng: `docs/research/terminal-copy-selection-root-cause.md`. Việc phải làm: `docs/plan/terminal-copy-selection.md`.

Toggle "khoá mouse reporting" đề xuất ở vòng trước **đóng, không lên lịch**: nó chỉ chữa nửa vùng-chọn, tắt luôn scroll/click bên trong TUI, và cần thêm UI trong một app đang theo luật Extreme Narrow.

## B11 — Tàn dư đổi tên: AkiClaudeDoc → AkiDevRule

Note `task-1787563509967`, vế thứ hai ("trong menu vẫn còn AkiClaudeDoc và nút Install từ thời chưa đổi tên").

**Done 2026-09-09**: menu App-icon còn nhãn "AkiClaudeDoc" và nút Repo trỏ tới `lacvietanh/AkiClaudeDoc` — repo đã chết, nên nửa "Repo" của mục này hỏng từ lúc đổi tên. Đổi đồng bộ cả chuỗi: nhãn + hai nút (`AppHeader.vue`), hằng URL sang `https://github.com/lacvietanh/akidevrule`, lệnh Tauri `install_akiclaudedoc` → `install_akidevrule` (`system.rs`, `lib.rs`, `hostInvoke.js`, `bridge.js` — tên lệnh lệch giữa Rust và JS là **silent no-op** trong Tauri nên phải đổi cùng lúc), danh sách đường dẫn checkout, và chuỗi lỗi tiếng Việt trong UI tiếng Anh viết lại thành tiếng Anh (`content.A2`). Prose ở `README.md`, `IntroModal.vue`, `docs/feat/in-app-terminal.md`, `docs/feat/remote-control.md` cập nhật theo; hai file `feat/` rewrite anchor stamp (`docs.A4`).

Cố ý **không** đụng: `CHANGELOG.md`, `docs/plan/done/`, `docs/research/` — bản ghi sự kiện, sửa lại là làm sai lịch sử (`docs.B2`); và `docs/ref/claudecode-cleanup-paths.md` vì `.akiclaudedoc-backup` là tên file thật có thể còn trên đĩa.

## B12 — Terminal tab: nới maxwidth + cuộn ngang

Note `task-1787274562374`.

**Done 2026-09-09**: gốc vấn đề không phải riêng con số maxwidth mà là `flex: 1 1 84px` — mọi tab dùng chung một chiều rộng co giãn, nên thêm tab là mọi tab cùng teo lại, và tên rename dài bị cắt ở trần 160px. Đổi thành `flex: 0 0 auto` (mỗi tab tự rộng theo nội dung, trong khoảng 84–220px) và cho `.tab-group` `overflow-x: auto` để tràn thì cuộn ngang như VSCode. Sửa hình dạng luồng chứ không chồng thêm guard (`pattern.A8`).

Không thêm nút cuộn, chevron hay menu overflow — luật Extreme Narrow. Scrollbar chỉ override đúng `height: 4px` so với 6px của global; màu kế thừa `main.css` chứ không định nghĩa lại (`pattern.A1`).

**Chưa verify được ở đây**: cân đối thị giác của 220px trên chiều rộng cửa sổ thật, và scrollbar 4px trông thế nào khi macOS bật "always show scrollbars". Cần liếc mắt trên Mac.

## B13 — Thêm Cursor vào IDE list của popup OPEN

Note `task-1787704363138`.

**Done 2026-09-09**: Cursor có ở cả nhánh Local và Remote SSH, cạnh VSCode / VSCode Insiders / Antigravity, và tự mờ đi khi chưa cài (`system.rs` thêm probe `/Applications/Cursor.app` vào `IdeAvailability` — đây là site thứ ba, dễ sót). Nhân tiện gộp chuỗi `if/else` dựng URI remote thành một bảng tra scheme: đủ 3 lần lặp cùng hình dạng nên vượt ngưỡng Rule of Three (`pattern.A2`); VSCode và Insiders sinh ra chuỗi URI y hệt trước.

Hai khối template (Local, Remote) **cố ý giữ lặp**, không gộp `v-for`: mỗi mục có markup hơi khác (icon ảnh vs icon font, class màu), và không có cách kiểm tra thị giác ở máy này để chắc gộp xong ba IDE cũ vẫn render y nguyên. Với 4 IDE × 2 hướng = 8 khối gần giống nhau, đây là ứng viên hợp lệ để chuyển sang v-for theo config — nhưng cần một lượt nhìn trên Mac, không làm mù.

Cursor chưa có icon riêng trong `public/` nên dùng `fa-code` như mục "Terminal", thay vì trỏ vào một ảnh 404.

**Chưa verify được ở đây**: máy Mac có cài Cursor không, và handler `cursor://` có mở đúng không. Thử: `open -a Cursor .`, rồi bấm mục "Cursor (Remote SSH)" một lần.

## B14 — Default excludes: thêm `__pycache__/`

Note `task-1788551073576` ("exclude cả 2 phía: pycache, wrangler, claude").

**Done 2026-09-09**: kiểm tra thì `.wrangler/` và `.claude/` **đã có sẵn** ở cả hai chiều từ trước; thiếu duy nhất `__pycache__/`. Thêm vào cả `pull_excludes` và `push_excludes` (`useProjectConfig.js`). Không có bản sao thứ hai của danh sách default này trong repo (default phía Rust ở `projects.rs:188-189` là vector rỗng — default khác, không phải bản sao).

**Migration cho project cũ — done 2026-09-09 (owner yêu cầu bổ sung).** `migrateAddPycacheExcludes` trong cùng file, copy đúng hình dạng `migrateStripNotesExcludes` đã có sẵn: chạy một lần trong `loadData`, dùng `ensureEntry` nên chỉ *thêm* vào cuối, không xoá / không đổi thứ tự / không dedupe entry nào khác; project đã có sẵn entry thì không bị ghi lại chút nào (`ensureEntry` trả về đúng reference cũ). Ghi log `appendGlobalLog("MIGRATE", ...)` như hai migration anh em. Chuỗi `'__pycache__/'` rút thành hằng `PYCACHE_EXCLUDE_ENTRY` dùng chung cho cả default lẫn migration (`pattern.A1`) — chỉ rút đúng chuỗi bị lặp, không hoisting cả mảng.

**Đánh đổi đã biết, chấp nhận có ý thức**: migration idempotent theo kiểu "thêm lại nếu thiếu", không có marker "đã migrate" (app không có hạ tầng đó). Nên nếu owner cố tình xoá `__pycache__/` khỏi một project thì lần load sau nó quay lại. Đây đúng là hành vi mà `migrateStripNotesExcludes` đã có từ trước, không phải lệ mới. Muốn xoá được vĩnh viễn thì cần thêm marker — việc riêng, chưa làm.

## B15 — Tàn dư: "ô host" thừa trong SSH config

Note `task-1787563509967`, vế thứ nhất: "trong ssh config vẫn còn ô host -> xóa, dọn".

**Done 2026-09-10. Kết luận 2026-09-09 là sai, và sai ở một chỗ đọc sót.** Hôm đó tôi kết luận không xoá được vì "xoá ô thì `selectedSshHost` không còn cách nào đặt, các slot rơi về `''` và mất theo dõi remote", rồi đẩy sang owner ba lựa chọn. Sai: `sshStore.js:7` là `get: () => _storedHost.value || sshHosts.value[0] || ''`. Xoá cái ô là xoá **người ghi**, không xoá **giá trị** — `_storedHost` vẫn nạp từ localStorage lúc boot nên ai từng chọn host thì giữ nguyên, ai chưa từng đụng thì rơi về `sshHosts[0]`, đúng bằng thứ cái select vẫn hiển thị sẵn cho họ. Hành vi không đổi cho bất kỳ ai, và cái "backfill" ở phương án (b) là chi phí không tồn tại — getter đã backfill ngay lúc đọc.

Watcher migrate legacy flag (`usageMonitorStore.js:59-73`) cũng không vướng: nó chờ host được resolve **bất đồng bộ** sau khi `sshHosts` nạp xong, không chờ người dùng bấm gì.

Thêm một bằng chứng nó là tàn dư thật, tìm ra khi đọc lại: `SshConfigModal.vue:11` bind thẳng `v-model="selectedSshHost"`, **không** đi qua `setSelectedSshHost` — mà đó mới là hàm bọc `action()` để mirror trạng thái sang companion. Nghĩa là ô đó bấm trên điện thoại thì host không đổi bên máy chủ; nó hỏng sẵn mà không ai nhận ra, vì mọi slot remote đã có dropdown host riêng ngay trong slot (`AgentUsageSlot.vue:34-43`, bind vào host *đã resolve* nên fallback hiện ra rõ và chỉ cách một cú bấm để thành giá trị tường minh).

Đã làm: xoá cả hàng `active-host-row` khỏi `SshConfigModal.vue` (kèm 4 rule CSS chỉ hàng đó dùng — `host-select-mini` ở chỗ khác là class khác, không đụng), `selectedSshHost` thành computed chỉ-đọc, `setSelectedSshHost` chết theo nên xoá luôn khỏi `sshStore.js` và `useSsh.js`. Giữ `_storedHost` + lần đọc localStorage: đó chính là thứ bảo toàn lựa chọn cũ của owner.

Bài học ghi lại vì nó là lỗi quy trình, không phải lỗi kiến thức: câu hỏi "xoá control này thì giá trị đi đâu" được trả lời trọn vẹn bởi 1 dòng getter. Tôi đã dừng ở "biến này còn được đọc" mà không đọc tiếp *nó được đọc ra cái gì* — rồi đóng gói phần chưa đọc thành ba lựa chọn cho owner. Đúng thứ `agent.A3` gọi là câu hỏi không qua nổi kill-test: nó không đổi artifact, vì mọi phương án đều dẫn về cùng một hành vi.

## B16 — CRITICAL: reload làm sai toàn bộ trạng thái terminal

Note `task-1787277872312`.

**Done 2026-09-09.** Owner nói đúng: biểu hiện sai vì gốc sai, không phải lệch UI. Reload webview xoá sạch state JS trong khi PTY bên Rust vẫn sống — mà Rust **chưa bao giờ lưu** tab thuộc project nào (`PtyTabInfo` chỉ có `{ id, alive }`, và comment `pty.rs:38` còn ghi rõ backend cố tình "scope-blind"). Nên `adoptTabs` không có gì để dựng lại và hardcode `projectId: null` cho mọi tab — badge về 0, pin biến mất, tên rename mất. Cả ba triệu chứng quy về đúng một dòng.

Sửa: thêm `TabMeta { project_id, title, pinned }` trong `PtyState`, upsert theo kiểu PATCH (`None` nghĩa là "không nói gì", không phải "xoá"), `pty_list_tabs` trả về, `adoptTabs` rehydrate. Tab thật sự không có chủ vẫn về global — giờ là ca suy biến chứ không còn là số phận của mọi tab. Hai phương án bị loại (chặn context menu; lưu ở `localStorage`) và lý do: `docs/research/terminal-tab-ownership-reload-loss.md`.

Bắt thêm khi review: `useTerminalTabs.js:374` ép scope về GLOBAL lúc boot với comment "setActiveTab would derive the same" — đúng khi mọi tab đều global, **sai** sau khi sửa vì `setActiveTab` tự suy scope từ tab. Comment đã sửa lại cho khớp sự thật; hành vi giữ nguyên (boot vào scope của tab đầu tiên).

**Chưa verify được ở đây**: Rust không compile trên máy này. Cần `cargo build` trên Mac, rồi thử đúng kịch bản gốc — mở tab cho một project, rename, ghim, reload webview, xem tab có về đúng project với đúng tên và pin không.

## B17 — Project Icon: nút reload, và cache icon chưa từng được quét lại

Note `task-1788494882695` ("project config: icon: add minimal section \"Project Icon\"", detail chỉ có `-`).

**Done 2026-09-10.** Owner giao thêm bằng lời: thêm nút reload project icon trong modal project setting, helptext ghi hết các pattern app dùng để nhận diện icon.

Đào vào thì lộ ra việc lớn hơn cái nút. `load_and_cache_project_icons` (`system.rs:543`) chỉ được gọi từ `load_projects_blocking` (`projects.rs:144`), mà `load_projects` chỉ được gọi từ `loadData`, mà `loadData` chỉ được gọi **một lần** từ `App.vue:79`. Còn `refreshProjectIcons()` (`projectStore.js:71`) — cái đang chạy ngay sau khi lưu config project (`remoteActions.js:248`) và trông y như một lần quét lại — chỉ `invoke('get_project_icons_map')`, mà lệnh đó **đọc cache**, không dựng lại cache. Nên trước bản này, thêm favicon cho một project xong thì không có đường nào để app thấy ngoài restart app. Đúng thứ note `task-1787801054655` gọi là tối kỵ. `pattern.A7`: hàm được đặt tên theo việc người ta mong nó làm, và cái tên đó giấu lỗi suốt nhiều tháng.

Đã làm: lệnh `reload_project_icons` (`projects.rs:156`) quét lại thật, `async` + `spawn_blocking` bắt buộc vì thân nó stat tới 7 đường dẫn/project và đọc tới 250 KB mỗi file, trên mount ngoài có thể treo (`tauri.A1`, và chính doc comment của `load_projects_blocking` đã cảnh báo). Dùng lại `load_projects_blocking` chứ không chép lại vòng quét (`pattern.A1`). Đăng ký trong `generate_handler!` và thêm vào `COMPANION_ALLOWED_COMMANDS` — thiếu một trong hai là **silent no-op**.

Section "PROJECT ICON" trong `ProjectConfigModal.vue` copy nguyên hình dạng của group "EXCLUDE PRESETS" ngay trên nó (cùng `config-group`, cùng `group-title`, cùng đoạn hint in nghiêng), không đẻ style mới.

**Helptext viết theo code chứ không theo kỳ vọng.** Owner nói "thứ tự ưu tiên", nhưng code **không** dừng ở candidate khớp đầu tiên: nó gom hết candidate tồn tại rồi chọn **file nhỏ nhất** (`system.rs:588`), và nếu file nhỏ nhất đó lớn hơn 250 KB thì **không hiện icon nào** chứ không rơi xuống candidate kế. Giữ nguyên hành vi (với icon 16px thì chọn nhỏ nhất là đúng) và viết helptext đúng sự thật. Muốn đổi sang ưu tiên-theo-thứ-tự thật thì đó là đổi hành vi, cần owner nói.

**Bắt khi review:** thợ để `refreshProjectIcons()` (giờ có quét lại) nguyên ở đường boot `App.vue:83`, chạy song song với `loadData` — thành ra cold start quét đĩa **hai lần đồng thời**, trên đúng thao tác mà comment trong code cảnh báo là có thể treo, và hai luồng cùng `clear()` rồi insert vào một cache. Tách theo vai trò thay vì thêm cờ boolean (`pattern.A4`): `loadProjectIconsMap()` chỉ đọc cache (đường boot, vì `load_projects` vừa dựng xong), `refreshProjectIcons()` quét lại rồi đọc (sau khi lưu config, và nút RELOAD).

**Chưa verify được ở đây**: Rust không compile trên máy này. Cần `cargo build` trên Mac, rồi thử: thêm `public/favicon.ico` vào một project đang mở, bấm RELOAD trong modal config của nó, xem icon hiện ra mà không cần restart.

## B18 — Audit: state nạp một lần lúc startup

Note `task-1787801054655`. Báo cáo đầy đủ: `docs/research/startup-cached-state-audit.md`.

Kết quả gọn: app **không có filesystem watcher nào**. `loadData` chạy đúng một lần. Nghiêm trọng nhất là cache icon — đã sửa ở B17. Còn lại, xếp theo giá trị trên chi phí, **chưa làm**:

1. **SSH host** — chỉ nạp lại khi *chính app* save/undo/redo file SSH (`applySshHostsChange`). Sửa `~/.ssh/config` bằng editor ngoài thì app không biết. Cách rẻ nhất: gọi lại `get_ssh_hosts` ngay lúc mở modal SSH Config.
2. **Danh sách project** — `projects.json` không bao giờ được đọc lại; nút refresh toàn cục ở header chỉ làm mới *trạng thái* từng project, không đọc lại danh sách. Muốn cho nó gọi `load_projects` thì phải kiểm trước ba migration trong `loadData` có idempotent thật không.
3. **Cờ `SEEN` của antigravity** (`agent_usage/antigravity.rs:11`) — "công cụ không có trên host" giữ suốt vòng đời app; cài công cụ lên host giữa phiên thì app vẫn báo thiếu tới lúc restart. Nên hết hạn theo thời gian như `ideAvailability` đã làm (TTL 60s).

Quyết định đã chốt trong doc: **không thêm filesystem watcher.** Mọi mục lệch đều có một thời điểm sử dụng rõ ràng để đọc lại; đọc-lúc-dùng rẻ hơn theo-dõi-liên-tục và không đẻ thêm trạng thái phải giữ đúng.


---

## Ghi chú

Mọi mục ở đây chưa được lên lịch vào version cụ thể; đây là backlog, không phải cam kết cho 1.26.1.
