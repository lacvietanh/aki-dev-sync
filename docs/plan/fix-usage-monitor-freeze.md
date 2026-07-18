# Fix: Usage monitor freeze — reset time đứng im cho tới khi reload webview

**Status: IMPLEMENTED (2026-07-18), chờ VERIFY trên Mac.** P1–P5 đã code xong trên máy remote viết code
(build Rust/Tauri không chạy được ở đây — rule machine-local). Chi tiết implementation + rationale từng
điểm: `docs/arch/usage-claudecode.md` §3d. Còn lại đúng nguyên `## 4. Verify plan` bên dưới — chưa có
session nào chạy verify thực địa (V-P1..V-P4 + audit) trên build thật. Đổi tên hàm lưu ý khi đọc lại các
mục bên dưới: `run_remote_script_timeout` → `run_interpreter_timeout` (tổng quát hoá cho cả CC/AG, P2).

| Mục | File chính đã sửa | Trạng thái |
|---|---|---|
| P1 (wake self-heal) | `src/composables/useAgentUsage.js` | Code xong, chưa verify sleep/wake thật |
| P2 (AG timeout) | `src-tauri/src/agent_usage.rs` (`Interpreter` enum, `run_interpreter_timeout`) | Code xong, chưa verify blackhole SSH |
| P3 (hook v3 merge) | `scripts/provision-claudecode.sh` | Code xong, chưa verify upgrade path trên host có hook v2 |
| P4 (CC boundary trigger) | `src/components/AgentUsage.vue` | Code xong, chưa verify |
| P5 (spawn_blocking) | `src-tauri/src/agent_usage.rs` (`provision_agent_usage`/`force_sync_agent_usage`) | Code xong |

Không nằm trong plan gốc, làm cùng đợt theo yêu cầu người dùng: nút "New Project" chuyển từ header xuống
cạnh "PROJECTS (n)" trong `ProjectTable.vue` — xem CHANGELOG 1.12.0, không liên quan usage-freeze.

## 1. Triệu chứng (user report, 1.11.0)

- Reset time của Claude Code (dòng "Reset 3h20m (23:30 Jul18)") bị **treo/đứng im**, không cập nhật.
- Phải right-click → Reload (reload webview) thì mới hiện đúng thời gian reset quota.
- User không chắc Antigravity (AG) có bị tương tự không.
- **Bổ sung (report đợt 2, cùng ngày)**: bình thường **vẫn thấy đếm/cập nhật** — freeze là **không ổn định
  (intermittent)**, không phải đứng im 100%. User nghi ngờ: xuất hiện **sau các phase máy tự sleep rồi
  dậy**, và đặt câu hỏi liệu có liên quan tới **pin/unpin workspace** (feature 1.11.0). → Đã audit lại
  cả hai hướng, xem §2.2 (đã sửa), §2.6 (audit pin) và §2.7 (defect mới P5).

## 2. Kết quả điều tra — facts đã verify (đọc code trực tiếp + runtime evidence)

### 2.1 Chuỗi cập nhật reset time của CC (đã trace từng lớp)

| Lớp | Cơ chế | Vị trí |
|---|---|---|
| Chữ đếm ngược | `computed` từ `ccNow`, tick bằng `setInterval` 60s | `src/components/AgentUsage.vue:450-456,512,517` |
| Dữ liệu `resets_at` | poll `checkUsage()` bằng `setInterval` mỗi `usage_interval_s` (default 30s) | `src/composables/useAgentUsage.js:572-583` |
| Tự phục hồi 1 | OAuth poll: cache già >120s hoặc resets_at đã qua → gọi `api.anthropic.com/api/oauth/usage` ghi cache | `scripts/get-claudecode-usage.sh:23-186` |
| Tự phục hồi 2 | `NOW > resets_at` → emit `\|\|\|STALE_RESET\|\|\|` → Rust trả null → JS `forceSync()` (`claude -p /usage`, retry ≤3) | `scripts/get-claudecode-usage.sh:215-229`, `useAgentUsage.js:426-429` |
| Tự phục hồi 3 | statusline hook (aki-rlcache v2) ghi cache mỗi turn khi có session CC đang chạy | `scripts/provision-claudecode.sh:23-32` |

Điểm mấu chốt: **cả 3 lớp tự phục hồi đều chỉ chạy khi poll timer thực sự tick.** Toàn bộ hệ thống sống
bằng `setInterval` thuần trong webview.

### 2.2 Root cause (P1): không có bất kỳ resume/wake handling nào

- Grep toàn `src/`: **0 kết quả** cho `visibilitychange`, window `focus` listener, hay watchdog phát hiện
  timer gap. (Đã verify 2026-07-18.)
- Trên macOS, WKWebView **suspend/throttle `setInterval`** khi cửa sổ bị che khuất hoàn toàn (occluded),
  bị minimize, hoặc máy sleep (App Nap). Khi đó: poll dừng → `resets_at` không được làm mới, cả 3 lớp
  tự phục hồi bất hoạt; `ccNow` cũng dừng → chữ đếm ngược đứng im. Đúng nguyên văn triệu chứng.
- **Triệu chứng intermittent (report đợt 2) khớp cơ chế này**: sau khi resume (wake từ sleep, hết
  occlusion), WKWebView **có thể** tự chạy lại timer — khi đó user thấy "vẫn đếm bình thường"; nhưng
  thời điểm resume timer là **nondeterministic** (đôi khi chỉ resume sau một input event vào webview,
  hoặc bị coalesce/trễ dài). Nghĩa là: cùng một root cause cho cả hai quan sát — lúc thì tự hồi, lúc
  thì đứng im tới khi user tương tác/reload. Right-click → Reload luôn fix được vì nó vừa là input
  event vừa tạo lại composable. **Sleep → wake của máy đúng là trigger chính** như user nghi ngờ.
- Right-click → Reload fix được vì webview reload tạo lại toàn bộ composable; `watch(hostRef,
  {immediate:true})` (`useAgentUsage.js:585-616`) gọi `checkUsage()` ngay lập tức.
- **Không phải regression code của 1.11.0**: `useAgentUsage.js`, `AgentUsage.vue` (phần timer/computed),
  `agent_usage.rs`, các script — tất cả không đổi giữa 1.10.1 → 1.11.0 (đã diff `3e52a99..1c1f8a0`).
- **ĐÍNH CHÍNH giả thuyết "why now" (bản đầu của plan này viết SAI chiều)**: bản đầu nói pin khiến cửa
  sổ "thường nằm sau cửa sổ khác (occluded)". Sai — pin bật **cả hai** `setAlwaysOnTop(true)` +
  `setVisibleOnAllWorkspaces(true)` (`useAppWindow.js:23-26`), tức cửa sổ pinned **luôn nổi trên cùng**,
  gần như không bao giờ bị che khuất khi màn hình đang bật. Giải thích "why now" đúng hơn:
  1. Pinned → cửa sổ luôn trong tầm mắt → user **nhìn thấy** trạng thái treo thường xuyên hơn hẳn
     (trước 1.11.0 app nằm ở Space riêng, ít khi được nhìn ngay sau khi wake).
  2. Sleep/lock-screen vẫn suspend timer bất kể pin (suspend toàn hệ thống / lock screen che toàn bộ).
  3. Kẽ hở cần verify trên Mac: `setVisibleOnAllWorkspaces` của tao/Tauri set
     `NSWindowCollectionBehaviorCanJoinAllSpaces` nhưng (theo hiểu biết hiện tại) **không** set
     `.fullScreenAuxiliary` — khi user ở một Space **fullscreen app**, cửa sổ pinned có thể không
     hiện ở Space đó → occluded → suspend, dù đang "pin". Nếu user hay làm việc fullscreen, đây là
     nguồn suspend lặp lại ngay cả khi máy không sleep. (Chỉ là giả thuyết phụ — P1 chữa được bất kể
     trigger nào, không cần chốt nhánh này trước khi thực hiện.)

### 2.3 Defect P2: AG IPC không có timeout → có thể treo vĩnh viễn (CC thì không)

- CC: `run_remote_script_timeout` có hard timeout 30s + kill + orphan cleanup
  (`src-tauri/src/agent_usage.rs:16,65-147`) → mọi `checkUsage` của CC luôn resolve.
- AG: `get_antigravity_usage` spawn `ssh host node` / `zsh -lc node` rồi `wait_with_output()` **không có
  timeout** (`agent_usage.rs:549-582`). Nếu SSH treo (mạng đứt không keepalive, TCP blackhole), promise
  không bao giờ resolve → guard `isChecking` kẹt `true` vĩnh viễn (`useAgentUsage.js:261-268`) → mọi poll
  tick sau chỉ set `pendingRecheck` rồi return → AG đóng băng cho tới khi reload webview.
- Trả lời câu hỏi của user: **AG có bị ảnh hưởng, ở kịch bản mạng treo còn nặng hơn CC** (CC tự thoát sau
  30s, AG thì không). Không vi phạm rule never-block-UI (đã bọc `spawn_blocking` đúng — UI không freeze),
  nhưng data-flow freeze vĩnh viễn là bug thật.

### 2.4 Defect P3 (runtime-confirmed): statusline hook v2 xoá mất `seven_day` khỏi cache

- Evidence trực tiếp trên host đang được monitor (2026-07-18): `~/.claude/rate-limits-cache.json` mtime
  cách 2s (statusline đang ghi mỗi turn) nhưng `rate_limits` **chỉ có `five_hour`, không có `seven_day`**.
- Nguyên nhân: hook v2 (`provision-claudecode.sh:23-32`) chỉ merge dữ liệu cũ khi turn **hoàn toàn không
  có** `rate_limits`; khi turn CÓ `rate_limits` (chỉ chứa `five_hour` trên bản CC hiện tại), nó
  `printf '%s' "$input" >` ghi đè cả file → `seven_day` mà OAuth poll (P3) vừa ghi bị clobber ngay turn
  kế tiếp.
- Hệ quả UI: thanh 7-Day biến mất (bar này `v-if` trên `seven_day.used_percentage != null`,
  `AgentUsage.vue:167`) hoặc không bao giờ cập nhật trong lúc có session chạy.

### 2.5 Defect P4 (asymmetry): CC không có boundary-trigger tự fetch như AG

- AG: `UsageCircle.vue:103-113` có timer 10s riêng, phát hiện `now > resetsAt` → emit `@timeout` →
  `retry` → `checkUsage()` ngay tại client.
- CC: `ccClockTimer` chỉ tick lại chữ hiển thị, không kích hoạt fetch. CC dựa hoàn toàn vào STALE_RESET
  phía server-script — vốn cũng chết theo poll timer khi webview suspend.

### 2.6 Audit flow pin/unpin (đợt 2, 2026-07-18) — KHÔNG phải nguyên nhân trực tiếp

Trả lời nghi vấn của user "có liên quan pin/unpin không": đã đọc toàn bộ implementation —
`useAppWindow.js` (46 dòng, toàn bộ feature) + điểm gọi `restorePin()` tại `AppHeader.vue:174`
(onMounted, fire-and-forget). Kết luận:

- `togglePin`/`restorePin` chỉ gọi 2 window API (`setAlwaysOnTop`, `setVisibleOnAllWorkspaces`) +
  ghi localStorage. **Không đụng** timer, poll, composable usage, hay IPC nào của flow usage; không
  await, không throw được vào đường đi của poll. → Pin/unpin **không thể trực tiếp** làm treo cập nhật.
- Ảnh hưởng của pin là **gián tiếp qua ngữ nghĩa hiển thị cửa sổ** (xem đính chính ở §2.2): pinned
  đổi tập trigger suspend của WKWebView (bớt occlusion thường, nhưng thêm nghi vấn fullscreen-Space),
  và làm user quan sát app sát hơn nên thấy freeze nhiều hơn. Không cần sửa gì trong `useAppWindow.js`.

### 2.7 Defect P5 (phát hiện đợt 2): `provision`/`force_sync` block trực tiếp async runtime

- `provision_agent_usage` (`agent_usage.rs:200`) và `force_sync_agent_usage` (`agent_usage.rs:239`)
  là `async fn` nhưng gọi thẳng `run_remote_script` — hàm blocking (spawn + poll-loop
  `thread::sleep`, tới 30s) — **không qua `spawn_blocking`**. Vi phạm đúng rule NEVER BLOCK THE UI
  trong CLAUDE.md (rule yêu cầu mọi command chạy subprocess phải wrap `spawn_blocking`;
  `get_agent_usage`/`logout_antigravity` là mẫu đúng, hai hàm này bị sót).
- Mức độ: không freeze repaint webview (không chiếm main thread), nhưng chiếm **1 tokio worker tới
  30s mỗi call**. Kịch bản xấu nhất khớp report "không ổn định sau wake": sau sleep dậy, STALE_RESET
  → forceSync bắn ra trên nhiều host cùng lúc trong khi SSH còn chết → nhiều worker bị giam 30s →
  các IPC async khác (kể cả `log_frontend`, `check_for_updates`) bị trễ dây chuyền.
- Fix: wrap thân hai hàm vào `tauri::async_runtime::spawn_blocking` y hệt pattern của
  `get_agent_usage` (`agent_usage.rs:346`). Nhỏ, độc lập, làm cùng đợt P2 (cùng file).

### 2.8 Ngữ cảnh liên quan phải đọc trước khi thực hiện

- `docs/plan/done/claudecode-oauth-usage-p3.md` — OAuth poll **no-op trên Mac** (credentials chỉ còn trong
  keychain, không có `.credentials.json`) → trên Mac local, lớp tự phục hồi 1 vốn đã chết; trên Linux
  remote (có file credentials) thì hoạt động. Điều này làm P1/P4 càng quan trọng với ccLocal.
- `docs/research/claude-headless-rate-limit-event-2026-07-09.md` — headless `-p` KHÔNG fire statusline
  hook; `--output-format json` trả `rate_limit_info.resetsAt` trực tiếp.
- `docs/arch/usage-claudecode.md`, `docs/arch/usage-antigravity.md` — kiến trúc nền của cả hai flow.
- `CLAUDE.md` (project): rule NEVER BLOCK THE UI, Extreme Narrow UI, Regression Guard multi-entity.

## 3. Phương án giải quyết

### P1 — Resume/wake self-heal (root cause, ưu tiên cao nhất)

**Phương án đề xuất (kết hợp 2 tầng, đều nằm trong `useAgentUsage.js`):**

1. **Listener đánh thức**: đăng ký `document.addEventListener('visibilitychange')` + `window.addEventListener('focus')`
   (một lần ở module scope, các instance subscribe chung) → khi visible/focus: gọi `checkUsage()` ngay
   + `restartPollTimer()`. Guard `isChecking`/`pendingRecheck` sẵn có đã chống double-fetch.
2. **Watchdog gap-detection**: lưu `lastTickAt = Date.now()` tại mỗi poll tick; một heartbeat interval nhẹ
   (5–10s) kiểm tra `Date.now() - lastTickAt > 2 × usage_interval_s × 1000` → coi như timer vừa bị
   suspend (sleep/occlusion dài) → `checkUsage()` + `restartPollTimer()`. Tầng này bắt được cả trường hợp
   WKWebView resume mà không fire visibilitychange (occlusion không đổi `document.visibilityState`).

Lý do cần cả hai: focus/visibility cho phản hồi tức thì đúng lúc user nhìn vào app; watchdog là lưới an
toàn cho mọi kịch bản suspend khác. Cả hai đều là JS thuần, không cần capability Tauri mới, không thêm
DOM element (tuân Extreme Narrow).

**Phương án thay thế đã cân nhắc và loại:**
- Dùng Tauri window event (`onFocusChanged`) từ Rust: cần thêm capability + IPC, phức tạp hơn mà không
  hơn gì listener JS. Loại theo MVP-first.
- Chuyển poll về phía Rust (tokio interval, đẩy event xuống JS): đúng về lý thuyết (timer Rust không bị
  WKWebView suspend) nhưng là refactor lớn đảo ngược ownership của toàn bộ flow — không tương xứng với
  mức độ bug. Ghi nhận là hướng dài hạn, không làm trong plan này.

### P2 — Timeout cho AG IPC

- Đưa `get_antigravity_usage` về cùng funnel timeout như CC: tổng quát hoá
  `run_remote_script_timeout(host, script, timeout)` thành nhận cả interpreter (hiện hardcode `sh`;
  AG cần `node`), hoặc thêm hàm anh em `run_remote_node_timeout` dùng chung phần poll/kill/drain.
  Ưu tiên tổng quát hoá một funnel duy nhất (SSoT, đúng tinh thần rule PATH-race preamble: một funnel,
  không patch từng call site).
- Timeout đề xuất: 30s, dùng chung `REMOTE_SCRIPT_TIMEOUT_SECS`.
- Lưu ý khác biệt cần giữ nguyên hành vi: AG local hiện chạy `zsh -lc node` (cần login shell để resolve
  node qua nvm — đây chính là PATH-race candidate, xem rule stack-tauri). Khi gộp funnel, giữ semantics
  resolve node (có thể thêm preamble resolve tương tự `CLAUDE_BIN` cho `node` nếu gặp lỗi 127; không bắt
  buộc trong plan này).
- Phía JS không cần đổi: timeout → IPC reject → nhánh `catch` sẵn có set `error.value`, `finally` thả
  `isChecking` (`useAgentUsage.js:436-447`).

### P3 — Statusline hook v3: merge thay vì overwrite

- Nâng hook lên marker `# aki-rlcache v3`: khi turn CÓ `rate_limits`, **deep-merge** vào rate_limits cũ
  của cache thay vì ghi đè cả object — cụ thể: key nào payload mới có thì lấy mới, key nào thiếu
  (`seven_day`) thì giữ từ cache cũ.
- `provision-claudecode.sh` phải xử lý upgrade: hiện `grep -q "aki-rlcache v2"` → skip; đổi thành detect
  v3; nếu thấy v1/v2 → xoá block cũ (sed range hiện tại `/^rl_input=/,/printf .*rate-limits-cache\.json/d`
  đã cover cả v2 vì cùng shape — verify lại trước khi dùng) rồi inject v3.
- Re-provision tự lan: `useAgentUsage.js:378` đã re-provision mỗi host một lần mỗi session
  (fire-and-forget) — hook mới sẽ tự tới các host hiện có, không cần bước thủ công.
- Giữ nguyên tính chất ghi non-atomic hiện tại hay chuyển atomic (`mv` temp)? OAuth poll đã ghi atomic
  (`os.replace`) và comment ghi rõ "never races statusLine hook's non-atomic write" — khi sửa hook, nên
  chuyển hook sang ghi atomic luôn (temp + mv) để đóng nốt race đọc-nửa-file. Chi phí ~2 dòng sh.

### P4 — Boundary-trigger cho CC (nhỏ, làm cùng P1)

- Trong `AgentUsage.vue`, tick `ccClockTimer` sẵn có: phát hiện `ccNow` vượt `cc5hResetsAt` (pattern
  `wasPast/nowPast` y hệt `UsageCircle.vue:104-112`) → `$emit('retry')`. Wiring `@retry` → `refresh` đã
  có sẵn ở `AgentUsageSlot.vue:72`.
- Không thêm DOM element nào (Extreme Narrow). Sau P1, tick này sống lại ngay khi webview resume nên
  boundary-trigger có tác dụng thực.

### P5 — spawn_blocking cho provision/force_sync (nhỏ, làm cùng P2)

- Wrap thân `provision_agent_usage` và `force_sync_agent_usage` vào
  `tauri::async_runtime::spawn_blocking(...).await` theo đúng pattern `get_agent_usage`
  (`agent_usage.rs:346`). Không đổi chữ ký IPC, không đổi hành vi phía JS.
- Lý do gộp với P2: cùng file, cùng bài audit `#[tauri::command]` cuối task.

### Thứ tự thực hiện đề xuất

1. P1 (root cause, chặn đứng triệu chứng user thấy)
2. P2 (deadlock thật, sửa độc lập, phía Rust) + P5 (cùng file, cùng bản chất blocking)
3. P3 (data-loss 7-Day, phía script — độc lập hoàn toàn với P1/P2)
4. P4 (5 phút, đi kèm P1)

Mỗi P một commit riêng để verify/bisect độc lập. KHÔNG commit từ máy remote (rule machine-local); để
working tree cho user tự quyết.

## 4. Verify plan — cách chứng minh fix đúng và hiệu quả

Môi trường verify: phải chạy trên Mac (app build tại Mac — rule machine-local: không cargo build trên
remote). Log runtime: `~/Library/Application Support/aki.devsync/usage.log`; bật `--debug` để thấy
`[USAGE:*] poll tick / check start` (xem `docs/arch/logger.md`).

### V-P1 (freeze self-heal)
1. Mở app, để poll chạy bình thường ≥2 phút (log có `poll tick` đều nhịp ~30s).
2. Che khuất hoàn toàn / minimize cửa sổ ≥10 phút (hoặc sleep máy 10 phút). Kỳ vọng log: `poll tick`
   ngừng xuất hiện (xác nhận WKWebView suspend thật — đây cũng là bằng chứng chốt root cause).
3. Focus lại cửa sổ. **Pass** khi: trong ≤2s có `check start` mới trong log và reset time trên UI đổi
   sang giá trị đúng, KHÔNG cần reload webview. **Fail** nếu phải đợi >1 poll interval hoặc phải reload.
4. Regression: để app visible liên tục 10 phút — không có fetch dư thừa (watchdog không được tự bắn khi
   timer vẫn tick đều; đếm `check start` ≈ 10min/interval ±1).
5. **Kịch bản sleep thật (report đợt 2)**: pin cửa sổ, để máy tự sleep ≥15 phút, wake bằng bàn phím
   nhưng **không click vào app** — quan sát trong ≤10s reset time tự nhảy đúng (chứng minh self-heal
   không cần input event vào webview; đây chính là kẽ hở nondeterministic-resume mô tả ở §2.2).
   Lặp 3–5 chu kỳ sleep/wake vì triệu chứng gốc là intermittent — một lần pass chưa đủ kết luận.
6. **Kịch bản pinned + fullscreen Space (nghi vấn §2.2 mục 3)**: pin cửa sổ, đưa một app khác vào
   fullscreen và ở lại đó ≥10 phút. Ghi nhận (a) cửa sổ pinned có hiện trên Space fullscreen không —
   chốt luôn nghi vấn fullScreenAuxiliary; (b) khi quay về Space thường, usage tự cập nhật ≤10s.

### V-P2 (AG timeout)
1. Cấu hình AG source trỏ tới một SSH host blackhole (vd host tồn tại trong `~/.ssh/config` nhưng route
   drop — có thể mô phỏng bằng `iptables`/pf drop, hoặc IP không phản hồi).
2. **Pass** khi: sau ~30s card AG hiện error (hoặc cached-fallback theo nhánh null sẵn có), và bấm nút
   reload trên card lại chạy được bình thường (chứng minh `isChecking` được thả). **Fail** nếu card kẹt
   loading vô hạn và nút reload không có tác dụng cho tới khi reload webview.
3. Regression AG happy-path: host local bình thường vẫn poll ra data như trước.

### V-P3 (seven_day sống sót qua statusline)
1. Trên host có credentials file (Linux remote này): xoá `seven_day` test — chạy một turn CC bất kỳ →
   xác nhận cache được statusline ghi. Sau đó đợi OAuth poll ghi `seven_day` (cache già >120s, xem log
   `oauth: ok`), rồi chạy tiếp ≥2 turn CC.
2. **Pass** khi: sau các turn mới, `python3 -c "import json;print(json.load(open('$HOME/.claude/rate-limits-cache.json'))['rate_limits'].keys())"`
   vẫn còn cả `five_hour` lẫn `seven_day`, và UI thanh 7-Day không biến mất giữa session. **Fail** nếu
   `seven_day` biến mất sau bất kỳ turn nào.
3. Verify upgrade path: host đang có hook v2 → mở app → provision tự chạy → `grep "aki-rlcache v3"
   ~/.claude/statusline-command.sh` có kết quả và block v2 cũ đã bị xoá sạch (không còn 2 block ghi cache).

### V-P4 (boundary trigger)
1. Cách nhanh không đợi 5h thật: sửa tay `resets_at` trong cache của host test thành `now + 90s`, reload
   app để nạp giá trị, đợi qua mốc.
2. **Pass** khi: trong ≤60s sau mốc (một tick `ccClockTimer`), log có `refresh`/`check start` tự phát và
   UI thoát trạng thái "ready" sang dữ liệu mới (hoặc N/A đúng thực tế). **Fail** nếu đứng ở "ready" đến
   tận poll STALE_RESET kế tiếp mà không có trigger nào từ client.

### Audit bắt buộc trước khi đóng task (theo CLAUDE.md + RULE-stack-tauri)
- `grep -n "#\[tauri::command\]" -A2 src-tauri/src/*.rs` — mọi command mới/đổi chạy subprocess/network
  phải là `async fn` + `spawn_blocking`.
- P2 đụng funnel spawn chung → chạy lại happy-path của CẢ CC lẫn AG, local lẫn remote (4 tổ hợp), vì
  funnel là điểm chung của mọi source.
- Không sửa version/CHANGELOG trong các commit fix này trừ khi user yêu cầu release (RULE-release).

## 5. Ngoài phạm vi (ghi nhận, không làm trong plan này)

- **Khởi động chậm 5–10s (báo cáo cùng đợt)**: điều tra 2026-07-18 không tìm thấy nguyên nhân trong diff
  1.11.0 (restorePin fire-and-forget sau khi window đã hiện; setup Rust không đổi; không dependency mới).
  Nghi vấn chính còn lại: Gatekeeper verify lần đầu sau khi cài bản mới. Cần user test trên Mac: mở app
  2 lần liên tiếp — nếu lần 2 nhanh thì đóng; nếu vẫn chậm → mở investigation riêng, bắt đầu bằng
  `log show --predicate 'process == "Aki Dev Sync"' --last 5m` để xem độ trễ nằm trước hay sau khi
  process chạy.
- Chuyển toàn bộ polling ownership sang Rust (miễn nhiễm webview suspend) — hướng dài hạn, chỉ cân nhắc
  nếu P1 vẫn còn kẽ hở sau verify.
- OAuth poll no-op trên Mac (keychain) — đã đóng ở `docs/plan/done/claudecode-oauth-usage-p3.md`, không
  mở lại ở đây.
