# Total Refactor — Phân tích PROCODING theo akirule

> Phạm vi: `Aki Remote Dev Sync` (Tauri v2 + Vue 3 + Rust). Bản phân tích kiến trúc và kế hoạch
> cải thiện, soi chiếu trực tiếp vào bộ quy tắc `~/.aki/claudedoc/` (RULE-coding,
> RULE-agent-behavior, RULE-docs, METHOD-flow-audit, METHOD-techbiz-optimizer).
>
> Tài liệu này là **bản kế hoạch**, không phải nhật ký công việc. Mọi phát hiện đều dẫn
> chiếu `file:line` tại thời điểm viết và phân loại *Verified* (đọc trực tiếp source) hay
> *Assumption* (suy luận, cần kiểm chứng khi thực thi).

---

## 1. Mục tiêu thật & nguyên tắc nền (METHOD-techbiz-optimizer)

### Goal thật (một câu)
Biến một công cụ rsync-GUI cá nhân đang ở mức MVP thành một codebase **đúng đắn, an toàn,
dễ bảo trì bởi một developer** — mà *không* rơi vào over-engineering.

### Nguyên tắc ràng buộc (từ RULE-coding)
- **Single-developer friendly, MVP-first, YAGNI.** Đây không phải SaaS đa người dùng.
- **DRY nhưng không trừu tượng hóa vô cớ.** Gộp khi giảm gánh nặng thật, không gộp để "đẹp".
- **Done means verified.** Hiện tại không có gì để verify (xem §6).

> Hệ quả định hướng: kế hoạch này **ưu tiên sửa lỗi đúng-đắn & bảo mật trước**, cấu trúc
> sau, và **chủ động từ chối** một loạt "cải tiến" nghe hay nhưng không có bằng chứng giá
> trị (liệt kê ở §9). Refactor toàn bộ ≠ viết lại tất cả.

---

## 2. Bản đồ kiến trúc hiện tại (Verified)

```
src-tauri/src/lib.rs        1077 dòng — TOÀN BỘ backend trong 1 file
  ├─ SSH config CRUD + undo/redo lịch sử (file-based)
  ├─ Git info / git status porcelain
  ├─ run_sync: engine rsync + hook pre/post (local/remote)
  ├─ agent-usage: provision + force-sync + get (Claude Code / Antigravity)
  └─ OS integration: open finder/terminal/vscode/url/remote-terminal/icon

src/composables/
  ├─ useProjects.js   362 — state + git + sync + 3 modal CRUD (đa trách nhiệm)
  ├─ useSsh.js        173 — SSH modal + undo/redo + auto-migrate host
  ├─ useLogs.js        97 — log store (global + per-project) + listener
  └─ useAgentUsage.js 102 — polling usage 30s + auto-provision

src/components/  — App.vue điều phối, ProjectTable.vue là bảng chính (nhiều inline style)
```

**Mô hình state:** các `ref()` khai báo ở **module scope** (vd `useProjects.js:20`,
`useLogs.js:4`) → singleton dùng chung như một "store nghèo". Mỗi lần gọi `useX()` lại
tạo mới toàn bộ closure functions. Đây là pattern cố ý nhưng nửa vời (xem §5.D).

---

## 3. Phương pháp đánh giá

Mỗi phát hiện được gắn nhãn quy tắc nguồn:

| Nhãn | Nguồn quy tắc |
|------|---------------|
| `[SEC]` | RULE-coding › Security |
| `[FLOW]` | METHOD-flow-audit |
| `[SRP/DRY]` | RULE-coding › Code quality |
| `[ERR]` | RULE-coding › Error handling |
| `[VERIFY]` | RULE-coding › Verification |
| `[LANG]` | RULE-coding › Language (English-only code) |
| `[DOCS]` | RULE-docs |

---

## 4. Phát hiện nhóm A — Bảo mật `[SEC]` (ưu tiên cao nhất)

### A1. Command/Script injection qua osascript — `open_remote_terminal` *(Verified)*
`lib.rs:1034-1042`. `host` và `path` được nội suy thẳng vào chuỗi AppleScript:
```rust
"tell application \"Terminal\" to do script \"ssh {} -t 'cd \\\"{}\\\"; exec bash'\""
```
`path`/`host` đến từ config project do người dùng nhập. Một giá trị chứa `"`, `'` hoặc
`$(...)` thoát khỏi chuỗi và chèn lệnh tùy ý vào AppleScript → thực thi shell. Dù đây là
công cụ cá nhân (trust boundary thấp), đây vẫn là **mẫu sai chuẩn** cần loại bỏ.
- **Hướng sửa:** truyền tham số qua `arg()` riêng biệt thay vì format chuỗi; hoặc shell-escape
  bằng cách bọc giá trị an toàn. Tránh dựng câu lệnh bằng `format!` rồi đưa cả khối vào `-e`.

### A2. Injection qua `--rsync-path` trong push — `run_sync` *(Verified)*
`lib.rs:381`. `args.push(format!("--rsync-path=mkdir -p \"{}\" && rsync", remote_dir))`.
`remote_dir` lấy từ `project.remote_path`. Ký tự `"` trong remote_path phá vỡ quoting →
nối lệnh tùy ý chạy trên remote.
- **Hướng sửa:** validate/escape `remote_path`; hoặc tạo thư mục bằng một lệnh `ssh` riêng
  có tham số tách bạch thay vì nhúng vào `--rsync-path`.

### A3. Validation đầu vào không đầy đủ — `run_sync` *(Verified)*
`lib.rs:321` chỉ chặn `local_path.contains("..")`. **Không** kiểm `remote_path`,
`remote_host`, hay các phần tử `specific_paths` (được đẩy thẳng vào rsync args tại
`lib.rs:389`). RULE-coding: *validate tại system boundary*. Boundary ở đây là toàn bộ
struct `SyncProject` đến từ JSON do người dùng sửa.
- **Hướng sửa:** một hàm `validate_project()` tập trung, gọi đầu `run_sync`; chặn `..`,
  ký tự control, và quoting nguy hiểm trên *tất cả* trường path.

### A4. CSP bị tắt — `tauri.conf.json:24` *(Verified)*
`"security": { "csp": null }`. App có nhúng `data:` base64 image và không tải nội dung
remote nên rủi ro hiện tại thấp, nhưng `null` = tắt hoàn toàn lớp phòng thủ XSS.
- **Hướng sửa:** đặt CSP tối thiểu (`default-src 'self'`, cho phép `data:` cho img). Chi phí
  thấp, đúng chuẩn Tauri.

### A5. Tác dụng phụ ẩn: tự vá môi trường remote — `provision_agent_usage` *(Verified)*
`lib.rs:603-642` dùng `sed -i` chèn block vào `~/.claude/statusline-command.sh` trên remote.
Được gọi **tự động, im lặng** ở lần poll usage đầu tiên (`useAgentUsage.js:25-29`). Đây vừa
là vấn đề bảo mật (sửa file người dùng không hỏi) vừa là vấn đề flow (§5). RULE-agent-behavior
xếp "modifying shared config" vào nhóm phải hỏi trước — nguyên tắc đó nên phản chiếu vào
hành vi runtime của app.
- **Hướng sửa:** tách "provision" thành hành động **người dùng chủ động bấm**, có thông báo
  rõ ràng những gì sẽ bị sửa; không tự chạy ngầm.

---

## 5. Phát hiện nhóm B — Tính đúng đắn & Flow `[FLOW]`

Áp dụng METHOD-flow-audit cho hai flow lõi.

### B1. BUG: toggle `dry_run` / `sync_git` không được lưu *(Verified — bug thật)*
- Struct `SyncProject` (`lib.rs:24-37`) **không có** field `dry_run` hay `sync_git`.
- Không có `#[serde(deny_unknown_fields)]` → khi JS gọi `save_projects` với object có
  `dry_run`/`sync_git`, serde **lặng lẽ bỏ qua** các field lạ.
- `save_projects` (`lib.rs:227`) ghi lại struct đã rớt field → `projects.json` không bao giờ
  chứa hai toggle này.
- Khi load lại, `useProjects.js:78-79` thấy `undefined` → **mặc định về `true`**.

→ **Hệ quả:** mỗi lần khởi động lại app, `dry_run` reset về `true` và `sync_git` về `true`.
Người dùng tắt Dry-Run cho một project, restart, nó bật lại. Trầm trọng hơn vì PULL luôn kèm
`--delete` (`lib.rs:419`) — trạng thái an toàn lại không bền vững.
- **Hướng sửa:** thêm `dry_run`/`sync_git` (kèm `#[serde(default)]`) vào struct để chúng được
  persist đúng; hoặc tách "UI runtime state" khỏi "persisted config" một cách tường minh.

### B2. `run_sync` async nhưng chặn executor *(Verified)*
`lib.rs:309` là `async fn` nhưng bên trong dùng `thread::spawn(...).join()` và `child.wait()`
— **blocking IO đồng bộ** chạy trực tiếp trên thread của async runtime Tauri. Trong lúc một
sync chạy, runtime async bị giữ.
- **Quan sát flow:** điều này lý giải (một phần) vì sao UI dùng khóa toàn cục `syncingProjectId`
  để cấm chạy song song — *enforcement nhân tạo che giấu một flow chặn ở backend*.
- **Hướng sửa:** chuyển phần blocking sang `tauri::async_runtime::spawn_blocking`, hoặc khai
  báo command là `fn` đồng bộ thường (Tauri tự đẩy sang thread pool). Khi backend không còn
  chặn, khóa toàn cục frontend trở thành lựa chọn chứ không phải sự bắt buộc.

### B3. Khóa sync im lặng — `startSync` *(Verified)*
`useProjects.js:109` `if (syncingProjectId.value !== null) return;` — nuốt ý định người dùng,
không phản hồi. Red-flag của flow-audit: "guard im lặng".
- **Hướng sửa:** hoặc cho phép queue/parallel sau khi B2 xong, hoặc tối thiểu báo Toast "đang
  có sync chạy".

### B4. Tác dụng phụ trong đường đọc — `useAgentUsage.checkUsage` *(Verified)*
`useAgentUsage.js:24-29`: hàm "kiểm tra usage" lại **kích hoạt provision** (ghi file remote)
ở lần đầu. Trộn read với mutation. Liên quan A5.
- **Hướng sửa:** tách `provision` ra ngoài đường poll định kỳ.

### B5. Deep-watch nặng để load icon — `ProjectTable.vue:132-146` *(Verified)*
`watch(projects, ..., { deep: true })` chạy lại toàn bộ vòng lặp mỗi khi *bất kỳ* field nào
của *bất kỳ* project đổi (mỗi lần cập nhật `git_status`, mỗi lần sync ghi `last_sync_time`).
Có guard `=== undefined` nên không gọi lại invoke, nhưng vẫn quét toàn mảng deep liên tục.
- **Hướng sửa:** watch theo danh sách `id` (shallow) thay vì deep toàn object; hoặc load icon
  một lần khi project được thêm.

### B6. PULL luôn `--delete` *(Verified)*
`lib.rs:419`. Mirror-delete là hành vi phá hủy mặc định; chỉ được bảo vệ bởi `dry_run` — mà
`dry_run` lại không bền vững (B1). Cặp đôi này là rủi ro mất dữ liệu thật.
- **Hướng sửa:** sau khi vá B1, cân nhắc để `--delete` thành tùy chọn tường minh per-project.

### B7. Global log & Project log không lưu lịch sử *(Verified)*
Hiện tại `useLogs.js` chỉ lưu log trên RAM (in-memory). Khi restart app, toàn bộ lịch sử chạy script hay sync đều bị mất. Điều này gây khó khăn cho việc kiểm tra lịch sử (audit) hoặc debug lỗi cũ.
- **Hướng sửa:** Triển khai lưu log xuống đĩa (disk-based logging). Thực hiện tại backend Rust (ví dụ ghi ra thư mục `app_data_dir/logs/`) hoặc dùng `tauri-plugin-log`. Cập nhật `useLogs.js` để có thể load lịch sử khi khởi động.

### B8. Thiếu metadata "Last Action" bền vững *(Verified)*
Thông tin runtime như "lần cuối sync lúc nào" hay trạng thái "thành công/thất bại" không được lưu lại. Git state thì có thể live-fetch, nhưng action history nếu không lưu sẽ bị mất khi tắt app.
- **Hướng sửa:** Bổ sung field `last_sync_time` và `last_sync_status` vào struct `SyncProject` (lưu vào `projects.json`). Cách này giúp UI render tức thì trạng thái của dự án mà không cần phải parse đọc lại file log nặng.

---

## 6. Phát hiện nhóm C — SRP / SOLID / DRY `[SRP/DRY]`

### C1. God-file `lib.rs` (1077 dòng) — vi phạm SRP cấp module *(Verified)*
Một file gánh 6 trách nhiệm rời rạc (xem §2). RULE-coding: *one clear responsibility per
module*.
- **Hướng sửa (tách module, không đổi logic):**
  `projects.rs`, `ssh.rs`, `git.rs`, `sync.rs`, `agent_usage.rs`, `system.rs`; `lib.rs` chỉ
  còn `run()` + đăng ký handler. Đây là refactor cơ học, rủi ro thấp, giá trị bảo trì cao.

### C2. DRY: cặp undo/redo SSH gần như copy-paste *(Verified)*
`undo_ssh_config` (`lib.rs:560-593`) và `redo_ssh_config` (`lib.rs:853-886`) là hai bản gương
~30 dòng mỗi cái, chỉ khác vai trò undo↔redo.
- **Hướng sửa:** một helper `swap_ssh_state(from, to)` dùng chung.

### C3. DRY: mẫu `ssh host sh` + stdin lặp 3 lần *(Verified)*
`provision_agent_usage`, `force_sync_agent_usage`, `get_agent_usage` lặp y hệt khối spawn ssh
+ ghi stdin + `wait_with_output` (`lib.rs:625-642`, `701-713`, `755-767`).
- **Hướng sửa:** helper `run_remote_script(host, script) -> Result<Output>`.

### C4. DRY: mẫu mở app macOS lặp 4 lần *(Verified)*
`open_local_dir` / `open_in_terminal` / `open_in_vscode` / `open_antigravity_app`
(`lib.rs:934-1020`) lặp khối `Command::new("open").args([...])` + nhánh non-macos.
- **Hướng sửa:** helper `open_with(app: Option<&str>, target: &str)`.

### C5. DRY: nở `~/` → `$HOME` lặp lại *(Verified)*
Logic xử lý tilde xuất hiện ở `run_sync` (`lib.rs:374-380`) và `open_remote_terminal`
(`lib.rs:1026-1032`).
- **Hướng sửa:** helper `expand_remote_tilde(path) -> String`.

### C6. Script nhúng dạng string literal trong Rust *(Verified)*
Trình parse Python (`lib.rs:657-696`) và bộ vá shell (`lib.rs:605-623`) sống dưới dạng raw
string trong Rust: không lint được, không test được, khó đọc. Lưu ý: đã có thư mục `scripts/`
(mới, untracked) nhưng hiện chỉ chứa `check-env.js`; các script remote vẫn nội tuyến.
- **Hướng sửa:** đưa ra file `.sh`/`.py` riêng, nhúng qua `include_str!`. Versioned, đọc được,
  có thể chạy thử độc lập.

### C7. SRP: `useProjects.js` ôm quá nhiều *(Verified)*
Một composable gánh: state projects + git status + điều phối sync + CRUD modal config +
modal special-push + modal git (`useProjects.js:50-327`).
- **Hướng sửa:** tách `useSync.js`, `useProjectConfig.js`, `useGit.js`; giữ `useProjects` cho
  state + load/save. Làm *sau* nhóm A/B (giá trị thấp hơn, không sửa lỗi).

### C8. State store nửa vời *(Verified)*
`ref()` ở module scope + factory `useX()` tạo lại closures mỗi lần gọi. Hoạt động được nhưng
mơ hồ về quyền sở hữu state.
- **Hướng sửa (cân nhắc, không bắt buộc):** gom thành module store thuần (export state + actions,
  bỏ wrapper factory). **Không** vội thêm Pinia trừ khi có nhu cầu thật (YAGNI — xem §9).

### C9. Inline style & handler DOM mệnh lệnh — `ProjectTable.vue` *(Verified)*
Style nội tuyến dày đặc và `onmouseover="this.style..."` (`ProjectTable.vue:40-46`) trộn thao
tác DOM mệnh lệnh vào Vue.
- **Hướng sửa:** chuyển sang class CSS + `:hover`. Giá trị thẩm mỹ/bảo trì, ưu tiên thấp.

---

## 7. Phát hiện nhóm D — Error handling & Language

### D1. `[ERR]` Nuốt lỗi & unwrap rủi ro *(Verified)*
- `.unwrap()` trên `child.stdout.take().unwrap()` (`lib.rs:289-290`) — panic được nếu pipe lỗi.
- `get_projects_path(&app)?.parent().unwrap()` lặp nhiều nơi (`lib.rs:522, 547, 562, 855`).
- Frontend `catch (err) {}` rỗng (`useLogs.js:68`, và nhiều `console.error` nuốt lỗi UX).
- **Hướng sửa:** thay unwrap bằng `?` + thông báo lỗi; với boundary thật thì fail loud theo
  RULE-coding ("fail loudly in development").

### D2. `[ERR]` `force_sync` cố ý bỏ qua exit code *(Verified, có chủ đích)*
`lib.rs:713-720` không kiểm `status.success()` vì Claude rate-limit trả non-zero nhưng vẫn ghi
cache. Đây là *quyết định hợp lý* nhưng đang dựa vào tác dụng phụ. Giữ nguyên hành vi, nhưng
nên kiểm sự tồn tại/định dạng của cache file thay vì giả định.

### D3. `[LANG]` Trộn Việt–Anh trong code *(Verified)*
RULE-coding: *code & comments English only*. Hiện có chuỗi tiếng Việt trong log/throw của JS:
`useProjects.js:62` (`"Lỗi khi tải trạng thái Git"`), comment tiếng Việt trong `ProjectTable.vue:100`
(`"Lấy code về local"`). Lưu ý phân biệt: **toast hướng người dùng** bằng tiếng Việt là *nội
dung UI* (chấp nhận được), nhưng **log kỹ thuật / comment / message throw** nên thống nhất tiếng
Anh.
- **Hướng sửa:** chuẩn hóa log & comment sang English; giữ toast UI theo quyết định nội dung.

---

## 8. Phát hiện nhóm E — Verification & Docs

### E1. `[VERIFY]` Không có lớp kiểm chứng nào *(Verified)*
Không có test (Rust hay JS), không cấu hình lint/clippy/eslint hiển thị, không CI. RULE-coding:
*"Done means verified"* — hiện không có gì để verify ngoài chạy tay.
- **Hướng sửa (MVP-phù hợp, không phủ test toàn bộ):** thêm tối thiểu
  (a) `cargo clippy` + `cargo fmt` như cổng chất lượng,
  (b) vài unit test Rust thuần (không cần SSH) cho: parse SSH host (`get_ssh_hosts`), nở tilde,
  validate path, parse porcelain → file list,
  (c) `eslint` cơ bản cho frontend.
  Không cần E2E/SSH mock — chi phí vượt giá trị cho công cụ cá nhân.

### E2. `[DOCS]` Thiếu `docs/index.md` *(Verified)*
RULE-docs yêu cầu `docs/index.md` là master index. Hiện có `docs/arch/`, `docs/ref/` nhưng
không có index, và `docs/plan/` chỉ vừa được tạo bởi tài liệu này.
- **Hướng sửa:** tạo `docs/index.md` liệt kê các doc hiện có + entry cho kế hoạch này. (Đây là
  việc kề cận; thực hiện riêng, không làm ngầm — xem §11.)

---

## 9. Áp dụng techbiz-optimizer: 20% mang phần lớn giá trị

**Phần đáng làm ngay (giá trị thật, chi phí thấp, dễ kiểm chứng/đảo ngược):**
1. B1 (toggle không persist) — sửa lỗi đúng-đắn ảnh hưởng an toàn dữ liệu.
2. A1–A3 (injection/validation) — loại bỏ mẫu sai chuẩn ở boundary.
3. B2 (async chặn) — gỡ nút thắt khiến flow phải khóa.
4. C1–C5 (tách module + 4 helper DRY) — refactor cơ học, rủi ro thấp, giảm bảo trì lâu dài.

**Phần CHỦ ĐỘNG TỪ CHỐI / HOÃN (red-flag "nghe hay nhưng chưa có bằng chứng"):**
- ❌ Thêm Pinia/Vuex — store module thuần là đủ; chưa có bằng chứng cần.
- ❌ i18n đầy đủ — công cụ cá nhân; chỉ cần thống nhất ngôn ngữ *code*, không cần khung dịch.
- ❌ Test coverage cao / E2E / mock SSH — chi phí vượt giá trị; chỉ test logic thuần.
- ❌ Trừu tượng hóa "agent provider" thành trait/plugin tổng quát — mới có 2 agent, YAGNI.
- ❌ Viết lại frontend sang TypeScript toàn bộ — không có bằng chứng đau đủ lớn.

> Nguyên tắc kiểm: trước khi thêm bất kỳ lớp/abstraction nào, hỏi *"có bằng chứng nào cho độ
> phức tạp này không?"* — nếu không, giữ đơn giản.

---

## 10. Native-flow redesign (đầu ra METHOD-flow-audit cho 2 flow lõi)

### Flow 1 — Sync
- **Intended:** người dùng bấm PUSH/PULL → hook pre → rsync stream log → hook post → lưu trạng thái.
- **Actual breakpoints:** backend chặn executor (B2) → frontend phải khóa toàn cục (B3) → trạng
  thái an toàn `dry_run` không bền (B1) → PULL `--delete` luôn bật (B6).
- **Native shape:** đẩy phần blocking sang `spawn_blocking`; persist `dry_run`/`sync_git` trong
  struct; khóa toàn cục trở thành *tùy chọn* (hoặc queue) thay vì *bắt buộc*. Khi đó các guard
  hiện tại không còn là "hàng rào quanh đường gãy".

### Flow 2 — Agent usage
- **Intended:** poll 30s → đọc cache remote → hiển thị %.
- **Actual breakpoints:** lần poll đầu *ngầm vá* file remote (A5/B4); đường "đọc" có tác dụng phụ ghi.
- **Native shape:** tách `provision` thành hành động người dùng chủ động, có xác nhận; đường poll
  chỉ đọc. Mutation và read không còn trộn lẫn.

### Fastest validation
- B1: sửa struct → đổi toggle → restart app → kiểm `projects.json` còn giữ giá trị. (phút)
- B2: chạy 1 sync dài, kiểm UI còn phản hồi (mở modal khác) trong lúc sync. (phút)
- A1/A2: nhập path chứa `"` → xác nhận không thoát được quoting (test logic escape thuần).

---

## 11. Kế hoạch theo phase

Mỗi phase độc lập, có thể dừng sau bất kỳ phase nào mà app vẫn chạy.

- **P0 — Correctness & Security (bắt buộc, làm trước):** B1, A1, A2, A3, A4, A5/B4.
- **P1 — Flow:** B2, B3, B6, B7, B8.
- **P2 — Structure & DRY (refactor cơ học):** C1, C2, C3, C4, C5, C6.
- **P3 — Frontend SRP & polish:** C7, C8, C9, D3.
- **P4 — Hardening & verify:** D1, D2, E1, E2.

> Ràng buộc thực thi (RULE-agent-behavior › scope discipline): mỗi mục là một thay đổi nhỏ,
> tách biệt, **không** gộp commit, **không** đổi hành vi ngoài phạm vi mục đó. Việc tạo
> `docs/index.md` (E2) là việc kề cận — báo cáo và làm riêng, không làm ngầm trong commit refactor.

---

## 12. Checklist thực thi

### P0 — Correctness & Security
- [ ] **B1** Thêm `dry_run` / `sync_git` (`#[serde(default)]`) vào struct `SyncProject`; xác minh persist qua restart.
- [ ] **A1** Loại bỏ nội suy chuỗi trong `open_remote_terminal`; escape/tham số hóa `host` + `path`.
- [ ] **A2** Sửa `--rsync-path` push: validate/escape `remote_path` hoặc tạo thư mục bằng lệnh ssh tách bạch.
- [ ] **A3** Viết `validate_project()`; áp cho mọi trường path trong `run_sync` (gồm `specific_paths`).
- [ ] **A4** Đặt CSP tối thiểu trong `tauri.conf.json` (`default-src 'self'` + `img-src 'self' data:`).
- [ ] **A5/B4** Tách `provision_agent_usage` thành hành động chủ động + thông báo rõ; bỏ auto-run trong đường poll.

### P1 — Flow
- [ ] **B2** Chuyển blocking IO của `run_sync` sang `spawn_blocking` / command đồng bộ.
- [ ] **B3** Thay return im lặng bằng phản hồi rõ (Toast) hoặc queue.
- [ ] **B6** Đưa `--delete` của PULL thành tùy chọn tường minh per-project (sau B1).
- [ ] **B7** Thiết lập disk-based logging để lưu lịch sử Global log và Project log xuống đĩa (Rust backend).
- [ ] **B8** Thêm `last_sync_time` và `last_sync_status` vào struct `SyncProject` để persist trạng thái.

### P2 — Structure & DRY (Rust)
- [ ] **C1** Tách `lib.rs` → `projects.rs` / `ssh.rs` / `git.rs` / `sync.rs` / `agent_usage.rs` / `system.rs`.
- [ ] **C2** Gộp undo/redo SSH thành `swap_ssh_state()`.
- [ ] **C3** Helper `run_remote_script(host, script)` cho 3 lệnh agent-usage.
- [ ] **C4** Helper `open_with()` cho 4 lệnh mở app macOS.
- [ ] **C5** Helper `expand_remote_tilde()` dùng chung.
- [ ] **C6** Đưa script Python/shell ra file riêng + `include_str!`.

### P3 — Frontend SRP & Language
- [ ] **C7** Tách `useProjects.js` → `useSync` / `useProjectConfig` / `useGit`.
- [ ] **C8** (Cân nhắc) Gom state thành module store thuần, bỏ factory wrapper. *Không thêm Pinia.*
- [ ] **C9** Chuyển inline style + `onmouseover` DOM của `ProjectTable.vue` sang class CSS.
- [ ] **D3** Chuẩn hóa log kỹ thuật & comment sang English (giữ toast UI tiếng Việt là nội dung).

### P4 — Hardening & Verify
- [ ] **D1** Thay `.unwrap()` rủi ro bằng `?` + thông báo lỗi; bỏ `catch {}` rỗng nuốt lỗi UX.
- [ ] **D2** `force_sync`: kiểm tra cache file tồn tại/định dạng thay vì giả định tác dụng phụ.
- [ ] **E1** Thêm cổng `cargo clippy` + `cargo fmt` + `eslint`; vài unit test Rust thuần (host parse, tilde, validate, porcelain).
- [ ] **E2** Tạo `docs/index.md` (master index) + entry cho tài liệu này.

---

## 13. Rủi ro & nguyên tắc khi thực thi

- **Không viết lại tất cả.** P2 là tách module *bảo toàn logic*, không đổi hành vi. Diff phải đọc được.
- **Mỗi thay đổi tự kiểm.** Sau mỗi mục: `cargo build` (Rust), `npm run build` (frontend), và với
  flow thì chạy thử quan sát (RULE-coding: *done means verified*).
- **Source of truth là code đang chạy**, không phải tài liệu này. Nếu khi thực thi phát hiện
  `file:line` đã đổi, ưu tiên code thực tế và cập nhật lại checklist.
- **Hỏi trước** với các mục chạm tới hành vi remote/SSH/persist nếu ngữ cảnh thay đổi
  (RULE-agent-behavior › decision boundaries).

---

*Tài liệu thuộc `docs/plan/`. Khi hoàn tất, chuyển sang `docs/plan/done/` theo RULE-docs.*
