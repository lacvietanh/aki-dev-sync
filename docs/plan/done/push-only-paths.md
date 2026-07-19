# Plan — Push-only paths bằng exclude-list semantics (bỏ hardcode .git, diệt badge ma)

**Status:** ✅ DONE — Implemented + verified trên Mac (2026-07-20, 1.13.0). Kết quả verify: §6b.
Còn đúng một hạng mục chưa chạy: PUSH/PULL thật vào host production (§6b mục 3) — cần thao tác có
chủ đích của người dùng, không tự động hoá.
**Thay thế:** bản draft cùng tên (2026-07-1x)
**Verify code:** đã đối chiếu từng dòng với `sync.rs` / `useSync.js` / `useSyncStatus.js` / `git.rs` / `ProjectTable.vue` @ HEAD (1.12.0)

---

## 1. Kết quả điều tra (facts, đã verify)

### 1.1 Không có refactor sync giữa 1.10 → 1.12

`git log v1.10.0..HEAD -- src-tauri/src/sync.rs src/composables/useSync.js src/composables/useSyncStatus.js`
= **rỗng**. Sync core đứng yên từ thời 1.9.2 (`85b4a35`). Cảm giác "dùng sai sai" là bệnh cũ
lâu năm, không phải regression 1.11/1.12.

### 1.2 Root cause badge PUSH ma ("local không sửa gì mà badge PUSH sáng")

- Toggle `.git` ON → status check **đếm cả `.git/`** vào push count (`sync.rs:532` truyền
  `project.sync_git` vào `build_rsync_args` → skip dòng exclude `.git/`).
- Baseline lại **loại `.git`** (`sync.rs:205-206`) → churn trong `.git` không bao giờ được
  suppression đỡ.
- **App tự gây churn**: background refresh chạy `git status --porcelain` (`git.rs`) → git
  opportunistic index refresh ghi lại `.git/index` → mtime đổi → bị đếm là local change.
  Cộng thêm `.git/logs/HEAD`, `FETCH_HEAD`, `packed-refs`… đổi theo thao tác git thường ngày.
- Fix cũ ("loại mtime thư mục khỏi dry-run", IntroModal:210) chỉ lọc dòng kết thúc `/` —
  file bên trong `.git` vẫn lọt.

**Nghi can phụ (chưa verify được trên máy này — cần Mac, xem §6):** parser output rsync tại
`sync.rs:556-577` lọc theo chuỗi literal (`"Skip newer "`, `"Transfer starting:"` — dấu vết
openrsync). Nếu macOS đổi format (openrsync thay rsync từ Sequoia) → dòng skip lọt qua filter
→ file remote-mới-hơn bị đếm nhầm vào push. Fix cấu trúc ở §3 làm badge miễn nhiễm với churn
push-only dir, nhưng nghi can parser này là bug độc lập, phải test thật.

> ### ⚠️ ĐÍNH CHÍNH §1.2 (đo thật trên Mac, 2026-07-20 — sau khi 1.13.0 đã release)
>
> **Gạch bỏ luận điểm "App tự gây churn" ở gạch đầu dòng thứ 3.** Đo trực tiếp: `.git/index`
> **KHÔNG** bị ghi lại khi chạy `git status --porcelain` trên working tree sạch — mtime đứng yên
> qua nhiều lần gọi liên tiếp. Git chỉ rewrite index khi thật sự cần cập nhật stat cache, không
> phải mỗi lần `status`. Luận điểm này viết mù trên máy Linux, chưa từng được đo, và là **tiền đề
> trung tâm** mà toàn bộ R2 dựa vào.
>
> **Nghi can parser cũng loại.** Máy này chạy **rsync thật 3.4.1** (Homebrew), không phải openrsync.
> Output status check sạch: chỉ `sending incremental file list` + block thống kê, không dòng lạ nào
> lọt qua blocklist. Push count = 0, đúng.
>
> **Hệ quả — câu chuyện gốc bị kể sai.** Nếu `.git/index` không tự churn thì `.git` chỉ đổi khi
> người dùng **thật sự thao tác git** (commit, fetch, checkout). Badge cũ vì vậy **không phải nhiễu
> — nó là tín hiệu thật, chỉ sai ngữ nghĩa**: nó đếm thay đổi `.git` vào một badge mà người dùng
> đọc là "có code cần deploy". Cảm giác "badge ma" đến từ chỗ *commit ≠ có code mới cần đẩy* trong
> đầu người dùng, không phải từ con số bịa ra.
>
> R2 vẫn **đúng về ngữ nghĩa** (badge = code cần deploy) nên giữ. Nhưng nó được bán trong CHANGELOG
> như "root-cause phantom badge", và điều đó **không đứng vững**: cái được sửa là ngữ nghĩa, không
> phải nhiễu. Xem §9.

### 1.3 Tại sao `.git` không hỏi mà `.wrangler` hỏi

Push thường **không bao giờ hỏi "replace"** — rsync ghi đè thẳng, không có confirm ghi đè nào
trong luồng push thường (confirm ghi đè chỉ có ở special push qua file picker). Cơ chế hỏi duy
nhất là **confirm Force Delete** (mirror push, `useSync.js:43-104`). Khác biệt `.git` vs
`.wrangler` chỉ là **tần suất deletion**: `.git` hằng ngày hầu như không xóa file (đến `git gc`
thì cũng sẽ hỏi, và hỏi hàng trăm file); `.wrangler` xóa file tạm liên tục → dialog gõ-tên-project
mỗi lần push. Cùng một cơ chế, khác nhịp churn — bỏ hardcode `.git` đơn thuần KHÔNG diệt được
prompt này.

---

## 2. Nguyên tắc thiết kế (SSOT)

> **Push-only path** = entry dạng thư mục (kết thúc `/`) **có trong `pull_excludes`** và
> **không có trong `push_excludes`**.

Suy ra hoàn toàn từ 2 exclude list — không field mới, không hardcode, không danh sách thứ ba.
`.git/`, `.wrangler/` hay bất kỳ dir tương lai đều đi chung một đường.

Ba luật hành vi:

| # | Luật | Ý nghĩa |
|---|------|---------|
| R1 | **Transfer thuần exclude theo chiều** | Push đọc `push_excludes`, pull đọc `pull_excludes`. Xóa toàn bộ hardcode `.git`. Pull `--delete` không đụng dir bị pull-exclude (native rsync: excluded ⇒ không xóa phía nhận, trừ khi `--delete-excluded` — ta không dùng). |
| R2 | **Badge chỉ đếm file hai chiều** | Status check (cả 2 chiều) áp `push_excludes ∪ pull_excludes`. Push-only dir được "mang theo" khi push thật, nhưng **không phải "thay đổi"** → không thắp badge, không đếm. Diệt tận gốc badge ma từ `.git` churn. |
| R3 | **Deletion trong push-only dir: tự duyệt** | Mirror push: deletion nằm gọn trong push-only dir → tự xóa + ghi log, không hỏi. Dialog gõ-tên chỉ hiện cho deletion NGOÀI các dir đó; không còn deletion ngoài → không dialog. (Chỉ đạo đã chốt: "file đã ignore khi pull thì push không cần hỏi.") |

**Trade-off chấp nhận (ghi để audit sau):** vì R2, khi user chủ đích chỉ đổi nội dung `.git`
(commit local), badge PUSH không sáng — nhưng nút PUSH vẫn luôn mang `.git` theo, và badge GIT
(changed_count) đã phản ánh hoạt động repo. "Carried, not counted."

> **⚠️ ĐÍNH CHÍNH (2026-07-20):** vế *"badge GIT (`changed_count`) đã phản ánh hoạt động repo"* là
> **SAI**. `changed_count` = số dòng `git status --porcelain` = **chỉ thay đổi chưa commit**. Đúng
> kịch bản mà trade-off này nhắc tới (commit xong, push lên origin xong) thì porcelain rỗng →
> `changed_count = 0`, `status = "Clean"`, badge PUSH = 0. **Không còn tín hiệu nào cả**, trong khi
> `.git` phía remote đã cũ đi thật. Trade-off này vì vậy đắt hơn mức được ghi khi chốt plan. Xem §9.

---

## 3. Thay đổi chi tiết theo file

### 3.1 Rust — `src-tauri/src/sync.rs`

1. **`build_rsync_args`**: bỏ param `sync_git`; xóa block skip-exclude (`:325-327`) và
   force-exclude (`:331-333`). Excludes = thuần list theo chiều.
2. **Status check `rsync_change_files`** (R2): khi build args cho status, dùng
   **hợp nhất `push_excludes ∪ pull_excludes`** (dedup theo `trim()`) cho CẢ hai chiều —
   thay vì list theo chiều. Chỉ áp cho status check; push/pull thật vẫn theo chiều (R1).
   Bỏ `let sync_git = …` tại `:532`.
3. **`get_sync_delete_preview`**: bỏ `sync_git` (`:676`); args theo chiều bình thường (preview
   phải thấy deletion trong push-only dir để còn tự duyệt + log ở JS).
4. **Baseline `collect_local_files_with_mtime`**: thay hardcode `.git` (`:205-206`) bằng skip
   mọi rel-path nằm dưới **dir-entry** (kết thúc `/`) của `push_excludes ∪ pull_excludes` —
   match theo ranh giới component (`rel == "x"` hoặc `rel.starts_with("x/")`), KHÔNG match
   prefix trần (tránh `.wrangler` ăn nhầm `.wrangler-backup`). Glob entries (`*.log`) bỏ qua,
   không match — vô hại vì chúng không bao giờ xuất hiện trong change list để đối chiếu baseline.
   → Hàm nhận thêm tham số danh sách dir-excludes; `write_baseline` nhận từ project.
5. **`run_sync`/`run_sync_blocking`**: bỏ param `sync_git` xuyên suốt.

### 3.2 Rust — `src-tauri/src/projects.rs` + `git.rs`

- **Addendum (implementation correction, 2026-07-19):** giữ nguyên field `sync_git` trong
  `SyncProject` (KHÔNG xóa như dự kiến ban đầu) — lý do: `load_projects` deserialize thẳng vào
  struct đã gõ kiểu; nếu xóa field khỏi struct, giá trị cũ trên disk bị Serde âm thầm bỏ qua
  **trước khi** bao giờ tới JS, khiến migration §3.3 (đọc `p.sync_git` từ mảng do `load_projects`
  trả về) không bao giờ thấy giá trị thật — mọi project cũ sẽ rơi vào nhánh "no field" và mất đúng
  hành vi cần bảo toàn (project từng `sync_git=false` sẽ bị bật lại push `.git`). Field giữ lại
  với `#[serde(default = "default_true")]`, đánh dấu DEPRECATED, không còn được đọc bởi bất kỳ
  logic sync nào (`build_rsync_args`/`run_sync`/status-check/`get_sync_delete_preview` đều đã bỏ
  tham số này) — xóa hẳn ở một release sau khi migration đã chạy qua toàn bộ user base.
- `git.rs get_project_files`: đổi param `sync_git: bool` → `include_git_entry: bool` (JS tự tính
  từ exclude lists: `.git` tồn tại && `.git/` không nằm trong `push_excludes`). Logic giữ nguyên.

### 3.3 JS — migration một lần (`useProjectConfig.js`, chạy lúc load projects, gate bằng flag localStorage)

Với TỪNG project (Regression Guard: chỉ thêm/bớt đúng MỘT entry, không bao giờ rewrite list):

| Trạng thái cũ | Hành động |
|---|---|
| `sync_git === true` | Gỡ entry `.git/` khỏi `push_excludes` nếu có (giữ hành vi: push kèm .git) |
| `sync_git === false` hoặc `undefined` | Thêm `.git/` vào `push_excludes` nếu chưa có (giữ hành vi: không push .git) |
| Mọi project | Đảm bảo `.git/` có trong `pull_excludes` nếu thiếu (giữ hành vi hardcode `:331` từng bảo vệ chiều pull) |
| Sau cùng | `delete p.sync_git`; save; set flag migration |

Default mới (project tạo sau này): giữ nguyên default hiện tại — `pull_excludes` có `.git/`,
`push_excludes` không → `.git` mặc định là push-only. `.wrangler/` vẫn nằm cả 2 list (không sync);
ai muốn push-only thì tự gỡ khỏi `push_excludes` — giờ sẽ chạy mượt không prompt.

### 3.4 JS — `useSync.js` (R3)

- Bỏ `syncGit: project.sync_git` (`:126`).
- Helper `pushOnlyDirs(project)` = dir-entries (`/`-suffixed) trong `pull_excludes` mà không có
  trong `push_excludes`.
- Trong nhánh `isDeleteOp`: partition `deleteList` theo component-boundary match với
  `pushOnlyDirs`:
  - phần trong push-only → `appendLog` (`>>> Auto-approved N deletion(s) in push-only paths (.wrangler/, …)`), không hỏi;
  - phần còn lại → dialog gõ-tên như cũ; rỗng → không dialog.
- Giữ nguyên: preview-fail dialog (không partition được thì vẫn hỏi như cũ — an toàn trước).

### 3.5 UI

- `ProjectTable.vue:215-218`: xóa checkbox `.git` (Extreme Narrow: bớt 1 element khỏi hàng).
- `IntroModal.vue:41,52`: sửa copy — bỏ nhắc "toggle .git", thay bằng 1 câu ngắn về ngữ nghĩa
  exclude list / push-only (không thêm section mới).
- `ProjectConfigModal.vue`: không đổi UI; đây là nơi duy nhất điều khiển chiều sync.
- Caller của `get_project_files` truyền bool tính từ exclude lists (3.2).

### 3.6 Docs/version

- CHANGELOG (minor bump, dự kiến 1.13.0): ghi rõ **(a)** badge PUSH không còn đếm churn của
  push-only dirs — kể cả `.git` (diệt badge ma), **(b)** mirror-push tự duyệt deletion trong
  push-only dirs, **(c)** toggle `.git` bỏ — hành vi cũ được migration bảo toàn per-project,
  **(d)** điều được BẢO TOÀN: exclude lists của từng project giữ nguyên ngoài đúng một entry
  `.git/` (claim cho audit đối chiếu diff, theo Regression Guard).
- `Cargo.toml` bump cùng số cùng commit.

---

## 4. Những thứ KHÔNG làm (chống scope creep)

- Không thêm field/config/danh sách "push_only_paths" riêng — mọi thứ suy từ 2 exclude list.
- Không đụng special push (`-R`, file picker) — vốn bypass excludes by design, giữ nguyên.
- Không làm UI hiển thị "chiều sync per-path" trong config modal (ý tưởng second-order, YAGNI).
- Không sửa parser output rsync trong plan này — đó là bug độc lập, chỉ **verify** trên Mac (§6),
  nếu dính thì mở fix riêng.
- Không xử lý glob entries (`*.log`) như push-only — chỉ dir-entries (`/`-suffixed) mang ngữ
  nghĩa push-only.

---

## 5. Edge cases đã cân (severity-weighted)

| EC | Mức | Xử lý |
|---|---|---|
| `.wrangler` match nhầm `.wrangler-backup` | Cao | Component-boundary match (3.1.4, 3.4) |
| Migration phá exclude list custom | Cao | Chỉ thêm/bớt đúng 1 entry `.git/`; test ≥2 project khác trạng thái toggle (Regression Guard) |
| Project cũ từng cố ý sync `.git` HAI chiều | Thấp (không tồn tại thực tế) | Migration thêm `.git/` vào pull_excludes → mất pull `.git`; ghi CHANGELOG |
| User chỉ đổi `.git` (commit) → badge không sáng | Trung | Trade-off có chủ đích ("carried, not counted"), PUSH vẫn mang theo; ghi CHANGELOG |
| Baseline cũ còn chứa entry rác | Thấp | Vô hại — entry không xuất hiện trong change list thì không bao giờ được đối chiếu; baseline ghi đè sau mỗi sync đầy đủ |
| rsync cũ không hỗ trợ gì đó trong args | Thấp | Không thêm flag rsync mới nào — args chỉ bớt đi |

---

## 6. Verification trên Mac (bắt buộc trước khi release)

> **Cập nhật 2026-07-19 (phiên release trên Mac):** một phần §6 đã được **chứng minh bằng phân tích
> tĩnh + mô phỏng trên dữ liệu thật**, không cần thao tác GUI. Xem §6b trước khi làm tay — danh sách
> việc còn lại ngắn hơn nhiều so với bản gốc dưới đây.

1. **Regression Guard ≥2 entities**: 2 project, một toggle ON một OFF trước migration → sau
   migration hành vi push/pull từng project y hệt trước; exclude lists chỉ lệch đúng entry `.git/`.
2. **Badge ma**: mở app, KHÔNG sửa gì local, làm việc trên remote → badge PUSH phải đứng 0
   (trước đây sáng do `.git/index` churn). Chạy `git status` local thủ công → badge vẫn 0.
3. **`.wrangler` push-only** (gỡ khỏi `push_excludes` một project test): chạy `wrangler dev`
   local cho churn → badge 0; push mirror → không dialog, log ghi "Auto-approved N deletion(s)";
   remote mirror đúng local; pull → `.wrangler` local nguyên vẹn.
4. **Deletion ngoài push-only**: xóa 1 file `src/` local + có file rác `.wrangler` → dialog chỉ
   liệt kê file `src/`, phần `.wrangler` nằm trong log auto-approved.
5. **Nghi can parser openrsync** (§1.2): sửa 1 file trên remote, KHÔNG sửa local → xem push
   count; nếu >0, capture raw output `rsync -avzun --modify-window=2 …` để mở bug fix parser
   riêng (không nhét vào plan này).
6. `grep -n "sync_git" -r src/ src-tauri/` = 0 kết quả sau khi xong.

---

## 6b. Kết quả verify thực tế (2026-07-19, trên Mac)

### Đã xong bằng máy — không cần thao tác tay

**`cargo test`: 43/43 pass**, gồm đủ 14 test mới (`is_under_dir_exclude_*`, `union_excludes_*`).
Không lỗi compile, không assert sai → không phải sửa test lẫn code production.

**Migration idempotent (§6.1 + ca "mở app 2 lần") — chứng minh bằng code flow:**
mấu chốt là `skip_serializing_if = "Option::is_none"` dùng **chung một `Serialize` impl** cho cả
ghi đĩa (`save_projects`) lẫn payload IPC trả về JS (`load_projects`). Vì vậy:

| | Đĩa | IPC → JS | `hasOwnProperty` | Kết quả |
|---|---|---|---|---|
| Mở lần 1 | có `sync_git` | **có** key | true | migrate → `delete` → save → đĩa sạch key |
| Mở lần 2 | không có key | **bỏ** key | false | `continue` → `changed=false` → **`saveProjectsList()` không được gọi** |

Tức lần 2 file **không hề được mở ra ghi** — mạnh hơn điều kiện "nội dung không đổi" của checklist gốc.
Đã kiểm thêm: không còn đường ghi `projects.json` nào khác lúc khởi động (`refreshAll` /
`startBackgroundRefresh` chỉ đụng `projectRuntime` — ref riêng, không persist; vòng lặp trong
`loadData` ghi `stack` vào `projectRuntime` chứ không vào `p`). Mọi caller `saveProjectsList` còn lại
đều do người dùng chủ động bấm.

**Regression Guard ≥2 entity (§6.1) — thoả sẵn bằng dữ liệu thật, đã mô phỏng offline:**
`projects.json` thật có **17 project: 15 `sync_git=true`, 2 `false`**. Replay migration offline dự đoán:

- 15 project `true` → **không đổi exclude list một chữ nào** (`.git/` vốn đã vắng khỏi `push_excludes`
  và đã có trong `pull_excludes`)
- 2 project `false` (`tachnhac.com`, `aki-gateway-js`) → thêm đúng `.git/` vào `push_excludes`
- Cả 17 → xoá key `sync_git`

→ Hành vi push/pull của cả 17 project được bảo toàn. **Không có key lạ**: tập key trên đĩa khớp
tuyệt đối 21/21 với field của `SyncProject`, nên cú ghi đè đầu tiên không nuốt mất field nào
(bẫy "Serde fields + old JSON" trong CLAUDE.md — đã chủ động kiểm, không dính).

### Kết quả chạy thật trên Mac (2026-07-20, build `Aki-DevSync-v1.13.0.2318-uni.dmg`)

**Migration — khớp dự đoán tuyệt đối, 0 thay đổi ngoài dự kiến.** Diff `projects.json` sau lần mở
đầu vs backup tiền-migration, bỏ qua churn `last_sync_*`:

- 17/17 project: key `sync_git` biến mất ✅
- Đúng 2 project (`tachnhac.com`, `aki-gateway-js` — hai project từng `false`): `push_excludes`
  **+`.git/`**, không gỡ entry nào ✅
- 15 project còn lại: exclude list **không đổi một ký tự** ✅
- Không project nào có thay đổi ở field ngoài danh sách trên ✅

**Idempotent — xác nhận thực nghiệm.** Người dùng đã mở/thoát app nhiều lần trước khi kiểm. Nếu
migration chạy lại (failure mode §7), hai project OFF đã bị **gỡ mất** `.git/` khỏi `push_excludes`.
Chúng vẫn còn nguyên ⇒ migration không tái chạy. Khớp với chứng minh tĩnh ở trên.

**R2 — bằng chứng runtime trực tiếp** (sample tiến trình `rsync` của status check nền):

- Mọi status check **cả hai chiều** đều mang `--exclude=.git/`, kể cả **chiều push** của project mà
  `.git/` KHÔNG nằm trong `push_excludes` (`Aki-Dev-Sync`, `AkiNuxtCf`, `lamnhac.net`,
  `oscarfamily.vn`, `akiworkflow.com`…). Dòng exclude đó chỉ có thể đến từ **union** ⇒ R2 hoạt động,
  `.git` churn không thể thắp badge.
- **Dedup đúng**: `tachnhac.com` có `.git/` ở cả hai list → dòng lệnh chỉ hiện `--exclude=.git/` **một lần**.
- Không status check nào chứa `--delete`; args đúng dạng `-avzu --dry-run` tự dựng mới.

**Bẫy môi trường phát hiện lúc test** — xem mục 0 dưới đây, đáng nhớ cho mọi release sau.

### Còn lại phải làm tay (đã rút gọn)

0. **THOÁT HẲN app phiên bản cũ trước khi test** (quan sát thật 2026-07-19: bản 1.12.0 trong
   `/Applications` vẫn đang chạy và ghi `projects.json` giữa lúc build). Bắt buộc, vì 1.12.0 có
   `#[serde(default = "default_true")]` nên **luôn serialize `sync_git`** — chỉ cần nó ghi file một
   lần sau khi migration của 1.13.0 chạy là `"sync_git": true` **hồi sinh cho toàn bộ project**,
   tái tạo đúng failure mode §7 nhưng do hai phiên bản chạy chồng nhau. Diff sẽ lệch và dễ bị đọc
   nhầm thành "bản vá không ăn". Backup phải lấy **sau** khi đã thoát app cũ, nếu không backup cũng
   lệch. Kiểm bằng `ps aux | grep -i "aki dev sync"` trước khi bắt đầu.
1. **Ca "mở 2 lần"** rút về thao tác cơ học, không cần nhìn UI: backup `projects.json` → mở app →
   `diff` với backup (phải khớp **đúng** dự đoán ở trên, không dư dòng nào) → mở lần 2 → so `mtime`,
   phải **y nguyên**. Backup đã tạo sẵn tại `projects.json.pre-1.13.0-migration`.
2. **Badge ma** (§6.2): trên project có `.git/` ở trạng thái push-only (15/17 project), chạy
   `git status` vài lần cho git ghi lại `.git/index`, bấm refresh → badge PUSH phải đứng **0**.
3. **PUSH/PULL thật** — nhưng test theo **bảng ba trạng thái ở §8**, không test riêng `.git`.
4. §6.5 (nghi can parser openrsync) — giữ nguyên, độc lập với plan này.

---

## 7. Findings sau implement — đã sửa

- **Serde hồi sinh `sync_git`** (bug thật, nặng nhất): `save_projects` nhận `Vec<SyncProject>` nên
  `default_true` ghi lại `"sync_git": true` xuống đĩa ngay sau khi JS `delete` nó, trong khi cờ
  "đã migrate" lại nằm ở localStorage — mất cờ ⇒ migration chạy lại ⇒ gỡ `.git/` khỏi
  `push_excludes` ⇒ mọi project âm thầm push cả `.git`. Sửa: `Option<bool>` +
  `skip_serializing_if`, bỏ cờ localStorage, nhánh "không có field" thành **no-op tuyệt đối**
  (nếu vẫn `ensureEntry` thì mỗi lần load sẽ ép `.git/` trở lại `pull_excludes`, đè lựa chọn của
  user — regression khác).
- **`get_project_files`** chết (0 caller) — đã xoá. Sống sót qua review vì refactor đổi tên tham số
  khiến nó trông như "đã được đụng tới".
- **`trim_start_matches("deleting ")`** strip lặp → path sai lọt vào danh sách xác nhận xoá.
  Sửa sang `strip_prefix`.
- **Preset đè excludes** — audit nghi là lỗi, **thẩm định lại: không phải**. Preset ghi đúng shape
  push-only, UI đã ghi rõ "overwrites current excludes", người dùng chủ động bấm. Ghi lại để lần
  sau không raise lại.

## 8. Rủi ro tồn dư — chấp nhận có chủ đích

> **Chỉnh khung khái niệm (chỉ đạo 2026-07-19):** bản §8 gốc phát biểu rủi ro quanh `.git` là **sai
> trọng tâm**. `.git` **chỉ là một biểu hiện**; gốc rễ là **SSOT = cặp `push_excludes` /
> `pull_excludes`**. Mọi phát biểu về rủi ro, và mọi ca test, phải nói ở tầng cặp exclude list.

**Bảng ba trạng thái — đây mới là hợp đồng hành vi thật của tính năng:**

| Trạng thái của một dir trong cặp list | Hành vi |
|---|---|
| Có ở **cả hai** list | Không sync chiều nào (vd `.wrangler/` mặc định) |
| Chỉ có ở `pull_excludes` | **PUSH-ONLY**: đẩy lên, không kéo về, **không tính vào badge** (R2), **deletion tự duyệt không hỏi** (R3) |
| Không ở list nào | Sync hai chiều bình thường |

**Rủi ro phát biểu đúng:** đưa **bất kỳ** thư mục nào vào trạng thái push-only sẽ *lặng lẽ* gán thêm
cho nó hai thuộc tính ngoài "không kéo về" — churn không thắp badge, và deletion trong nó bị xoá
trên remote **không hỏi**. Hai thuộc tính này suy ra từ cặp exclude list, **không được ghi ở đâu
trong UI config**. `.git` chỉ tình cờ là thư mục mà mọi project đều sẵn ở trạng thái đó, nên là ca
dễ quan sát nhất — không phải là bản chất vấn đề.

Không vá bằng logic: mọi phương án vá đều làm sống lại đúng cái hộp thoại mà R3 sinh ra để giết.
Nếu muốn giảm rủi ro thì đúng chỗ là **một dòng chữ trong `ProjectConfigModal`** nói rõ ngữ nghĩa
push-only — vẫn hợp Extreme Narrow (không thêm element vào bảng), không đụng logic. Chưa làm ở
1.13.0; ghi lại đây làm ứng viên cho lần sau.

**Hệ quả cho cách test:** không test riêng `.git` lên/xuống. Lấy **một thư mục rác bất kỳ** trên một
project nháp, đẩy qua đủ ba trạng thái ở bảng trên, xác nhận hành vi khớp từng dòng.
- **"Carried, not counted"**: push-only dir đi theo push thật nhưng không tính là "changed" — chi
  phí đã chấp nhận khi chốt plan.
- **`--modify-window=2`** có thể bỏ sót thay đổi trong vòng 2 giây (che khoảng lệch mtime APFS vs
  ext4). Lý thuyết, chưa từng quan sát thấy.

**§6 (verify trên Mac) vẫn là điều kiện đóng plan.** Còn thiếu: `cargo test`; chạy app 2 lần liên
tiếp xác nhận `projects.json` không đổi ở lần 2 (tính idempotent của migration); mirror-push khi
`.git/` là push-only; đường dẫn chỉ-pull; `.wrangler/` không khớp nhầm `.wrangler-backup`.
*(→ đã xong, xem §6b.)*

---

## 9. Xét lại R2 sau release (2026-07-20) — mở, chưa quyết

Người dùng dùng thật 1.13.0 và phản ánh: sửa `.git` ở local (commit, push origin) → **badge không
hiện gì dù refresh**, nhưng bấm PUSH thì rsync đẩy nguyên một loạt file `.git`. **Hành vi này đúng
thiết kế** (R2 badge dùng union / R1 push đọc `push_excludes`), không phải bug. Nhưng nó phơi ra
hai chỗ lý lẽ hỏng, đã đính chính tại §1.2 và §2.

### 9.1 R2 là mảnh tháo rời được

Kiểm chứng phụ thuộc: **R3 không hề dựa vào R2.** R3 sống ở `useSync.js` (`pushOnlyDirs` +
partition `deleteList`) và ăn output của `get_sync_delete_preview`, vốn đã dùng exclude **theo
chiều** chứ không phải union. R2 chỉ tồn tại trong `rsync_change_files`. Gỡ R2 không đụng gì tới R3.

**Nếu đảo R2** (status check quay lại đọc exclude theo chiều), badge sẽ **y hệt 1.12** — kể cả con
số phồng lên hàng trăm object sau một commit. Nhưng 1.13 vẫn khác 1.12 ở:

| Vẫn khác 1.12 kể cả khi bỏ R2 | |
|---|---|
| **R3** | Hết hộp thoại gõ tên project cho deletion trong push-only dir ("replace là hiển nhiên") |
| **R1 / SSOT** | `sync_git` biến mất, hết hardcode `.git`; **mọi** dir đều có thể push-only, không riêng `.git` |
| **Baseline** | Áp cho mọi push-only dir thay vì hardcode `.git` |
| **Bug fix** | `strip_prefix` (path hỏng trong delete-preview) + serde hồi sinh `sync_git` |
| **UI** | Bớt một element mỗi hàng |

⇒ Bỏ R2 thì 1.13 = *badge của 1.12 + hộp thoại delete đã sửa + mô hình config sạch*.

### 9.2 Thế kẹt thật sự

Ba thứ không thể cùng thoả bằng **một** con số duy nhất:

1. Badge = "có code cần deploy" (ngữ nghĩa đúng, R2 cho cái này)
2. Có tín hiệu "remote đang giữ `.git` cũ" (1.12 vô tình cho cái này)
3. Không thêm element/trạng thái mới (Extreme Narrow — **chỉ đạo 2026-07-20: không đánh đổi, giữ
   nguyên số trạng thái**)

1.13.0 chọn (1)+(3), trả giá bằng (2). 1.12 chọn (2)+(3), trả giá bằng (1). **Không có phương án nào
lấy được cả ba** trong ràng buộc hiện tại.

### 9.3 Trạng thái quyết định

**Giữ nguyên 1.13.0, không vá, không thêm trạng thái** (chỉ đạo 2026-07-20). Phương án "trạng thái
phụ trên nút PUSH (viền/màu)" đã được cân và **bác** vì phức tạp hoá. Phương án "mở rộng nút Git"
cũng **bác** — sai domain: nút Git là quan hệ local↔GitHub (`Ahead` = hơn origin), không phải
local↔host rsync; nhồi thêm nghĩa vào đó làm hỏng nghĩa sẵn có.

Nếu sau này thật sự cần (2), cách đúng **không phải mtime** (mtime luôn lệch, sẽ thành nhiễu vĩnh
viễn) mà là **so `git rev-parse HEAD` local vs remote** — chính xác, im lặng khi trùng. Đó là tính
năng mới, tốn một round-trip SSH, không phải bug fix. Ứng viên tương lai, chưa lên lịch.
