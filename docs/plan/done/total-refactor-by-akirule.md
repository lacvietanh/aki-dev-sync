# Total Refactor — Phân tích PROCODING theo akirule

> **✅ HOÀN TẤT — 27/27 items · Released: v1.1.0**

> Phạm vi: `Aki Remote Dev Sync` (Tauri v2 + Vue 3 + Rust). Bản phân tích kiến trúc và kế hoạch
> cải thiện, soi chiếu trực tiếp vào bộ quy tắc `~/.aki/claudedoc/` (RULE-coding,
> RULE-agent-behavior, RULE-docs, METHOD-flow-audit, METHOD-techbiz-optimizer).
>
> **Bản này được tổ chức lại theo gốc rễ flow, không theo nhóm triệu chứng.** Trọng tâm là
> *chuẩn code pro + luồng native gốc rễ*, không sa đà vào security vụn vặt của một công cụ
> single-user local. Thứ tự thực thi tuân theo nguyên tắc khoa học ở §3 để các mục không
> chồng chéo, không vá đè lên nhau.

---

## 1. Mục tiêu thật & nguyên tắc nền (METHOD-techbiz-optimizer)

### Goal thật (một câu)
Biến một công cụ rsync-GUI cá nhân đang ở mức MVP thành codebase **đúng đắn, luồng native,
dễ bảo trì bởi một developer** — mà *không* rơi vào over-engineering.

### Nguyên tắc ràng buộc (từ RULE-coding)
- **Single-developer friendly, MVP-first, YAGNI.** Đây không phải SaaS đa người dùng.
- **DRY nhưng không trừu tượng hóa vô cớ.** Gộp khi giảm gánh nặng thật, không gộp để "đẹp".
- **Done means verified.** Hiện không có lớp kiểm chứng nào (xem E1).
- **Source of truth là code đang chạy, không phải tài liệu này.** (Áp dụng ngay: §8 ghi lại
  những chỗ bản kế hoạch *cũ* đã lệch khỏi code thật.)

> Hệ quả định hướng: kế hoạch này **sửa gốc shape của flow trước**, để các guard/triệu chứng
> tự tan; refactor cơ học (tách module, DRY) làm **sau cùng** khi shape đã ổn định. Refactor
> toàn bộ ≠ viết lại tất cả.

---

## 2. Hai trục gốc rễ — nơi 80% giá trị PROCODING nằm ở đây

Chạy METHOD-flow-audit nghiêm túc, hầu hết các phát hiện rời rạc của bản cũ quy về **hai
root-shape problem**. Sửa đúng hai trục này thì một loạt mục con tự biến mất thay vì phải vá
từng cái — đây mới là "thông minh, khoa học, native" thay vì "chắp vá".

### Trục R1 — Persisted config và Runtime state bị trộn trong CÙNG một object `[FLOW][SRP]`
*(Verified)*

Object `project` ở frontend vừa giữ **config bền** (`id`, paths, `hooks`, `excludes`) vừa giữ
**state runtime/derived** (`git_status`, `git_log`, `remote_url`, `dry_run`, `sync_git`) — tất
cả nhét chung một chỗ rồi đẩy qua lại giữa UI ↔ persist.

Bằng chứng trong code thật:
- `saveProjectsList()` (`useProjects.js:100`) serialize **cả cục** `projects.value`. Struct Rust
  `SyncProject` (`lib.rs:24-37`) **không có** `dry_run`/`sync_git` → serde lặng lẽ rớt → khi load
  lại `useProjects.js:78-79` thấy `undefined` → **mặc định `true`**. (Đây là bug B1.)
- `saveConfig` (`useProjects.js:194-196`) phải **copy ngược thủ công** `dry_run`/`sync_git`/
  `git_status` từ object cũ sang object đang sửa — *artificial enforcement* kinh điển: guard tay
  vì ownership không rõ.
- `openConfig`/`saveConfig` phải `JSON.parse(JSON.stringify())` deep-clone cả cục vì không tách
  được phần "đang sửa" khỏi phần "runtime giữ nguyên".
- `fetchGitStatus` mutate `project.git_status` **in-place** trên object đã persist → kích
  deep-watch nặng ở `ProjectTable.vue:132` (`{ deep: true }`). (Đây là bug B5.)

**Một root cause này sinh ra: B1, B5, B8, và toàn bộ màn clone + copy-back tay.**

> **Native shape:** tách hai mô hình ra.
> - `SyncProject` = **chỉ** persisted config (struct Rust + `#[serde(default)]` cho field mới,
>   `projects.json` là nguồn duy nhất).
> - `projectRuntime` = map `id -> { git_status, git_log, remote_url, syncing, ... }` sống ở
>   frontend, **không bao giờ** đi vào `save_projects`.
>
> Khi tách đúng: B1 tan (mọi field config persist tự nhiên), B5 tan (mutate runtime không đụng
> object persisted nên không deep-watch), B8 tan (`last_sync_status` là persisted field hợp lệ),
> và xóa luôn deep-clone + copy-back ở `saveConfig`. Đây là *smallest structural change with the
> biggest effect* (flow-audit §7).

### Trục R2 — `run_sync` là `async fn` nhưng chặn executor → frontend phải khóa nhân tạo `[FLOW]`
*(Verified)*

`run_sync` khai báo `async fn` (`lib.rs:310`) nhưng bên trong dùng `thread::spawn(...).join()`
+ `child.wait()` (`lib.rs:456-460`) — **blocking IO đồng bộ chạy thẳng trên worker thread của
async runtime Tauri**. Suốt một lần rsync, một worker bị giữ.

- **Artificial enforcement đi kèm:** frontend phải khóa toàn cục `if (syncingProjectId.value
  !== null) return;` (`useProjects.js:109`) — *fence quanh path gãy*. Khóa này còn **nuốt ý
  định người dùng im lặng** (B3): bấm sync khi đang bận → không phản hồi gì.

**Root cause này sinh ra: B2, B3, và lý do tồn tại của khóa toàn cục.**

> **Native shape:** đẩy phần blocking sang `tauri::async_runtime::spawn_blocking`, hoặc khai báo
> command là `fn` đồng bộ thường (Tauri tự đẩy sang thread pool). Khi backend không còn chặn,
> khóa toàn cục frontend trở thành **lựa chọn UX** (hoặc queue) chứ không phải *bắt buộc*, và
> guard im lặng được thay bằng phản hồi rõ.

### Trục phụ R3 — Đường "đọc" usage có tác dụng phụ "ghi" `[FLOW]` *(Verified)*

`useAgentUsage.checkUsage` (`useAgentUsage.js:24-29`) — hàm *poll định kỳ 30s để đọc* — lại
**kích hoạt provision** ở lần đầu: `provision_agent_usage` (`lib.rs:606`) dùng `sed -i` chèn
shim vào `~/.claude/statusline-command.sh` trên remote, **tự động & im lặng**. Read trộn
mutation, và mutation đó còn sửa file người dùng không hỏi.

> **Native shape:** tách `provision` thành **hành động người dùng chủ động bấm**, có thông báo
> rõ sẽ sửa gì; đường poll **chỉ đọc**. (Liên quan tới A5/B4 của bản cũ — gộp về đây.)
>
> *Honesty (RULE-coding › source of truth):* cả subsystem agent-usage (`provision`/`force_sync`/
> `get_agent_usage`) về bản chất là **scraping hack** — `sed` patch statusline, chạy
> `claude -p /usage` rồi **regex parse stdout** (`lib.rs:665`), nhúng parser Python dạng
> raw-string. Refactor (R3 + C3 + C6) làm nó *đọc được hơn và đúng flow read/write hơn*, nhưng
> **không** biến nó thành "native sạch" — đừng kỳ vọng quá. Đây là giới hạn cố hữu, không phải
> nợ kỹ thuật sửa được bằng cấu trúc.

---

## 3. Nguyên tắc xếp thứ tự (để "ưu tiên" không xung đột "đúng luồng")

Bốn quy tắc sắp xếp, áp theo đúng thứ tự này:

1. **Root shape trước symptom.** Sửa R1/R2 trước, rồi các guard (B3, B5, copy-back) tự tan — không
   đi vá từng triệu chứng rồi sau đó lại gỡ.
2. **Theo dependency.** Ví dụ: persist `dry_run` (R1) **phải trước** việc đưa `--delete` thành
   tùy chọn per-project (B6) — vì tùy chọn đó cần một field bền để lưu. Sửa struct (R1) **phải
   trước** tách module (C1) — vì C1 chỉ là di chuyển code đã ổn định.
3. **Co-location — chạm mỗi hàm đúng một lần.** `run_sync` bị nhiều mục đụng tới (R2/B2, A1/A2
   command-construction, A3 validate, B6 delete-option). Gom tất cả vào **một pass** trên
   `run_sync` thay vì sửa nó 4 lần ở 4 phase khác nhau.
4. **Refactor cơ học làm cuối.** Tách module (C1) và 4 helper DRY (C2–C5), `include_str!` (C6)
   đáp lên shape **đã chốt** → diff thuần cơ học, đọc được, không double-touch, không rework.

> Hệ quả: thứ tự **không** đơn thuần là "security cao nhất trước". Một mục security nhỏ (A4 CSP,
> A3 validate) có thể nằm rất sau, trong khi một mục cấu trúc (R1) lên đầu — vì leverage và
> dependency, không vì nhãn.

---

## 4. Bản đồ kiến trúc hiện tại (Verified)

```
src-tauri/src/lib.rs        1081 dòng — TOÀN BỘ backend trong 1 file
  ├─ SSH config CRUD + undo/redo lịch sử (file-based)
  ├─ Git info / git status porcelain
  ├─ run_sync: engine rsync + hook pre/post (local/remote)   ← Trục R2
  ├─ agent-usage: provision + force-sync + get (scraping)    ← Trục R3
  └─ OS integration: open finder/terminal/vscode/url/remote-terminal/icon

src/composables/
  ├─ useProjects.js   362 — state + git + sync + 3 modal CRUD  ← Trục R1 (đa trách nhiệm)
  ├─ useSsh.js        173 — SSH modal + undo/redo + auto-migrate host
  ├─ useLogs.js        97 — log store (global + per-project) + listener
  └─ useAgentUsage.js 102 — polling usage 30s + auto-provision ← Trục R3

src/components/  — App.vue điều phối, ProjectTable.vue (196 dòng) là bảng chính
                  (deep-watch load icon + inline style dày đặc)
```

**Mô hình state:** các `ref()` khai báo ở **module scope** (vd `useProjects.js:20`,
`useLogs.js:4`) → singleton dùng chung như một "store nghèo", nhưng factory `useX()` lại tạo mới
closures mỗi lần gọi → mơ hồ quyền sở hữu (C8). Trục R1 chính là phiên bản nghiêm trọng của
sự mơ hồ này.

---

## 5. Phương pháp đánh giá

Mỗi phát hiện gắn nhãn quy tắc nguồn:

| Nhãn | Nguồn quy tắc |
|------|---------------|
| `[FLOW]` | METHOD-flow-audit |
| `[SRP/DRY]` | RULE-coding › Code quality |
| `[ERR]` | RULE-coding › Error handling |
| `[VERIFY]` | RULE-coding › Verification |
| `[SEC]` | RULE-coding › Security |
| `[LANG]` | RULE-coding › Language (English-only code) |
| `[DOCS]` | RULE-docs |

> **Lưu ý reframe security:** A1/A2 của bản cũ *trông như* `[SEC]` nhưng gốc là `[FLOW][SRP]` —
> chúng là lỗi *dựng lệnh bằng nội suy chuỗi* (`format!`), tức "chắp vá command", không phải lỗ
> hổng cần lo hacker (công cụ single-user, path do chính người dùng gõ). Vì vậy chúng được xếp
> theo flow, không theo "ưu tiên bảo mật". A3/A4 là defensive thật sự nhỏ → để cuối.

---

## 6. Phát hiện — tổ chức theo gốc rễ

### 6.1 Thuộc Trục R1 (persisted vs runtime state)

- **B1 `[FLOW]` BUG: toggle `dry_run`/`sync_git` không persist** *(Verified)*. Struct
  `SyncProject` (`lib.rs:24-37`) thiếu hai field; không có `#[serde(deny_unknown_fields)]` nên
  serde rớt im lặng khi `save_projects`. Load lại → `useProjects.js:78-79` mặc định `true`. Mỗi
  lần restart, Dry-Run bật lại, `sync_git` bật lại. Trầm trọng vì PULL luôn kèm `--delete`
  (`lib.rs:423`) — trạng thái an toàn lại không bền (xem B6).
- **B5 `[FLOW]` Deep-watch nặng để load icon** *(Verified)*. `watch(() => projects.value, ...,
  { deep: true, immediate: true })` (`ProjectTable.vue:132-146`) chạy lại toàn bộ vòng lặp mỗi
  khi *bất kỳ* field nào đổi — kể cả `git_status`/`last_sync_time` mutate in-place. Có guard
  `=== undefined` chặn invoke lặp, nhưng vẫn quét deep toàn mảng liên tục. Triệu chứng trực tiếp
  của R1 (runtime field sống chung object persisted).
- **B8 `[FLOW]` (ĐÃ LỖI THỜI MỘT PHẦN)** *(Verified — xem §8)*. Bản cũ nói "thiếu metadata Last
  Action". **Sai:** struct đã có `last_sync_action` + `last_sync_time` (`lib.rs:35-36`), và
  `startSync` đã ghi + persist (`useProjects.js:140-142`). Chỉ còn thiếu **`last_sync_status`**
  (success/fail). Co B8 lại đúng một field này, làm trong cùng pass R1.
- **C7 `[SRP]` `useProjects.js` ôm quá nhiều** *(Verified)*. Một composable gánh: state projects
  + git status + điều phối sync + CRUD modal config + modal special-push + modal git
  (`useProjects.js:50-327`). Sau khi tách runtime ra (R1), việc chẻ `useSync`/`useProjectConfig`/
  `useGit` trở nên **tự nhiên** thay vì gượng ép.
- **C8 `[SRP]` State store nửa vời** *(Verified)*. `ref()` module-scope + factory tạo lại
  closures. Gom thành module store thuần (export state + actions, bỏ wrapper) là hệ quả gọn của
  R1. **Không** thêm Pinia (YAGNI — §9).

### 6.2 Thuộc Trục R2 (sync flow blocking)

- **B2 `[FLOW]` `run_sync` async nhưng chặn executor** *(Verified)*. `lib.rs:456-460`. → đẩy
  blocking sang `spawn_blocking` hoặc command đồng bộ.
- **B3 `[FLOW]` Khóa sync im lặng** *(Verified)*. `useProjects.js:109` return trần. Sau B2:
  thay bằng Toast "đang có sync chạy" hoặc cho queue.
- **A1 `[FLOW][SEC]` Command dựng bằng nội suy chuỗi — `open_remote_terminal`** *(Verified)*.
  `lib.rs:1038` nội suy `host`/`path` thẳng vào AppleScript trong `osascript -e`. Gốc là *string-
  built command*. Sửa: tham số hóa / shell-escape, không `format!` cả khối.
- **A2 `[FLOW][SEC]` Command dựng bằng nội suy chuỗi — `--rsync-path`** *(Verified)*.
  `lib.rs:385` `--rsync-path=mkdir -p "{}" && rsync` với `remote_path` nội suy. Cùng anti-pattern
  A1. Sửa: validate/escape, hoặc tạo thư mục bằng lệnh `ssh` tham số tách bạch. *Nằm trong
  `run_sync` → làm cùng pass R2.*
- **A3 `[SEC]` Validation đầu vào không đầy đủ** *(Verified, ưu tiên thấp)*. `lib.rs:321` chỉ chặn
  `local_path.contains("..")`; không kiểm `remote_path`/`remote_host`/`specific_paths`. Một hàm
  `validate_project()` gọi đầu `run_sync`. *Làm cùng pass R2 vì cùng hàm.*
- **B6 `[FLOW]` PULL luôn `--delete`** *(Verified)*. `lib.rs:423` mirror-delete mặc định, chỉ
  được bảo vệ bởi `dry_run` (vốn không bền — B1). **Phụ thuộc B1**: chỉ làm sau khi `dry_run`
  persist, rồi đưa `--delete` thành tùy chọn tường minh per-project (một field struct mới).

### 6.3 Thuộc Trục R3 (agent-usage read/write)

- **A5/B4 `[FLOW]` Tự vá môi trường remote trong đường poll** *(Verified)*. `useAgentUsage.js:25-29`
  + `lib.rs:606-642`. Tách `provision` ra hành động chủ động + thông báo; đường poll chỉ đọc.
- **D2 `[ERR]` `force_sync` cố ý bỏ exit code** *(Verified, có chủ đích)*. `lib.rs:717-724` không
  kiểm `status.success()` vì Claude rate-limit trả non-zero nhưng vẫn ghi cache. Quyết định hợp
  lý, **giữ hành vi**, nhưng nên kiểm *cache file tồn tại/đúng định dạng* thay vì giả định tác
  dụng phụ.

### 6.4 Refactor cơ học (làm sau, đáp lên shape đã chốt)

- **C1 `[SRP]` God-file `lib.rs` (1081 dòng)** *(Verified)*. Tách `projects.rs` / `ssh.rs` /
  `git.rs` / `sync.rs` / `agent_usage.rs` / `system.rs`; `lib.rs` chỉ còn `run()` + đăng ký
  handler. **Làm sau R2/R3** để `sync.rs`/`agent_usage.rs` chứa logic đã ở dạng cuối.
- **C2 `[DRY]` undo/redo SSH copy-paste** *(Verified)*. `undo_ssh_config` (`lib.rs:564-597`) và
  `redo_ssh_config` (`lib.rs:857-890`) là hai bản gương ~30 dòng → helper `swap_ssh_state(from, to)`.
- **C3 `[DRY]` mẫu `ssh host sh` + stdin lặp 3 lần** *(Verified)*. `provision`/`force_sync`/
  `get_agent_usage` (`lib.rs:629-641`, `705-717`, `759-771`) → helper
  `run_remote_script(host, script) -> Result<Output>`.
- **C4 `[DRY]` mở app macOS lặp 4 lần** *(Verified)*. `open_local_dir`/`open_in_terminal`/
  `open_in_vscode`/`open_antigravity_app` (`lib.rs:938-1024`) → helper `open_with()`.
- **C5 `[DRY]` nở `~/` → `$HOME` lặp** *(Verified)*. `run_sync` (`lib.rs:378-384`) và
  `open_remote_terminal` (`lib.rs:1030-1036`) → helper `expand_remote_tilde()`. *Sau A1/A2 để
  helper bọc đúng logic cuối.*
- **C6 `[SRP]` Script nhúng raw-string trong Rust** *(Verified)*. Parser Python (`lib.rs:661-700`)
  + patcher shell (`lib.rs:609-627`) → file `.sh`/`.py` riêng + `include_str!`. Đã có thư mục
  `scripts/` (untracked, mới chỉ chứa `check-env.js`).

### 6.5 Polish & hardening (cuối cùng)

- **C9 `[SRP]` Inline style + handler DOM mệnh lệnh** *(Verified)*. `ProjectTable.vue:40-46` dày
  `style="..."` + `onmouseover="this.style..."` → class CSS + `:hover`.
- **D1 `[ERR]` unwrap rủi ro & nuốt lỗi** *(Verified)*. `child.stdout.take().unwrap()`
  (`lib.rs:289-290`); `get_projects_path(&app)?.parent().unwrap()` lặp nhiều nơi (`lib.rs:526,
  551, 566, 859`); `catch (err) {}` rỗng (`useLogs.js:68`). → `?` + thông báo; fail loud ở dev.
- **D3 `[LANG]` Trộn Việt–Anh trong code** *(Verified)*. Chuỗi Việt trong log/throw kỹ thuật:
  `useProjects.js:62` (`"Lỗi khi tải trạng thái Git"`), comment Việt `ProjectTable.vue`. Chuẩn hóa
  log kỹ thuật + comment sang English; **giữ toast UI tiếng Việt** (đó là *nội dung UI*, hợp lệ).
- **A4 `[SEC]` CSP bị tắt** *(Verified, trivial)*. `tauri.conf.json:24` `"csp": null`. Đặt CSP
  tối thiểu (`default-src 'self'`, `img-src 'self' data:`). 1 dòng config, để cuối.
- **B7 `[FLOW]` Log không lưu lịch sử** *(Verified)*. `useLogs.js` chỉ in-memory; restart mất hết.
  *(Đánh giá lại — xem §9: đây là feature-add, không phải sửa lỗi shape. Cân nhắc kỹ trước khi
  nhận, dễ thành scope creep.)*
- **E1 `[VERIFY]` Không có lớp kiểm chứng** *(Verified)*. Không test, không lint/CI. Thêm tối
  thiểu: `cargo clippy` + `cargo fmt` làm cổng; vài unit test Rust thuần (parse SSH host, nở
  tilde, validate path, parse porcelain); `eslint` cơ bản. Không E2E/mock SSH (chi phí vượt
  giá trị).
- **E2 `[DOCS]` Thiếu `docs/index.md`** *(Verified)*. Tạo master index + entry cho kế hoạch này.
  Việc kề cận — làm riêng, không làm ngầm trong commit refactor.

---

## 7. Kế hoạch theo phase (dependency-ordered, không chồng chéo)

Mỗi phase độc lập, dừng sau bất kỳ phase nào app vẫn chạy. Thứ tự theo §3.

### P0 — Trục R1: tách persisted vs runtime state ✅ DONE
- [x] **R1.1 / B1** Thêm `dry_run`/`sync_git`/`last_sync_status`/`delete_on_pull` vào struct `SyncProject`
      với `#[serde(default)]` → `projects.rs`. Persist đúng qua restart.
- [x] **R1.2 / B8** Backend frontend ghi `last_sync_status: "success"|"error"` sau mỗi sync.
- [x] **R1.3 / B5** Tách runtime (`git_status`/`git_log`/`remote_url`/`syncing`) ra `projectRuntime` map;
      watch shallow theo ID list → không deep-watch nữa.
- [x] **R1.4** Xóa copy-back tay ở `saveConfig`; `openConfig` dùng spread clone.
- [x] **B3** Toast "đang sync" thay vì return im lặng; per-project lock (parallel sync ready).

### P1 — Trục R2: sync flow native ✅ DONE
- [x] **R2.1 / B2** `run_sync` chuyển thành sync `fn` — Tauri tự đẩy vào thread pool, không chặn executor.
- [x] **R2.2 / A1** `open_remote_terminal`: validate host (allowlist chars) + `applescript_escape(path)` → `system.rs`.
- [x] **R2.3 / A2** Bỏ `--rsync-path` string injection; tạo remote dir bằng `ssh mkdir -p` tách bạch → `sync.rs`.
- [x] **R2.4 / A3** `validate_project()` + `validate_specific_paths()` tại đầu `run_sync` → `projects.rs`.
- [x] **R2.6 / B6** `delete_on_pull: bool` (default true) trong struct; toggle trong `ProjectConfigModal.vue`.

### P2 — Trục R3: agent-usage read/write thuần ✅ DONE
- [x] **R3.1 / A5+B4** Tách `provision()` ra khỏi `checkUsage`; poll chỉ đọc. `provision()` phải gọi tường
      minh từ UI. `provisioned` + `provision` exported cho component dùng.
- [x] **R3.2 / D2** `force_sync`: log stderr cho diagnostics nhưng vẫn `Ok(true)` — behavior intentional,
      bây giờ documented rõ trong code.

### P3 — Refactor cơ học Rust ✅ DONE
- [x] **C1** `lib.rs` → `projects.rs` / `ssh.rs` / `git.rs` / `sync.rs` / `agent_usage.rs` / `system.rs`.
      `lib.rs` chỉ còn module declarations + `run()`.
- [x] **C2** `swap_ssh_state(from, to)` helper → `ssh.rs`; undo/redo dùng chung.
- [x] **C3** `run_remote_script(host, script)` → `agent_usage.rs`; 3 lệnh agent dùng chung.
- [x] **C4** `open_with(app_name, target)` → `system.rs`; 4 open commands dùng chung.
- [x] **C5** `expand_remote_tilde()` → `sync.rs` (pub); `system.rs` import và dùng.
- [x] **C6** Script Python + shell → `scripts/provision-claudecode.sh`, `scripts/force-sync-claudecode.sh`,
      `scripts/force-sync-parse.py`; Rust dùng `include_str!`.
- [x] **D1** `.unwrap()` trên stdout/stderr → `.ok_or(...)?`; `parent().unwrap()` → `get_app_data_dir()` helper.

### P4 — Frontend SRP & polish ✅ DONE
- [x] **C7** `useProjects.js` tách thành 4 module: `src/store/projectStore.js` (pure state + Toast),
      `useGit.js`, `useProjectConfig.js`, `useSync.js`. `useProjects.js` còn lại là thin re-export
      facade + factory shim — 8 component không cần đổi import. useSsh.js vẫn import trực tiếp.
- [x] **C8** Module store pattern: tất cả state và functions ở module level (không có factory closure).
      `src/store/projectStore.js` export `projects`, `projectRuntime`, `anySyncing`, `isReloading`,
      `Toast` trực tiếp. Mỗi domain composable export functions tường minh.
- [x] **C9** Inline style + `onmouseover` DOM → `.icon-link`/`.path-link` CSS class + `:hover` → `ProjectTable.vue`.
- [x] **D3** Technical logs & comments chuẩn hóa sang English trong `useProjects.js`.

### P5 — Hardening & verify ✅ DONE
- [x] **D1** unwrap rủi ro → `?` + error message (trong Rust modules).
- [x] **A4** CSP tối thiểu trong `tauri.conf.json`: `default-src 'self'; img-src 'self' data:`.
- [x] **E1** Unit tests Rust thuần trong `#[cfg(test)]` modules:
      - `projects.rs`: 6 tests cho `validate_project` (traversal, control chars, empty host, valid)
      - `sync.rs`: 9 tests cho `expand_remote_tilde` + `validate_specific_paths`
      - `system.rs`: 8 tests cho `validate_ssh_host` + `applescript_escape`
      `cargo test --lib` chạy tất cả unit tests (không cần GTK). `cargo clippy`/`fmt` chạy trên macOS.
      Frontend: `npm run build` clean ✓ (Vite bắt lỗi import/type).
- [x] **E2** `docs/index.md` master index tạo xong.

> **B7 (disk-based logging)** *không* nằm trong phase nào ở trên — đây là *feature mới*, không
> phải sửa shape. Chỉ nhận nếu có nhu cầu audit thật (xem §9).

---

## 8. Đính chính bản kế hoạch cũ (RULE-coding › source of truth)

`lib.rs` đang ở trạng thái Modified; bản kế hoạch cũ viết trước một số thay đổi nên đã lệch:

- **B8 cũ sai:** struct **đã có** `last_sync_action` + `last_sync_time` và **đã persist**. Bản
  này co B8 còn duy nhất `last_sync_status`.
- **B1 và B8 cũ tách rời:** thực chất cùng root cause R1 → gộp vào P0.
- **Nhóm A "ưu tiên cao nhất" cũ:** đánh giá lại — A1/A2 là `[FLOW]` (string-built command), A3/A4
  là defensive trivial. Trọng tâm thật là R1/R2, không phải "bảo mật".
- **C8 cũ "không bắt buộc":** nâng thành hệ quả của R1 (P0/P4) vì nó là gốc của sự mơ hồ state.

Khi thực thi, nếu `file:line` đã đổi tiếp, **ưu tiên code thật** và cập nhật checklist.

---

## 9. techbiz-optimizer: chủ động TỪ CHỐI / HOÃN

**Red-flag "nghe hay nhưng chưa có bằng chứng giá trị":**
- ❌ **Pinia/Vuex** — module store thuần đủ; R1 đã giải quyết gốc ownership.
- ❌ **i18n đầy đủ** — chỉ cần thống nhất ngôn ngữ *code*, không cần khung dịch.
- ❌ **Test coverage cao / E2E / mock SSH** — chỉ test logic thuần.
- ❌ **Trừu tượng "agent provider" thành trait/plugin** — mới 2 agent, YAGNI.
- ❌ **Rewrite frontend sang TypeScript** — không có bằng chứng đau đủ lớn.
- ⚠️ **B7 disk-based logging** — *feature mới*, không phải sửa shape. Dễ thành scope creep
  (rotation, format, retention...). Chỉ nhận nếu có nhu cầu audit thật, và làm như feature
  độc lập **sau** toàn bộ refactor — không trộn vào phase sửa lỗi.

> Nguyên tắc kiểm: trước khi thêm lớp/abstraction, hỏi *"có bằng chứng nào cho độ phức tạp
> này không?"* — không có thì giữ đơn giản.

---

## 10. Fastest validation (mỗi trục một slice nhỏ)

- **R1:** sửa struct → đổi toggle Dry-Run → restart → kiểm `projects.json` còn giữ giá trị (phút).
  Mở `ProjectTable`, đổi `git_status` runtime → xác nhận không trigger lại vòng load icon.
- **R2:** chạy 1 sync dài → trong lúc đó mở modal khác / bấm reload → UI còn phản hồi (phút).
  Nhập `remote_path` chứa `"` → xác nhận không thoát được quoting (test escape thuần).
- **R3:** mở app với host đã cấu hình → xác nhận **không** có lần `sed` ngầm nào chạy khi chỉ
  poll; provision chỉ chạy khi bấm nút.

---

## 11. Rủi ro & nguyên tắc khi thực thi

- **Không viết lại tất cả.** P3 là tách module *bảo toàn logic*, diff phải đọc được.
- **Mỗi thay đổi tự kiểm.** Sau mỗi mục: `cargo build` (Rust), `npm run build` (frontend), và
  với flow thì chạy thử quan sát (RULE-coding: *done means verified*).
- **Source of truth là code đang chạy**, không phải tài liệu này.
- **Hỏi trước** với mục chạm hành vi remote/SSH/persist nếu ngữ cảnh đổi (RULE-agent-behavior ›
  decision boundaries). Riêng R3 (sửa file remote) và B6 (`--delete`) là vùng phá hủy — xác nhận
  trước khi đổi hành vi.
- **Không gộp commit xuyên phase.** Việc tạo `docs/index.md` (E2) làm riêng, không làm ngầm.

---

*Tài liệu thuộc `docs/plan/`. Khi hoàn tất, chuyển sang `docs/plan/done/` theo RULE-docs.*
