# Test case — dóng flow theo code (1.20.0)

> **Phương pháp:** 98 use case do người dùng thật sẽ gặp, mỗi case được dóng ngược vào code —
> UI → composable/store → IPC → Rust → về — chứ không chạy app. Mọi khẳng định kèm `file:line`.
> Đây là **static trace**, không thay thế kiểm thử runtime trên Mac (xem `docs/plan/done/1.20.0-terminal-and-remote-sync.md` §6).
>
> **Ngày:** 2026-07-26 · **Base:** commit `f67eaa0` (release 1.20.0, chưa tag)
> **Phạm vi:** A sync/project (18) · B usage monitor (20) · C remote control (20) · D terminal (20) · E vỏ app (20)

## Tổng kết

| Cụm | Case | 🔴 | 🟠 | Sạch |
|---|---|---|---|---|
| A — Vòng đời project & Sync | 18 | 1 | 14 | — |
| B — Usage Monitor | 20 | 1 | 8 | B3, B4, B8, B9, B11, B18 |
| C — Remote Control | 20 | 2 | 8 | C12, C13, C14, C19 |
| D — In-App Terminal | 20 | 1 | 8 | D1, D3, D11, D17, D18 |
| E — Vỏ app & công cụ | 20 | 1 | 13 | E4, E10, E13, E20 |
| **Tổng** | **98** | **6** | **51** | |

### 6 phát hiện 🔴

| # | Vấn đề | Vị trí | Đã xác minh trực tiếp |
|---|---|---|---|
| 1 | Companion gọi thẳng `run_sync`/`run_project_command` không qua allowlist → bỏ qua toàn bộ xác nhận `--delete` | `src/services/hostInvoke.js:23-37` | Đúng, nhưng **đã tuyên bố là deferred có chủ ý** trong CHANGELOG 1.20.0 mục *Security note* — xếp lại thành nợ đã biết, không phải phát hiện mới |
| 2 | Restart app hoặc bấm Off **xoá token của mọi phone** — close code 4001 dùng chung cho "tắt" và "token sai" | `web_server.rs:90,494-505` + `useCompanionPairing.js:31-33` | ✅ xác minh: `enabled: AtomicBool::new(false)` không persist; `useCompanionPairing.js:31` gọi `clearDeviceToken()` trên 4001. Trái lời hứa "reconnects silently across app restarts… until revoked" |
| 3 | Peer trong tailnet chiếm được vai `host` không cần token khi bật HTTPS | `web_server.rs:486` + `:886` | ✅ xác minh: `tailscale serve --bg http://127.0.0.1:PORT` làm mọi kết nối tailnet đến từ loopback, mà guard chỉ kiểm `is_loopback()` |
| 4 | Terminal: `alive` không đồng bộ sau RESTART → chạm phím trên phone **giết shell Mac vừa tạo** + xoá scrollback | `pty.rs:313-324`, `ptyBridge.js:53-55`, `usePtyTerminal.js:166-171,214-217` | Chưa xác minh trực tiếp |
| 5 | Cache AG: cùng email có 2 `sourceType` (IDE + CLI) trên cùng host → xoá nhầm bản ghi và **hiện số sai** | `agUsageCache.js:151-152,179-182,203-206` | Chưa xác minh trực tiếp — đúng lớp Regression Guard, ưu tiên kiểm lại |
| 6 | Project thêm giữa phiên không bao giờ có icon cho tới khi restart | `App.vue:79`, `remoteActions.js:166-201`, `projects.rs:145-152` | Chưa xác minh trực tiếp |

### Ghi nhận làm đúng (đừng sửa nhầm)

- **NEVER BLOCK THE UI**: audit toàn bộ `#[tauri::command]` — không lệnh nào chạy subprocess/network mà thiếu `spawn_blocking`. Read/flusher thread của PTY dùng `std::thread` thô là **cố ý đúng**.
- **Capabilities**: 13/13 window API đều đã cấp quyền trong `capabilities/default.json`.
- **Regression Guard** ở cụm A: `removeProject`, `applyTaskEdit`, `reorderProjects` đều đúng phạm vi một-entity.
- Bảng màu statusline khớp 100% giữa `statuslineColors.js` / `statusline.rs:141-152` / `statusline-unified.sh:187-193`.
- Bug "terminal render bé ở góc" (D1) và CLEAR đa màn hình (D11) đã sửa đủ tầng.
- Base64 transport của PTY binary-safe end-to-end; UTF-8 cắt biên do xterm tự ghép (D3).
- Generation counter chống race RESTART hoạt động — chỉ hở đúng nhánh `append_scrollback`.

---

## Cụm A — Vòng đời project & Sync

> Dóng flow theo code thật (đọc file, không suy diễn). Mọi khẳng định kèm `file:line`.
> Mức độ: 🔴 chặn · 🟠 nghiêm trọng · 🟡 nhỏ.

**Bối cảnh chung cần nhớ khi đọc mọi case:**
- Mọi hành động ghi đều đi qua `action(key, fn)` (`src/services/action.js:29-41`). Trên HOST `action(fn) === fn` (line 36) → gọi trực tiếp. Trên COMPANION (điện thoại) nó chỉ gửi intent và **trả về `undefined` ngay** (line 37-40).
- Dialog quyết định là **state được mirror** (`src/store/dialogStore.js:20-67`), render bởi `src/components/DialogHost.vue:26-99`, ai bấm trước thắng.
- `invoke()` trên companion là RPC qua WS (`src/utils/tauri.js:17-25`), host trả lời tại `src/services/hostInvoke.js:23-37` — **không có allowlist lệnh** (xem phát hiện A3-3).

---

### A1. Tạo project mới (local path + remote path + SSH host + lưu)

**Flow**
1. `src/components/ProjectTable.vue:11` nút NEW → `handleCreateNew()` (`ProjectTable.vue:322-324`) → `createNewProject(sshHosts)`.
2. `src/composables/useProjectConfig.js:197-236`: mở native folder picker (`@tauri-apps/plugin-dialog`), dựng object mặc định (`useProjectConfig.js:214-233`), rồi `openConfig(p)`.
3. `openConfig` (`useProjectConfig.js:145-157`) clone nông `{...project}` + clone riêng `hooks`, `pull_excludes`, `push_excludes` → set `editingProject`, `showConfigModal = true`.
4. Modal `src/components/modals/ProjectConfigModal.vue:10-151` bind `v-model` thẳng vào `editingProject`.
5. Save → `saveConfig()` (`useProjectConfig.js:164-195`) → normalize `production_url` (167-174) → `applyProjectConfig({...editingProject.value})` (`src/store/remoteActions.js:166-203`) → push vào `projects.value` (line 196), `saveProjectsList()` (line 199) → `invoke('save_projects')` → `src-tauri/src/projects.rs:145-152` ghi `projects.json`.

**Mong đợi**: project mới có đủ 3 trường, hợp lệ, xuất hiện trong list, được persist.
**Thực tế**: đúng, nhưng **không có bất kỳ validation nào ở tầng JS** trước khi ghi.

**Phát hiện**
- 🟠 **`local_path` rỗng → PUSH đẩy toàn bộ `/` lên remote.** `saveConfig` (`useProjectConfig.js:164-195`) không kiểm tra trường nào rỗng. `validate_project` (`src-tauri/src/projects.rs:101-108`) chỉ chặn `..`, ký tự control, và `remote_host` rỗng — **không chặn `local_path` rỗng**. Trong `run_sync_blocking`, `let local = format!("{}/", project.local_path.trim_end_matches('/'))` (`src-tauri/src/sync.rs:418`) → với `local_path = ""` cho ra `"/"`. PUSH khi đó là `rsync -avz / host:remote/` — nếu `delete_on_push` bật thì còn `--delete` trên remote (`sync.rs:371-373`). Người dùng chỉ cần xoá trắng ô "Local Path (Absolute)" (`ProjectConfigModal.vue:20`) là chạm được. PULL thì an toàn hơn vì `create_dir_all("")` lỗi (`sync.rs:440-441`).
- 🟠 **Project mới có `epoch: 0`, phá vỡ bất biến "project sống luôn có epoch ≥ 1".** `applyProjectConfig` nhánh `isNew` set `epoch: 0` (`remoteActions.js:193`). `beginRefresh` dùng `current?.epoch ?? 1` (`src/store/projectStore.js:70`) → vì entry đã tồn tại với `epoch = 0`, `?? ` không kích hoạt, epoch vẫn 0. Trong khi `currentEpoch` trả 0 để báo "project đã bị xoá" (`projectStore.js:113-115`). Hệ quả: nếu xoá project mới tạo trong lúc `check_sync_status` đang bay, `currentEpoch(id) === epoch` (0 === 0) → kết quả cũ **vẫn được ghi**, làm sống lại `projectRuntime[id]` cho project đã xoá (`src/composables/useSyncStatus.js:19,47-53,58`). Đối chiếu `loadData` làm đúng: `epoch: (prev?.epoch ?? 0) + 1` ≥ 1 (`useProjectConfig.js:86`).
- 🟡 **Không có SSH host nào → `remote_host = "localhost"` nhưng `<select>` không có option đó.** `createNewProject` set `sshHosts.value[0] || "localhost"` (`useProjectConfig.js:219`), còn select chỉ render từ `sshHosts` (`ProjectConfigModal.vue:14-16`). Select hiện trống, người dùng tưởng chưa chọn, nhưng giá trị `"localhost"` vẫn được lưu.
- 🟡 **Không kiểm tra `local_path` có tồn tại thật không.** Chỉ là `<input type="text">` tự do (`ProjectConfigModal.vue:20`). Sai đường dẫn → lỗi chỉ lộ ra khi rsync chạy.
- 🟡 `id = "project-" + Date.now()` (`useProjectConfig.js:212`) — trùng nếu tạo 2 project trong cùng 1 mili-giây. Thực tế gần như không xảy ra.

---

### A2. PUSH thường (không `--delete`), project bình thường

**Flow**
1. `ProjectTable.vue:246` `@click="requestSync(p.id, 'push')"` → `remoteActions.js:43-50` → `byId(id)` → `startSync(project, 'push')`.
2. `src/composables/useSync.js:40-252`: guard `syncCheckEnabled` (41-44), guard `syncing` (45-48), set `syncing: true` (50), mở LOG panel (54-64).
3. `isDeleteOp` = false (`useSync.js:66-67`) → bỏ qua toàn bộ khối confirm.
4. `invoke("run_sync", {...})` (`useSync.js:204-209`) → `src-tauri/src/sync.rs:384-399` — **có `spawn_blocking`, đúng quy tắc NEVER BLOCK THE UI**.
5. `run_sync_blocking` (`sync.rs:401-514`): pre-hook (416) → `ssh host mkdir -p` (427-437) → `build_rsync_args` `-avzu` + `--exclude=` (350-377) → `spawn_and_stream` (495, stdin null tại `sync.rs:76-77`) → `write_baseline` (499-507) → post-hook (509).
6. Về JS: cập nhật `last_sync_*` (`useSync.js:210-213`), `saveProjectsList()`, xoá badge push (217-233), auto-đóng log sau 1.5s (235-240), `finally` set `syncing:false` (250).

**Mong đợi vs thực tế**: khớp. Streaming log qua event `sync-log` (`sync.rs:41-63` → `useLogs.js:78-85`) hoạt động đúng.

**Phát hiện**
- 🟠 **`ssh host mkdir -p <remote_dir>` không quote → hỏng với remote path có dấu cách.** `sync.rs:427-430` truyền `["mkdir", "-p", &remote_dir]` làm các argv riêng; `ssh` **nối chúng bằng dấu cách rồi giao cho shell remote parse lại**. Với `remote_path = "~/my app"` → `expand_remote_tilde` cho `$HOME/my app` (`sync.rs:534-542`) → remote chạy `mkdir -p $HOME/my app` → tạo **2 thư mục** `$HOME/my` và `app`, rồi rsync đẩy vào đường dẫn thứ ba. Đây là chỗ duy nhất trong PUSH đi qua shell remote mà không quote. (Đối chiếu: `git.rs:184,193` có quote `cd "{safe_remote}"`.)
- 🟡 `last_sync_status` được ghi (`useSync.js:213,246`) nhưng **không được đọc ở đâu trong UI** — grep toàn `src/` chỉ ra 3 chỗ ghi, 0 chỗ đọc. PUSH thất bại không để lại dấu vết nào trên hàng project (`ProjectTable.vue:126-132` chỉ đọc `last_sync_action`/`last_sync_time`/`last_sync_host`).
- 🟡 `validate_project` không kiểm tra charset của `remote_host` khi chạy `run_sync` (`projects.rs:101-108`), trong khi `open_remote_subprocess` thì có (`src-tauri/src/system.rs:28-37`). Rủi ro thấp vì host đến từ `<select>` parse ssh config, nhưng là bất đối xứng ở ranh giới hệ thống.

---

### A3. PUSH với `--delete` → dialog gõ tên project

**Flow**
1. `requestSync(p.id,'push')` → `startSync`. `isDeleteOp = !isDryRun && specificPaths.length===0 && project.delete_on_push` (`useSync.js:66-67`).
2. `invoke('get_sync_delete_preview')` (`useSync.js:81`) → `sync.rs:702-750` (**có `spawn_blocking`**, line 708) → chạy rsync dry-run `--delete`, lọc dòng `deleting ` (733-744).
3. Lọc artifact `REPORT.html` (`useSync.js:114-141`), lọc push-only dirs (143-153).
4. `askConfirm({kind:'typed', requireText: project.name, ...})` (`useSync.js:164-180`) → `dialogStore.js:39-45` → `DialogHost.vue:61-76` (gate `preConfirm` phía client) → người dùng trả lời → `resolveDialog` (`dialogStore.js:58-67`).
5. **Kiểm tra thẩm quyền trên host**: `answer.typed === project.name` (`useSync.js:182`). Sai → `abortSync()` (183-186).

**Ba kịch bản**
- *Gõ SAI tên*: `preConfirm` (`DialogHost.vue:69-75`) chặn tại chỗ, không đóng dialog. Kể cả bị bypass, `useSync.js:182` vẫn chặn → `abortSync()`. ✅ Đúng.
- *Gõ ĐÚNG tên*: `typedOk = true` → chạy tiếp `run_sync` với `--delete` (`sync.rs:371-373`). ✅
- *Bấm huỷ*: `result.isConfirmed = false` → `{confirmed:false}` (`DialogHost.vue:93`) → `abortSync()` khôi phục `syncing`, `activeLogProjectId`, `isLogExpanded`, `activePanel` (`useSync.js:69-74`). ✅

**Phát hiện**
- 🟠 **A3-1: Danh sách file bị xoá được nhúng vào HTML dialog KHÔNG escape.** `useSync.js:157` `const fullFileList = deleteList.map(f => \`  ${f}\`).join('\n')`, nhúng thẳng vào `html` tại `useSync.js:170`. So sánh với dialog SELECT ngay bên dưới **có** dùng `escHtml` (`useSync.js:328-330`, hàm tại 364-370). Một file tên chứa `</pre><div>…` (hoàn toàn hợp lệ trên ext4/APFS) có thể che giấu phần còn lại của danh sách, hoặc chèn nội dung giả mạo vào chính cái dialog xác nhận thao tác phá huỷ dữ liệu. Đây là dialog bảo vệ cuối cùng trước khi xoá vĩnh viễn → mức nghiêm trọng.
- 🟠 **A3-2: `REPORT.html` bị auto-approve xoá khi SSH lỗi.** `useSync.js:118-130`: nếu `get_file_conflict_info` chạy được nhưng SSH bên trong thất bại, hàm Rust **nuốt lỗi và trả về mảng với `remote_mtime = 0`** (`src-tauri/src/git.rs:199-218` — `if let Ok(out) { if out.status.success() {...} }`, không có nhánh else). `destMtime(f)` khi đó = `0`, mà `0 <= lastSync` → file rơi vào `staleArtifacts` → **tự động duyệt xoá** (`useSync.js:130,135`). Comment ở `useSync.js:126` nói "couldn't verify - treat as fresh, ask" nhưng nhánh đó chỉ chạy khi *không tìm thấy entry*, còn ở đây entry luôn tồn tại (được khởi tạo sẵn tại `git.rs:146-167`).
- 🔴 **A3-3: Companion bypass được toàn bộ dialog xác nhận `--delete`.** `src/services/hostInvoke.js:23-37` gọi `invoke(cmd, args)` với `cmd`/`args` **lấy nguyên từ frame của companion, không có allowlist**. Một companion đã pair có thể gửi thẳng `{t:'invoke', cmd:'run_sync', args:{project:{…delete_on_push:true…}, direction:'push', dryRun:false, specificPaths:[]}}` — `run_sync` (`sync.rs:384`) là lệnh Tauri công khai, chỉ chạy `validate_project` (traversal/control chars), **không hề biết đến bước gõ tên project**. Toàn bộ khối bảo vệ ở `useSync.js:76-189` nằm ở tầng JS nên bị đi vòng hoàn toàn. Cùng cơ chế đó cũng gọi được `run_project_command` (shell tuỳ ý trong Terminal, `system.rs:582-589`) và `save_projects`. Pairing (`PairingGate.vue`) là rào duy nhất.
- 🟡 `projectLogs.value[project.id] = []` xoá log cũ **trước** khi hỏi (`useSync.js:64`), nên bấm Huỷ vẫn mất log trước đó — `abortSync` chỉ khôi phục trạng thái panel (`useSync.js:69-74`), không khôi phục nội dung.
- 🟡 Có khoảng trống thời gian giữa lúc chụp `deleteList` (`useSync.js:81`) và lúc rsync thật chạy (`useSync.js:204`) — người dùng có thể gõ tên chậm vài phút; danh sách file thực bị xoá lúc đó có thể khác với danh sách đã duyệt.

---

### A4. PULL với `--delete`

**Flow**: giống A3, nhánh `direction === 'pull' && project.delete_on_pull` (`useSync.js:67`). `get_sync_delete_preview` với `is_push=false` → `src = remote`, `dest = local` (`sync.rs:709-717`) → danh sách là **file LOCAL sẽ bị xoá**. `build_rsync_args` với `is_mirror` → dùng `-avz` (bỏ `-u`) + `--delete` (`sync.rs:350-351,371-373`).

**Mong đợi vs thực tế**: khớp. Lưu ý đúng: khối auto-approve push-only dirs chỉ áp cho `direction === 'push'` (`useSync.js:143`), nên PULL không tự duyệt xoá `.git/` — và `.git/` nằm trong `pull_excludes` mặc định nên rsync cũng không đụng tới (`sync.rs:364-369`).

**Phát hiện**
- 🟠 Kế thừa toàn bộ A3-1 (HTML injection), A3-2 (auto-approve khi SSH lỗi) và A3-3 (companion bypass) — cùng code path, `direction` chỉ là biến.
- 🟡 **Project mới mặc định `delete_on_pull: true`** (`useProjectConfig.js:229`), tức PULL đầu tiên của project mới là thao tác mirror phá huỷ file local. Được giảm nhẹ bởi `dry_run: true` mặc định (`useProjectConfig.js:228`) — nhưng ngay khi người dùng tắt DRY (một cú click ở `ProjectTable.vue:259`) thì PULL trở thành mirror. Bất đối xứng với `delete_on_push: false` (line 230).
- 🟡 `-u` bị bỏ ở chế độ mirror (`sync.rs:350-351`) là **cố ý và có lý do được ghi rõ** (`sync.rs:345-352`) — không phải lỗi, ghi ra đây để khỏi bị hiểu nhầm khi đọc log lệnh.

---

### A5. DRY RUN trước khi push

**Flow**: toggle DRY (`ProjectTable.vue:259`) → `setDryRun(p.id, checked)` (`remoteActions.js:104-109`) → `project.dry_run = !!value` + `saveProjectsList()`. PUSH → `isDryRun = project.dry_run` (`useSync.js:51`) → `isDeleteOp` = false vì `!isDryRun` sai (`useSync.js:66`) → **bỏ qua confirm** → `run_sync(dryRun: true)` → `--dry-run` (`sync.rs:353-355`).

**Mong đợi vs thực tế**: khớp. Dry run không ghi baseline (`sync.rs:499`), không chạy hook (`sync.rs:124-127`), `last_sync_action` có hậu tố `" (Dry)"` (`useSync.js:210`).

**Phát hiện**
- 🟡 Dùng `:checked` + `@change` thay `v-model` (`ProjectTable.vue:259`) là đúng cho companion. Nhưng trên companion `setDryRun` trả `undefined` ngay (`action.js:37-40`) → checkbox DOM đã đổi trạng thái, phải đợi mirror về mới đúng; nếu host từ chối (project vừa bị xoá, `remoteActions.js:106`) checkbox sẽ kẹt ở trạng thái sai cho tới lần mirror tiếp theo.
- Không phát hiện vấn đề nào khác ở case này.

---

### A6. Preview (dry-run) THẤT BẠI → prompt "continue anyway"

**Flow**
1. `try { deleteList = await invoke('get_sync_delete_preview', …) } catch { previewFailed = true }` (`useSync.js:80-84`).
2. `askConfirm({kind:'confirm', …'Vẫn tiếp tục (nguy hiểm)'})` (`useSync.js:90-99`).
3. Từ chối → `abortSync()` (100-103). Đồng ý → chạy tiếp.

**Mong đợi vs thực tế**: khớp — nhưng có một hệ quả logic cần chỉ rõ.

**Phát hiện**
- 🟠 **Đồng ý "continue anyway" sẽ BỎ QUA luôn cả bước gõ tên project.** Khi `previewFailed = true`, `deleteList` vẫn là `[]` (khởi tạo tại `useSync.js:78`), nên cả ba khối `if (deleteList.length > 0)` (`useSync.js:106, 143, 155`) đều không chạy → dialog typed-confirm không bao giờ xuất hiện. Kết quả: thao tác **nguy hiểm hơn** (không biết file nào sẽ mất) lại có **rào chắn yếu hơn** (một click) so với thao tác biết rõ danh sách (phải gõ đúng tên). Nên đảo lại: preview fail thì càng phải yêu cầu gõ tên.
- 🟡 Trong thực tế `get_sync_delete_preview` fail hầu như luôn đồng nghĩa SSH chết, nên `run_sync` ngay sau đó cũng fail → thiệt hại thực tế thường bằng 0. Nhưng preview cũng fail khi rsync trả non-zero vì lý do khác (`sync.rs:727-730`, ví dụ mã 23 partial transfer) — lúc đó kết nối vẫn sống và `--delete` vẫn chạy thật.

---

### A7. "Upload (select files)" — native picker + dialog "Ghi đè & Push"

**Flow**
1. `ProjectTable.vue:214` `@click="… requestSelectPush(p.id)"` → `remoteActions.js:57-64` → `openSelectDialog(project)` (`useSync.js:261-362`).
2. Native picker `openDialog({multiple:true, defaultPath: local_path})` (`useSync.js:262-268`).
3. Chuyển abs → rel theo `localBase` (`useSync.js:279-288`); file ngoài project bị loại + Toast cảnh báo (290-295).
4. `invoke('get_file_conflict_info', …)` (`useSync.js:303-308`) → `git.rs:136-222` (**có `spawn_blocking`**, line 142).
5. Lọc conflict (`useSync.js:309-317`) → nếu có, `askConfirm` bảng so sánh mtime, **có escape HTML** (`useSync.js:326-357`).
6. Đồng ý → `startSync(project, 'push', relPaths)` (`useSync.js:361`) → `sync.rs:357-362` dùng `-R` + `current_dir(local_path)` (`sync.rs:491-493`).

**Phát hiện**
- 🟠 **A7-1: SSH lỗi → KHÔNG có dialog cảnh báo, push vẫn chạy và ghi đè im lặng.** `get_file_conflict_info` **không bao giờ trả Err khi SSH thất bại**: `git.rs:195-218` bọc trong `if let Ok(out) { if out.status.success() { … } }`, không có else, cuối cùng `Ok(results)` (line 220) với mọi `remote_exists = false`. Nên nhánh `catch` ở `useSync.js:318-322` ("Không thể kiểm tra conflict với remote - hủy push") **chết code cho trường hợp SSH chết** — nó chỉ bắt được lỗi ở tầng IPC. Kết quả: `conflicts = []` → không hỏi gì → `startSync` push đè lên toàn bộ file remote. Đây chính là kịch bản mà dialog "Ghi đè & Push" sinh ra để ngăn.
- 🟠 **A7-2: Với DRY bật, "Upload (select files)" không truyền gì cả — trái với tooltip.** Tooltip ở `ProjectTable.vue:215` ghi rõ *"unaffected by the DRY toggle"*. Nhưng `startSync` set `isDryRun = project.dry_run` (`useSync.js:51`) **không xét `specificPaths`**, rồi truyền `dryRun: isDryRun` xuống (`useSync.js:207`) → `build_rsync_args` thêm `--dry-run` (`sync.rs:353-355`) kể cả ở nhánh `specific_paths` (357-362). Người dùng chọn file, xác nhận ghi đè, thấy "Sync complete" — nhưng **không có byte nào được truyền**. `last_sync_action` mới là chỗ duy nhất lộ ra ("PUSH SPECIAL (Dry)", `useSync.js:210`).
- 🟡 **A7-3: `stat` lỗi trên remote → file tồn tại nhưng bị coi là không tồn tại.** Script dùng `stat -c '%Y'` (GNU, `git.rs:189`). Trên remote BSD/macOS, `stat -c` fail → `$(…)` rỗng → dòng thành `STAT  file` → `mtime_str` rỗng → `parse::<i64>()` fail (`git.rs:206`) → `remote_exists` giữ `false` → không cảnh báo conflict. Cùng lý do làm A12 không bao giờ kéo được bản mới.
- 🟡 `validate_specific_paths` chặn mọi chuỗi chứa `..` (`sync.rs:142-147` → `projects.rs:91-93`), nên file tên hợp lệ như `notes..md` bị từ chối push với thông báo "directory traversal not allowed".
- 🟡 Không kiểm tra `syncCheckEnabled` trước khi mở picker — người dùng chọn xong file, `startSync` mới báo "Sync check is off" (`useSync.js:41-44`). Thực tế mục này bị ẩn khi sync check tắt (`ProjectTable.vue:193`), nên chỉ chạm được qua companion/race.

---

### A8. Sync đang chạy thì bấm huỷ giữa chừng

**Flow**: Không tồn tại. Grep `cancel|abort|kill` trong `useSync.js` + `sync.rs` chỉ ra `abortSync` (`useSync.js:69-74`) — hàm này chỉ dùng cho **huỷ ở dialog trước khi rsync chạy**, không phải huỷ giữa chừng. Trong lúc sync, `fieldset :disabled` vô hiệu hoá PUSH/DRY/PULL (`ProjectTable.vue:235`), nút gear cũng bị disable (`ProjectTable.vue:287`).

**Mong đợi**: có cách dừng một rsync đang chạy.
**Thực tế**: **không có**. `spawn_and_stream` (`sync.rs:67-95`) giữ `child`, `join()` hai thread đọc stream rồi `child.wait()` — không lưu handle ra ngoài, không có registry PID, không có lệnh Tauri nào để kill.

**Phát hiện**
- 🟠 **Không thể huỷ sync.** Một PUSH `--delete` bấm nhầm là **không thể dừng lại** bằng UI; cách duy nhất là kill cả app. Đối chiếu: in-app terminal thì có `pty_kill` và cả hook dọn dẹp lúc thoát (`lib.rs:126,134-142`) — nghĩa là kiến trúc đã có tiền lệ, chỉ riêng rsync là không có.
- 🟠 **Thoát app giữa chừng để lại rsync/ssh mồ côi.** Hook `RunEvent::Exit` chỉ gọi `pty::shutdown()` (`lib.rs:139-141`), không đụng tới process rsync/ssh do `spawn_and_stream` sinh ra. Chúng thành con của init và tiếp tục truyền/xoá file remote sau khi app đã đóng — đúng cái vấn đề mà comment `lib.rs:135-138` mô tả cho terminal, nhưng chưa áp cho sync.

---

### A9. Hai project cùng sync một lúc

**Flow**: `startSync` chỉ chặn theo từng project: `if (projectRuntime.value[project.id]?.syncing)` (`useSync.js:45-48`). Không có khoá toàn cục. `anySyncing` (`projectStore.js:49-51`) chỉ dùng để disable nút NEW (`ProjectTable.vue:12`).

**Mong đợi**: hoặc chặn, hoặc chạy song song sạch sẽ.
**Thực tế**: chạy song song, `run_sync` mỗi cái một `spawn_blocking` riêng (`sync.rs:394-398`). Log tách theo `project_id` (`sync.rs:36-43`), `projectLogs` tách theo id (`useLogs.js:26-32`) → không trộn nội dung.

**Phát hiện**
- 🟡 **Panel log bị "cướp".** `startSync` luôn ép `activeLogProjectId.value = project.id` + `activePanel = 'log'` (`useSync.js:58-62`). Bắt đầu sync project B khi đang xem log project A → màn hình nhảy sang B, không có cách nào quay lại mà không bấm tay.
- 🟡 **Khôi phục trạng thái log sai khi huỷ lồng nhau.** Sync B chụp `prevLogProjectId = A` (`useSync.js:54`); nếu B bị huỷ ở dialog sau khi A đã kết thúc và tự đóng panel (`useSync.js:236-239`), `abortSync` khôi phục về A (`useSync.js:71`) → panel mở lại log của một project không còn chạy gì.
- 🟡 Hai `saveProjectsList()` chạy chồng (`useSync.js:214`) đều ghi cả mảng `projects.value` — an toàn vì mutation nằm trên chính object sống, nhưng là hai lần ghi file toàn phần cạnh tranh không có khoá (`projects.rs:145-152` dùng `fs::write`, không atomic-rename). Mất điện đúng lúc → `projects.json` cụt, `load_projects` sẽ báo "corrupt" (`projects.rs:138-139`) và **trả Err** → `loadData` rơi vào catch (`useProjectConfig.js:118-121`), người dùng mất toàn bộ danh sách project mà không có backup.
- Không phát hiện vấn đề tranh chấp dữ liệu giữa hai rsync khác project.

---

### A10. Sync khi SSH host đã bị xoá khỏi config → dialog chọn host thay thế

**Flow**
1. Kích hoạt **không phải** từ nút sync mà từ luồng lưu SSH config: `useSsh.js` → `applySshHostsChange()` (`remoteActions.js:253-323`).
2. Đọc lại `get_ssh_hosts`, tính `missingHosts`/`addedHosts` (`remoteActions.js:265-269`).
3. Nếu đúng 1 mất + 1 thêm → **tự động migrate không hỏi** (`remoteActions.js:274-283`).
4. Ngược lại → vòng lặp `askConfirm({kind:'select', …})` cho từng host mất (`remoteActions.js:285-315`), `DialogHost.vue:77-81` render `input:'select'`.
5. `needsSave` → `saveProjectsList()` (`remoteActions.js:317-322`).

**Mong đợi**: bấm PUSH với host không tồn tại phải được cảnh báo.
**Thực tế**: **không có kiểm tra nào ở đường sync**. `startSync` không đối chiếu `project.remote_host` với `sshHosts`; `validate_project` chỉ chặn chuỗi rỗng (`projects.rs:104-106`). Sync sẽ chạy và fail ở tầng ssh với thông báo thô trong log.

**Phát hiện**
- 🟠 **Rename host trong ssh config = tự động repoint mọi project, không hỏi.** `remoteActions.js:274-283`: heuristic "1 mất + 1 thêm" gán toàn bộ project của host cũ sang host mới rồi chỉ Toast báo sau (line 282). Nhưng "1 mất + 1 thêm" cũng đúng khi người dùng **xoá host A và thêm host B hoàn toàn không liên quan** trong cùng một lần sửa file. Khi đó mọi project đang trỏ vào A âm thầm chuyển sang B — và lần PUSH `--delete` kế tiếp sẽ mirror lên sai máy chủ. Không có bước xác nhận, không thể undo (project đã được ghi đĩa tại line 318).
- 🟡 Sau khi đổi `remote_host` (`remoteActions.js:279` và `310`), **không gọi `bumpEpoch`** — trái với đường `applyProjectConfig` vốn bump khi identity đổi (`remoteActions.js:176-186`). Kết quả `check_sync_status` đang bay với host cũ vẫn được ghi nhận là hợp lệ (`useSyncStatus.js:19`), badge push/pull hiển thị số đo từ host cũ.
- 🟡 Người dùng bấm "Skip" (`remoteActions.js:305`) → project giữ host chết vô thời hạn, không có chỉ báo trực quan nào trên hàng project.

---

### A11. Mạng đứt giữa chừng / host không reachable

**Flow**: rsync/ssh thoát non-zero → `spawn_and_stream` trả Err (`sync.rs:90-94`) → `run_sync_blocking` trả Err → `run_sync` trả Err → `useSync.js:243-248` catch: append log, `last_sync_status = "error"`, Toast, `finally` `syncing:false` (250).

**Mong đợi vs thực tế**: khớp về mặt luồng. Một điểm làm đúng đáng ghi nhận: `stdin(Stdio::null())` (`sync.rs:76-77`) chặn ssh treo chờ nhập hostkey/password.

**Phát hiện**
- 🟡 `write_baseline` bị bỏ qua khi lỗi (vì `?` ở `sync.rs:495` thoát trước line 499) — **đúng**, không ghi baseline sai. Ghi ra đây vì đây là chỗ dễ hỏng nếu ai đó refactor.
- 🟡 Lỗi hầu như vô hình sau khi Toast tắt (3s, `projectStore.js:9`): `last_sync_status` không được render ở đâu (đã nêu ở A2). Hàng project vẫn hiển thị badge của lần sync thành công cũ.
- 🟡 `checkProjectSyncStatus` nuốt lỗi hoàn toàn (`useSyncStatus.js:54-55`) và giữ nguyên badge — comment giải thích là để tránh nhấp nháy. Hệ quả: mất mạng kéo dài thì badge PUSH/PULL đóng băng ở số cũ mà không có dấu hiệu "stale".
- 🟠 Không có timeout ở tầng rsync/ssh (không truyền `ConnectTimeout`/`--timeout` ở bất kỳ call site nào trong `sync.rs`). Host treo (không refuse, không reset) → project kẹt `syncing: true` vô hạn, mà A8 đã cho thấy **không có cách huỷ**. Hai lỗi này cộng lại thành một trạng thái chỉ thoát được bằng restart app.

---

### A12. Mở REPORT.html khi bản remote mới hơn

**Flow**
1. `ProjectTable.vue:149` `@click.stop="openReportHtml(p)"` → `ProjectTable.vue:523-535`.
2. `invoke('resolve_report_html', {localPath, remoteHost, remotePath})` → `system.rs:427-472`.
3. So mtime qua `git::get_file_conflict_info` (`system.rs:442-455`); nếu remote mới hơn → `spawn_blocking(rsync_pull_file)` (`system.rs:464-469`) → `sync.rs:520-531`.
4. Trả path local → `invoke('macos_open', {args:[path]})` (`ProjectTable.vue:530`).

**Mong đợi vs thực tế**: đúng về cấu trúc, `spawn_blocking` đầy đủ. Nhưng chuỗi so sánh mtime kế thừa hai lỗ hổng của `get_file_conflict_info`.

**Phát hiện**
- 🟠 **Remote dùng BSD `stat` → không bao giờ kéo được bản mới, im lặng mở bản cũ.** `git.rs:189` hardcode `stat -c '%Y'` (GNU coreutils). Trên remote macOS/FreeBSD lệnh này fail → không parse được → `remote_exists = false` (`git.rs:206-211`) → điều kiện `remote_exists && (…)` ở `system.rs:460` sai → bỏ qua bước pull → mở bản local cũ **mà không báo gì**. Chính là kịch bản mà nút này sinh ra để xử lý.
- 🟡 **Thông báo lỗi sai sự thật khi SSH chết.** SSH lỗi → `remote_exists = false` (do `git.rs` nuốt lỗi, xem A7-1) → nếu local cũng không có file thì trả `"No REPORT.html found locally or on the remote."` (`system.rs:457-459`), trong khi thực tế là không kết nối được. Người dùng sẽ đi tìm nhầm chỗ.
- 🟡 `macos_open` (`system.rs:128-135`) chỉ `spawn()`, không đọc exit code → nếu `open` thất bại thì im lặng hoàn toàn.
- Không phát hiện vấn đề ở phần `spawn_blocking`/blocking-UI cho case này.

---

### A13. Xoá project (Remove Project) — và kiểm project KHÁC có bị đụng không

**Flow**
1. `ProjectConfigModal.vue:155` → `confirmRemove()` (`useProjectConfig.js:238-262`).
2. Lấy `id`/`name` từ `editingProject` local (line 240-241) → `requestRemoveProject(id, name)` (`remoteActions.js:227-240`).
3. `askConfirm({kind:'confirm', …})` (`remoteActions.js:228-236`) → nếu Yes → `removeProject(id)` (`remoteActions.js:208-215`).
4. `projects.value = projects.value.filter(p => p.id !== id)` (line 210), `delete projectRuntime.value[id]` (line 213), `saveProjectsList()` (line 214).
5. Về JS: reset `activeLogProjectId` nếu trùng, đóng modal, log (`useProjectConfig.js:257-261`).

**Mong đợi**: chỉ đúng 1 project biến mất, mọi project khác nguyên vẹn.
**Thực tế**: ✅ **Đúng — tuân thủ Regression Guard.** `filter` theo id, `delete` đúng một khoá runtime, tên hàm phản ánh đúng phạm vi (`removeProject`, không phải `clearProjects`). Không tìm thấy hàm clear/reset nào chạm nhiều entity hơn cần thiết trong cụm A.

**Phát hiện**
- 🟡 **Rác còn lại sau khi xoá**: file baseline `<appDataDir>/baselines/<id>.json` (`sync.rs:181-183`) không bao giờ bị xoá; `projectLogs.value[id]` (`useLogs.js:27`) và cache icon (`system.rs:300-304`, chỉ được `clear()` khi `load_projects` chạy lại, `system.rs:307-308`) cũng vậy. Không gây lỗi dữ liệu, chỉ tích rác.
- 🟡 **Xoá project đang sync có thể để lại `projectRuntime` mồ côi.** Nút gear bị disable khi sync (`ProjectTable.vue:287`) nên trên Mac không chạm được; nhưng companion gọi được `requestRemoveProject` trực tiếp. Khi đó `finally` của `startSync` chạy sau: `projectRuntime.value[project.id] = {...projectRuntime.value[project.id], syncing:false}` (`useSync.js:250`) → **tạo lại** entry cho project đã xoá. Entry mồ côi này lọt vào `anySyncing`/`anyRefreshing` (`projectStore.js:49-51,87-89`) mãi mãi.
- 🟡 Trên companion `requestRemoveProject` trả `undefined` ngay (`action.js:37-40`) nên modal config không tự đóng — đã được ghi nhận là gap đã biết ở comment `useProjectConfig.js:250-254`, không phải lỗi mới.

---

### A14. Sửa config project rồi đóng modal không lưu

**Flow**: `openConfig` (`useProjectConfig.js:145-157`) tạo bản sao `{...project}` + clone riêng `hooks` (148-150), `pull_excludes` (151), `push_excludes` (152). Modal chỉ `v-model` vào `editingProject` (`ProjectConfigModal.vue:10-151`). Cancel → `closeConfig()` (`useProjectConfig.js:159-162`) set `showConfigModal = false`, `editingProject = null`.

**Mong đợi**: mọi thay đổi bị vứt bỏ, `projects.value` không đổi.
**Thực tế**: đúng cho toàn bộ các trường modal có sửa. `applyPreset` (`ProjectConfigModal.vue:209-226`) cũng chỉ gán mảng mới vào `editingProject`, không đụng bản gốc.

**Phát hiện**
- 🟡 **Clone nông chia sẻ tham chiếu `tasks`.** `{...project}` (`useProjectConfig.js:146`) khiến `editingProject.tasks` **là chính mảng của project gốc**. Hiện tại modal config không sửa `tasks` nên vô hại, nhưng đây là bẫy: chỉ cần thêm một control chạm `tasks` vào modal này là thay đổi sẽ rò rỉ ra `projects.value` ngay cả khi bấm Cancel. Các mảng khác đều đã được clone tường minh (151-152) → sự thiếu nhất quán này chính là dấu hiệu rủi ro.
- 🟡 Không có cảnh báo "unsaved changes" khi đóng — nhất quán với phần còn lại của app, ghi nhận chứ không tính là lỗi.
- Không phát hiện rò rỉ dữ liệu thực tế ở case này.

---

### A15. Đường dẫn có DẤU CÁCH hoặc dấu nháy — kiểm tra escape của mọi lệnh shell sinh ra

Rà từng call site sinh lệnh trong luồng cụm A:

| Call site | Cơ chế | Dấu cách | Nháy / `$` / backtick |
|---|---|---|---|
| `rsync` push/pull (`sync.rs:490-495`) | `Command::args()`, không qua shell | ✅ an toàn | ✅ an toàn |
| `ssh host mkdir -p <dir>` (`sync.rs:427-430`) | argv nối bằng space rồi shell **remote** parse lại | ❌ **hỏng** | ❌ `$`/backtick giãn nở trên remote |
| `ssh host <script>` conflict-check (`git.rs:193-197`) | `cd "{safe_remote}"`, chỉ escape `"` (line 184) | ✅ | ⚠️ `$`/backtick **không** escape |
| tên file trong script đó (`git.rs:186-190`) | escape `'` đúng chuẩn `'"'"'` | ✅ | ✅ |
| `sh -c <hook>` (`sync.rs:109-111`) | lệnh do người dùng nhập | n/a (cố ý) | n/a (cố ý) |
| `cd "<local_path>"` → Terminal (`system.rs:142`, `system.rs:578`) | double-quote trong shell, rồi `applescript_escape` (39-41) | ✅ | ❌ `$(…)`/backtick chạy được |
| `ssh <host> -t 'mkdir -p "…" && cd "…"'` (`system.rs:172-175`) | quote lồng | ✅ | ⚠️ `$` cố ý giãn nở |
| `bash -c "echo <expanded>"` (`system.rs:407`) | **không escape gì** | ⚠️ | ❌ `$(…)` chạy trên remote |

**Phát hiện**
- 🟠 **`ssh … mkdir -p` không quote (`sync.rs:427-430`)** — chi tiết đã nêu ở A2. Đây là lỗi dấu-cách thực sự duy nhất trong đường sync chính.
- 🟠 **`resolve_remote_path` chèn thẳng path vào `bash -c "echo …"` (`system.rs:405-408`)** — `remote_path` do người dùng gõ tự do (`ProjectConfigModal.vue:24`). Path chứa `$(rm -rf ~/x)` sẽ **được thực thi trên remote** khi bấm SSH Terminal / VSCode Remote (`ProjectTable.vue:539`). Không có `validate_path_segment` nào chặn `$`, `(`, backtick (`projects.rs:90-98` chỉ chặn `..` và ký tự control).
- 🟡 **`cd "<local_path>"` vào Terminal (`system.rs:142, 578`)** — `applescript_escape` (`system.rs:39-41`) chỉ lo tầng AppleScript; tầng shell nhận `cd "/path"` nên dấu cách OK, nhưng `$`/backtick trong đường dẫn sẽ giãn nở. Rủi ro thấp (đường dẫn do chính người dùng chọn qua native picker) nhưng escape ở đây là **hai tầng và chỉ xử lý một tầng**.
- 🟡 `git.rs:184` chỉ escape `"` cho `cd "{safe_remote}"` — `$`/backtick trong `remote_path` giãn nở trên remote. Cùng gốc rễ với mục trên.

---

### A16. Sync check (kiểm tra lệch local/remote) chạy nền

**Flow**
1. `startBackgroundRefresh()` (`useBackgroundRefresh.js:96-112`) từ `loadData` (`useProjectConfig.js:114`).
2. `restartDiffTimer` (`useBackgroundRefresh.js:59-70`) — gate đúng trên `syncCheckEnabled` (line 63), tear-down/rebuild sạch khi toggle (line 106-109).
3. Mỗi tick → `checkAllSyncStatus()` (`useSyncStatus.js:62-64`) → `checkProjectSyncStatus` cho **mọi project song song**.
4. Mỗi project: `beginRefresh` → chụp `epoch` → `invoke('check_sync_status')` (`sync.rs:682-697`, **có `spawn_blocking`**) → `compute_sync_counts` chạy **2 lần rsync dry-run qua SSH** (`sync.rs:638-639`) → đối chiếu baseline (641-674).
5. Kết quả về: kiểm `currentEpoch === epoch` mới ghi (`useSyncStatus.js:19,47-53`).

**Mong đợi vs thực tế**: khớp. Cơ chế epoch/`bumpEpoch` (`projectStore.js:103-108`) được dùng nhất quán ở cả 3 nguồn huỷ (đổi host/path, tắt sync check, reload).

**Phát hiện**
- 🟠 **Kế thừa lỗi epoch = 0 của A1**: project vừa tạo có `epoch: 0` (`remoteActions.js:193`) nên mất khả năng phát hiện "project đã bị xoá" (`projectStore.js:110-115`) — kết quả check cũ sẽ hồi sinh `projectRuntime` cho project đã xoá.
- 🟡 **Không giới hạn đồng thời.** `Promise.all(projects.value.map(...))` (`useSyncStatus.js:63`) → N project × 2 rsync-over-ssh = 2N kết nối SSH bung ra cùng lúc mỗi tick. Với ~10 project trên cùng một host là 20 kết nối đồng thời, dễ chạm `MaxStartups` của sshd (biểu hiện: check fail ngẫu nhiên, mà lỗi lại bị nuốt tại `useSyncStatus.js:54-55` nên hoàn toàn im lặng).
- 🟡 `compute_sync_counts` chạy tuần tự 2 rsync (`sync.rs:638-639`), không song song — mỗi project tốn 2 lần round-trip nối tiếp. Chỉ là hiệu năng.
- 🟡 Baseline không bao giờ được dọn cho project đã xoá (xem A13).

---

### A17. Kéo thả sắp xếp lại thứ tự project

**Flow**
1. `mousedown` trên icon → `isHandleMouseDown = true` (`ProjectTable.vue:79`); `dragstart` không phải từ handle thì `preventDefault` (`ProjectTable.vue:365-368`).
2. `dragover` → tính ngưỡng trung điểm chống jitter (`ProjectTable.vue:384-405`), **mutate `projects.value` tại chỗ** (line 399-403).
3. `drop` → `onRowDrop` → `onRowDragEnd` (`ProjectTable.vue:407-409`).
4. `onRowDragEnd` → `reorderProjects(ids)` (`ProjectTable.vue:411-420`) → `remoteActions.js:149-156`: map id → object, **nếu độ dài không khớp thì bỏ toàn bộ reorder** (line 153), rồi `saveProjectsList()`.

**Mong đợi vs thực tế**: khớp. Guard ở `remoteActions.js:153` đúng tinh thần Regression Guard — thà không đổi thứ tự còn hơn `projects.value = <mảng ngắn hơn>` làm mất project.

**Phát hiện**
- 🟡 **`reorderProjects` bị gọi 2 lần cho một thao tác thả.** `onRowDrop` gọi `onRowDragEnd` (`ProjectTable.vue:408`), rồi trình duyệt vẫn phát `dragend` → handler chạy lần nữa (`ProjectTable.vue:71`). Idempotent (cùng danh sách id) nên chỉ là một lần ghi file thừa; trên companion là một intent thừa qua WS.
- 🟡 **Trên companion, mutation lạc quan có thể không khớp host.** `onRowDragOver` ghi thẳng `projects.value` của điện thoại (`ProjectTable.vue:403`), nhưng nếu host từ chối vì độ dài lệch (`remoteActions.js:153` — ví dụ có project vừa được thêm/xoá), điện thoại giữ thứ tự sai cho tới lần `broadcastFull` tiếp theo. Không có rollback.
- 🟡 Kéo rồi thả **ra ngoài danh sách** vẫn persist thứ tự giữa chừng, vì `dragend` luôn chạy `reorderProjects` (`ProjectTable.vue:419`) và không có khái niệm "huỷ kéo".
- Không phát hiện nguy cơ mất project.

---

### A18. DEV / BUILD button mở Terminal

**Flow**
1. Popup OPEN → `ProjectTable.vue:183` DEV / `:186` BUILD → `runProjectDev` / `runProjectCommand` (`ProjectTable.vue:456-462`) → `invokeProjectRun` (446-454).
2. Lệnh lấy từ `getDevCmd`/`getBuildCmd`: override của project trước, rồi `stack_info` dò tự động (`ProjectTable.vue:557-563`; nguồn `system.rs:527-573`).
3. `invoke('run_project_dev'|'run_project_command', {localPath, cmd})` → `system.rs:582-603` → `run_in_project_terminal` (`system.rs:576-580`) → `open_terminal_with_command` (`system.rs:46-75`) → `osascript … .spawn()` (68-72) + `snap_frontmost_terminal_window()` (73).

**Mong đợi vs thực tế**: mở Terminal đúng thư mục, chạy lệnh. Khớp.

**Phát hiện**
- 🟡 **Vi phạm hình thức quy tắc NEVER BLOCK THE UI, nhưng không gây treo.** `run_project_command` và `run_project_dev` là `pub fn` thuần (`system.rs:582, 592`), không `async`, không `spawn_blocking`, mà lại chạy subprocess. Tuy nhiên đường đi cuối cùng là `Command::spawn()` (`system.rs:71`, `119`) — **không** `.output()`/`.wait()` — nên không có chờ đồng bộ, UI không đóng băng. Cùng nhóm này: `macos_open` (`system.rs:129`), `open_local_terminal` (`system.rs:139`), `open_remote_subprocess` (`system.rs:164`), `install_akiclaudedoc` (`system.rs:274`). Khuyến nghị: hoặc chuyển sang `spawn_blocking` cho nhất quán với quy tắc "không xét từng ca", hoặc ghi rõ ngoại lệ "spawn-only" vào CLAUDE.md — hiện tại người đọc code không phân biệt được đâu là bỏ sót, đâu là cố ý.
- 🟡 **Thất bại hoàn toàn im lặng.** `open_terminal_with_command` chỉ `spawn()` osascript và trả `Ok(())` ngay (`system.rs:68-74`); nếu AppleScript lỗi (chưa cấp quyền Accessibility cho `snap_frontmost_terminal_window`, `system.rs:81` ghi rõ yêu cầu này) thì Toast vẫn báo *"Command started in Terminal!"* (`ProjectTable.vue:457,461`) trong khi không có gì xảy ra.
- 🟡 `cmd` được nội suy thẳng vào chuỗi AppleScript sau `applescript_escape` (`system.rs:578` → `48`). Đây là **cố ý** (người dùng tự nhập lệnh ở `ProjectConfigModal.vue:43-56`), nhưng nó chia sẻ đúng cái escape một-tầng đã nêu ở A15 với `local_path`.

---

## Tổng hợp phát hiện 🔴 / 🟠

| # | Mức | Vấn đề | Vị trí |
|---|---|---|---|
| 1 | 🔴 | Companion bypass toàn bộ xác nhận `--delete`: `hostInvoke` không có allowlist lệnh, gọi thẳng `run_sync` được | `src/services/hostInvoke.js:23-37` ↔ `src-tauri/src/sync.rs:384-399` |
| 2 | 🟠 | `local_path` rỗng → PUSH đẩy toàn bộ `/`; không có validation ở cả JS lẫn Rust | `src/composables/useProjectConfig.js:164-195`, `src-tauri/src/projects.rs:101-108`, `src-tauri/src/sync.rs:418` |
| 3 | 🟠 | Danh sách file bị xoá nhúng vào HTML dialog không escape (dialog SELECT kế bên thì có) | `src/composables/useSync.js:157,170` vs `useSync.js:328-330,364-370` |
| 4 | 🟠 | SSH lỗi → `REPORT.html` bị auto-approve xoá (mtime trả về 0, hiểu nhầm là "cũ") | `src/composables/useSync.js:118-130` + `src-tauri/src/git.rs:199-218` |
| 5 | 🟠 | SELECT push: SSH lỗi → không hỏi conflict, ghi đè remote im lặng; nhánh catch là dead code | `src/composables/useSync.js:303-322` + `src-tauri/src/git.rs:199-220` |
| 6 | 🟠 | SELECT push với DRY bật → không truyền gì, trái tooltip "unaffected by the DRY toggle" | `src/composables/useSync.js:51,207` vs `src/components/ProjectTable.vue:215` |
| 7 | 🟠 | Preview fail → "continue anyway" bỏ qua luôn bước gõ tên project (nguy hiểm hơn nhưng rào yếu hơn) | `src/composables/useSync.js:78,86-104,155` |
| 8 | 🟠 | `ssh host mkdir -p <dir>` không quote → hỏng với remote path có dấu cách, giãn nở `$`/backtick | `src-tauri/src/sync.rs:427-430` |
| 9 | 🟠 | `resolve_remote_path` chèn path thô vào `bash -c "echo …"` → command injection trên remote | `src-tauri/src/system.rs:405-408` |
| 10 | 🟠 | Không có cách huỷ sync đang chạy; PUSH `--delete` bấm nhầm không dừng được | `src-tauri/src/sync.rs:67-95` (không lưu handle), `src/components/ProjectTable.vue:235` |
| 11 | 🟠 | Thoát app không kill rsync/ssh đang chạy → process mồ côi tiếp tục xoá file remote | `src-tauri/src/lib.rs:134-142` |
| 12 | 🟠 | Không có timeout ssh/rsync → host treo làm project kẹt `syncing:true` vĩnh viễn (cộng hưởng với #10) | `src-tauri/src/sync.rs` (mọi call site) |
| 13 | 🟠 | Project mới có `epoch: 0`, phá bất biến "sống ⇒ epoch ≥ 1" → kết quả check cũ hồi sinh project đã xoá | `src/store/remoteActions.js:193` vs `src/store/projectStore.js:64-70,110-115` |
| 14 | 🟠 | Rename SSH host = tự động repoint mọi project không hỏi; heuristic "1 mất + 1 thêm" dễ khớp nhầm | `src/store/remoteActions.js:274-283` |
| 15 | 🟠 | `stat -c '%Y'` (GNU-only) → remote BSD/macOS làm REPORT.html không bao giờ kéo bản mới, im lặng | `src-tauri/src/git.rs:189` → `src-tauri/src/system.rs:460` |

**Điểm làm đúng đáng ghi nhận** (không phải lỗi, để tránh "sửa" nhầm):
- Mọi `#[tauri::command]` trong đường sync đều có `spawn_blocking`: `run_sync` (`sync.rs:394`), `check_sync_status` (`sync.rs:686`), `get_sync_delete_preview` (`sync.rs:708`), `get_file_conflict_info` (`git.rs:142`), `resolve_report_html` (`system.rs:465`), `resolve_remote_path` (`system.rs:402`).
- `removeProject` (`remoteActions.js:208-215`), `applyTaskEdit` (135-141), `reorderProjects` (149-156) đều đúng phạm vi một-entity / có guard — không vi phạm Regression Guard.
- Xác nhận gõ tên được re-validate trên host (`useSync.js:182`), không tin `preConfirm` phía client (`DialogHost.vue:69-75`).
- `stdin(Stdio::null())` chặn ssh treo chờ prompt (`sync.rs:76-77`).
- `strip_prefix("deleting ")` thay vì `trim_start_matches` (`sync.rs:736-743`) — xử lý đúng edge case tên file bắt đầu bằng "deleting ".
## Cụm B — Usage Monitor

Phạm vi đọc: `src/composables/usageMonitor.js`, `usageMonitorRegistry.js`, `agUsageCache.js`,
`src/store/usageMonitorStore.js`, `usageSlotStore.js`, `usageTierStore.js`, `refreshStore.js`,
`sshStore.js`, `claudeModeStore.js`, `src/components/AgentUsage*.vue`, `src/services/{action,mirror,hostInvoke}.js`,
`src/utils/{tauri,scheduler}.js`, `src-tauri/src/agent_usage.rs`, `src-tauri/src/lib.rs`,
`scripts/get-claudecode-usage.sh`, `scripts/get-antigravity-usage.js`.

Quy ước mức: 🔴 sai dữ liệu / mất dữ liệu người dùng nhìn thấy · 🟠 lỗi hành vi thật, có kịch bản
tái hiện rõ · 🟡 nợ kỹ thuật / lệch so với ý định đã ghi trong comment, tác động hẹp.

---

### B1. Khởi động lần đầu (localStorage trống) — 4 slot dựng thế nào, slot D scope='remote'

**Flow**

1. `src/App.vue:10` render `<AgentUsageSection />`.
2. `src/components/AgentUsageSection.vue:32-39` — `ALL_TIER_ROWS` = `[[A,B],[C,D]]`,
   `activeTierRows = slice(0, tierCount)`. `src/store/usageTierStore.js:5-7` — không có
   `aki-usage-tier-count` → `tierCount = 1` → **chỉ A và B được mount**; C/D chưa tồn tại.
3. `src/components/AgentUsageSlot.vue:100` → `slotTarget('A')`.
   `src/store/usageSlotStore.js:42-57 seed()` — không có `aki-usage-slot-targets` → rơi xuống
   `migrateSlot()` (`:30-40`), không có key legacy nào → trả nguyên `DEFAULTS` (`:22-27`):
   A `local/ag`, B `local/ag`, C `local/cc`, D `remote/cc`, `remoteHost: ''`.
4. `slotTarget` (`:68-74`): với D → `host = '' || selectedSshHost.value || ''`.
   `src/store/sshStore.js:5-9` — `_storedHost=''`, `sshHosts=[]` lúc boot → `host=''`.
5. `AgentUsageSlot.vue:105-109 monitorsFor()` gọi
   `getMonitor('antigravity', host)` **và** `getMonitor('claudecode', host)`.
6. `usageMonitorRegistry.js:43` `monitorId` (`usageMonitorStore.js:36-38`) → `ag@local`, `claudecode@local`.
   `enabled = !!host && isMonitorEnabled(id)` (`:48`); `isMonitorEnabled` trả **true khi chưa có entry**
   (`usageMonitorStore.js:108-111`).
7. `usageMonitor.js:413-434` watch `enabled` `{immediate:true}` → `on=true` → `onHostBoot(checkUsage)`
   (`utils/scheduler.js:19` → chạy ngay trên host) + `restartPollTimer()`.

**Mong đợi vs thực tế** — Khớp. Mặc định A/B/C/D giống hệt bản 1.19.0 (đối chiếu
`git show 59aeccc^:src/components/AgentUsageSection.vue:39-46`: A `local/ag`, B `local/ag`,
C `local/cc`, D `remote/cc`) → không có regression về default. Slot D thật sự `scope='remote'`,
và vì `tierCount=1` mặc định nên lần chạy đầu D chưa render.

**Phát hiện**: 🟡 — lần khởi động đầu, **cả hai** monitor `antigravity@local` và `claudecode@local`
được tạo và bật (`AgentUsageSlot.vue:107` tạo cả cặp AG|CC cho mỗi slot; `usageMonitorStore.js:110`
mặc định ON), dù giao diện chỉ hiển thị AG (A và B đều `localAgent:'ag'`). Tức ngay lần chạy đầu đã
có 2 vòng poll trong khi người dùng chỉ nhìn 1. Trên `local` chi phí thấp, nhưng đây chính là gốc của
🟠 ở B5/B6/B7 khi host là SSH.

---

### B2. Nâng cấp từ 1.19.0 (4 flag cũ `aki-src-*-enabled`)

**Flow**

1. `src/store/usageMonitorStore.js:66-73 seed()` — chưa có `aki-usage-monitor-enabled` →
   `legacyRemotePending = true`, gọi `readLegacyFlags(selectedSshHost.value)`.
2. `legacyKeysFor` (`:41-51`): luôn map 2 flag local (`aki-src-ag-enabled` → `antigravity@local`,
   `aki-src-cclocal-enabled` → `claudecode@local`); chỉ khi `host` truthy mới map
   `aki-src-agremote-enabled` / `aki-src-ccremote-enabled` sang `<agent>@<host>`.
3. `sshStore.js:5-9`: `selectedSshHost` = `_storedHost || sshHosts[0]`. `_storedHost` đọc
   **đồng bộ** từ `localStorage['aki-selected-ssh-host']`.
4. Nếu ở bước 3 `host` rỗng → watcher hoãn `usageMonitorStore.js:87-100` chờ `selectedSshHost` có giá
   trị (sau IPC `get_ssh_hosts`), rồi chỉ vá **những id chưa có** (`:94`) và ghi localStorage.
5. Slot: `usageSlotStore.js:30-40 migrateSlot()` đọc đúng 3 key cũ `-top` / `-sub` / `-remote-sub`
   (khớp `git show 59aeccc^:src/components/AgentUsageSlot.vue:110-112`), `remoteHost` để `''` =
   "theo picker toàn cục" → hành vi y hệt 1.19.0.
6. `syncCheckStore.js:16-23` seed `aki-sync-check-enabled` từ `aki-remote-mode-enabled` — độc lập, đúng.

**Mong đợi vs thực tế** — Trường hợp **đã có host ghim** (`aki-selected-ssh-host` tồn tại): carry-over
đủ 4 flag, đúng. Trường hợp **chưa ghim host**: có 2 lỗ.

**Phát hiện**

- 🟡 `readLegacyFlags` dùng `selectedSshHost`, mà getter của nó fallback `sshHosts.value[0]`
  (`sshStore.js:7`). Nếu người dùng 1.19.0 chưa từng ghim host, host được migrate là **host đầu tiên
  trong `~/.ssh/config`**, không nhất thiết là host họ đang theo dõi → cờ remote bám nhầm entity;
  host thật rơi về mặc định ON.
- 🟡 Mất cờ remote nếu trong phiên đầu sau nâng cấp `selectedSshHost` **không bao giờ** truthy
  (chưa cấu hình SSH) **và** người dùng bấm 1 nút power bất kỳ: `setMonitorEnabled`
  (`usageMonitorStore.js:120-123`) ghi `aki-usage-monitor-enabled` chỉ với 2 id local. Phiên sau,
  `seed()` (`:68-70`) thấy STORAGE_KEY → `return` sớm, `legacyRemotePending` không bao giờ được đặt
  → 2 cờ legacy remote không còn đường nào đọc tới, và vì mặc định là ON (`:110`) một người đã cố ý
  tắt remote sẽ bị bật lại — đúng kịch bản mà comment `:80-83` nói là muốn tránh.

---

### B3. Máy CHƯA cấu hình SSH host — slot D (REMOTE) hiện gì, nút power làm gì

**Flow**

1. `usageSlotStore.js:72` → `host = '' `.
2. `AgentUsageSlot.vue:107` → `getMonitor('claudecode', '')`.
3. `usageMonitorStore.js:36-38` → `monitorId('claudecode','')` = `claudecode@(none)`
   (`NO_HOST = '(none)'`, `:33`) — **không** sập về `local`.
4. `usageMonitorRegistry.js:48` `enabled = !!host && ...` → ghim `false` vĩnh viễn.
5. `usageMonitorRegistry.js:49-55 toggle()` → `if (!host) return;` → **no-op**.
6. `usageMonitorRegistry.js:30-32 lockedFor()` → `host !== LOCAL_HOST` → `locked=false`.
7. `AgentUsageSlot.vue:64` truyền `sourceOff = !monitor.enabled = true` →
   `AgentUsage.vue:316-322` → `uiStatus.kind='off'`, chữ **"Monitoring off"**, icon power.
8. `usageMonitor.js:184-188` — `checkUsage` return sớm, không có IPC nào phát ra.
9. Dropdown host `AgentUsageSlot.vue:38-47`: `:value="target.host"` = `''` khớp option
   `value="" disabled` "Select Host", danh sách rỗng.

**Mong đợi vs thực tế** — **Đã hết thật.** Hai điều kiện cũ gây bug (chung instance với monitor local,
và nút power ghi đè cờ của monitor local) đều bị chặn ở đúng 2 chỗ: `NO_HOST` tách identity, và
`toggle()` return sớm khi `!host`. Không còn đường nào để slot REMOTE chưa trỏ host chạm vào
`claudecode@local`. Kiểm tra thêm: `agUsageCache.persistAgAccount:138` và `listAgAccounts:199`
đều guard `!host` → cache cũng không bị ghi rác dưới khoá rỗng.

**Phát hiện**

- Không phát hiện vấn đề về đúng/sai. 🟡 duy nhất: monitor `(none)` vẫn được tạo thật và đăng ký
  vĩnh viễn vào `_wakeSubscribers` (`usageMonitor.js:404-408`) mà không bao giờ gỡ — vô hại
  (`onWake` return ở `:397`) nhưng là rác thường trú.

---

### B4. Bật monitor local Claude Code → poll → hiển thị số

**Flow**

1. Bấm icon power: `AgentUsageSlot.vue:33-34` → `src.monitor.toggle()` →
   `usageMonitorRegistry.js:54 setMonitorEnabled('claudecode@local', true)` →
   `usageMonitorStore.js:120-123` thay cả object map (để mirror bắn) + ghi localStorage.
2. `usageMonitor.js:413` watch `enabled` → reset `provisioned/provisionFailCount/isChecking/pollCount`,
   `resumePolling()` (`:390-393`), rồi `checkUsage()` (`:430`) + `restartPollTimer()` (`:433`).
3. `usageMonitor.js:212` `invoke('get_agent_usage', { agentName:'claudecode', host:'local' })`
   → `src/utils/tauri.js:17` (host = Tauri IPC thật).
4. `src-tauri/src/agent_usage.rs:361-378` — `async fn` + `spawn_blocking` ✅ (đúng luật
   NEVER BLOCK THE UI) → `get_claudecode_usage` (`:407`).
5. `:420 run_remote_script("local", script)` → `:222 run_interpreter_timeout` → `host_lock` (`:48-55`)
   → `is_local_host` true → `Interpreter::Sh` local = `Command::new("sh")` (`:91`) + preamble
   `CLAUDE_BIN_RESOLVER_PREAMBLE` (`:113-119`, `:154`).
6. Script `scripts/get-claudecode-usage.sh:3` đọc `~/.claude/rate-limits-cache.json`, in
   `<json>|||MTIME|||…|||AUTHINFO|||<json>`; Rust parse `:453-520`, chèn `email/orgName/rateLimitTier`,
   trả `AgentUsageResponse` (`:548-552`).
7. `usageMonitor.js:216-273` parse; stale = `resetIsPast || dataAge > 600` (`:227-233`);
   `dataAt = mtime` (`:235`); `data.value = parsed` (`:269`).
8. `AgentUsageSlot.vue:54-60` → `AgentUsage.vue:314-331 uiStatus='data'` → thanh 5-Hour/7-Day
   (`AgentUsage.vue:190-227`), `cc5hPct` (`:599`).
9. Poll định kỳ: `usageMonitor.js:374-377 hostInterval(checkUsage, usage_interval_s*1000)`
   (mặc định 30s, `refreshStore.js:8`).

**Mong đợi vs thực tế** — Khớp hoàn toàn. Chống chồng lệnh có `isChecking` + `pendingRecheck`
(`:189-194`, `:341-344`); provision idempotent (`:163-181`).

**Phát hiện**: không phát hiện vấn đề.

---

### B5. Slot C → host A, slot D → host B, cùng Claude Code, hai tài khoản khác nhau

**Flow**

1. `usageSlotStore.js:83-87 setSlotTarget('C', {remoteHost:'A'})` — chỉ vá đúng slot C
   (Regression Guard OK), slot D không đổi.
2. `AgentUsageSlot.vue:109` watch `target.value.host` → `monitorsFor()` → `getMonitor('claudecode','A')`
   / `('claudecode','B')` → 2 id khác nhau `claudecode@A`, `claudecode@B`
   (`usageMonitorRegistry.js:43-44`) → 2 instance riêng, mỗi cái `data/loading/error` riêng
   (`usageMonitor.js:127-135`).
3. Rust: `agent_usage.rs:394-398 cc_auth_force_needed(host)` theo từng host; `host_lock(host)`
   (`:48-55`) mỗi host một mutex → 2 host chạy song song, không chặn nhau.
4. CC không dùng cache localStorage nào (email + quota nằm trong payload từng host).

**Mong đợi vs thực tế** — Số của host A hiện ở slot C, host B ở slot D, không lẫn. Đúng.

**Phát hiện**: 🟠 (xem B6/B7 để tránh lặp) — cùng lúc trỏ C→A và D→B, registry tạo **4** monitor
(`AgentUsageSlot.vue:107` luôn tạo cả cặp AG|CC cho host của slot), và cả 4 đều bật mặc định
(`usageMonitorStore.js:110`). Nghĩa là chỉ vì trỏ slot vào host B, app tự khởi động thêm một vòng
SSH `antigravity@B` mà người dùng chưa từng chọn xem. Chi tiết ở B6.

---

### B6. Hai slot cùng trỏ vào (agent, host) giống hệt — có gộp thành 1 vòng SSH không?

**Flow**

1. `usageMonitorRegistry.js:17` `const registry = new Map()`; `:43-45` — cùng `id` → trả instance đã có,
   không tạo mới. Monitor được dựng trong `monitorScope = effectScope(true)` (`:23`, `:59`) nên
   không bị unmount của component nào giết.
2. Vòng poll nằm trong monitor (`usageMonitor.js:374`), nên 2 slot ⇒ 1 timer ⇒ 1 lần
   `invoke('get_agent_usage')` mỗi chu kỳ.
3. Tầng 2 phòng vệ ở Rust: `agent_usage.rs:48-55 host_lock` tuần tự hoá mọi script tới cùng host.

**Mong đợi vs thực tế** — Có gộp, đúng như thiết kế. Xác nhận 1 vòng SSH.

**Phát hiện**: 🟠 — **gộp đúng theo `(agent,host)` nhưng số monitor được BẬT lại theo `host`, không
theo cái đang xem.** `AgentUsageSlot.vue:107` bắt buộc tạo cả `antigravity@host` lẫn `claudecode@host`
(vì thanh tab cần 2 icon power), và `isMonitorEnabled` mặc định `true` cho id chưa có entry
(`usageMonitorStore.js:108-111`). Hệ quả: mỗi host từng được một slot trỏ tới sẽ chạy **2** vòng SSH
30s vĩnh viễn, kể cả agent chưa bao giờ được chọn hiển thị. Trên host chậm/qua Tailscale đây là chi
phí thật (và `host_lock` khiến 2 probe cùng host xếp hàng, kéo dài thời gian giữ thread
`spawn_blocking`).

---

### B7. Đổi host slot C: A → B → A — dữ liệu của A còn không?

**Flow**

1. Đổi sang B: `setSlotTarget('C',{remoteHost:'B'})` → `AgentUsageSlot.vue:109` → `monitors.value`
   trỏ sang `claudecode@B`. Monitor `claudecode@A` **vẫn nằm trong registry và vẫn chạy**
   (`usageMonitorRegistry.js:17`, comment `usageMonitor.js:12` — cố ý không có `onUnmounted`).
2. Quay lại A: `getMonitor('claudecode','A')` → `registry.get(id)` trả đúng instance cũ
   (`usageMonitorRegistry.js:44`) với `data/dataAt/stale` còn nguyên, thậm chí đã được cập nhật liên tục.

**Mong đợi vs thực tế (CC)** — Dữ liệu của A còn nguyên và còn *mới*. Đúng.

**Phát hiện**

- 🟠 **(AG) ghim tài khoản của slot bị xoá khi tạt qua host khác.**
  `AgentUsageSlot.vue:165-200 slotAccountInfo`: khi slot C đang ghim `viewingEmail = "x@gmail.com:ide"`
  và bị trỏ sang host B, `loadAgAccount(key, 'B')` (`:194`, `agUsageCache.js:173-183`) trả `null`
  (đúng: cache phân vùng theo host) → nhánh `:199` trả `isMissing: true`. Watcher "defensive"
  `:203-208` thấy `isMissing && !loading` → `slotViewingEmail = null` và
  **`localStorage.removeItem(slotViewingEmailKey)`**. Quay lại host A thì ghim đã mất.
  Điều này mâu thuẫn trực tiếp với ý định ghi ở `agUsageCache.js:230-232`
  ("the pinned-account preference … should survive the slot being pointed somewhere else and back").
  Điều kiện `!monitor.value.loading` không cứu được vì monitor `@B` thường ở trạng thái rảnh giữa 2 nhịp poll.
- 🟡 Registry không bao giờ evict: đi qua N host ⇒ 2N monitor sống hết phiên, mỗi cái một
  `_wakeSubscribers` entry (`usageMonitor.js:404`). Không có đường nào tắt trừ bấm power từng cái —
  mà nút power của host không còn slot nào trỏ tới thì không hiển thị nữa.

---

### B8. Tắt monitor slot C → mất reading không, monitor khác có bị ảnh hưởng?

**Flow**

1. `AgentUsageSlot.vue:34` → `toggle()` → `usageMonitorRegistry.js:54` → `setMonitorEnabled(id,false)`.
2. `usageMonitorStore.js:121` — `{ ...map, [id]: false }`: **chỉ một khoá**, không có hàm
   "clear all" nào tồn tại (`:113-119` ghi rõ) → không vi phạm Regression Guard.
3. `usageMonitor.js:413-434` watch → `on=false` → nhánh `:423-427`: nếu `data !== null` thì
   `isCached=true`, `cachedAt=lastFetchedAt`; **không** đụng `data`. `restartPollTimer()` → `:369`
   `!enabled` → không tạo timer, và timer cũ đã bị `clearInterval` ở `:365`.
4. `checkUsage` nếu có tick muộn lọt vào → return ở `:184-188`.
5. UI: `AgentUsage.vue:316-322` — `sourceOff` được kiểm **trước** `data` → hiện "Monitoring off",
   nên `data` giữ lại nhưng không vẽ; bật lại thì thấy ngay số cũ trong lúc chờ fetch.

**Mong đợi vs thực tế** — Không mất reading, monitor khác không hề bị chạm (map thay bằng spread,
đúng 1 khoá). Đúng.

**Phát hiện**: không phát hiện vấn đề.

---

### B9. Cùng 1 tài khoản Google AG đăng nhập trên 2 máy — cache có ghi đè nhau?

**Flow**

1. `usageMonitor.js:259 persistAgAccount(parsed, fetchedAt, host)` — `host` là identity bất biến của
   monitor, truyền xuống từ `createUsageMonitor` (`:122`).
2. `agUsageCache.js:33 accountKey(host,email,sourceType)` → `"<host>|<email>:<sourceType>"`.
   Host nằm **trong khoá**, không phải metadata (rationale `:19-24`).
3. `persistAgAccount:151-152` vòng dọn khoá cũ chạy trên `entriesForHost(store, host)` (`:128-130`)
   → không chạm phân vùng host khác.
4. `store.lastActiveEmailByHost[host]` (`:161`) — cũng theo host; `lastActiveEmailFor(host)` (`:186-188`).
5. Migration v2→v3 `:60-71` re-key mọi bản ghi cũ về `local` — hợp lý vì probe AG remote hỏng đến 1.20.0.

**Mong đợi vs thực tế** — Local Mac và host remote cùng email giữ 2 bản ghi độc lập, không ghi đè.
Đúng, đây chính là lỗi v2 đã sửa.

**Phát hiện**: không phát hiện vấn đề với chiều "2 máy". (Vấn đề còn lại nằm ở chiều "2 sourceType
trên cùng 1 máy" — xem B10.)

---

### B10. Mở dropdown chọn account AG (có dedup + XOÁ record)

**Flow**

1. `usageMonitor.js:143 refreshAccounts()` / `:146` khi khởi tạo / `:260` sau mỗi lần fetch sống →
   `listAgAccounts(host)`.
2. `agUsageCache.js:198-225` — dedup chạy **trong** `entriesForHost(store, host)` (`:204`);
   record thua bị `delete store.accounts[...]` (`:217`, `:220`) rồi `saveStore` (`:225`).
3. `AgentUsage.vue:80-104` render `accounts`; chọn → `pickAccount` (`:340-343`) → emit →
   `AgentUsageSlot.vue:155-163 handleSelectAccount` → ghi `aki-usage-slot-<id>-viewing-account`.

**Mong đợi vs thực tế (đa máy / đa account)** — record của host khác **sống sót**: bộ lọc `:204`
chặn hoàn toàn. Record của account khác trên **cùng** host cũng sống (dedup theo email, mỗi email 1 hàng).

**Phát hiện**

- 🔴 **Cùng một email nhưng hai `sourceType` trên cùng một host bị huỷ mất một bản, và bản ghim còn
  lại trả về số của sourceType SAI.**
  - `scripts/get-antigravity-usage.js:605-621`: dedup phía script chỉ gộp `desktop`+`cli` → `desktop_cli`;
    một snapshot `ide` **cùng email** vẫn được đẩy riêng vào `finalSnapshots` (`:618`) → `allAccounts`
    (`:627`) có thể chứa `{email:X, sourceType:'ide'}` **và** `{email:X, sourceType:'desktop_cli'}`.
    Đây đúng là cấu hình của máy này (CLAUDE.md liệt kê cả AGY CLI lẫn IDE).
  - `agUsageCache.js:144-159 persistAgAccount` lặp qua `allAccounts`: vòng lặp 1 ghi `A|X:ide`;
    vòng lặp 2 với `X:desktop_cli` chạy `:151-152` — *"xoá mọi khoá cùng host, cùng email, khác
    canonicalKey"* → **xoá luôn `A|X:ide` vừa ghi** rồi ghi `A|X:desktop_cli`. Trong đúng một lần
    persist, một trong hai bản ghi biến mất.
  - `listAgAccounts:203-223` gộp thêm một lần nữa **theo `email` thuần** (`:206`), nên dropdown chỉ còn
    1 hàng cho X — trong khi UI cố ý vẽ icon phân biệt ide/cli/desktop (`AgentUsage.vue:88-90`) và
    nút Log Out đổi theo `sourceType` (`AgentUsage.vue:350-360`).
  - Hệ quả sai số: slot đã ghim `X:ide` → `loadAgAccount('X:ide', host)` (`agUsageCache.js:177`) trượt
    khoá trực tiếp, rơi vào fallback so khớp **chỉ theo email** (`:179-182`) → trả record
    `X:desktop_cli` → thẻ hiển thị quota của phiên CLI dưới nhãn IDE. Đây là *hiển thị số sai*, không
    chỉ là mất lịch sử.
  - Comment ở `:149-150` chỉ biện minh cho trường hợp "sourceType đổi"; nó không phân biệt được với
    trường hợp "hai sourceType tồn tại đồng thời", nên phạm vi xoá vượt quá entity thật —
    đúng lớp lỗi Regression Guard mô tả trong CLAUDE.md.
- 🟡 `activeEmails` (`usageMonitor.js:241-257`) nạp cả `email` lẫn `email:sourceType`, còn
  `data.value` chọn bằng `allAccounts.find(a => a.email === activeEmail)` (`:261-263`) — với email
  trùng nhau, lấy phần tử đầu tiên một cách tuỳ tiện.

---

### B11. Log out Antigravity (IDE / AG / CLI) — lịch sử multi-account có sống sót?

**Flow**

1. `AgentUsage.vue:385-397 logoutAntigravity()` → `remoteActions.requestAgLogout(sourceType)`
   (`src/store/remoteActions.js`, action key `remoteActions.requestAgLogout`).
2. Trong đó: `askConfirm` (dialog mirrored) → nếu Yes thì
   `invoke('logout_antigravity')` hoặc `invoke('logout_antigravity_cli')`, trả `true`.
3. Rust: `agent_usage.rs:682-726 logout_antigravity` — `async fn` + `spawn_blocking` ✅; quit app,
   xoá `ANTIGRAVITY_ACCOUNT_ONLY_PATHS` (`:610-621`), xoá 2 dòng OAuth trong `state.vscdb`
   (`:644-680`), xoá Keychain item (`:714-720`). `logout_antigravity_cli` `:728-755` cũng
   `spawn_blocking` ✅, chỉ xoá 3 file trong `~/.gemini`.
4. Về JS: `AgentUsage.vue:396` emit `logout-success` → `AgentUsageSlot.vue:72` →
   `monitor.recheckAfterLogout` → `usageMonitor.js:353-357`: **chỉ gọi `checkUsage()`**, không xoá gì.
5. `grep -rn "clearAg\|clearLastActive\|resetAccount" src/` → chỉ khớp trong *comment* ở
   `usageMonitor.js:350,352`. Không tồn tại hàm xoá cache AG nào trong toàn bộ frontend.

**Mong đợi vs thực tế** — Lịch sử multi-account **sống sót toàn bộ**: không có lệnh ghi/xoá
`aki-antigravity-usage-cache-v3` nào trên đường logout. Regression v1.9.3 đã được đóng đúng cách
(bỏ hẳn hành vi xoá, và đổi tên hàm theo phạm vi thật).

**Phát hiện**: không phát hiện vấn đề.

---

### B12. Xoá account Claude Code rồi tạo lại cùng email — quota cũ có còn hiện?

**Flow**

1. Email: `scripts/get-claudecode-usage.sh:63-101` — `claude auth status` được cache tại
   `~/.claude/auth-cache.json`, TTL `AUTH_REFRESH_AGE_S=300` (`:64`), và bị **ép làm mới đúng 1 lần
   mỗi host mỗi phiên app** qua `AKI_FORCE_AUTH_REFRESH` (`agent_usage.rs:411-419`,
   `cc_auth_force_needed` `:394-398`).
2. Quota: `scripts/get-claudecode-usage.sh:3` — `~/.claude/rate-limits-cache.json`, **một file duy
   nhất cho cả máy**, không mang định danh account nào. File này do statusline hook ghi, chỉ đổi khi
   có lượt chat CC mới.
3. Rust ghép 2 nguồn lại thành một object (`agent_usage.rs:501-520` chèn `email` vào cùng JSON quota).

**Mong đợi vs thực tế** — Sau khi xoá/tạo lại account, trong khoảng từ lúc đăng nhập lại tới lượt
chat CC đầu tiên: email hiển thị là **mới** (sau ≤300s hoặc ngay nếu vừa mở app), còn 5h/7d vẫn là
**quota của account cũ**. Không có cơ chế nào phát hiện lệch — hai nguồn không chia sẻ khoá định danh.

**Phát hiện**: 🟠 — email và quota có thể thuộc hai account khác nhau trên cùng một thẻ, không có
badge/stale nào cảnh báo (`stale` chỉ nhìn mtime, `usageMonitor.js:227-233`; file cũ vẫn "mới" nếu
hook vừa ghi trước khi đổi account). Sửa tối thiểu: ghi email vào `rate-limits-cache.json` và so khớp
với `AUTHINFO` trước khi hiển thị.

---

### B13. Bật Claude Code proxy mode → monitor local CC bị khoá; tắt proxy thì sao?

**Flow**

1. `claudeModeStore.js:11-18 refreshClaudeMode()` (gọi ở `App.vue:78`, trong `onHostBoot`).
2. `usageMonitorRegistry.js:30-32 lockedFor` → `locked = agentId==='claudecode' && host===LOCAL_HOST
   && claudeMode==='proxy'` → chỉ đúng monitor local CC.
3. `usageMonitorRegistry.js:69-73` watch `claudeMode`: khi `'proxy'` →
   `monitorEnabled.value = { ...monitorEnabled.value, ['claudecode@local']: false }` —
   **ghi thẳng vào ref, cố ý không qua `setMonitorEnabled` để không persist** (comment `:64-68`).
4. `toggle()` (`usageMonitorRegistry.js:50`) → `if (locked.value) return` → nút power vô hiệu.
5. UI: `AgentUsageSlot.vue:30` tooltip "locked OFF - Proxy mode active";
   `AgentUsage.vue:316-322` → "Monitor only for native Claude - Proxy mode active".
6. Tắt proxy: `locked` về `false` (mở khoá nút), nhưng **không** có nhánh nào bật lại
   `monitorEnabled['claudecode@local']` — cố ý theo comment `:64-66`.

**Mong đợi vs thực tế** — Khoá/mở khoá đúng, không đụng monitor nào khác.

**Phát hiện**

- 🟠 **Trạng thái tạm (do proxy ép) rò rỉ thành preference được lưu vĩnh viễn.**
  `usageMonitorRegistry.js:72` đặt `claudecode@local: false` vào chính cái map mà
  `setMonitorEnabled` sẽ spread khi ghi localStorage (`usageMonitorStore.js:121-122`). Chỉ cần sau đó
  người dùng bật/tắt **bất kỳ monitor nào khác** (kể cả trên host khác), toàn bộ map — kèm
  `claudecode@local:false` — được ghi vào `aki-usage-monitor-enabled`. Tắt proxy và khởi động lại app,
  monitor CC local vẫn OFF như thể người dùng đã tự tắt. Đúng cái mà comment `:66-68` tuyên bố là
  không xảy ra ("is not persisted").
- 🟡 Trên companion (điện thoại), watcher `:69-73` chạy cục bộ và ghi thẳng vào ref mirrored mà không
  qua `action()` → bản sao trên phone lệch với host cho tới delta kế tiếp.

---

### B14. Host SSH không reachable / node không có trên PATH — breaker sau N lần fail, log ra sao

**Flow (CC)**

1. `usageMonitor.js:212 invoke('get_agent_usage')` → `agent_usage.rs:420 run_remote_script` →
   `:222 run_interpreter_timeout` → `polling_ssh` (`:64-75`, `ConnectTimeout=10`, `BatchMode=yes`).
2. Hai kết cục khác hẳn nhau:
   - **spawn lỗi / quá 30s** → `Err` (`:234`, `:272-275`) → `?` ở `:420` → IPC reject →
     `usageMonitor.js:332-336` `consecutiveFailCount++`, `>=5` → `haltPolling()` (`:381-387`).
   - **ssh chạy rồi thoát khác 0** (connection refused, ConnectTimeout hết, host key sai,
     `exit 255`) → `agent_usage.rs:433-436` `logger::error` rồi **`return Ok(None)`**.
     Ở JS đây là nhánh "không có cache" (`usageMonitor.js:299-331`), và ngay trước đó `:214`
     đã **`consecutiveFailCount = 0`**.

**Flow (AG)**

3. `agent_usage.rs:561-567` timeout → `Ok(None)`; `:575-589` mọi exit khác 0 → `Ok(None)`.
   Riêng exit 127 / "command not found" được nâng lên `logger::error` **một lần mỗi host mỗi phiên**
   (`ag_node_missing_once` `:401-405`), các tick sau về `debug`.
4. Node được resolve bằng `NODE_BIN_RESOLVER_PREAMBLE` (`:215`) — thử `[ -x path ]` trước, `command -v`
   sau (chống race PATH cold-start).

**Mong đợi vs thực tế** — Lệch nghiêm trọng so với comment `usageMonitor.js:359-363`.

**Phát hiện**

- 🟠 **Breaker gần như không bao giờ nổ đúng ca mà nó được viết ra để chặn.** Sự cố 2026-07-20 mà
  comment `usageMonitor.js:359` trích ("host had stopped accepting TCP") sẽ khiến `ssh` thoát 255 sau
  `ConnectTimeout=10` → `agent_usage.rs:435` trả `Ok(None)` → JS coi là "không có cache",
  `consecutiveFailCount` bị **reset về 0** ở `:214` → poll 30s tiếp diễn vô hạn. Chỉ khi ssh treo quá
  30s (blackhole thuần) mới nổ breaker. Với AG thì **không bao giờ** nổ, vì mọi thất bại đều bị nuốt
  thành `Ok(None)` (`:576`).
- 🟠 **Watchdog vô hiệu hoá breaker khi nó thực sự nổ.** `haltPolling` (`:381-387`) dừng timer nhưng
  không dừng `checkUsage`. Watchdog module-scope (`:106-114`) so `now - lastTickAt > gapThresholdMs`
  (`:407` = 2×interval = 60s); sau khi halt, `lastTickAt` đứng yên → mỗi 7s watchdog gọi
  `onWake('watchdog')` (`:112`) → `:400 checkUsage()` **chạy vô điều kiện** (chỉ `restartPollTimer`
  ở `:401` mới tôn trọng `pollHalted`). Mỗi lần probe lại cập nhật `lastTickAt` (`:399`, `:197`) nên
  chu kỳ ổn định thành **một probe mỗi ~60s tới host đã chết, vĩnh viễn** — đúng cái mà comment
  `:363` khẳng định là "Notably NOT the wake listeners".
- 🟡 Sau khi halt, icon power vẫn **is-on** (`AgentUsageSlot.vue:33`, `enabled` không đổi) trong khi
  thực tế đã ngừng poll; chỉ có chuỗi `error` giải thích. UI nói sai trạng thái.
- 🟡 `pendingRecheck` (`usageMonitor.js:341-344`) có thể gọi `checkUsage()` ngay sau một lần fail,
  cộng thêm 1 fail nữa vào bộ đếm mà không chờ interval.

---

### B15. Cache AG quá 10 ngày — sweep expiry có đụng máy/account khác không?

**Flow**

1. `agUsageCache.js:84-110 loadStore()` → mọi đường vào (`persistAgAccount`, `loadAgAccount`,
   `lastActiveEmailFor`, `listAgAccounts`) đều đi qua đây → `:89 prune(parsed)`.
2. `prune` (`:113-125`) duyệt **toàn bộ** `store.accounts`, xoá entry có
   `nowSec - fetchedAt > EVICTION_TTL_SEC` (`:28` = 10 ngày), ghi lại nếu có thay đổi.

**Mong đợi vs thực tế** — Quét xuyên host nhưng tiêu chí là **thuộc tính của chính entry đó**
(tuổi của nó), không phải "vì host X poll nên xoá của host Y". Đây không phải vi phạm
Regression Guard; comment `:112` nói đúng bản chất. Entry của máy/account khác chỉ bị xoá khi
tự nó quá 10 ngày.

**Phát hiện**

- 🟡 `prune` không dọn `lastActiveEmailByHost` (`:114` chỉ khởi tạo object). Sau khi entry cuối của
  host X hết hạn, `lastActiveEmailFor(X)` vẫn trả email cũ (`:186-188`) → nhánh AG-offline
  `usageMonitor.js:304-320` gọi `loadAgAccount(lastActive, host)` → `null` → rơi vào
  `ag offline no cache`. Không sai kết quả, chỉ là con trỏ treo.
- 🟡 Entry thiếu `fetchedAt` (`v?.fetchedAt` falsy ở `:118`) **không bao giờ** hết hạn — v1/v2
  migration có thể sinh ra loại này (`:78`, `:67` copy nguyên `...v`).

---

### B16. Đổi Usage row 1 hàng (2 slot) ↔ 2 hàng (4 slot)

**Flow**

1. `AppHeader.vue:145-153` → `setTierCount(1|2)` → `usageTierStore.js:12-15` (là `action`, mirror
   được từ phone) → ghi `aki-usage-tier-count`.
2. `AgentUsageSection.vue:37-39` → `ALL_TIER_ROWS.slice(0, tierCount)`; `:41-45` đổi chiều cao.
3. Xuống 1 hàng: `AgentUsageSlot` C/D unmount. Vì monitor sống trong `monitorScope`
   (`usageMonitorRegistry.js:23`, `:59`) nên watcher/timer của monitor **không** bị teardown —
   đúng chủ ý (`usageMonitor.js:12`).
4. `AgentUsage.vue:561,570` dọn `ccClockTimer`/`agoTimer` trong `onUnmounted` ✅ (không rò timer UI).
5. Lên 2 hàng lại: `slotTargets` giữ nguyên trong localStorage → C/D trở lại đúng cấu hình cũ,
   `getMonitor` trả lại instance cũ kèm dữ liệu.

**Mong đợi vs thực tế** — Khớp; không mất cấu hình, không mất reading.

**Phát hiện**: 🟡 — thu về 1 hàng **không** làm ngừng poll của C/D. Nếu D đang trỏ vào một host
remote, app vẫn tiếp tục 2 vòng SSH tới host đó dù thẻ không còn trên màn hình, và người dùng cũng
không còn nút power nào để tắt (nút nằm trong chính slot vừa bị ẩn). Cùng gốc với B6/B7.

---

### B17. Reload app — state monitor/slot có khôi phục đúng không?

**Flow**

1. `usageSlotStore.js:44-52` — đọc `aki-usage-slot-targets`, **merge với DEFAULTS theo từng slot**
   (`:49 {...dflt, ...parsed[id]}`) → field mới thêm sau này không làm hỏng record cũ.
2. `usageMonitorStore.js:68-70` — đọc `aki-usage-monitor-enabled`; entry thiếu → mặc định ON (`:110`).
3. `usageTierStore.js:5-7` — `aki-usage-tier-count`, guard `NaN`/`<1`.
4. Per-slot, không mirror: `aki-usage-slot-<id>-show-email` (`AgentUsageSlot.vue:135-136`),
   `aki-usage-slot-<id>-viewing-account` (`:152-153`).
5. AG: monitor khởi tạo đọc lại cache ngay (`usageMonitor.js:144-147`) → dropdown và
   `activeEmail` có sẵn trước fetch đầu tiên; thẻ hiện số cached rồi mới live.
6. CC: `data=null` cho tới fetch đầu → `uiStatus='loading'` (`AgentUsage.vue:325`).

**Mong đợi vs thực tế** — Khôi phục đúng.

**Phát hiện**: 🟡 — `showEmail` và `slotViewingEmail` được đọc/ghi thẳng localStorage trong component
(`AgentUsageSlot.vue:135-163`), không nằm trong `src/store/*.js` nên **không được mirror**
(`services/mirror.js:22-28` chỉ gom `isRef` export của `store/*.js`). Mac và phone sẽ ghim hai account
AG khác nhau cho cùng một slot — trái với tinh thần "slot target là một thiết lập của Mac" đã áp
dụng cho `slotTargets` (`usageSlotStore.js:11-13`).

---

### B18. Quota 7 ngày đạt 100% → số 5h bị dim

**Flow**

1. `AgentUsage.vue:632 cc7dPct` ← `data.rate_limits.seven_day.used_percentage`.
2. `:612 cc5hMutedBy7d = cc7dPct !== null && cc7dPct >= 100`.
3. `:613 cc5hColorClass` → `'color-muted'`; template `:194-195` thêm class `is-muted` + tooltip
   "Dimmed - the 7-day pool is full (100%)…".
4. CSS tồn tại: `:1267 .cc-bar-pct.color-muted`, `:1271 .cc-usage-bar.is-muted`,
   `:1305 .cc-progress-fill.color-muted`.
5. AG có cơ chế song song: `:481 gemini5hMutedByWeekly`, `:484 MUTED_BY_WEEKLY_REASON`,
   truyền vào `UsageCircle` `:243-244`, `:263-264`.

**Mong đợi vs thực tế** — Khớp. `cc7dPct === null` (không có seven_day) thì không dim
(`:612` kiểm null tường minh) — đúng.

**Phát hiện**: không phát hiện vấn đề.

---

### B19. Điện thoại đã pair: bật/tắt monitor từ phone (mirrored state)

**Flow (chiều điều khiển — phone → Mac)**

1. Phone chạm icon power: `AgentUsageSlot.vue:34 src.monitor.toggle()` →
   `usageMonitorRegistry.js:50-54`. `locked` đọc `claudeMode` (mirrored ref) ✅;
   `enabled` đọc `monitorEnabled` (mirrored ref) ✅ → giá trị lật là đúng.
2. `setMonitorEnabled` là `action('usageMonitorStore.setMonitorEnabled', …)`
   (`usageMonitorStore.js:120`) → trên companion `services/action.js:38-41` trả stub gửi
   `{t:'intent', key, args:[id, value]}`; Mac chạy hàm thật, đổi `monitorEnabled`,
   ghi localStorage của Mac.
3. `services/mirror.js:22-28` gom `usageMonitorStore.monitorEnabled` (là `isRef` export) →
   `:134-146` host watch deep → delta → `:161-171` phone apply vào **chính ref của nó**
   → cả hai màn hình đồng bộ.
4. `slotTargets` (`usageSlotStore.js:59`) và `tierCount` (`usageTierStore.js:7`) cũng là ref export
   → mirror + action đầy đủ.

**Flow (chiều dữ liệu — số hiển thị trên phone)**

5. `usageMonitor.js:127-135` — `data/loading/error/stale/dataAt/accounts` là ref **cục bộ trong
   composable**, không phải export của `store/*.js` → **không được mirror**.
6. Trên companion: `utils/scheduler.js:16` `hostInterval` = no-op (trả `null`), `:19 onHostBoot` = no-op.
   Nên `usageMonitor.js:431` (boot fetch) không chạy và `:374` không tạo timer.
7. Cách duy nhất phone lấy số: `checkUsage()` → `utils/tauri.js:19` RPC `{t:'invoke'}` →
   `services/hostInvoke.js:26-38` Mac chạy `get_agent_usage` thật → trả kết quả. Được kích bởi:
   (a) `usageMonitor.js:430` khi người dùng bật monitor (nhánh `watchBooted`),
   (b) `:443-451` khi `manualRefreshCount` (mirrored ref) tăng,
   (c) `:396-402 onWake` từ `visibilitychange`/`focus` — hai listener DOM này **không** bị gate
   theo `isHost` (`:101-104`), nên vẫn chạy trên phone; watchdog thì không (dùng `hostInterval`).

**Mong đợi vs thực tế** — Bật/tắt monitor: đồng bộ đúng hai chiều. Nhưng **số** thì không đồng bộ:
phone tự gọi RPC chứ không nhận qua mirror.

**Phát hiện**

- 🟠 **Mở dashboard trên phone, các thẻ Usage trống cho tới khi có tương tác.** Lúc trang vừa load,
  không có `onHostBoot`, không có timer, và `focus`/`visibilitychange` chưa từng phát (trang load
  trong trạng thái đã focus, sự kiện không phát lại) → `data === null` →
  `AgentUsage.vue:324-330` hiện "No data - waiting for next session" / "Not connected".
  Chỉ khi người dùng chuyển tab rồi quay lại, bấm Reload (`AgentUsage.vue:29` → `@retry` →
  `monitor.refresh`), hoặc Mac tình cờ bấm Refresh (`refreshStore.js:38 triggerManualRefresh` →
  mirror `manualRefreshCount`) thì số mới xuất hiện. Comment `usageMonitor.js:429`
  ("the real numbers arrive mirrored from the host anyway") **không đúng** — `data` không nằm trong
  tập được mirror.
- 🟡 Sau lần fetch đầu, phone **không tự làm mới** (không có interval, không có watchdog): số trên
  phone đứng yên cho tới lần focus/refresh kế tiếp, trong khi Mac cập nhật mỗi 30s. Hai màn hình
  hiển thị hai con số khác nhau mà không có dấu hiệu nào.
- 🟡 `monitor.accounts` (dropdown AG) trên phone đọc `agUsageCache` của **localStorage phone**
  (`usageMonitor.js:143-147`) — trống → phone không có lịch sử account nào để chọn.

---

### B20. Hai monitor cùng chạy poll đúng lúc app bị đóng

**Flow**

1. Hai monitor tick gần nhau → 2 lần `invoke('get_agent_usage')` → 2 lần
   `tauri::async_runtime::spawn_blocking` (`agent_usage.rs:367`).
2. Cùng host → `host_lock` (`:48-55`) tuần tự hoá: cái thứ hai **chặn trong thread blocking-pool**
   cho tới khi cái đầu xong. Đồng hồ timeout bắt đầu **sau** khi lấy được lock (`:228` trước `:264`)
   → không có chuyện timeout ăn oan thời gian chờ lock. Khác host → 2 mutex khác nhau, song song.
3. Đóng app: `src-tauri/src/lib.rs:134-142` — hook `RunEvent::Exit` **chỉ** gọi `pty::shutdown()`.
   Không có gì huỷ các `ssh`/`sh` con của `run_interpreter_timeout`.
4. JS: monitor cố ý không có teardown (`usageMonitor.js:12`); promise `checkUsage` chưa resolve chỉ
   ghi vào ref của một context đã chết — vô hại.

**Mong đợi vs thực tế** — Không có race về dữ liệu (`isChecking` `:189` + `pendingRecheck` `:341`
đảm bảo mỗi monitor tối đa 1 request; `persistAgAccount` là read-modify-write **đồng bộ**
`agUsageCache.js:138-162` nên hai monitor không thể lost-update lẫn nhau).

**Phát hiện**

- 🟡 **Tiến trình mồ côi khi thoát app.** `ssh <host> sh` / `ssh <host> <node resolver>` đang chạy
  lúc `RunEvent::Exit` không bị giết (`lib.rs:139-141` chỉ lo PTY). Vòng poll/kill của
  `run_interpreter_timeout` (`agent_usage.rs:264-281`) chết theo process, nên con `ssh` sống tiếp
  cho tới khi script remote tự kết thúc. Với CC còn có `AKI_CLAUDE_TMO` chặn (`:182-197`, ≤45s), với
  AG (`Interpreter::Node`, `:105`) **không** có `bounded_remote_sh` — chỉ `run_remote_script_bounded`
  của statusline mới dùng nó (`:36-43`). Đây đúng lớp lỗi mà comment `lib.rs:135-138` đã mô tả cho
  PTY, chỉ là chưa áp cho đường usage.
- 🟡 Nhiều host × 2 agent (xem B6) ⇒ nhiều `spawn_blocking` giữ thread trong lúc chờ `host_lock`;
  với 4-5 host thì mỗi chu kỳ 30s có tới 8-10 thread blocking-pool bị chiếm cùng lúc.

---

## Tổng hợp

| Case | Mức | Vấn đề | Vị trí |
|---|---|---|---|
| B10 | 🔴 | Cùng email + 2 sourceType trên 1 host: 1 record bị xoá, ghim trả về số của sourceType khác | `agUsageCache.js:151-152`, `:179-182`, `:203-206`; `scripts/get-antigravity-usage.js:605-621` |
| B6/B5/B16 | 🟠 | Mỗi host được trỏ tới tự bật 2 monitor (AG+CC) mặc định ON, không bao giờ evict | `AgentUsageSlot.vue:107`; `usageMonitorStore.js:108-111`; `usageMonitorRegistry.js:17,44` |
| B7 | 🟠 | Đổi host tạm thời xoá vĩnh viễn ghim account AG của slot | `AgentUsageSlot.vue:194,199,203-208` |
| B12 | 🟠 | Email mới + quota account cũ hiển thị cùng thẻ, không cảnh báo | `scripts/get-claudecode-usage.sh:3,63-101`; `agent_usage.rs:501-520` |
| B13 | 🟠 | Trạng thái do proxy ép được persist ngoài ý muốn qua spread của `setMonitorEnabled` | `usageMonitorRegistry.js:72`; `usageMonitorStore.js:121-122` |
| B14 | 🟠 | Breaker không nổ với host thoát nhanh (`Ok(None)` + reset counter) | `agent_usage.rs:433-436,576`; `usageMonitor.js:214` |
| B14 | 🟠 | Watchdog probe lại host đã halt mỗi ~60s, vô hiệu hoá breaker | `usageMonitor.js:106-114,396-402,381-387` |
| B19 | 🟠 | Thẻ Usage trên phone trống tới khi có focus/reload; `data` không nằm trong mirror | `usageMonitor.js:127-135,429-431`; `utils/scheduler.js:16-19`; `services/mirror.js:22-28` |
| B2 | 🟡 ×2 | Cờ legacy remote bám nhầm host / mất khi chưa có SSH host | `usageMonitorStore.js:41-51,66-73,87-100`; `sshStore.js:7` |
| B1/B3 | 🟡 | Monitor thừa được tạo & đăng ký wake vĩnh viễn | `AgentUsageSlot.vue:107`; `usageMonitor.js:404-408` |
| B14 | 🟡 | Icon power vẫn ON sau khi halt | `AgentUsageSlot.vue:33`; `usageMonitor.js:381-387` |
| B15 | 🟡 ×2 | `lastActiveEmailByHost` không được prune; entry thiếu `fetchedAt` không hết hạn | `agUsageCache.js:113-125,186-188` |
| B17 | 🟡 | `showEmail` / `viewingEmail` không mirror → Mac và phone lệch | `AgentUsageSlot.vue:135-163`; `services/mirror.js:22-28` |
| B19 | 🟡 ×2 | Phone không tự refresh; dropdown AG trên phone rỗng | `utils/scheduler.js:16`; `usageMonitor.js:143-147` |
| B20 | 🟡 ×2 | `ssh` mồ côi khi thoát app (AG không bounded); áp lực thread blocking-pool | `lib.rs:134-142`; `agent_usage.rs:36-43,105` |

**Không phát hiện vấn đề**: B3 (bug đã fix thật), B4, B8, B9 (chiều đa máy), B11 (regression 1.9.3
đã đóng đúng), B18.
## Cụm C — Remote Control

Phạm vi: đọc code thật theo từng use case, dóng flow phone → WS → host → state/IPC → mirror ngược.
Mọi khẳng định kèm `file:line`. Mức độ: 🔴 nghiêm trọng · 🟠 đáng sửa · 🟡 ghi nhận.

File chính đã đọc: `src-tauri/src/web_server.rs` (1006 dòng), `src-tauri/src/pty.rs` (phần write/scrollback),
`src/services/{bridge,mirror,intents,action,hostInvoke,ptyBridge,index}.js`,
`src/store/{remoteActions,dialogStore,logStore,projectStore,refreshStore,syncCheckStore}.js`,
`src/composables/{useRemoteControl,useCompanionPairing,usePtyTerminal,useSync,useLogs,useProjectTasks,useProjectConfig}.js`,
`src/components/{PairingGate,DialogHost,ProjectTable,AppHeader,AppConsole}.vue`, `src/App.vue`,
`src/constants/protocol.js`, `src/boot/roleStamp.js`.

---

### C1. Bật Remote Control, phone mở trang lần đầu, nhập mã 6 số ĐÚNG → pair, lưu token

**Flow**
1. Mac: `AppHeader.vue:66` toggle → `useRemoteControl.js:69 start()` → `invoke('start_companion_server')`
   → `web_server.rs:698-710`: sinh mã 6 số (`generate_pairing_code`, `web_server.rs:199-202`),
   `pair_failures = 0`, `enabled = true`. Listener TCP đã bind từ `init()` lúc app khởi động
   (`web_server.rs:246-260`), lệnh này **chỉ lật cờ**, không rebind.
2. Phone mở `http://<lan-ip>:1421` → `release_asset_handler` (`web_server.rs:440`) qua
   `reject_if_disabled()` (`web_server.rs:333`) → serve SPA nhúng.
3. `main.js` → `boot/roleStamp.js:16` không stamp (không có `__TAURI_INTERNALS__`) → `bridge.js:26`
   `isHost = false`. `App.vue:69 initRemote()` → `services/index.js:31-42` → `connect()`
   (`bridge.js:235`) với token rỗng → `web_server.rs:498-506` không tìm thấy device → close 4001
   → `bridge.js:261` `connectionState='unpaired'` → `useCompanionPairing.js:29-35` hiện gate.
4. Nhập mã → `PairingGate.vue:69 onSubmit` → `useCompanionPairing.js:44 submitCode` → `bridge.js:295
   pairDevice()` → `POST location.origin/pair` → `web_server.rs:599`: so mã, reset `pair_failures`
   (`:630`), tạo `PairedDevice` token 128-bit (`:642`), ghi đĩa qua `spawn_blocking`
   (`:649`), rollback nếu ghi lỗi (`:658`).
5. `bridge.js:308-309` lưu localStorage + `connect()` → `ws?role=companion&token=…` →
   `web_server.rs:507-508` → `handle_companion_socket` → `notify_host_companion_connected`
   (`web_server.rs:560` → `:109-112`) → host `mirror.js:160-162` `broadcastFull()` →
   `mirror.js:102-109` gửi `init` → phone `mirror.js:163-174 applyFrame`. Song song
   `ptyBridge.js:69-71` push scrollback.

**Mong đợi vs thực tế**: khớp. `connect()` không bị chặn bởi guard `bridge.js:237` vì socket cũ đã
CLOSED sau 4001.

**Phát hiện**
- 🟡 `/pair` không kiểm tra `Origin` và CORS là `CorsLayer::permissive()` (`web_server.rs:279`).
  Một trang web bất kỳ mà phone/máy khác trong LAN đang mở có thể POST `/pair` tới `192.168.x.x:1421`.
  Xác suất đoán trúng mã ≤ 10 lần là 1e-5 nên **không phải lỗ hổng chiếm quyền thực tế**, nhưng nó là
  đường DoS: đốt hết 10 lần thử là relay tự tắt (xem C2), user tưởng app hỏng.
- 🟡 `pairDevice` (`bridge.js:304-306`) chỉ map 401 → "Invalid pairing code"; 429 (đã khoá) hiện ra
  chuỗi kỹ thuật `Pairing failed (429)`, không nói cho user biết đã bị khoá và phải bật lại trên Mac.

---

### C2. Nhập mã SAI 10 lần liên tiếp → tự khoá, mã bị wipe

**Flow**: `web_server.rs:610` sai mã → `pair_failures.fetch_add(1)` (`:611`) →
`if failures >= MAX_PAIR_FAILURES (=10, :48)` → `enabled=false` (`:616`),
`pairing_code.clear()` (`:617`), trả 429 (`:622-626`). Reset về 0 chỉ ở
`start_companion_server` (`:704`) và khi pair thành công (`:630`).

**Mong đợi vs thực tế**: đúng như CHANGELOG 1.19.0 (dòng 60) và doc bảng C4. Lần sai thứ 10 mới khoá
(lần 1..9 trả 401). Sau khi khoá, `/ws`, `/pair` và toàn bộ HTTP đều bị chặn
(`web_server.rs:333-344`, `:494`, `:601`) — "off means off" thật.

**Phát hiện**
- 🟡 Bộ đếm là **toàn cục**, không theo IP. Bất kỳ ai trong LAN cũng đốt được quota của user hợp lệ
  (`web_server.rs:78`, `:611`). Đây là đánh đổi có chủ ý (comment `:74-78` nói rõ), nhưng hệ quả DoS
  không được ghi ở đâu.
- 🟠 Khi bị khoá, phone của **thiết bị đã pair từ trước** cũng bị đá ra và **mất token** — xem C5/C4,
  cùng một gốc: close code 4001 dùng chung cho "sai token" và "server tắt".

---

### C3. Phone đã pair, khoá màn hình 5 phút rồi mở lại → reconnect im lặng, state có đúng không

**Flow**
- Ping loop `bridge.js:171-180`: 15s/ping, 5s timeout. iOS/Android throttle timer khi background.
- Khi socket chết: `close` (`bridge.js:258`) → `clearPingTimers` + `rejectAllPending` →
  code ≠ 4001 → `'closed'` → `scheduleReconnect` backoff 1s→10s (`bridge.js:223-230`).
- Reconnect thành công → relay `notify_host_companion_connected` (`web_server.rs:560`) →
  host `broadcastFull()` (`mirror.js:160-162`) → phone nhận `init` đầy đủ mọi key store
  (`mirror.js:102-109`) → `applyFrame` (`mirror.js:114-131`). PTY: `ptyBridge.js:69-71 pushScrollback`.

**Mong đợi vs thực tế**: state **đúng** sau reconnect — đây là điểm mạnh của thiết kế (không có
delta-log, chỉ snapshot toàn phần). `App.vue:7` `v-if="ready"` unmount dashboard khi mất kết nối nên
không có component nào invoke qua socket đóng.

**Phát hiện**
- 🟡 Rò timer nhỏ: `bridge.js:175` gán `pingTimeoutTimer = setTimeout(...)` mỗi tick **mà không clear
  handle cũ**. Bình thường không xảy ra (timeout 5s < interval 15s), nhưng đúng kịch bản C3 —
  tab bị throttle rồi resume, trình duyệt bắn dồn nhiều tick — sẽ có timer mồ côi còn sống, và nó gọi
  `ws.close()` (`bridge.js:177`) trên **biến `ws` module-scope**, tức có thể đóng socket vừa
  reconnect xong → flap thêm một nhịp. Tự lành, nhưng là lỗi thật.
- 🟠 State UI **theo màn hình** bị snapshot ghi đè: `logStore.activeLogProjectId` / `isLogExpanded`
  (`logStore.js:5-6`) là ref được mirror, nhưng `useLogs.toggleProjectLog` (`useLogs.js:50-57`)
  **không** bọc `action()` → phone bấm mở log của project X chỉ set copy cục bộ; `init`/`delta` kế
  tiếp (`mirror.js:126`) ghi đè lại bằng giá trị của Mac. Sau mỗi lần reconnect, panel log trên phone
  nhảy về đúng trạng thái đang mở trên Mac.

---

### C4. Restart app trên Mac trong khi phone đang mở

**Flow**
1. App thoát → process chết → listener :1421 biến mất → phone `close` code 1006 → `'closed'` →
   backoff reconnect (`bridge.js:223-230`), tối đa 10s/lần.
2. App khởi động lại: `web_server::init` (`web_server.rs:211`) đọc `companion-devices.json`
   (token cũ **vẫn còn**, `:216-222`) và `serve_forever` bind lại. Nhưng
   `RelayState::new()` đặt **`enabled: AtomicBool::new(false)`** (`web_server.rs:90`) và
   **không có gì bật lại** — `start_companion_server` chỉ được gọi từ tay user
   (`useRemoteControl.js:74`); `syncFromHost` (`useRemoteControl.js:119-134`) chỉ **đọc** trạng thái.
3. Phone retry trong ≤10s → `ws_handler` role=companion → `enabled == false` →
   `close_with_code(socket, 4001, "remote control is disabled on the host")` (`web_server.rs:494-496`).
4. `bridge.js:261-265`: code 4001 + không phải host → `'unpaired'` →
   `useCompanionPairing.js:29-35`: **`clearDeviceToken()`** → token trong localStorage bị xoá.

**Mong đợi vs thực tế**: doc `docs/feat/remote-control.md:30` hứa *"That phone reconnects silently
after that (no code, no QR) — across app restarts on both ends"*, CHANGELOG 1.19.0 dòng 60 hứa
*"reconnects silently... until revoked"*. **Thực tế: mọi lần restart app trên Mac đều xoá token của
mọi phone đang mở**, vì (a) `enabled` không persist, (b) 4001 không phân biệt "token sai" với
"server đang tắt". Device vẫn nằm trong `companion-devices.json` (chưa revoke) nhưng phone đã vứt
token đi rồi → phải đi tới Mac đọc mã 6 số để pair lại.

**Phát hiện**
- 🔴 **Token bị wipe sai ngữ cảnh sau mỗi lần restart Mac.**
  Gốc: `web_server.rs:90` (`enabled` mặc định false, không persist) + `web_server.rs:494-496`
  (dùng chung 4001) + `bridge.js:261-265` + `useCompanionPairing.js:31-33`.
  Bảng B3/B4 trong `docs/feat/remote-control.md:146-148` mô tả đúng cơ chế nhưng **mâu thuẫn** với
  lời hứa ở `docs/feat/remote-control.md:30`. Hai hướng sửa tối thiểu: dùng close code riêng cho
  "disabled" (vd 4003) và chỉ `clearDeviceToken()` khi là 4001 thật; hoặc persist `enabled`.
- 🟡 Trong lúc Mac chưa bật lại, phone bám vòng reconnect 10s vô hạn — không tốn gì đáng kể, nhưng
  mỗi vòng lại đá `clearDeviceToken()` (idempotent) và ghi lại error text.

---

### C5. Tắt Remote Control (Off) khi phone đang kết nối

**Flow**: `AppHeader.vue:66` → `useRemoteControl.js:88 stop()` → tắt tailscale serve trước
(`:95-100`) → `invoke('stop_companion_server')` → `web_server.rs:716-731`:
`enabled=false` (`:719`) rồi `companions.drain()` (`:721`) và gửi `Message::Close(4001)` cho **từng**
socket. `handle_companion_socket` (`web_server.rs:575-579`) đẩy Close ra dây → phone đóng.

**Mong đợi vs thực tế**
- Cắt phone **ngay lập tức**: ✅ đúng.
- Port "ngừng phục vụ": listener TCP **vẫn bound** (cố ý, `web_server.rs:235-238` giải thích tránh
  bind-race), nhưng mọi route đều 503/4001: fallback release `:441`, fallback dev `:363`,
  `/pair` `:601`, `/ws` `:494`. Nên "off means off" ở tầng nội dung là đúng.

**Phát hiện**
- 🔴 (cùng gốc C4) Off → phone nhận 4001 → **xoá token**. Bật lại On sinh mã mới và phone phải
  pair lại, dù `companion-devices.json` chưa hề đổi. Đây chính là thứ biến "toggle Off/On" thành
  "revoke toàn bộ thiết bị" ngoài ý muốn — trái với `revoke_device` được thiết kế rất cẩn thận theo
  quy tắc scoped-clear (`web_server.rs:923-956`).
- 🟡 `stop()` (`useRemoteControl.js:105-111`) clear `pairingCode`/`urls` trong khối `finally` **kể cả
  khi RPC ném lỗi** → UI báo Off trong khi relay có thể vẫn On. `syncFromHost` chỉ chạy một lần
  (`useRemoteControl.js:118-120 synced`) nên không tự sửa lại cho tới khi reload webview.
- 🟡 Port 1421 vẫn mở TCP nên vẫn bị scan thấy; đây là quyết định có chủ ý, chỉ ghi nhận.

---

### C6. Hai phone cùng pair và cùng mở một lúc

**Flow**: mỗi phone là một `conn_id` riêng (`web_server.rs:507`, map `companions`
`web_server.rs:64`). Mọi frame host → `broadcast_to_companions` (`web_server.rs:95-100`) gửi cho tất
cả. Mọi frame companion → `forward_to_host` (`:102-107`) đổ vào **một** `host_tx`.
Mỗi lần một phone join, relay bắn `companion-connected` (`:560`) → host `broadcastFull()`
(`mirror.js:161`) tới **toàn bộ** companion.

**Mong đợi vs thực tế**: state ứng dụng nhất quán — host là SSOT duy nhất, ai bấm sau thì thắng, và
các guard thật đều nằm ở host (`useSync.js:45-48` chặn sync trùng, `dialogStore.js:59` first-answer-wins,
`remoteActions.js:151-153` bỏ reorder nếu tập id không khớp). Không có xung đột dữ liệu.

**Phát hiện**
- 🟠 **State UI per-screen bị ghi đè chéo giữa các phone.** Phone B vừa connect → relay
  `companion-connected` → host `broadcastFull()` gửi cho **cả** A và B (`mirror.js:158-162`, comment
  ở `:159` thừa nhận "we ignore its `id` here"). Phone A đang mở log project X / đang cuộn panel bị
  reset về trạng thái của Mac: `logStore.activeLogProjectId`, `isLogExpanded` (`logStore.js:5-6`),
  và cả `usageSlotStore.slotTargets` (`usageSlotStore.js:59`) — vốn là lựa chọn hiển thị của từng
  màn hình. Không mất dữ liệu, nhưng là "giật màn hình" khó truy nguyên với user.
- 🟡 Cùng gốc: `refreshStore.js:21-23` có `watch(refreshSettings)` ghi `localStorage`. Trên companion,
  delta mirror kích hoạt watch này → **localStorage của phone bị ghi đè bằng cấu hình của Mac**.
  `applying` guard (`mirror.js:112`) chỉ chặn watcher của mirror trên host, không chặn watcher ứng dụng.
- 🟡 Cả hai phone dùng **chung một** PTY và **chung một** `pendingDialog` — đúng thiết kế "một phiên Mac",
  không phải lỗi, nhưng cần nhớ khi đọc C7/C11/C12.

---

### C7. PUSH có `--delete` từ phone → dialog trên CẢ Mac lẫn phone

**Flow**
1. Phone bấm PUSH → `ProjectTable.vue:312` `requestSync` → `action()` stub (`action.js:37-40`) gửi
   `{t:'intent', key:'remoteActions.requestSync', args:[id,'push']}`.
2. Relay `forward_to_host` → host `intents.js:57-59 dispatchIntent` → registry
   (`intents.js:27-37`, key = `<file>.<export>`) → `remoteActions.js:43 requestSync` → `byId`
   → `useSync.js:40 startSync` (chạy **trên host**).
3. `useSync.js:66` xác định `isDeleteOp` → `invoke('get_sync_delete_preview')` (`:81`) →
   `useSync.js:164 askConfirm({kind:'typed', requireText: project.name})` →
   `dialogStore.js:39-45`: tạo id `dlg-<ts>-<seq>`, cất `resolve` vào `_waiters`, set `pendingDialog`.
4. `pendingDialog` là ref của `src/store/*.js` → mirror phát hiện tự động (`mirror.js:22-27`) →
   delta → **cả hai màn hình** `DialogHost.vue:26-42` mở Swal.
5. Trả lời từ **phone**: `DialogHost.vue:83` → guard `:89` (pendingDialog còn đúng id) → `openId=null`
   → `resolveDialog(id, {confirmed, typed})` → action stub → intent → host `dialogStore.js:58-67`
   → clear `pendingDialog`, gọi waiter → `useSync.js:182` kiểm tra lại `answer.typed === project.name`
   **trên host** (đúng — phone không thể bỏ qua bằng cách bỏ preConfirm).
6. Trả lời từ **Mac**: `resolveDialog` là hàm thật → resolve ngay; delta `pendingDialog=null` →
   phone `DialogHost.vue:32-35` `Swal.close()` → promise của phone settle → guard `:89` thấy
   `pendingDialog` đã null → return, **không gửi answer thứ hai**.
7. Bấm gần như đồng thời: bên nào chạy `resolveDialog` trước sẽ null hoá `pendingDialog`; lệnh của
   bên thua vào `dialogStore.js:59` gặp `!d` → return. **Không có double-resolve.**

**Mong đợi vs thực tế**: đúng, logic first-answer-wins chặt. Đã soi kỹ cả 4 nhánh, không thấy đường
nào gọi waiter hai lần.

**Phát hiện**
- 🟠 **`pendingDialog` chỉ có MỘT slot; `askConfirm` ghi đè vô điều kiện.**
  `dialogStore.js:43-44` set `pendingDialog.value = {...spec, id}` mà không kiểm tra đã có dialog
  đang chờ. Nếu có dialog thứ hai (ví dụ: Mac bấm PUSH --delete project A, phone bấm PUSH --delete
  project B trong lúc A còn đang hỏi — `useSync.js:45-48` chỉ chặn **cùng một** project), thì:
  - dialog A biến mất khỏi màn hình mà không ai trả lời;
  - `_waiters` giữ resolve của A **mãi mãi** (`dialogStore.js:25`, chỉ xoá trong `resolveDialog`);
  - `startSync` của A treo tại `await askConfirm` (`useSync.js:164`) → `projectRuntime[A].syncing`
    kẹt `true` vĩnh viễn (đặt ở `useSync.js:50`, chỉ clear ở `:70` hoặc `finally :250` — cả hai đều
    không tới được) → project A không sync được nữa cho tới khi restart app.
  Đây là rò bộ nhớ + deadlock UI thật, không phải giả định: cả `requestAgLogout`
  (`remoteActions.js:85`), `requestRemoveProject` (`:228`), `applySshHostsChange` (`:296`) và 3 chỗ
  trong `useSync.js` đều gọi `askConfirm` không hàng đợi.
- 🟡 `DialogHost.vue:37` `if (d.id === openId) return` chỉ so id; nếu host đổi **nội dung** dialog
  cùng id (không xảy ra hiện tại) sẽ không cập nhật. Ghi nhận, không phải lỗi hiện hữu.

---

### C8. Phone bấm Remove Project

**Flow**: `useProjectConfig.js:255-256` → `requestRemoveProject(id, name)` → intent →
host `remoteActions.js:227-240`: `askConfirm` mirrored → nếu Yes gọi `removeProject`
(`remoteActions.js:208-215`: lọc `projects.value`, xoá `projectRuntime[id]`, `saveProjectsList`) →
delta về mọi màn hình. Trên phone `action()` trả `undefined` ngay (`action.js:39-40`) →
`removed` falsy → `useProjectConfig.js:257` không đóng modal.

**Mong đợi vs thực tế**: known limitation trong CHANGELOG dòng 52 **mô tả đúng** phần "modal không tự
đóng". Nhưng phạm vi thực tế **rộng hơn** một chút so với văn bản đó:

**Phát hiện**
- 🟠 **Project đã xoá có thể sống lại từ phone.** Modal không đóng ⇒ `editingProject`
  (`useProjectConfig.js:8`) vẫn giữ nguyên object của project vừa bị xoá. User bấm **Save** (nút vẫn
  ở đó) → `saveConfig` (`:164`) → `applyProjectConfig` (`remoteActions.js:166`) → host
  `findIndex` không thấy → nhánh `isNew` (`:187-198`) **push lại project vào `projects.value`** và
  `saveProjectsList()`. Project quay lại danh sách. CHANGELOG khẳng định "Nothing is lost and nothing
  is left half-removed" — đúng về dữ liệu file, nhưng đường sống-lại này không nằm trong phạm vi đã
  tuyên bố.
- 🟡 Khi xoá từ phone, `appendGlobalLog("REMOVE", ...)` (`useProjectConfig.js:260`) không chạy ở đâu
  cả (phone bỏ qua vì `removed` falsy; host không log trong `remoteActions.js:208-215`) → global log
  trên Mac **không ghi nhận** lần xoá đó. `activeLogProjectId` cũng không được clear.

---

### C9. Phone sửa note/checklist của task (PERSIST-1)

**Flow**: `ProjectTasksModal` → `useProjectTasks.js:90/104/112` → `applyTaskEdit(project.id, {tasks})`
→ intent → host `remoteActions.js:135-141`: `byId(projectId)`, gán **chỉ** `patch.tasks`/`patch.notes`,
rồi `saveProjectsList()` (`useProjectConfig.js:137` ghi `projects.value` **của host**).

**Mong đợi vs thực tế**
- Sống sót qua reconnect: ✅. Sửa nằm trên ref của host → `broadcastFull` sau này phát lại chính
  giá trị mới, không còn cảnh "note tự revert" như bản trước (comment `remoteActions.js:126-134`).
- Project khác bị ghi đè: ✅ không. `byId` + gán từng field, đúng quy tắc multi-entity của CLAUDE.md.

**Phát hiện**
- 🟠 **Object project mà modal đang giữ bị "detach" sau mỗi delta.** Trên companion,
  `mirror.js:126` gán `target.value = decode(encoded)` — tức `projectStore.projects` bị thay bằng
  **mảng object hoàn toàn mới**. Nhưng `tasksProject` (`useProjectTasks.js:10`, set ở `:13`) và
  `editingProject` (`useProjectConfig.js:8`) là ref composable **không** được mirror, vẫn trỏ vào
  object cũ. Hệ quả trên phone:
  - modal Tasks ngừng phản ánh thay đổi mirror về (mở modal, Mac sửa task, phone không thấy);
  - lần sửa kế tiếp gửi `patch.tasks` lấy từ **snapshot cũ** → ghi đè mất thay đổi vừa xảy ra ở phía
    Mac trong khoảng thời gian modal đang mở (last-writer-wins trên toàn mảng `tasks`, không phải
    trên từng task).
  Đây là biến thể còn sót của chính lớp bug PERSIST-1, chỉ hẹp hơn (giới hạn trong thời gian modal mở).
- 🟡 Mọi thay đổi bất kỳ project nào đều làm dirty key `projectStore.projects` (deep watch,
  `mirror.js:137-144`) → phát lại **toàn bộ** mảng projects, không phải diff theo project.

---

### C10. Phone kéo thả sắp xếp project

**Flow**: `ProjectTable.vue:65-70` `draggable="true"` + `dragstart/dragover/drop/dragend`;
`onRowDragOver` (`:384-404`) ghi trực tiếp `projects.value = arr` (mutation lạc quan cục bộ);
`onRowDragEnd` (`:410-420`) → `reorderProjects(ids)` → host `remoteActions.js:149-156`:
map id → object, **bỏ qua** nếu độ dài không khớp, gán `projects.value = reordered`, persist.

**Mong đợi vs thực tế**: phần host **đúng và an toàn** (không bao giờ ghi mảng ngắn hơn). Nhưng:

**Phát hiện**
- 🟠 **Trên phone thao tác này thực tế không dùng được.** Cơ chế là HTML5 Drag & Drop
  (`dragstart`/`dragover`/`drop`) — trình duyệt di động (iOS Safari, Android Chrome) **không phát**
  các sự kiện này từ chạm; ngoài ra `onRowDragStart` còn yêu cầu `isHandleMouseDown` được set từ
  `mousedown` (`ProjectTable.vue:365-368`, `:377-382`), vốn không có trong luồng touch. Không có
  pointer-events fallback ở đâu trong file. `docs/feat/remote-control.md:320-321` liệt kê
  "drag-reorder" như một call site đã được đưa vào seam — đúng về mặt seam, nhưng trên phone gesture
  không bao giờ khởi động.
- 🟡 Nếu delta chạm `projectStore.projects` giữa lúc đang kéo (vd một sync vừa xong cập nhật
  `last_sync_time`), `mirror.js:126` thay cả mảng → thứ tự đang kéo dở bị reset ngay giữa chừng.

---

### C11. Phone mở In-App Terminal, gõ lệnh, output có về cả 2 màn không

**Flow**
- Mở tab → `usePtyTerminal.js:254 start()` → `invoke('pty_spawn')` (companion → `utils/tauri.js:18`
  → `bridge.request()` → `hostInvoke.js:23-37` trên host → Tauri IPC) → `hydrateScrollback` (`:136`).
- Gõ: `usePtyTerminal.js:210 term.onData` → `sendRaw` (`:186`) → companion gửi
  `{t:'pty_input', data:b64}` (`:195`) → host `ptyBridge.js:64-68` → `invoke('pty_write')` →
  `pty.rs:372-384` (giữ mutex session suốt `write_all`+`flush`).
- Output: reader thread trong `pty.rs` emit `pty-output` → host render trực tiếp
  (`usePtyTerminal.js:152-157`) **và** `ptyBridge.js:48-56` relay thành `FRAME_PTY_OUTPUT` →
  companion `usePtyTerminal.js:166-171`.

**Mong đợi vs thực tế**: ✅ output về cả hai màn. `reset` đi kèm nên CLEAR/RESTART đồng bộ.
Companion join giữa chừng được replay scrollback (`ptyBridge.js:69-71` → `:27-38`), có `cols/rows/alive`.

**Phát hiện**
- 🟡 `ptyBridge.js:48-56` relay **mọi** byte PTY cho **mọi** companion, kể cả phone chưa từng mở tab
  TERMINAL — `usePtyTerminal` chỉ đăng ký `onFrame` khi component mount (`:164`), nên frame bị bỏ đi
  sau khi đã tốn băng thông. Chạy `npm run build` dài trong PTY sẽ đẩy liên tục qua WS tới cả các
  phone không quan tâm.
- 🟡 `pty_resize` không thể phân biệt "host gọi" với "companion gọi qua seam invoke" — chính
  `pty.rs:385` (doc comment) thừa nhận đây là khoảng hở được chấp nhận; frontend tự kỷ luật
  (`usePtyTerminal.js:239`). Ghi nhận đúng như đã tuyên bố, không thổi phồng.

---

### C12. Phone gõ khi Mac đang gõ cùng lúc vào terminal

**Flow**: cả hai đường đều đổ vào `pty.rs:372 pty_write`; hàm này lấy
`state.session.lock()` (`pty.rs:378`) rồi `write_all` + `flush` **trong cùng một guard**
(`:381-382`). Output: `emit_locked` (`pty.rs:89` doc) yêu cầu giữ mutex `OutBuf`, mỗi lần drain
toàn bộ accumulator → thứ tự byte ra đúng thứ tự PTY sinh ra, không xen kẽ, không lặp, không mất.

**Mong đợi vs thực tế**: **không phát hiện vấn đề**. Một chunk keystroke/paste là nguyên tử; không có
nguy cơ xé đôi chuỗi UTF-8 nhiều byte giữa hai người gõ.

**Phát hiện**
- 🟡 Xen kẽ ở mức *chunk* vẫn xảy ra theo bản chất (hai người gõ vào một shell) — đúng thiết kế
  "một PTY chia sẻ", không phải bug.

---

### C13. Phone bấm nút chỉ chạy được trên Mac (Finder / VSCode / Terminal.app)

**Flow**: `ProjectTable.vue:429-441 openIdeLocal` → `invoke('open_local_terminal')` hoặc
`invoke('macos_open', {args})`. Trên companion `invoke` **không phải** Tauri IPC mà là
`utils/tauri.js:18-27` → `bridge.request({t:'invoke', cmd, args, id})` → host
`hostInvoke.js:41-46` → `respondToInvoke` (`:23-37`) chạy lệnh thật trên Mac, trả `invoke_result`
(`bridge.js:204-211`). Tương tự `openIdeRemote` (`ProjectTable.vue:537-549`) và
`usePtyTerminal.js:115-122 openExternal`.

**Mong đợi vs thực tế**: ✅ **được định tuyến về host, không hề thử chạy trên phone.** Không có
skip-list, không có "host-only command" — đúng như thiết kế seam N mô tả.

**Phát hiện**
- 🟡 Ngữ nghĩa hơi lạ nhưng nhất quán: phone bấm "REPORT" (`ProjectTable.vue:530-540 openReportHtml`)
  sẽ mở trình duyệt **trên Mac**, không mở gì trên phone — dù đã có `web_server::read_text_file`
  (`web_server.rs:987-1006`) sẵn sàng cho một FileView; grep toàn `src/` không thấy call site nào
  dùng command đó. Tính năng FILE-1 đang là code chết ở phía frontend.
- 🟡 `copyLocalPath`/`copyRemotePath` (`ProjectTable.vue:499-518`) dùng `navigator.clipboard` cục bộ —
  đúng (copy vào clipboard của phone), ghi nhận vì nó là ngoại lệ có chủ ý so với mục trên.

---

### C14. Token của phone bị thu hồi trong lúc đang dùng

**Flow**: `revoke_device(id)` (`web_server.rs:923-956`) trong `spawn_blocking`:
xoá **đúng một** phần tử khỏi `devices` (`:928-931`), `persist_devices()` (`:936`), rồi quét
`companions` lọc theo `device_id` (`:938-943`) và chỉ đóng socket của thiết bị đó với 4001
(`:946-949`). Phone: `bridge.js:258` → `rejectAllPending` (mọi invoke đang bay bị reject với lỗi có
tên) → 4001 → `'unpaired'` → `useCompanionPairing.js:33 clearDeviceToken()` → gate.
`App.vue:7` unmount dashboard.

**Mong đợi vs thực tế**: **không phát hiện vấn đề.** Đây là đúng ngữ cảnh duy nhất mà việc xoá token
phía phone là hành vi mong muốn; và scoped-clear được tuân thủ chính xác (thiết bị khác + kết nối
khác không hề bị đụng, đúng quy tắc multi-entity trong CLAUDE.md).

---

### C15. Bật HTTPS qua Tailscale, phone truy cập `wss://`

**Flow**
- `set_tailscale_https(true)` (`web_server.rs:883-907`) → `tailscale serve --bg http://127.0.0.1:1421`.
- Phone mở `https://<magicdns>/` → `bridge.js:104-105` chọn `wss:` + `location.host` (không gắn cứng
  `:1421`) → `wss://<magicdns>/ws?role=companion&token=…` → tailscaled terminate TLS → proxy tới
  `127.0.0.1:1421`. `pairDevice` (`bridge.js:299`) dùng `location.origin` nên `/pair` cũng đi đúng
  đường. Logic phía JS **đúng và đã được lý giải kỹ**.

**Phát hiện**
- 🔴 **Guard loopback cho `role=host` bị vô hiệu hoá khi bật `tailscale serve`.**
  `web_server.rs:486` chấp nhận `role=host` **chỉ dựa trên `addr.ip().is_loopback()`**, không token.
  Nhưng khi `tailscale serve` (`:886`) đứng trước, **mọi** kết nối từ tailnet tới relay đều đến từ
  `127.0.0.1` — `ConnectInfo` (`:472`) thấy loopback. Do đó một peer bất kỳ trên tailnet có thể mở
  `wss://<magicdns>/ws?role=host` **không cần token** và:
  - `handle_host_socket` (`web_server.rs:525`) ghi đè `host_tx` bằng socket của kẻ đó → Mac thật
    ngừng nhận intent/invoke, mọi companion ngừng nhận mirror từ Mac;
  - mọi frame nó gửi được `broadcast_to_companions` (`:532-534`) → giả mạo `init`/`delta` trên màn
    hình các phone đã pair;
  - nó nhận được toàn bộ `intent`/`invoke` do các phone gửi lên.
  Đây **không** nằm trong Security note đã tuyên bố (CHANGELOG:49 và :82 nói về "thiết bị **đã pair**
  gọi được mọi command") — ở đây là bỏ qua hoàn toàn cơ chế token. Comment `web_server.rs:481-485`
  giả định `addr` là địa chỉ client thật, giả định đó sai sau khi có reverse proxy. Sửa tối thiểu:
  yêu cầu token/secret cho `role=host` thay vì (hoặc bổ sung cho) kiểm tra loopback.
- 🟡 `tailscale_serve_on()` (`web_server.rs:846-851`) nhận diện bằng cách tìm chuỗi `127.0.0.1:1421`
  trong stdout của `serve status` — dễ vỡ nếu tailscale đổi định dạng, nhưng chỉ ảnh hưởng hiển thị.
- 🟡 Token đi trong **query string** của URL WS (`bridge.js:105`) — với `wss://` thì được mã hoá,
  nhưng vẫn dễ lọt vào log của proxy. Xem thêm C18.

---

### C16. Mất mạng giữa lúc đang gửi intent

**Flow**
- Intent: `action.js:37-39` gọi `send()`; `bridge.js:118-125` **trả false và chỉ `console.debug`**
  nếu socket không OPEN. `action()` trả `undefined` bất kể thành công hay không.
- Invoke: `bridge.request` (`bridge.js:130-157`) reject ngay nếu không gửi được; nếu socket rớt sau
  khi gửi, `rejectAllPending` (`bridge.js:260`) reject toàn bộ với lỗi có tên; watchdog 20s
  (`bridge.js:142-151`) chặn treo im lặng.

**Mong đợi vs thực tế**
- **Mất**: có. Intent gửi khi socket vừa chết (hoặc `readyState===OPEN` nhưng TCP đã hỏng, trước khi
  ping 15s/5s phát hiện) biến mất **không có bất kỳ phản hồi UI nào**.
- **Lặp**: không. Không có buffer/retry, WS trên TCP nên frame hoặc tới hoặc không.

**Phát hiện**
- 🟠 **Intent thất bại là im lặng hoàn toàn.** `bridge.js:120` dùng `console.debug` (mức thấp nhất),
  `action.js:37-40` không kiểm tra giá trị trả về của `send()` và không có toast. User trên phone bấm
  PUSH, không thấy gì xảy ra, bấm lại — may là `useSync.js:45-48` chặn double-sync, nhưng với
  `setDryRun`/`reorderProjects`/`applyTaskEdit` thì không có guard nào và user không biết thao tác đã
  rơi. Ít nhất `action()` nên hiển thị lỗi khi `send()` trả false.
- 🟡 Ngược lại, nếu socket rớt **sau khi** host đã bắt đầu `run_sync`, invoke của phone reject
  (`bridge.js:260`) trong khi host vẫn chạy tiếp → phone báo lỗi cho một thao tác thực ra thành công.
  Với đường intent (PUSH/PULL) thì không gặp vì không chờ kết quả.

---

### C17. Phone mở đúng lúc Mac đang chạy sync — log có stream về phone không

**Flow**: `pty`-độc lập. Log sync: Rust emit `sync-log` → `useLogs.js:79-84` (host-only qua
`onHostBoot`) → `appendLog` (`useLogs.js:26-32`) push vào `logStore.projectLogs`
(`logStore.js:4`) → là ref của `src/store/*.js` → mirror deep-watch (`mirror.js:137-144`) →
`scheduleFlush` (`mirror.js:84-88`) → `delta` → phone `applyFrame`. Phone mới join thì
`broadcastFull` (`mirror.js:102-109`) gửi kèm toàn bộ log hiện có.

**Mong đợi vs thực tế**: ✅ log **có** stream về phone, và phone join giữa chừng thấy đủ lịch sử.
`activeLogProjectId`/`isLogExpanded` cũng được host set trong `useSync.js:58-62` nên panel tự mở.

**Phát hiện**
- 🟠 **Chi phí truyền là bậc hai.** Mỗi dòng log làm dirty key `logStore.projectLogs`
  (`mirror.js:139-142`), và `flushDirty` (`mirror.js:90-100`) mã hoá lại **toàn bộ map log của mọi
  project** rồi gửi đi. Một rsync 5.000 dòng ⇒ ~5.000 delta, delta thứ n chứa n dòng. Coalescing
  `queueMicrotask` không giúp vì mỗi event `sync-log` là một task riêng. Cùng lỗi với `globalLogs`.
  Đây là vấn đề thực dụng chứ không lý thuyết: sync lớn sẽ làm phone lag và đốt băng thông LAN/tailnet.

---

### C18. `hostInvoke` không có allowlist command — đánh giá bề mặt rủi ro thực tế

**Flow**: `hostInvoke.js:23-37` nhận `{cmd, args}` từ dây và gọi thẳng `invoke(cmd, args)` — không
lọc tên lệnh, không kiểm tra args. Danh sách lệnh khả dụng: `lib.rs:57-129`.

**Đánh giá trung thực theo posture đã tuyên bố** (CHANGELOG:49 và CHANGELOG:82 — "thiết bị đã pair
gọi được mọi lệnh, token là cổng duy nhất, đây là quyết định có chủ ý"):

- Từ một thiết bị **đã pair**, các lệnh cho phép thực thi mã tuỳ ý dưới quyền user: `pty_spawn` +
  `pty_write` (`lib.rs:121-122`), `run_project_command` / `run_project_dev` (`lib.rs:93-94`),
  `macos_open` với args tuỳ ý (`lib.rs:82`), `save_ssh_config` (`lib.rs:64` — có thể chèn
  `ProxyCommand`), `apply_statusline_config` (`lib.rs:103` — ghi script shell được source), `run_sync`
  (`lib.rs:73` — nhận cả object `project` từ dây, tức đường dẫn nguồn/đích do kẻ gọi kiểm soát).
- **Kết luận: allowlist không làm giảm rủi ro một cách có ý nghĩa** khi tính năng in-app terminal đã
  cố ý cấp shell đầy đủ. Quyết định defer là **hợp lý** và đã được tuyên bố đúng. Không thổi phồng.
- `read_text_file` (`web_server.rs:987-1006`) ngược lại **có** confinement đàng hoàng
  (`canonicalize` + `starts_with` theo component, chặn `..` và symlink) — đây là điểm sáng, không phải
  điểm yếu.

**Phát hiện** (những thứ **nằm ngoài** posture đã tuyên bố, nên đáng nêu):
- 🟠 **Token đi qua HTTP thuần theo mặc định.** Đường LAN mặc định là `http://<ip>:1421`
  (`web_server.rs:782`) và token nằm trong query string WS (`bridge.js:105`). Trên Wi-Fi không tin cậy,
  token — thứ mà chính CHANGELOG gọi là "cổng duy nhất" — bị nghe lén được, và lấy được token là có
  RCE. TLS chỉ có khi bật Tailscale HTTPS (tuỳ chọn). Security note không nói tới điểm này.
- 🟡 `intents.js:30-36` đăng ký **mọi hàm export** của `src/store/*.js`, không chỉ hàm bọc `action()` —
  ví dụ `projectStore.bumpEpoch`, `beginRefresh`, `logStore.setGlobalListener`. Companion có thể
  dispatch bất kỳ hàm nào với args tuỳ ý. Không phải leo thang (đã có invoke tuỳ ý), chỉ là bề mặt
  rộng hơn mức cần, và có thể gây trạng thái không hợp lệ nếu gọi nhầm.

---

### C19. Mirror gặp giá trị không serialize được (DOM node trong `logStore.consoleRef`)

**Flow**: `logStore.js:7 consoleRef` là ref → `mirror.js:22-27` đăng ký key `logStore.consoleRef` →
khi component gán element, watch (`mirror.js:137-144`) đánh dirty → `flushDirty` (`:90-100`) →
`encodeKeyed` (`:56-66`) → `encode` (`:42`) ném `'DOM node cannot be mirrored'` → key bị **loại khỏi
payload**, `console.warn` **một lần duy nhất** (`warnedDrop`, `mirror.js:54`) → nếu không còn key nào
thì `mirror.js:99` không gửi frame rỗng.

Đã kiểm tra thêm nguy cơ `deep: true` traverse một DOM element: Vue `traverse` chỉ đệ quy vào
plain object / array / Map / Set / ref, mà `HTMLDivElement` không phải plain object ⇒ **không** bị
duyệt sâu, không bị force layout.

**Mong đợi vs thực tế**: **không phát hiện vấn đề.** Cơ chế xử lý đúng, generic (không cần danh sách
loại trừ tay), và không spam log.

**Phát hiện**
- 🟡 Trên companion, `logStore.consoleRef` **không bao giờ** nhận giá trị từ mirror (đúng), nhưng
  cũng không có gì kiểm tra rằng một ref quan trọng bị drop im lặng chỉ vì lồng một giá trị không
  encode được — cảnh báo chỉ ra console của Mac, không lên UI.

---

### C20. `logStore.projectLogs` không có giới hạn dòng — chạy build dài trên phone

**Flow**: `logStore.js:4 projectLogs = ref({})`; `useLogs.js:26-32 appendLog` chỉ `push`, không cắt.
`clearLog` (`useLogs.js:42-48`) chỉ chạy khi user bấm. `useSync.js:63-64` reset log **mỗi lần bắt đầu
sync** cho project đó (đây là thứ duy nhất giới hạn tăng trưởng, và chỉ theo từng phiên sync).
`AppConsole.vue:80` render `v-for` toàn bộ `displayedLogs`, không ảo hoá, không cắt.

Đối chiếu: PTY **có** giới hạn (`pty.rs:22 SCROLLBACK_CAP = 256KB`, drain ở `:73-75`) — nên
"chạy build dài" trong tab TERMINAL thì an toàn; nhưng `run_sync`/`run_project_*` ghi vào `projectLogs`
thì không.

**Mong đợi vs thực tế**: không có cap ⇒ ba hệ quả cộng dồn:

**Phát hiện**
- 🟠 **Bộ nhớ tăng không giới hạn** trên **cả Mac lẫn mọi phone** (mirror sao chép nguyên vẹn), cho tới
  khi user bấm Clear hoặc bắt đầu sync mới. `globalLogs` (`logStore.js:3`) thì **không** có bất kỳ
  điểm reset tự động nào — nó chỉ tăng suốt vòng đời app.
- 🟠 Cộng hưởng với C17: kích thước payload delta tỉ lệ với số dòng đã tích luỹ, nên vừa tốn RAM vừa
  tốn băng thông theo bậc hai. Mỗi lần một phone reconnect, `broadcastFull` (`mirror.js:102-109`) gửi
  lại **toàn bộ** log tích luỹ cho **tất cả** phone.
- 🟡 `AppConsole.vue:80` render mọi dòng vào DOM (không virtual scroll) — trên phone, vài nghìn node
  đủ để giật.

---

## Tổng hợp

| Mức | Case | Vấn đề | Vị trí |
| :-- | :-- | :-- | :-- |
| 🔴 | C4, C5, C2 | `enabled` không persist + close 4001 dùng chung cho "server tắt" và "token sai" ⇒ **mỗi lần restart Mac / bấm Off đều xoá token của mọi phone**, trái với lời hứa "reconnect silently across app restarts" | `web_server.rs:90`, `web_server.rs:494-496`, `bridge.js:261-265`, `useCompanionPairing.js:31-33`, đối chiếu `docs/feat/remote-control.md:30` |
| 🔴 | C15 | `tailscale serve` khiến mọi kết nối tailnet đến từ `127.0.0.1` ⇒ guard loopback cho `role=host` bị vượt, peer tailnet chiếm được vai host **không cần token** | `web_server.rs:486`, `web_server.rs:525`, `web_server.rs:886` |
| 🟠 | C7 | `askConfirm` chỉ 1 slot, ghi đè vô điều kiện ⇒ dialog trước không bao giờ settle, `_waiters` rò, `syncing:true` kẹt vĩnh viễn | `dialogStore.js:39-45`, `dialogStore.js:25`, `useSync.js:50/164/250` |
| 🟠 | C8 | Project vừa xoá từ phone có thể **sống lại** nếu bấm Save trên modal chưa đóng | `useProjectConfig.js:255-261`, `remoteActions.js:187-198` |
| 🟠 | C9 | Delta thay cả mảng `projects` ⇒ `tasksProject`/`editingProject` detach, edit tiếp theo gửi snapshot cũ | `mirror.js:126`, `useProjectTasks.js:10-13`, `useProjectConfig.js:8` |
| 🟠 | C3, C6 | State UI per-screen (`activeLogProjectId`, `isLogExpanded`, `slotTargets`) bị mirror 1 chiều ⇒ phone B join làm reset màn hình phone A | `mirror.js:158-162`, `logStore.js:5-6`, `useLogs.js:50-57` |
| 🟠 | C17, C20 | `projectLogs`/`globalLogs` không cap + deep-watch ⇒ mỗi dòng log phát lại toàn bộ map (bậc hai), RAM tăng vô hạn trên cả Mac lẫn phone | `logStore.js:3-4`, `useLogs.js:26-40`, `mirror.js:90-100`, `AppConsole.vue:80` |
| 🟠 | C16 | Intent rơi im lặng khi socket không open — chỉ `console.debug`, không phản hồi UI | `bridge.js:118-125`, `action.js:37-40` |
| 🟠 | C10 | Kéo-thả sắp xếp dùng HTML5 DnD + `mousedown` ⇒ **không hoạt động trên phone** | `ProjectTable.vue:65-70`, `:365-382` |
| 🟠 | C18 | Token — "cổng duy nhất" — đi qua HTTP thuần trong query string theo mặc định trên LAN | `web_server.rs:782`, `bridge.js:105` |

**Không phát hiện vấn đề**: C12 (ghi PTY nguyên tử nhờ mutex), C13 (định tuyến về host đúng),
C14 (revoke scoped-clear chuẩn), C19 (drop giá trị không encode được đúng và chỉ warn một lần),
và cơ chế first-answer-wins của dialog ở C7 (đã soi cả 4 nhánh, không có double-resolve).

**Về Security note đã tuyên bố**: việc không có allowlist cho `hostInvoke` (C18) là **hợp lý và đã
được tuyên bố đúng** — in-app terminal đã cấp shell đầy đủ nên allowlist chỉ là hình thức. Hai thứ
**nằm ngoài** posture đó và cần được xem là lỗi thật: host-role hijack qua reverse proxy (C15) và
token không được bảo vệ trên đường truyền mặc định (C18).
## Cụm D — In-App Terminal

Phạm vi đọc: `src-tauri/src/pty.rs`, `src-tauri/src/lib.rs:121-142`, `src-tauri/src/system.rs:39-146`,
`src-tauri/src/web_server.rs` (đường relay), `src/components/TerminalView.vue`,
`src/components/AppConsole.vue`, `src/composables/usePtyTerminal.js`, `useTerminalPanel.js`,
`src/services/ptyBridge.js`, `bridge.js`, `hostInvoke.js`, `src/utils/tauri.js`,
`src/constants/protocol.js`.

Quy ước mức: 🔴 mất/hỏng dữ liệu hoặc session của người dùng · 🟠 lỗi chức năng thật, tái hiện được ·
🟡 rủi ro/khó chịu, chưa chắc gặp.

---

### D1. Mở tab TERMINAL lần đầu — spawn shell + fit kích thước

**Flow**
`AppConsole.vue:11` (nút tab) → `activePanel='terminal'` (`useTerminalPanel.js:15`) →
`AppConsole.vue:86-88` mount `TerminalView` → `TerminalView.vue:110-124` tạo `Terminal` + `FitAddon`,
`term.open()` → `TerminalView.vue:126` `usePtyTerminal(term)` → `:134` `await start(cwd)` →
`usePtyTerminal.js:254-260`: `wireOutput()` → `wireInput()` → `ensureSpawned()` →
`invoke('pty_spawn')` → `pty.rs:203-207` → `spawn_if_absent` `pty.rs:210-252` (openpty 80x24,
`$SHELL -l`, `TERM=xterm-256color`, `std::thread::spawn(read_loop)`) → về JS `hydrateScrollback()`
`usePtyTerminal.js:136-145` (`term.resize(80,24)`, `writeChunk(data, reset=true)`) →
`TerminalView.vue:140` `scheduleFit()` (rAF) → `:84-99` `doFit()` → guard `width<40||height<24` →
`fitAddon.fit()` → `ptyApi.hostResize(cols, rows)` → `usePtyTerminal.js:238-251` → `pty_resize`
(`pty.rs:387`) + `FRAME_PTY_RESIZE` cho companion. Thêm 2 lưới an toàn:
`document.fonts.ready.then(scheduleFit)` (`TerminalView.vue:141`) và `ResizeObserver`
(`TerminalView.vue:143-144`).

**Mong đợi vs thực tế** — bug "render bé ở góc" đã được sửa ở đúng 3 chỗ độc lập, và cả 3 đều cần:
(1) `.pty-terminal { flex:1; min-width:0 }` + `.pty-terminal-mount { flex:1; min-height:0 }`
(`TerminalView.vue:181-198`) — nguyên nhân gốc là cha `.terminal-mount-wrap` là flex row
(`AppConsole.vue:157-161`) nên không có `flex:1` thì phần tử co về content rỗng;
(2) hoãn fit sang frame sau (`:140`) thay vì fit đồng bộ trong `onMounted`;
(3) guard kích thước 0 trong `doFit` (`:91-93`).
Thứ tự hydrate-trước-fit (`:134` rồi `:140`) cũng đúng: fit của host là authoritative và ghi đè
kích thước hydrate, nếu đảo lại thì hydrate sẽ ghi đè fit mới.

**Phát hiện** — không phát hiện vấn đề với chính đường fit lần đầu. Hai điểm phụ:

- 🟡 `alive` khởi tạo `false` (`usePtyTerminal.js:55`) và chỉ lên `true` sau khi `pty_spawn` resolve.
  Trong khoảng đó `AppConsole.vue:29-31` render `TERMINAL - EXITED` màu đỏ → nháy đỏ mỗi lần mở tab.
  Nên khởi tạo `null` (chưa biết) và chỉ đỏ khi `=== false`.
- 🟡 `doFit` bỏ qua im lặng khi `width<40||height<24` (`TerminalView.vue:91-93`). Nếu panel bị thu
  quá nhỏ và `ResizeObserver` không bắn thêm lần nào nữa thì xterm giữ nguyên size cũ vô hạn — không
  có retry theo thời gian. Rủi ro thấp vì ResizeObserver luôn bắn lại khi layout đổi.

---

### D2. Đóng/mở dock, chuyển LOG ↔ TERMINAL nhiều lần

**Flow**
- *Collapse dock*: `AppConsole.vue:35` toggle `isLogExpanded` → `AppConsole.vue:2` class
  `is-collapsed` → `main.css:146-148` `height: 110px` (KHÔNG unmount TerminalView) → ResizeObserver
  → `scheduleFit` → `doFit`.
- *Chuyển tab*: `AppConsole.vue:75` vs `:86` là `v-if/v-else` → chuyển sang LOG **unmount**
  TerminalView → `TerminalView.vue:156-160` `resizeObserver.disconnect()` + `term.dispose()`, và
  `usePtyTerminal.js:262-266` gỡ listener. Quay lại TERMINAL → mount mới → `start()` →
  `pty_spawn` no-op (`pty.rs:213-215`) → `hydrateScrollback` dựng lại nội dung từ ring buffer.

**Mong đợi vs thực tế** — session sống sót đúng như thiết kế (scrollback ở host, spawn idempotent).
Fit sau khi mở lại dock chạy qua ResizeObserver, đúng.

**Phát hiện**

- 🟠 **Rò listener `pty-output`/`pty-exit` khi unmount sớm.** `usePtyTerminal.js:152-162` đăng ký
  bằng `listen(...).then(un => unlistenHostOutput = un)`. `onBeforeUnmount` (`:262-266`) chỉ gỡ nếu
  biến đã được gán. Nếu component bị unmount **trước khi** promise của `listen` resolve (bấm tab
  TERMINAL rồi bấm LOG ngay — hoàn toàn khả thi vì `listen` là một round-trip IPC), unlisten không
  bao giờ chạy: listener sống mãi và tiếp tục gọi `writeChunk` → `term.write()` trên một `Terminal`
  đã `dispose()` (`TerminalView.vue:159`) → ném lỗi trên **mỗi** chunk output. Lặp lại nhiều lần
  thì số listener chết cộng dồn. Cách sửa chuẩn: cờ `disposed` kiểm tra trong `.then`, hoặc giữ
  promise và `await` nó trong hook unmount.
- 🟡 **Fit khi dock đang collapsed đang sát ngưỡng nguy hiểm.** Collapse KHÔNG unmount terminal, và
  `doFit` vẫn chạy: `110px` − header ~34px (`main.css:723-731`) − key row ~29px
  (`TerminalView.vue:212-220`) ≈ 47px, trừ padding 8px ≈ 39px nội dung; với `fontSize 12` ×
  `lineHeight 1.4` ≈ 16.8px/dòng → ~2 dòng → `hostResize` từ chối vì `rows < 3`
  (`usePtyTerminal.js:244`). Tức là hiện tại **may mắn** không đẩy size 2 dòng vào PTY thật. Nhưng
  biên an toàn chỉ rộng đúng một dòng: nâng `is-collapsed` lên ~130px, hay giảm `lineHeight`, là
  `rows` thành 3 và một `npm run build` đang chạy sẽ bị re-wrap về 3 dòng vĩnh viễn. Đáng lo hơn vì
  `isLogExpanded` nằm trong `store/logStore.js:6` nên **được mirror** — điện thoại collapse dock là
  Mac cũng collapse và cũng chạy `doFit`. Sửa đúng: `doFit` nên bỏ qua khi `!isLogExpanded`, chứ
  không dựa vào số học pixel.

---

### D3. Gõ lệnh thường, tiếng Việt / emoji (UTF-8 nhiều byte)

**Flow (vào)** `term.onData(chunk)` (`usePtyTerminal.js:210`) → `sendRaw` (`:186-197`) →
`new TextEncoder().encode(str)` → `encodeBytesToBase64` (`:31-35`) → host: `invoke('pty_write')`
(`pty.rs:372-383`, `STANDARD.decode` → `write_all`) · companion: `send({t:FRAME_PTY_INPUT})` →
`ptyBridge.js:65-68` → cùng `pty_write`.
**Flow (ra)** `read_loop` `pty.rs:158-172` (buffer 8192) → `append_scrollback` → `OutBuf.acc` →
`flush_locked` `:90-99` base64 → `emit("pty-output")` → `usePtyTerminal.js:152-155` /
`ptyBridge.js:48-56` → `writeChunk` → `decodeBase64ToBytes` → `term.write(Uint8Array)`.

**Mong đợi vs thực tế** — chuỗi này **binary-safe thật**, không phải chỉ trong comment:
không chỗ nào biến byte PTY thành `String` của Rust; `String::from_utf8_lossy` chỉ xuất hiện ở
`pty_cwd` (`pty.rs:354`, không phải đường output). Cắt giữa chuỗi UTF-8 tại biên 8KB read hoặc tại
biên coalescing được xterm.js tự ghép lại nhờ decoder UTF-8 có trạng thái qua các lần `write()`.
`atob/btoa` chỉ dùng như codec byte thô (`:24-35`), không dùng để decode text → không dính bẫy
mojibake.

**Phát hiện** — không phát hiện vấn đề về UTF-8. Hai ghi chú:

- 🟡 `ctrlArmed` + ký tự nhiều byte: `usePtyTerminal.js:221` kiểm `chunk.length === 1`. Một emoji là
  surrogate pair (`length === 2`) nên không lọt vào nhánh Ctrl — đúng. Ký tự Việt tổ hợp
  (`length === 1`) đi qua `toCtrlByte` (`:199-204`) và trả `null` vì code > 95 → giữ nguyên. Đúng,
  nhưng cờ `ctrlArmed` đã bị **tiêu thụ mất** dù không tạo ra byte Ctrl nào (`:220`) — người dùng
  bấm Ctrl rồi lỡ gõ 'ệ' thì Ctrl im lặng biến mất.
- 🟡 `encodeBytesToBase64`/`decodeBase64ToBytes` dựng chuỗi bằng vòng `+=` từng ký tự
  (`:31-35`, `:24-29`). Với 256KB scrollback replay là ~256K lần nối chuỗi — chậm nhưng chạy được;
  `String.fromCharCode(...bytes)` theo lô sẽ nhanh hơn nhiều (không dùng spread một phát vì tràn
  stack).

---

### D4. Output cực nhiều (`npm run build`, `yes`, `cat` file lớn)

**Flow** `read_loop` `pty.rs:157-175` → ngưỡng `FLUSH_BYTES = 16KB` / `FLUSH_INTERVAL = 20ms`
(`:25-26`) → `flush_locked` → `emit("pty-output")` → (host) webview; (companion)
`ptyBridge.js:48-56` `send(...)` → `bridge.js` `ws.send` → `web_server.rs:95-101`
`broadcast_to_companions` qua `mpsc::UnboundedSender`.

**Mong đợi vs thực tế** — logic coalescing đúng: fast-path trong reader khi đã quá cửa sổ, còn
deadline do `flusher_loop` (`:106-134`) giữ; `flush_locked` luôn được gọi khi đang giữ mutex `OutBuf`
nên thứ tự byte tuyệt đối không đảo/không nhân đôi (`:89`). Nhưng dưới firehose thì `elapsed >= 20ms`
gần như không bao giờ đúng (read liên tục), nên thực tế mỗi event là ~16KB → ở 10MB/s là ~640
event/giây, mỗi frame JSON ~21KB base64.

**Phát hiện**

- 🟠 **Không có backpressure về phía companion.** `web_server.rs:63,95-101` dùng
  `mpsc::UnboundedSender`; `bridge.js` `send()` cũng không kiểm `ws.bufferedAmount`. Một điện thoại
  chậm/qua Tailscale trong lúc `yes` đang chạy sẽ khiến hàng đợi phía host phình không giới hạn
  (RAM của app tăng theo tốc độ shell, không theo tốc độ mạng). Không có chính sách drop/thu gọn
  (ví dụ: khi tồn đọng vượt ngưỡng thì thay bằng một frame `reset` mang scrollback hiện tại).
  Đây là con đường thực tế nhất để OOM app trên máy 16GB.
- 🟡 **`append_scrollback` là O(n) mỗi lần read.** `pty.rs:70-77`: khi buffer đã đầy, mỗi chunk 8KB
  kéo theo `drain(0..excess)` → memmove ~256KB. Với `yes` (nhiều MB/s) là hàng trăm MB/s memmove
  chỉ để duy trì ring buffer. Ring buffer thật (`VecDeque` + `make_contiguous` khi đọc) hoặc drain
  theo lô lớn (ví dụ chỉ cắt khi vượt 1.5× cap) sẽ bỏ hẳn chi phí này.
- 🟡 **Ring buffer cắt giữa escape sequence.** `drain(0..excess)` cắt theo byte nên đoạn đầu
  scrollback có thể là nửa chuỗi ESC hoặc nửa ký tự UTF-8. Màn hình mở tab muộn (hoặc phone
  reconnect) sẽ render sai vài dòng đầu, và tệ hơn nếu chuỗi bị cắt là "vào alternate screen".
- Không phát hiện vấn đề về mất byte hay đảo thứ tự ở hướng output.

---

### D5. `Ctrl+C` từ Mac và từ phone (sticky Ctrl)

**Flow (Mac)** bàn phím thật → xterm sinh `\x03` → `onData` (`usePtyTerminal.js:210`) →
`sendRaw` → `pty_write` → tty gửi SIGINT cho foreground process group. Đúng.
**Flow (phone)** `TerminalView.vue:16` nút Ctrl → `ptyApi.armCtrl()` (`usePtyTerminal.js:230-232`)
→ người dùng gõ 'c' → `onData('c')` → `:219-225` `toCtrlByte('c')` → `\x03` → `FRAME_PTY_INPUT` →
`ptyBridge.js:65-68` → `pty_write`.

**Phát hiện**

- 🟠 **Các nút key row không chặn mất focus.** `TerminalView.vue:14-23` chỉ có `@click`, không có
  `@mousedown.prevent` / `@touchstart.prevent`. Chạm nút trên điện thoại sẽ blur cái textarea ẩn
  của xterm → bàn phím mềm iOS/Android đóng lại. Với sticky Ctrl thì đây đúng là kịch bản chính:
  chạm Ctrl (bàn phím biến mất) → phải chạm lại vào vùng terminal để bàn phím hiện ra (mà cú chạm
  đó không huỷ `ctrlArmed`, nên vẫn còn armed — may) → gõ 'c'. Ba thao tác cho một `Ctrl+C`, và
  luồng này chưa được kiểm chứng trên thiết bị thật (docs cũng ghi là chưa verify). Cũng ảnh hưởng
  Esc/Tab/mũi tên/Enter: mỗi lần chạm là một lần đóng bàn phím.
- 🟡 **`ctrlArmed` chỉ được tiêu thụ bởi `onData`, không bởi chính key row.** Chạm Ctrl rồi chạm
  mũi tên → `TerminalView.vue:19` gọi thẳng `ptyApi.sendRaw('\x1b[A')` (`usePtyTerminal.js:186`),
  bỏ qua hoàn toàn nhánh Ctrl ở `:219`. Cờ vẫn armed và sẽ "ăn" ký tự gõ tiếp theo — hành vi bất
  ngờ. Không có timeout tự huỷ.
- 🟡 Gõ khi shell đã chết: `:214-217` nuốt phím và gọi `restart()` — nhưng `ctrlArmed` không được
  reset, nên cờ vẫn treo sang shell mới.
- Không phát hiện vấn đề với `Ctrl+C` từ Mac.

---

### D6. Chương trình full-screen (`vim`, `htop`) rồi thoát

**Flow** không có xử lý đặc biệt: byte alternate-screen (`\x1b[?1049h/l`) đi qua nguyên vẹn theo
đúng đường D3; resize từ Mac (`pty.rs:387-399` `master.resize`) làm kernel gửi SIGWINCH → vim tự vẽ
lại.

**Mong đợi vs thực tế** — đúng thiết kế PTY thật, `vim`/`htop` chạy được.

**Phát hiện**

- 🟡 Scrollback là byte thô, **không phải trạng thái màn hình**. Một màn (phone reconnect, hoặc Mac
  chuyển LOG→TERMINAL) hydrate giữa lúc `vim` đang mở sẽ phát lại 256KB byte cuối; nếu chuỗi "vào
  alternate screen" đã bị đẩy ra khỏi ring buffer (D4) thì màn đó sẽ thấy nội dung vim vẽ lên
  main screen, và khi thoát vim (`\x1b[?1049l`) sẽ khôi phục về một main screen sai. Cách chữa đúng
  là serialize trạng thái (xterm `SerializeAddon`) thay vì phát lại byte thô — ngoài phạm vi MVP,
  nhưng nên ghi vào "Not in this version".
- Không phát hiện vấn đề chức năng khi chạy/thoát bình thường trong một phiên không bị cắt ring.

---

### D7. `exit` → `[process exited]`, tab đỏ, gõ tiếp thì shell mới sinh ra

**Flow** shell thoát → `reader.read()` trả `Ok(0)` (`pty.rs:159`) → thoát vòng → flush đuôi
(`:177-181`) → `done=true` + `notify_one` → kiểm generation (`:186-190`) → `*guard = None` →
`append_scrollback(EXIT_NOTICE)` (`:194`) → `emit("pty-output")` + `emit("pty-exit")` (`:196-198`)
→ host: `usePtyTerminal.js:158-161` `alive=false`; companion: `ptyBridge.js:60` →
`FRAME_PTY_EXIT` → `usePtyTerminal.js:172-173` `alive=false` → `AppConsole.vue:29-31` tab đỏ,
`:58` RESTART chuyển amber. Gõ tiếp → `:214-217` → `restart()` → `pty_restart` (`pty.rs:315-324`).

**Mong đợi vs thực tế** — đường đi cơ bản đúng và đã sửa hẳn bug "shell chết im lặng": thứ tự flush
đuôi TRƯỚC khi emit EXIT_NOTICE được đảm bảo bằng việc reader tự flush chứ không nhờ flusher
(`:176-181`) — chi tiết này đúng.

**Phát hiện**

- 🔴 **Trạng thái `alive` không đồng bộ giữa hai màn sau khi respawn — và hậu quả là mất session.**
  Không tồn tại sự kiện "shell đã sống lại": `pty_restart` (`pty.rs:315-324`) chỉ emit `reset` với
  data rỗng; `ptyBridge.js:53-55` relay đúng `{data, reset}` và **không kèm `alive`**; phía nhận
  `usePtyTerminal.js:166-171` chỉ cập nhật `alive` khi `frame.alive !== undefined`. `alive` chỉ được
  đặt `true` tại chỗ *chính màn đó* gọi lệnh (`:73`, `:86`).
  Kịch bản tái hiện: shell `exit` → cả hai màn `alive=false` → Mac bấm RESTART (hoặc gõ một phím) →
  Mac có prompt mới, `alive=true`; **điện thoại vẫn `alive=false`**. Người dùng chạm một phím bất kỳ
  trên điện thoại → `:214-217` không gửi phím mà gọi `restart()` → `pty_restart` **giết shell Mac
  vừa tạo (kèm mọi thứ đang chạy trong đó) và xoá sạch scrollback**. Đối xứng theo chiều ngược lại:
  phone RESTART → Mac kẹt `alive=false` → phím đầu tiên gõ trên Mac giết shell của phone.
  Chỉ tự khỏi khi màn kia chuyển tab hoặc reconnect (đường `hydrateScrollback`/`pushScrollback` có
  mang `alive`, `pty.rs:409`, `ptyBridge.js:29-32`).
  Sửa: emit một sự kiện `pty-spawn`/gắn `alive` vào frame `reset` do `emit_reset` phát ra (cần đọc
  `session.is_some()` ngay sau `spawn_if_absent`), và cho `ptyBridge` forward trường đó.

---

### D8. `ssh` sang máy khác rồi `exit` hai lần

**Flow** `ssh host` là con trong process group của shell; `exit` lần 1 kết thúc `ssh` (chỉ là output
bình thường, không có sự kiện nào ở host); `exit` lần 2 kết thúc login shell → đúng đường D7.

**Mong đợi vs thực tế** — đúng như D7. Trước 1.20.0 đây là ca báo lỗi "terminal ngồi chết"; nay có
EXIT_NOTICE (`pty.rs:142`) + `pty-exit`.

**Phát hiện** — không phát hiện vấn đề riêng cho ca này; nó thừa hưởng nguyên 🔴 của D7 (sau khi
respawn từ một màn, màn kia sẽ giết shell mới ở phím đầu tiên).
🟡 ghi chú: nếu `ssh` treo (mạng chết) thì `exit` không về; người dùng phải KILL — và KILL đi qua
`kill_process_group` (`pty.rs:264-275`) nên `ssh` thật sự bị dọn, đúng.

---

### D9. Bấm RESTART hai lần thật nhanh (double-tap)

**Flow** `AppConsole.vue:58` → `TerminalView.vue:167` → `usePtyTerminal.js:81-92`
(`if (restarting) return`) → `pty_restart` `pty.rs:315-324` → `kill_current` `:278-291` →
`scrollback.clear()` → `emit_reset(b"")` → `spawn_if_absent` (generation +1, `pty.rs:246`) →
reader cũ đến EOF, so generation (`:187`) thấy khác → `return` sớm, không xoá slot, không emit
EXIT_NOTICE.

**Mong đợi vs thực tế** — cơ chế generation counter **đúng và đủ** cho phần nó bảo vệ: reader cũ
không thể null hoá session mới, không thể phát EXIT_NOTICE giả, và slot luôn giữ session mới nhất
nên không rò shell (vì `spawn_if_absent` chỉ spawn khi slot `None`, còn `kill_current` luôn giết cái
đang nằm trong slot). Cờ `restarting` chặn double-tap trên **cùng một màn**; hai màn cùng bấm thì
tuần tự hoá bằng mutex `session` và kết quả cuối vẫn là đúng một shell sống.

**Phát hiện**

- 🟠 **Byte đuôi của shell cũ lọt qua sau `reset` và làm bẩn cả scrollback mới.** `read_loop:162`
  gọi `append_scrollback` **vô điều kiện**, không kiểm generation (khác hẳn nhánh EXIT_NOTICE ở
  `:187` có kiểm). Trong `pty_restart` thứ tự là: `kill_current` → `scrollback.clear()` →
  `emit_reset` → `spawn`. Reader cũ vẫn có thể đang cầm dữ liệu đã đọc và flush trong tối đa 20ms
  sau đó (`flusher_loop`), nghĩa là:
  (a) một `pty-output` không-reset của shell **đã chết** đến sau `reset` → hiện lên trên đầu prompt
  mới ở cả hai màn; (b) chính những byte đó được `append_scrollback` ghi vào ring buffer **vừa
  xoá** → mọi màn hydrate sau đó đều thấy rác của shell cũ.
  Sửa: truyền `generation` vào `append_scrollback`/`flush_locked` và bỏ qua nếu không còn là
  session hiện hành (đúng cùng một guard đã có ở `:187`).
- 🟡 `kill_current` giữ mutex `session` trong tối đa ~300ms (`pty.rs:267-274`, 12×25ms + `wait()`).
  Trong khoảng đó mọi `pty_write` / `pty_resize` / `pty_get_scrollback` đều nằm chờ. **Không vi phạm
  NEVER BLOCK THE UI** vì tất cả đều ở trong `spawn_blocking`, nhưng nếu người dùng gõ liên tục
  trong lúc đó thì mỗi phím chiếm một slot của pool `spawn_blocking` cho tới khi lock nhả — cùng
  loại rủi ro cạn slot mà chính module này viện dẫn để biện minh cho raw thread. Nên nhả lock trước
  vòng grace (đã lấy được `pid` rồi thì không cần giữ lock để chờ).

---

### D10. Bấm KILL rồi bấm RESTART

**Flow** `AppConsole.vue:62` → `pty_kill` `pty.rs:302-311`: `kill_current` → `append_scrollback
(EXIT_NOTICE)` → emit `pty-output` + `pty-exit` → cả hai màn `alive=false`, nút KILL tự disable
(`AppConsole.vue:62`). Rồi RESTART → `pty_restart` → `kill_current` no-op (slot đã `None`) →
clear scrollback (xoá luôn EXIT_NOTICE) → reset → spawn mới.

**Mong đợi vs thực tế** — khớp. Reader của shell bị KILL sẽ thấy slot `None` (`:187`
`unwrap_or(false)`) nên không phát EXIT_NOTICE lần hai → không bị double notice. Đúng.

**Phát hiện** — không phát hiện vấn đề riêng. Thừa hưởng 🔴 D7 (sau RESTART, màn còn lại vẫn
`alive=false`) và 🟠 D9 (đuôi byte).

---

### D11. CLEAR — trên Mac, trên phone, và sau khi phone reconnect

**Flow** `AppConsole.vue:54` → `usePtyTerminal.js:96-102` `invoke('pty_clear')` →
`pty.rs:328-335`: `scrollback.clear()` + `emit_reset(&app, b"")` →
host `usePtyTerminal.js:152-155`: `payload.data=""` falsy nhưng `payload.reset` true → qua điều
kiện → `writeChunk("", true)` → `term.reset()` (`:65`) → không `term.write` vì base64 rỗng. Đúng.
companion: `ptyBridge.js:53-55` cùng điều kiện `data || reset` → gửi `{data:'', reset:true}` →
`usePtyTerminal.js:166-171` → `term.reset()`; `frame.cols/rows/alive` undefined → bỏ qua, giữ
nguyên size và `alive`. Đúng.
Reconnect: `web_server.rs:109-111` `companion-connected` → `ptyBridge.js:69-70` → `pushScrollback`
(`:27-38`) gửi `reset` với `data` rỗng + `cols/rows/alive` → phone vẫn sạch. Đúng.

**Mong đợi vs thực tế** — CLEAR trên cả ba tình huống đều đúng. Chọn `term.reset()` thay
`term.clear()` (`:63-65`) là chính xác (giữ SGR/cursor mode sẽ để lại màu của shell cũ).

**Phát hiện** — không phát hiện vấn đề.
🟡 nhỏ: CLEAR do **companion** bấm đi qua `invoke` (`utils/tauri.js`) → `hostInvoke.js` →
`pty_clear`, tức là round-trip có ack; nếu socket rớt đúng lúc thì phone không clear nhưng host đã
clear — tự khỏi ở lần reconnect kế (pushScrollback). Chấp nhận được.

---

### D12. OPEN → Terminal.app đúng cwd (parse `lsof`) — dấu cách, dấu nháy, unicode

**Flow** `AppConsole.vue:66` → `usePtyTerminal.js:115-122`: `invoke('pty_cwd')` →
`pty.rs:341-368` (`/usr/sbin/lsof -a -d cwd -p <pid> -Fn`, lấy dòng đầu bắt đầu bằng `n`, bỏ tiền
tố 1 ký tự) → `invoke('open_local_terminal', { localPath: cwd || '~' })` → `system.rs:139-146`
`format!("cd \"{}\"", local_path)` → `open_terminal_with_command` `system.rs:47-75` →
`applescript_escape` (`system.rs:39-41`: chỉ escape `\` và `"`) → `osascript -e`.

**Mong đợi vs thực tế**
- Parse `lsof`: đầu ra `-Fn` là `p<pid>` / `fcwd` / `n<path>`; lấy dòng `n` đầu tiên là **đúng**, và
  đường dẫn có **dấu cách** hay **unicode** đi qua nguyên vẹn (không tách theo whitespace, không
  qua `String` lossy nào làm hỏng — `from_utf8_lossy` ở `:354` chỉ hại nếu tên thư mục không phải
  UTF-8, cực hiếm trên macOS). Không phát hiện vấn đề ở khâu parse.
- Dựng lệnh: **có vấn đề.**

**Phát hiện**

- 🟠 **Command injection / hỏng lệnh vì dùng nháy kép ở `system.rs:142`.** `cd "{path}"` — trong
  nháy kép của shell, `$`, `` ` ``, `\` và `"` vẫn có nghĩa. Một thư mục tên `it's $(id)` hoặc
  `a"; rm -rf ~; "` sẽ được **thực thi** trong cửa sổ Terminal.app mới. `applescript_escape`
  (`:39-41`) chỉ lo phần AppleScript, không lo phần shell — thậm chí nó escape `\` cho AppleScript
  rồi shell lại nhận `\\` nên đường dẫn có backslash cũng sai.
  Đối lập rõ ràng: hàm `cd()` in-app (`usePtyTerminal.js:129-132`) escape đúng chuẩn POSIX
  `'\''` — docs (`docs/feat/in-app-terminal.md`) khoe đúng chỗ này mà quên chỗ kia. Nên dùng lại
  cùng một escaper single-quote cho `open_local_terminal`. Lưu ý đây là lỗi có sẵn từ trước
  (`ProjectTable.vue:434` cũng gọi hàm này), nhưng 1.20.0 mở thêm một nguồn đầu vào mới: cwd do
  người dùng `cd` tới, và cwd đó có thể do **điện thoại** đặt.
- 🟠 **Fallback `'~'` chắc chắn hỏng.** `usePtyTerminal.js:118` truyền chuỗi `~` khi không đọc được
  cwd → `system.rs:142` sinh `cd "~"` → shell **không** expand tilde trong nháy kép → Terminal.app
  mở ra rồi báo `cd: no such file or directory: ~`. Docs ghi "falls back to `$HOME`" nhưng code
  không làm được điều đó. Sửa: truyền `null`/rỗng và để phía Rust bỏ hẳn phần `cd`, hoặc dùng
  `$HOME` không nháy.
- 🟡 `pty_cwd` đọc cwd của **process shell**, không phải của foreground job. Đúng cho ca dùng bình
  thường; nhưng khi đang ở trong `ssh`/`vim` thì trả về nơi shell đứng — hợp lý, chỉ cần biết.

---

### D13. "In-App Terminal" từ popup project → `cd` vào thư mục có dấu cách

**Flow** `ProjectTable.vue:167` → `useTerminalPanel.js:27-31` (`isLogExpanded=true`,
`activePanel='terminal'`, `pendingCd=localPath`) → mount `TerminalView` → `:134` `start()` →
`:148-152` tiêu thụ `pendingCd` (hoặc `watch` `:103-107` nếu tab đã mở sẵn) →
`usePtyTerminal.js:129-132` `cd '<escaped>'\r` → `sendRaw` → `pty_write`.

**Mong đợi vs thực tế** — escape `String(path).replace(/'/g, "'\\''")` bọc trong nháy đơn là **đúng
chuẩn POSIX**: dấu cách, `'`, `$`, `` ` ``, `"`, `\` đều an toàn. Cơ chế "hàng đợi một phần tử"
xử lý được cả hai thời điểm (tab chưa mount / đã mount). Không phát hiện vấn đề về injection.

**Phát hiện**

- 🟡 `cd` được gửi như **phím gõ** nên nếu shell đang chạy một lệnh foreground (build, `vim`, `ssh`)
  thì chuỗi `cd '...'\r` chui vào stdin của chương trình đó chứ không phải shell. Đây là hệ quả
  cố ý của thiết kế (docs nêu rõ), nhưng từ điện thoại người dùng không nhìn thấy Mac đang chạy gì
  → dễ vô tình gửi một dòng lạ vào `vim`. Không có bất kỳ kiểm tra "shell có đang rảnh không".
- 🟡 Nếu shell vừa mới được spawn ở cùng nhịp (`start()` → `pty_spawn` → `cd` gửi ngay), chuỗi `cd`
  nằm trong bộ đệm tty và sẽ được zsh đọc sau khi ZLE khởi động — hoạt động, nhưng với `.zshrc`
  nặng thì người dùng thấy dòng `cd '...'` bị echo ra giữa các dòng khởi tạo, trông như lỗi.
- 🟡 Không kiểm `alive`: nếu shell đã chết, `cd()` gọi thẳng `sendRaw` (`:131`) chứ không đi qua
  `wireInput`, nên `pty_write` sẽ lỗi `no PTY session` vào console và cú click không làm gì cả —
  không có respawn như khi gõ phím.

---

### D14. Resize cửa sổ Mac trong khi lệnh đang chạy

**Flow** kéo cửa sổ → `ResizeObserver` (`TerminalView.vue:143`) → `scheduleFit` (rAF throttle,
`:76-82`) → `doFit` → `fitAddon.fit()` → `hostResize` (`usePtyTerminal.js:238-251`) →
`invoke('pty_resize')` → `pty.rs:387-399` `master.resize` (TIOCSWINSZ → kernel gửi SIGWINCH cho
foreground group) → rồi `send({t:FRAME_PTY_RESIZE, cols, rows})` cho companion →
`usePtyTerminal.js:174-178` `term.resize`.

**Mong đợi vs thực tế** — đúng: host là authority duy nhất (`if (!isHost) return` ở `:239`), sàn
chống size vô lý `cols<8||rows<3` (`:244`) chặn đúng ca container 0px, và echo cho companion chỉ
gửi **sau khi** `pty_resize` thành công (`:246-247`), nên phone không bao giờ nhận size mà PTY
chưa nhận.

**Phát hiện**

- 🟡 Không so sánh với size trước đó: mỗi frame của một cú kéo chuột là một `invoke('pty_resize')`
  (một task `spawn_blocking`, tranh mutex `session` với `pty_write`) **và** một frame WS broadcast
  tới mọi companion — kể cả khi `cols/rows` không đổi (kéo 1px không đổi số cột vẫn bắn). Thêm một
  `if (cols === lastCols && rows === lastRows) return` là đủ và loại luôn một nguồn tranh lock lúc
  đang gõ.
- Không phát hiện vấn đề về tính đúng đắn của resize khi lệnh đang chạy.

---

### D15. Phone (viewport nhỏ) join — có reshape output của Mac không?

**Flow** phone mount `TerminalView` (cùng file, ENV-1 giữ template trung tính) → `start()` →
`ensureSpawned` → `invoke('pty_spawn')` qua WS (`utils/tauri.js` → `hostInvoke.js:24-27`) →
`hydrateScrollback` → `term.resize(cols, rows)` của **host** (`usePtyTerminal.js:139`) →
`TerminalView.vue:140` `scheduleFit` → `doFit` → **`fitAddon.fit()` chạy** → `hostResize` no-op
(`usePtyTerminal.js:239`).

**Mong đợi vs thực tế** — **Không reshape Mac**: đúng, `hostResize` chặn ở dòng đầu và companion
không có đường nào khác gọi `pty_resize` từ UI. Câu hỏi chính của D15 trả lời an toàn.

**Phát hiện**

- 🟠 **Nhưng phone tự resize xterm của chính nó, trái với hợp đồng T-4.** `doFit`
  (`TerminalView.vue:97`) gọi `fitAddon.fit()` **không phân biệt vai trò**, và nó chạy *sau*
  `hydrateScrollback`. Nên kích thước cuối cùng trên phone là kích thước fit cục bộ (ví dụ 40 cột),
  trong khi PTY đang phát ra các dòng đã wrap cho 200 cột → phone render gãy dòng lung tung, và
  trạng thái đó **tồn tại cho tới khi Mac tình cờ resize** (`FRAME_PTY_RESIZE` là đường duy nhất
  sửa lại, `usePtyTerminal.js:174-178`) — có thể là không bao giờ trong cả phiên.
  Tệ hơn: mỗi lần bàn phím mềm mở/đóng hay xoay ngang máy, ResizeObserver lại bắn → lại ghi đè size
  authoritative bằng size cục bộ.
  Docs (`docs/feat/in-app-terminal.md`, bảng Resize) mô tả đúng hành vi mong muốn — code chưa khớp.
  Sửa: chỉ gọi `fitAddon.fit()` khi `isHost`; companion nên scale font/zoom để lấp khung chứ không
  đổi `cols/rows`.

---

### D16. Phone khoá màn hình giữa lúc build, rồi mở lại — replay scrollback

**Flow** iOS đóng WS (hoặc ping timeout `bridge.js` PING 15s / TIMEOUT 5s) → `close` handler →
`scheduleReconnect` (backoff 1s→10s) → connect lại kèm token → `web_server.rs:556-560`
`handle_companion_socket` → `notify_host_companion_connected` → host `ptyBridge.js:69-70` →
`pushScrollback` (`:27-38`) → `invoke('pty_get_scrollback')` (`pty.rs:414-433`) → frame
`{data, reset:true, cols, rows, alive}` → `usePtyTerminal.js:166-171`.

**Mong đợi vs thực tế** — replay chạy đúng, và mang theo cả `cols/rows/alive` (đúng thứ mà một màn
join muộn không thể suy ra được). Build vẫn chạy trên Mac trong lúc phone ngủ vì PTY không phụ thuộc
màn hình nào.

**Phát hiện**

- 🟡 **Thứ tự ngược: ghi trước, resize sau.** `usePtyTerminal.js:167-169` gọi `writeChunk(...)`
  **rồi** mới `term.resize(frame.cols, frame.rows)`. 256KB scrollback được wrap theo size cũ của
  phone rồi mới đổi size → xterm re-wrap lại toàn bộ, kết quả bố cục lệch. `hydrateScrollback`
  (`:139-140`) làm **đúng thứ tự ngược lại** (resize trước, write sau) — hai đường nhận cùng dữ
  liệu mà làm khác nhau; nên thống nhất.
- 🟡 Trong lúc phone ngủ, host vẫn `send()` mọi frame output. `bridge.js send()` drop khi socket
  không OPEN — nhưng iOS thường giữ socket ở trạng thái OPEN "giả" tới lúc ping timeout (tối đa
  20s). Trong 20s đó, ở tốc độ build cao, các frame vào `UnboundedSender` phía relay
  (`web_server.rs:95-101`) — xem 🟠 backpressure ở D4.
- Không phát hiện mất dữ liệu: sau reconnect nội dung luôn là ring buffer đầy đủ nhất mà host có.

---

### D17. Thoát app khi terminal còn `ssh` / lệnh đang chạy

**Flow** `lib.rs:134-142` `.run(|_app, event| if let RunEvent::Exit = event { pty::shutdown() })`
→ `pty.rs:296-298` → `kill_current` `:278-291` → `kill_process_group(pid)` `:264-275`
(`killpg(SIGHUP)` → poll `killpg(pgid,0)` 12×25ms → `killpg(SIGKILL)`) → `child.kill()` →
`child.wait()`.

**Mong đợi vs thực tế** — **có, giết cả process group**: `portable-pty` gọi `setsid`+`TIOCSCTTY` nên
pid của shell chính là pgid, `killpg` chạm tới mọi hậu duệ. Có unit test end-to-end
(`pty.rs:464-514`) chứng minh tính chất "sau teardown không còn gì người dùng khởi động bên trong",
và comment ở `:258-260` trung thực về việc `killpg` không phải phần chịu lực (kernel tự SIGHUP
foreground group) — hook `RunEvent::Exit` mới là phần chịu lực. Đánh giá này đúng.

**Phát hiện**

- 🟡 `RunEvent::Exit` không chạy khi app bị `SIGKILL` / crash / Force Quit — khi đó `ssh` vẫn mồ côi.
  Không có cách nào chữa hoàn toàn trong process, nhưng có thể giảm bằng cách cho shell con một
  `PR_SET_PDEATHSIG` tương đương (macOS không có) hoặc ghi pgid ra file để dọn ở lần khởi động sau.
  Đáng ghi vào docs như giới hạn đã biết, hiện docs chưa nêu.
- 🟡 `shutdown()` chạy **đồng bộ trên main thread** lúc exit và có thể chiếm tới ~300ms + `wait()`.
  Không vi phạm NEVER BLOCK THE UI theo tinh thần (UI đang đóng), nhưng nếu shell chặn SIGHUP thì
  người dùng thấy app "dính" một nhịp khi quit.
- Không phát hiện vấn đề về việc process group không được dọn ở đường quit bình thường.

---

### D18. Mở terminal trước khi shell rc (nvm/zinit) source xong — có `command not found` không?

**Flow** `spawn_if_absent` `pty.rs:217-233`: `openpty` → `CommandBuilder::new($SHELL)` +
`arg("-l")` + `env("TERM","xterm-256color")` → `slave.spawn_command`.

**Mong đợi vs thực tế** — **Không dính bug PATH race của CLAUDE.md**, và lý do mang tính cấu trúc
chứ không phải may mắn: race đó xảy ra khi app spawn `zsh -lc "<lệnh>"` và *dựa* vào PATH do rc
dựng ra để tìm binary — tức là lệnh chạy đồng thời với việc rc đang source. Ở đây process con **là
chính cái shell tương tác**: nó tự source `.zshenv/.zprofile/.zshrc` xong rồi mới in prompt và mới
đọc phím. Vì stdin/stdout là tty và không có `-c`, zsh/bash tự nhận mình là interactive nên `.zshrc`
(nơi nvm/zinit thường nằm) **được** đọc — `-l` đơn thuần không đủ nếu shell là non-interactive, đây
là chi tiết mà comment `pty.rs:222` nói chưa hết nhưng kết luận thì đúng.
Không có `command -v` hay đường resolve binary tĩnh nào cần thiết ở module này.

**Phát hiện** — không phát hiện vấn đề.
🟡 ghi chú: `CommandBuilder` kế thừa môi trường của process app (app mở từ Finder có PATH tối thiểu
`/usr/bin:/bin:/usr/sbin:/sbin`). Login shell sẽ dựng lại PATH nên không sao, **nhưng** các biến
khác mà Tauri/macOS bơm vào (ví dụ `WEBKIT_*`, `__CFBundleIdentifier`) cũng được kế thừa nguyên vẹn
vào shell của người dùng — khác với một cửa sổ Terminal.app thật. Chưa gây lỗi nào quan sát được,
nhưng là khác biệt im lặng đúng loại "tốn một giờ để chẩn đoán" mà chính docs cảnh báo.

---

### D19. Lệnh hỏi mật khẩu (`sudo`) — echo tắt, phone có lộ ký tự không?

**Flow** `sudo` gọi `tcsetattr(ECHO off)` trên **PTY slave** ở phía Mac. Phím từ cả hai màn đều đi
vào cùng một master (`pty_write`, `pty.rs:372-383`); vì không màn nào echo cục bộ
(`usePtyTerminal.js:207-210` chỉ `send`, không bao giờ `term.write` phím vừa gõ), nên **không màn
nào hiện ký tự** — line discipline không echo, output không có gì để relay.

**Mong đợi vs thực tế** — đúng, không lộ trên màn hình. Quyết định "no local echo" (T-5) ở đây trả
cổ tức đúng chỗ: một terminal có local echo sẽ lộ mật khẩu ngay.
Mật khẩu cũng không vào scrollback (`append_scrollback` chỉ nhận output từ `reader`, `pty.rs:162`).

**Phát hiện**

- 🟡 **Mật khẩu vẫn đi qua dây ở dạng gần-plaintext khi gõ từ phone**: `FRAME_PTY_INPUT` là base64
  (không phải mã hoá) trong JSON trên `ws://` thuần nếu người dùng dùng LAN IP
  (`bridge.js` `wsUrl()` chọn `wss:` chỉ khi trang là https). Kịch bản "gõ `sudo` password từ điện
  thoại qua WiFi quán cà phê" là plaintext trên đường truyền. Đã có đường `tailscale serve`/https
  và docs có nói về nó, nhưng **không có cảnh báo nào ở tầng terminal** và không có gì ngăn người
  dùng dùng http. Nên ghi rõ trong `docs/feat/in-app-terminal.md` § Security.
- 🟡 Không có lỗi trong code, nhưng lưu ý: bàn phím mềm của điện thoại (predictive text bar) có thể
  hiển thị ký tự vừa gõ — nằm ngoài tầm kiểm soát của app; có thể giảm bằng `autocomplete="off"
  autocorrect="off"` trên textarea của xterm (xterm đặt sẵn phần lớn) — chưa kiểm chứng.

---

### D20. Hai màn hình cùng gõ đồng thời — thứ tự ký tự

**Flow** Mac: `onData` → `sendRaw` → `invoke('pty_write')` (Tauri IPC, `pty.rs:372`) →
`spawn_blocking` → lock `session` → `write_all` + `flush`.
Phone: `onData` → `send({t:FRAME_PTY_INPUT})` → WS (giữ thứ tự) → `ptyBridge.js:65-68` →
`invoke('pty_write')` → cùng đường trên.

**Mong đợi vs thực tế** — trộn giữa hai người gõ là ngẫu nhiên và **chấp nhận được** (một shell
chung, đúng thiết kế). Mỗi chunk được `write_all` **trọn vẹn** dưới mutex nên không có ca "nửa ký
tự của Mac xen giữa ký tự của phone" — điểm này đúng.

**Phát hiện**

- 🟠 **Không có gì đảm bảo thứ tự giữa hai `pty_write` liên tiếp của CÙNG một màn.** Mỗi phím là một
  lệnh Tauri độc lập: Tauri chạy các `async` command đồng thời, và mỗi cái lại `spawn_blocking`
  sang một thread khác nhau (`pty.rs:373`). Không có sequence number, không có hàng đợi, không có
  `await` phía JS (`usePtyTerminal.js:190` là fire-and-forget `.catch`). Thứ tự cuối cùng do thứ tự
  giành mutex quyết định → **có thể đảo**. Với gõ tay tốc độ người thì gần như không gặp; với key
  repeat, dán bằng phím ảo, hoặc `sendRaw` từ key row bấm nhanh, xác suất là thật. Với `cd()`
  (`:131`) thì một chuỗi dài đi trong **một** chunk nên an toàn.
  Sửa gọn: một kênh writer chuyên dụng ở Rust (`mpsc` + một thread ghi) — cũng loại luôn việc mỗi
  phím chiếm một slot `spawn_blocking` và tranh mutex với `kill_current` (xem 🟡 D9).
- Không phát hiện vấn đề với thứ tự **output** (được đảm bảo bằng `flush_locked` gọi khi đang giữ
  lock, `pty.rs:89-99`) và thứ tự frame companion→host (WS giữ thứ tự tới tận `ptyBridge`).

---

## Tổng hợp mức độ

| Mức | Ca | Vấn đề | Vị trí |
| :-- | :-- | :-- | :-- |
| 🔴 | D7, D8, D10 | `alive` không đồng bộ sau respawn → phím đầu tiên ở màn kia gọi `restart()`, **giết shell đang chạy + xoá scrollback** | `usePtyTerminal.js:73,86,166-171,214-217` · `ptyBridge.js:53-55` · `pty.rs:313-324` |
| 🟠 | D9 | Byte đuôi của shell cũ lọt qua sau `reset` và ghi vào scrollback vừa xoá (thiếu guard generation) | `pty.rs:162,177-181,313-324` |
| 🟠 | D2 | Rò `listen('pty-output'/'pty-exit')` khi unmount trước lúc promise resolve → ghi vào `Terminal` đã dispose | `usePtyTerminal.js:152-162,262-266` |
| 🟠 | D12 | `cd "{path}"` nháy kép → command injection qua tên thư mục (`$( )`, `` ` ``, `"`) | `system.rs:139-146` |
| 🟠 | D12 | Fallback `'~'` sinh `cd "~"` → luôn lỗi, không về `$HOME` như docs | `usePtyTerminal.js:118` · `system.rs:142` |
| 🟠 | D15 | Companion vẫn `fitAddon.fit()` → tự đặt `cols/rows`, ghi đè size authoritative (trái T-4/docs) | `TerminalView.vue:97` |
| 🟠 | D5 | Key row không `preventDefault` mousedown → mất focus, bàn phím mềm đóng mỗi lần chạm | `TerminalView.vue:14-23` |
| 🟠 | D4 | Không backpressure cho companion (`UnboundedSender`, không xét `bufferedAmount`) | `web_server.rs:63,95-101` · `ptyBridge.js:53-55` |
| 🟠 | D20 | `pty_write` không đảm bảo thứ tự giữa các lệnh liên tiếp (mỗi phím một `spawn_blocking`) | `pty.rs:372-383` · `usePtyTerminal.js:190` |

🟡 (không liệt kê lại chi tiết): nháy đỏ "EXITED" lúc mount (D1) · fit khi dock collapsed sát ngưỡng
`rows<3` và `isLogExpanded` lại được mirror (D2) · `append_scrollback` O(n) mỗi read (D4) · ring
buffer cắt giữa escape sequence (D4/D6) · `ctrlArmed` bị tiêu thụ sai/không tự huỷ (D3/D5) ·
`kill_current` giữ mutex ~300ms (D9) · `cd()` gửi phím khi có lệnh foreground và không kiểm `alive`
(D13) · `pty_resize` bắn cả khi size không đổi (D14) · replay resize-sau-write (D16) · không dọn
được khi Force Quit (D17) · input plaintext trên `ws://` khi gõ mật khẩu từ phone (D19).

**Không phát hiện vấn đề** ở: D1 (đường fit lần đầu — đã sửa đủ ở cả 3 tầng), D3 (UTF-8/emoji, chuỗi
binary-safe đúng end-to-end), D11 (CLEAR ở cả 3 tình huống), D17 (đường quit bình thường có dọn
process group), D18 (không dính PATH race vì shell tương tác tự source rc).
## Cụm E — Vỏ app & công cụ

Phạm vi soi: 20 use case E1–E20, đọc code thật theo chuỗi UI → composable/store → IPC → Rust → về.
Đã đối chiếu **toàn bộ** `#[tauri::command]` (kết quả ở E10) và toàn bộ `capabilities/default.json` (E4).

Ký hiệu: 🔴 nghiêm trọng (mất dữ liệu / treo / sai chức năng) · 🟠 lỗi thật, ảnh hưởng người dùng · 🟡 rủi ro / lệch quy tắc, chưa chắc gây hại.

---

### E1. Khởi động lạnh lần đầu (chưa có config nào)

**Flow**
1. `src/main.js:1` — `import "./boot/roleStamp"` chạy đầu tiên; `src/boot/roleStamp.js:16-18` đặt `window.__AKI_ROLE__='host'` khi có `__TAURI_INTERNALS__`.
2. `src/main.js:8` `createApp(App).mount('#app')`.
3. Rust: `src-tauri/src/lib.rs:17-23` — `logger::init()` rồi `web_server::init()` trong `.setup()`.
4. `src/App.vue:59` `useCompanionPairing()` → `ready` = true trên host (`useCompanionPairing.js:24`) → cả subtree mount.
5. `src/App.vue:69` `initRemote()` → `services/index.js:34-41` (mirror → intents → hostInvoke → ptyBridge → `connect()`).
6. `src/App.vue:75-86` `onHostBoot(...)` → `loadData()` + `initGlobalNote()` + `refreshClaudeMode()` + `refreshProjectIcons()` + `cleanup_legacy_baselines` (gate localStorage `aki-legacy-baseline-cleanup-v1`).
7. `useProjectConfig.js:66-68` → `get_ssh_hosts` (`ssh.rs:36`, trả `[]` nếu không có `~/.ssh/config`) và `load_projects` (`projects.rs:131-143`, `path.exists()` false → `vec![]`).

**Mong đợi vs thực tế**
Boot lần đầu không crash, mọi lệnh đều degrade về rỗng đúng cách. `get_projects_path` (`projects.rs:106-118`) tự `create_dir_all` app-data dir → đúng.

**Phát hiện**

🟡 **IntroModal không tự mở lần chạy đầu.** `src/composables/useIntro.js:3` `showIntroModal = ref(false)` và `IntroModal.vue:2` chỉ bind `:show="showIntroModal"`; không có cờ localStorage kiểu `intro-seen` ở đâu cả. Người dùng mới thấy bảng rỗng, không có onboarding. Đồng thời `AppHeader.vue:249` `<span class="badge-dot">` (chấm đỏ nhấp nháy trên nút INTRO) là **vô điều kiện** — nó không bao giờ tắt sau khi đã đọc Intro, nên mất hết ý nghĩa "có cái mới".

🟡 **`startBackgroundRefresh()` tạo `watch()` ngoài effect scope.** `useProjectConfig.js:114` gọi `startBackgroundRefresh()` sau nhiều `await` trong `loadData`, tức đã rời khỏi synchronous setup của App.vue → 3 `watch` ở `useBackgroundRefresh.js:100-109` là watcher toàn cục không gắn scope, không bao giờ stop. Không rò rỉ thực tế vì App không unmount, nhưng cờ `watching` (dòng 94) mới là thứ duy nhất chặn nhân bản.

---

### E2. Preset cửa sổ + ⌘1/⌘2 + remember

**Flow**
`AppHeader.vue:176-221` (lưới 2x2) → `applyViewSafe` (`AppHeader.vue:552`) → `useAppWindow.js:177-183` `applyView(axis,name)` → `VIEWS[axis][name]()` (`useAppWindow.js:156-165`) → `setWidthPreset` (128) / `stickTopLeft` (221) / `centerPrimary` (248).
Phím tắt: `AppHeader.vue:444` đăng ký `keydown` → `onViewShortcut` (`AppHeader.vue:562-567`) → `applyViewCombo` (`useAppWindow.js:186-191`).
Khởi động lại: `AppHeader.vue:443` `restoreView()` → `useAppWindow.js:208-213`.

**Mong đợi vs thực tế**

🟠 **"Stick Top-Left" được nhớ sẽ khôi phục sai chiều cao sau khi khởi động lại.** `restoreView()` chạy trong `onMounted` của AppHeader (`AppHeader.vue:443`), còn dữ liệu project nạp bất đồng bộ qua `loadData()` (`App.vue:76` → `useProjectConfig.js:60`). `stickTopLeft()` (`useAppWindow.js:239`) đo bằng `measureRequiredContentHeight()` (`useAppWindow.js:46-55`), đọc `.grid-body`'s `scrollHeight` — tại thời điểm đó bảng project **còn rỗng**, nên tổng đo được rất nhỏ, bị `Math.max(MIN_WINDOW_HEIGHT, ...)` (dòng 240) kẹp về đúng 500px. Kết quả: bấm ⌘1 thủ công cho chiều cao vừa danh sách, nhưng "remember + restart" luôn cho 500px. Không có `await nextTick`/chờ project nào giữa hai chỗ.

🟡 **⌘1/⌘2 bắt phím toàn cục, kể cả khi đang gõ trong textarea/modal.** `onViewShortcut` (`AppHeader.vue:562`) chỉ loại trừ `ctrlKey/shiftKey/altKey/repeat`; không kiểm tra `e.target` là input/textarea, cũng không kiểm tra có modal đang mở. Gõ ⌘1 trong Global Note hoặc Statusline Customizer sẽ resize cửa sổ.

🟡 **Bật "remember" không ghi lại preset đang dùng.** `toggleRememberView` (`useAppWindow.js:193-202`) chỉ xử lý nhánh tắt (xoá `savedView`). Bật lên → `savedView` vẫn `{}` cho tới khi bấm một preset; nhãn "remember" gợi ý ngược lại.

Không có vấn đề: `applyView` ghi `savedView` trước khi thực thi và `restoreView` áp width trước place (đúng thứ tự đã ghi chú ở `useAppWindow.js:204-207`); `companionWindow()` (68-86) trả đủ 12 key nên phone không crash.

---

### E3. Narrow mode 420px

**Flow**
`tauri.conf.json` `minWidth: 420`; `NARROW_WIDTH = 420` (`useAppWindow.js:34`).
Breakpoint duy nhất: `src/assets/main.css:1244` `@media (max-width: 700px)`, tiện ích `.u-narrow-hide` (1245) / `.u-wide-hide` (1293).

**Mong đợi vs thực tế — không phát hiện lỗi vỡ layout rõ ràng.** Cụ thể đã kiểm:
- Khối narrow của `main.css` nằm **cuối file** (1244–1297) đúng như comment 1236-1239 yêu cầu → không mất cascade.
- Modal: `.modal-overlay` bắt đầu ở `top: var(--titlebar-h)` (`main.css:810`) → tuân thủ ranh giới titlebar 42px.
- `.modal-footer { flex-wrap: wrap }` ở `main.css:1281` xử lý hàng 5 nút của GitModal.
- `.form-grid { grid-template-columns: 1fr }` (`main.css:1288`) cho ProjectConfigModal.
- Hai modal có `<style scoped>` đè `.modal-body`/`.modal-footer` đều đã lặp lại override narrow của mình: `ClaudeProfileModal.vue:144/235` + `@media` ở 288, `ClaudeSettingModal.vue:1059/1568` + `@media` ở 1647. Không còn modal nào đè mà thiếu override.
- Nhãn ẩn trước icon: `AppConsole.vue:37,42,46,55,60,64,68` và `AppHeader.vue:248` đều dùng `u-narrow-hide` trên `<span>` text, icon `<i>` giữ nguyên → đúng quy tắc Extreme Narrow.

🟡 **Comment ở `main.css:1240-1242` đã lệch thực tế**: nó liệt kê ChangelogModal / ClaudeProfileModal / SshConfigModal / UpdateModal là các modal "phải lặp lại override" — nhưng ChangelogModal, SshConfigModal, UpdateModal thực ra **không** đè `.modal-body`/`.modal-footer` (grep không thấy), còn ClaudeSettingModal thì có và không được nhắc tên. Chỉ là tài liệu sai, không phải lỗi runtime.

---

### E4. Kéo titlebar tự vẽ / minimize / close — quyền capabilities

**Flow**
`AppHeader.vue:3` `@mousedown.prevent="startDragging"` → `useAppWindow.js:104` `appWindow.startDragging()`.
`AppHeader.vue:289/292` → `minimize()` / `closeWin()` (`useAppWindow.js:96-102`).
`AppHeader.vue:285` `togglePin` → `applyPinned` (`useAppWindow.js:108-111`) → `setAlwaysOnTop` + `setVisibleOnAllWorkspaces`.

**Đối chiếu `src-tauri/capabilities/default.json` với mọi window API mà JS gọi:**

| API gọi trong JS | file:line | Permission | Có |
|---|---|---|---|
| `startDragging` | useAppWindow.js:105 | `core:window:allow-start-dragging` | ✅ |
| `minimize` | :97 | `core:window:allow-minimize` | ✅ |
| `close` | :101 | `core:window:allow-close` | ✅ |
| `setAlwaysOnTop` | :109 | `core:window:allow-set-always-on-top` | ✅ |
| `setVisibleOnAllWorkspaces` | :110 | `core:window:allow-set-visible-on-all-workspaces` | ✅ |
| `setSize` | :136, :244 | `core:window:allow-set-size` | ✅ |
| `setPosition` | :147, :232, :254 | `core:window:allow-set-position` | ✅ |
| `outerSize` | :131, :236, :251 | `core:window:allow-outer-size` | ✅ |
| `outerPosition` | :132 | `core:window:allow-outer-position` | ✅ |
| `scaleFactor` | :130 | `core:window:allow-scale-factor` | ✅ |
| `currentMonitor` | :133 | `core:window:allow-current-monitor` | ✅ |
| `availableMonitors` | :222 | `core:window:allow-available-monitors` | ✅ |
| `primaryMonitor` | :249 | `core:window:allow-primary-monitor` | ✅ |

**Không phát hiện thiếu quyền.** (`window.show()`/`set_focus()` trong `lib.rs:29-30` là phía Rust, không qua IPC nên không cần capability.)

🟡 **`@mousedown.prevent` trên `<header>` bắt sự kiện bubble từ mọi nút con.** Chỉ `.app-icon-menu` có `@mousedown.stop` (`AppHeader.vue:5`); các nút INTRO/Note/Refresh và 3 nút traffic-light (`AppHeader.vue:247-294`) không có, nên mousedown của chúng bubble lên header và gọi `startDragging()` + `preventDefault()`. Theo CHANGELOG (mục "Narrow mode: changelog click disabled" — họ phải fix bằng `pointer-events:none` cho `.app-version` vì click **vẫn** kích hoạt khi kéo cửa sổ), click không bị nuốt → các nút vẫn bấm được. Rủi ro còn lại là chiều ngược: kéo cửa sổ khởi đầu trên vùng một nút sẽ kích hoạt nút đó. Cần kiểm chứng tay, không kết luận từ code được.

Modal **không** bị dính vấn đề này: `BaseModal.vue:2` dùng `<Teleport to="body">`, nên dù `ClaudeSettingModal`/`GlobalNoteModal`… được khai báo bên trong `<header>` (`AppHeader.vue:266-277`), DOM thật nằm ngoài header.

---

### E5. Statusline Customizer — đổi màu/field, Apply CC / AG / cả hai

**Flow**
Mở modal → `ClaudeSettingModal.vue:645-656` `watch(props.show)` → `loadCfg()` + `checkAndAutoInstall()`.
→ `invoke('check_statusline_status', {hosts})` (`:660`) → `statusline.rs:397-433`, `spawn_blocking` + 1 thread/host → `run_remote_script_bounded` (`agent_usage.rs:36`, ceiling `STATUSLINE_TIMEOUT_SECS=5`).
Apply → `ClaudeSettingModal.vue:885` → `statusline.rs:435-490` → `build_installer_script` (`:341-364`) → `generate_statusline_script` (`:274-297`) splice `config_block` vào giữa 2 marker của `statusline-unified.sh`.

**Mong đợi vs thực tế — bảng màu preview khớp ANSI thật.** Đã đối chiếu từng dòng:
- `src/utils/statuslineColors.js:22-31` (SSOT) ↔ `statusline.rs:141-152` `ansi_for()`: white 97, cyan 36, green 01;32, blue 01;34, grey 90, red 31, yellow 01;33, magenta 35 — **khớp 8/8**.
- `statuslineColors.js:39-45` `STATUSLINE_TIERS` ↔ `statusline-unified.sh:187-193` `BOLD_*`: calm `01;38;5;86`, green `01;32`, yellow `01;33`, orange `01;38;5;208`, red `01;31` — **khớp 5/5**.
- Ngưỡng: `tierHex()` (`ClaudeSettingModal.vue` phần preview) đi thang giảm dần đúng như `color_for_pct()` (`statusline-unified.sh:269-273`).
→ **Không phát hiện lệch màu.**

**Phát hiện**

🟠 **Apply "cả hai cùng lúc" không nguyên tử: CC hỏng thì AG không được ghi, mà thông báo không nói ra.** `build_installer_script` (`statusline.rs:355-362`) nối các installer thành **một** script bắt đầu bằng `set -e`. Nếu nửa Claude Code fail (điển hình nhất: host không có `jq` — cả hai installer đều bắt buộc `jq`, `statusline.rs:319` và `:334`), `set -e` cắt ngay tại đó, nửa AGY không bao giờ chạy. Kết quả trả về chỉ có `ok:false` + stderr thô (`statusline.rs:466-473`) — người dùng không biết là "CC ghi file xong nhưng chưa patch settings, AG không đụng tới gì".

🟠 **CC fail giữa chừng để lại trạng thái nửa vời.** Thứ tự trong installer là: `cat > $FILE` → `chmod +x` → rồi mới `jq` patch settings (`statusline.rs:313-320`). Nếu `jq` vắng, script `statusline-command.sh` đã bị **ghi đè** trong khi `settings.json` vẫn trỏ chỗ cũ. Không có bước kiểm tra `command -v jq` đầu script, cũng không rollback.

🟡 **Auto-install im lặng khi mở modal.** `checkAndAutoInstall()` (`ClaudeSettingModal.vue:657-694`) tự Apply target `cc` lên **mọi** host có Claude Code mà chưa cấu hình statusline, chỉ vì mở modal — không hỏi. Có ghi log ở `status.msg` sau khi xong, nhưng đây là hành vi ghi file lên host từ xa mà người dùng chưa bấm Apply.

---

### E6. Script đích chưa tồn tại / đã bị người dùng sửa tay

**Flow**
`statusline.rs:307-322` (Claude Code) và `:325-339` (AGY), chạy trên host qua `run_remote_script_bounded`.

Đoạn quyết định:
```
if [ -f "$FILE" ] && [ ! -f "$FILE.aki-bak" ]; then cp "$FILE" "$FILE.aki-bak"; fi
cat > "$FILE" <<'AKI_STATUSLINE_CLAUDE_EOF' … 
```

**Mong đợi vs thực tế**
- Chưa tồn tại: `mkdir -p` + `cat >` tạo mới, `[ -f ]` false nên không backup — đúng.
- `settings.json` chưa có: `[ -f "$SETTINGS" ] || echo '{}' > "$SETTINGS"` — đúng.

**Phát hiện**

🟠 **Chỉ backup đúng MỘT lần trong đời; sửa tay sau lần Apply đầu tiên sẽ mất trắng.** Điều kiện `[ ! -f "$FILE.aki-bak" ]` (`statusline.rs:310` và `:329`) có nghĩa: lần Apply đầu backup bản gốc; từ lần thứ hai trở đi, dù người dùng đã tự sửa `~/.claude/statusline-command.sh`, `cat >` ghi đè thẳng và **không tạo backup mới** — `.aki-bak` vẫn là bản từ trước khi dùng app. Không có so sánh checksum, không có cảnh báo "file này khác bản app sinh ra", không có `.aki-bak.2`. Đây là điểm khác biệt đáng chú ý so với `install_ssh_terminal_color` (E16), nơi vùng do app quản lý được đánh dấu bằng marker BEGIN/END và chỉ vùng đó bị thay.

🟡 **Không có nút "khôi phục bản gốc"** trong UI, dù `.aki-bak` tồn tại — người dùng phải tự vào terminal.

---

### E7. SSH config editor — thêm/sửa/xoá host

**Flow**
`AppHeader.vue:35` `openSshConfig()` → `useSsh.js` → `read_ssh_config` (`ssh.rs:56-64`).
Lưu: `SshConfigModal.vue:50` `save()` → `saveSshConfig()` (action) → `save_ssh_config` (`ssh.rs:75-92`).
Undo/Redo: `ssh.rs:94-104` → `swap_ssh_state` (`ssh.rs:21-33`).
Danh sách host cho combobox project: `get_ssh_hosts` (`ssh.rs:35-54`), lọc bỏ pattern chứa `*`/`?`.

**Mong đợi vs thực tế**
Undo/redo 1 bậc có backup file trong app-data (`ssh_undo_state.txt` / `ssh_redo_state.txt`) — an toàn ở mức cơ bản.

**Phát hiện**

🟠 **Xoá/đổi tên một host đang được project khác dùng: không có cảnh báo, không có migrate.** `save_ssh_config` (`ssh.rs:75`) chỉ ghi file, hoàn toàn không biết gì về `projects.json`. Sau khi lưu, `p.remote_host` của các project trỏ tới host vừa xoá trở thành chuỗi chết: `validate_project` (`projects.rs:100-107`) chỉ kiểm tra rỗng, nên `check_sync_status`/`run_sync` vẫn chạy và fail ở tầng `ssh` với lỗi thô. Comment `SshConfigModal.vue:48` nói *"host-side project migration now lives in the saveSshConfig/undo/redo action path"* — nhưng phía Rust không có một dòng nào đụng tới projects; nếu migration có tồn tại thì nó nằm trong action JS, cần xác nhận riêng ở phía `useSsh.js`/`sshStore.js` (chỉ 116 + 21 dòng, không thấy tham chiếu `projects`).

🟡 **Undo/redo chỉ 1 bậc và bị cụt sau lần lưu tiếp theo.** `save_ssh_config:90` `remove_file(&redo_path)` xoá redo mỗi lần lưu; `swap_ssh_state:31` xoá file nguồn sau khi swap. Không có lịch sử nhiều bậc.

🟡 **`get_ssh_hosts` không xử lý `Include`.** `ssh.rs:39-51` chỉ đọc `~/.ssh/config` và bắt dòng `Host `; các host khai báo trong file được `Include` (rất phổ biến) sẽ không xuất hiện trong dropdown.

---

### E8. Claude Code Profile (Local) — đổi profile

**Flow**
`AppHeader.vue:55` mở `ClaudeProfileModal` → `set_claude_profile` (`claude_profile.rs:48-86`).
Đọc trạng thái: `App.vue:78` `refreshClaudeMode()` → `claudeModeStore.js:12-18` → `get_claude_mode` (`claude_profile.rs:26-38`).

**Mong đợi vs thực tế**
`set_claude_profile` đọc `~/.claude/settings.json`, xoá 5 key `PROXY_ENV_KEYS` (`:40-46`) trong `env`, chèn lại nếu mode=proxy, xoá luôn `env` nếu rỗng (`:81-83`). Merge đúng — **không** ghi đè cả file, giữ nguyên các key khác của người dùng. Đây là hành vi đúng theo Regression Guard multi-entity trong CLAUDE.md.

**Phát hiện**

🟠 **Đổi profile không refresh `claudeMode` → UI vẫn hiển thị mode cũ.** `refreshClaudeMode()` chỉ được gọi đúng một lần ở `App.vue:78` (boot). `ClaudeProfileModal` gọi `set_claude_profile` nhưng không có chỗ nào gọi lại `refreshClaudeMode()` sau đó (grep `refreshClaudeMode` chỉ ra 2 hit: định nghĩa `claudeModeStore.js:12` và call site `App.vue:78`). Hệ quả cụ thể: `claudeMode` khoá/mở khối theo dõi usage native (theo comment `claudeModeStore.js:4-9`) — chuyển sang proxy xong, khối usage vẫn tiếp tục hiển thị số native "trông như thật nhưng không phải" cho tới lần khởi động lại app. Đây đúng là kịch bản mà comment ấy nói phải chặn.

🟡 **`set_claude_profile` không backup `settings.json`.** Nếu file JSON của người dùng bị lỗi cú pháp, `read_settings` (`claude_profile.rs:8-13`) `unwrap_or_else(|| json!({}))` → **toàn bộ settings.json bị thay bằng một object chỉ có `env`**. Đây là mất dữ liệu im lặng: file hỏng nhẹ (thừa dấu phẩy) đủ để kích hoạt.

🟡 Cả `get_claude_mode` (`:27`) và `set_claude_profile` (`:49`) là `pub fn` đồng bộ đọc/ghi file — theo CLAUDE.md thì file I/O nhỏ cục bộ không thuộc lớp bug này, nên đúng luật; ghi lại để đối chiếu.

---

### E9. Cài AkiClaudeDoc từ menu

**Flow**
`AppHeader.vue:132` → `installAkiClaudeDoc()` (`AppHeader.vue:530-536`) → `invoke('install_akiclaudedoc')` → `system.rs:273-289` → `find_akiclaudedoc_install_script` (`system.rs:261-270`) → `open_terminal_with_command` (`system.rs:47-75`).

**Mong đợi vs thực tế**
Dò 5 đường dẫn ứng viên bằng `Path::exists()`, chạy `bash install.sh` trong Terminal.app có chống double-window (poll 20×100ms). `osascript` chỉ `.spawn()` → **không chặn UI** dù là `pub fn` đồng bộ.

**Phát hiện**

🟡 **Thông báo lỗi bằng tiếng Việt lẫn trong app tiếng Anh.** `system.rs:284-287` trả `"Không tìm thấy AkiClaudeDoc trên máy này. Clone repo trước: …"`, hiển thị nguyên văn qua `Toast.fire({... text: String(e)})` (`AppHeader.vue:534`). CLAUDE.md yêu cầu text người dùng thấy là English, terse.

🟡 **Không phản hồi khi thành công.** `installAkiClaudeDoc` (`AppHeader.vue:530`) chỉ Toast khi catch; đường happy path không Toast gì — nếu Terminal.app mở ở Space khác, người dùng bấm xong tưởng menu chết.

🟡 **Danh sách ứng viên hard-code `/Volumes/DEV/AkiClaudeDoc/install.sh`** (`system.rs:263`) — đường dẫn riêng của một máy nằm trong binary phân phối.

---

### E10. Kiểm tra cập nhật (thủ công + tự động lúc khởi động) — NEVER BLOCK THE UI

**Flow**
Tự động: `AppHeader.vue:448-462` `onHostBoot(async () => invoke('check_for_updates'))`.
Thủ công: `AppHeader.vue:476-511` `triggerManualUpdateCheck()`.
Rust: `system.rs:486-508` — `pub async fn` + `spawn_blocking` bọc `curl …output()`. ✅ **Đã sửa đúng, không còn chặn UI.**

**Audit toàn bộ `#[tauri::command]` (mọi lệnh chạy subprocess/network):**

| Lệnh | file:line | subprocess/net? | `spawn_blocking`? |
|---|---|---|---|
| `get_git_info` | git.rs:70 | có | ✅ :72 |
| `run_git_command` | git.rs:109 | có | ✅ :111 |
| `get_file_conflict_info` | git.rs:135 | có (ssh) | ✅ :142 |
| `run_sync` | sync.rs:384 | có (rsync) | ✅ (async, :285 pattern) |
| `check_sync_status` | sync.rs:682 | có | ✅ :686 |
| `get_sync_delete_preview` | sync.rs:702 | có | ✅ |
| `cleanup_legacy_baselines` | sync.rs:289 | **không** (chỉ fs cục bộ) | n/a |
| `provision_agent_usage` | agent_usage.rs:328 | có (ssh) | ✅ :331 |
| `get_agent_usage` | agent_usage.rs:361 | có | ✅ |
| `logout_antigravity` | agent_usage.rs:682 | có | ✅ :684 |
| `logout_antigravity_cli` | agent_usage.rs:728 | có | ✅ :730 |
| `check_statusline_status` | statusline.rs:397 | có (ssh) | ✅ :399 |
| `apply_statusline_config` | statusline.rs:435 | có (ssh) | ✅ :444 |
| `install_ssh_terminal_color` | system.rs:215 | fs | ✅ :217 |
| `resolve_remote_path` | system.rs:395 | có (ssh) | ✅ :402 |
| `resolve_report_html` | system.rs:427 | có (rsync) | ✅ :465 |
| `check_for_updates` | system.rs:486 | có (curl) | ✅ :488 |
| `macos_open` | system.rs:128 | `.spawn()` không wait | n/a |
| `open_local_terminal` | system.rs:138 | `.spawn()` | n/a |
| `open_remote_subprocess` | system.rs:163 | `.spawn()` | n/a |
| `install_akiclaudedoc` | system.rs:273 | `.spawn()` | n/a |
| `run_project_command` / `run_project_dev` | system.rs:582 / 592 | `.spawn()` | n/a |
| `check_ide_availability`, `check_project_stack`, `find_in_downloads`, `read_project_changelog` | system.rs:373/527/475/605 | chỉ `Path::exists`/read | n/a |
| toàn bộ `web_server::*` | web_server.rs:698–1000 | có | ✅ mọi lệnh |
| toàn bộ `pty::*` | pty.rs:202–420 | có | ✅ mọi lệnh |
| `ssh::*`, `projects::*`, `claude_profile::*`, `global_note::*`, `logger::*` | — | file I/O nhỏ cục bộ | n/a (đúng luật) |

**Kết quả: không phát hiện lệnh nào vi phạm quy tắc NEVER BLOCK THE UI.**

🟡 **`hasUpdate()` so sánh semver bằng `Number()` trên từng phần, không xử lý hậu tố.** `AppHeader.vue:421-431`: tag dạng `1.20.0-rc1` → `Number('0-rc1')` = `NaN`, `NaN > c` là false, `NaN < c` cũng false → vòng lặp chạy tiếp rồi trả `false`, tức bản pre-release bị coi là "không có update". Chấp nhận được nếu không bao giờ tag pre-release, nhưng là giả định ngầm.

🟡 **Chỉ nhớ "đã bỏ qua" cho auto-check, không cho manual.** `UPDATE_DISMISS_KEY` chỉ đọc ở `:454`; `triggerManualUpdateCheck` (`:485`) luôn mở modal — hợp lý (người dùng chủ động hỏi), ghi lại cho đủ.

---

### E11. Background refresh theo lịch — chồng lấn khi ẩn/hiện, sleep/wake

**Flow**
`useProjectConfig.js:114` `startBackgroundRefresh()` → `useBackgroundRefresh.js:96-112`:
- `restartGitTimer()` (`:42-53`) — `clearInterval` trước, rồi `hostInterval(..., git_interval_s*1000)`.
- `restartDiffTimer()` (`:59-70`) — gated thêm bởi `syncCheckEnabled`.
- 3 `watch` (`:100-109`) tái tạo timer khi setting đổi.
`utils/scheduler.js:14` `hostInterval` = `setInterval` trên host, `() => null` trên companion.

**Mong đợi vs thực tế**
Chống chồng lấn ở mức thay đổi setting: **đúng** — mọi `restart*Timer` đều `clearInterval` trước. Companion không có timer nào (scheduler gate) → không nhân đôi khi ghép phone.

**Phát hiện**

🟠 **Không có cơ chế wake self-heal cho timer git / remote-diff, dù đúng vấn đề đó đã được xử lý cho usage monitor.** `usageMonitor.js:85-113` mô tả và cài đặt hẳn hai lớp phục hồi cho việc **WKWebView đình chỉ `setInterval`** khi cửa sổ bị che, minimize hoặc máy sleep: `visibilitychange`/`focus` (`:101-104`) + watchdog heartbeat 7s (`:106-113`). `useBackgroundRefresh.js` dùng **cùng một `setInterval`** cho `gitTimer`/`diffTimer` nhưng **không** đăng ký gì cả — grep `visibilitychange` chỉ trả về `usageMonitor.js`. Hệ quả: sau khi thu nhỏ cửa sổ / máy ngủ, trạng thái git và diff remote có thể đứng im vô thời hạn trong khi vòng usage vẫn tự hồi phục; người dùng thấy usage cập nhật nên tin là cả bảng đang tươi.

🟡 **`refreshAllProjects()` khởi động lại cả hai timer** (`useBackgroundRefresh.js:87-88`). Bấm Refresh toàn cục liên tục sẽ liên tục dời mốc đếm ngược của vòng nền — có chủ ý theo comment, nhưng nghĩa là một người bấm nhiều có thể khiến vòng nền không bao giờ tự tick.

---

### E12. Global event log — cap và phình DOM

**Flow**
`useLogs.js:34-40` `appendGlobalLog()` → `globalLogs.value.push(line)`.
`useLogs.js:26-32` `appendLog(projectId,line)` → `projectLogs.value[id].push(line)`.
Nguồn: listener Tauri `sync-log` (`useLogs.js:78-85`), `remoteActions.js:36`, và mọi call `appendGlobalLog` khắp `useProjectConfig.js` / `useSync.js`.
Render: `AppConsole.vue:80` `v-for="(line,index) in displayedLogs"` — không ảo hoá.

**Phát hiện**

🟠 **Không có cap ở bất kỳ đâu — mảng log và DOM đều tăng vô hạn.** `store/logStore.js` chỉ có 10 dòng, `globalLogs = ref([])` (`:3`) và `projectLogs = ref({})` (`:4`), không có `slice`/`shift`/`MAX_LINES`. Grep toàn repo không tìm thấy chỗ nào cắt bớt; chỉ có `clearLog()` thủ công (`useLogs.js:42-48`) và `useSync.js:64,188` reset log của **một** project khi bắt đầu sync. `AppConsole.vue:80` sinh một `<div>` cho mỗi dòng → một phiên sync dài (rsync verbose) tạo hàng chục nghìn node. Trái ngược với phía Rust, nơi PTY scrollback có giới hạn rõ ràng (`pty.rs`).

🟠 **Mỗi dòng log mới phát lại TOÀN BỘ mảng log sang phone.** `logStore.js` nằm trong `src/store/*.js`, mà `services/mirror.js:14` `import.meta.glob('../store/*.js', {eager:true})` đăng ký **mọi** ref export từ đó, rồi `initMirror` (`mirror.js:136-147`) đặt `watch(ref, …, {deep:true})` cho từng key. `encodeKeyed` (`mirror.js:103-105` trong `broadcastFull`, và cùng cơ chế cho delta) mã hoá **giá trị đầy đủ** của ref, không phải diff nội bộ mảng. Vậy khi log đạt 10k dòng, mỗi dòng mới đẩy ~1 MB JSON qua WebSocket. Hai vấn đề này cộng lại làm phiên dài trở nên rất tốn.

🟡 `projectLogs` là map theo project id nhưng không bao giờ xoá entry khi project bị remove — `removeProject` (`remoteActions.js:208-215`) xoá `projectRuntime[id]` nhưng không xoá `projectLogs.value[id]`.

---

### E13. Ghi chú global — lưu ở đâu, mirror sang phone thế nào

**Flow**
`AppHeader.vue:251` → `openGlobalNote()` (`useGlobalNote.js:24-32`) → `invoke('read_global_note')`.
Gõ → `onNoteInput` (`useGlobalNote.js:39-46`) debounce 500ms → `flushSave` (`:48-62`) → `saveNote` action (`noteStore.js:17-20`) → `invoke('write_global_note')`.
Rust: `global_note.rs:3-18` đọc / `:20-32` ghi `{appDataDir}/globalnote.json` dạng `{"content": …}`.
Mirror: `noteContent` là ref export từ `src/store/noteStore.js` → tự động vào `STATE` của `mirror.js:14`; edit trên phone chạy qua `action()` nên thực thi trên host (comment `noteStore.js:12-16`).

**Mong đợi vs thực tế**
Kiến trúc đúng: nội dung là shared state ở store (mirror host→companion), còn `showGlobalNote`/`noteSaving` là local (`useGlobalNote.js:10-11`). `openGlobalNote` chờ `pendingSave` trước khi đọc lại đĩa (`:28`) để không đè bản vừa lưu. **Không phát hiện lỗi logic.**

🟡 **Ghi không nguyên tử.** `global_note.rs:30` `std::fs::write` thẳng vào `globalnote.json` — không ghi file tạm rồi rename. Mất điện / crash giữa chừng làm hỏng ghi chú. (Cùng vấn đề với `projects.rs:146-152` `save_projects`.)

🟡 **`read_global_note` nuốt lỗi JSON hỏng.** `global_note.rs:16` `serde_json::from_str(&raw).unwrap_or_default()` → file hỏng trả về chuỗi rỗng, rồi lần gõ tiếp theo ghi đè bằng nội dung rỗng đó. Ghi chú biến mất im lặng thay vì báo lỗi.

---

### E14. Task/checklist của project

**Flow**
`ProjectTasksModal.vue` → `useProjectTasks.js`: `addTask` (`:75-92`), `toggleTaskProp` (`:98-105`), `removeTask` (`:107-114`), `sortedTasks` (`:46-64`).
Mọi mutation đi qua `applyTaskEdit(project.id, patch)` (`remoteActions.js:133-139`) — resolve project theo **id** trên host rồi `saveProjectsList()`. Đúng PERSIST-1 và đúng Regression Guard (chỉ chạm 1 project).
Phía Rust: `ProjectTask` (`projects.rs:23-38`) có `#[serde(default)]` trên `detail/done/pin/wish/created_at/updated_at` — bản ghi cũ không mất field. ✅

**Phát hiện**

🟡 **`id` task trùng nếu thêm 2 task trong cùng 1 mili-giây.** `useProjectTasks.js:80-81`: `id: 'task-' + now` với `now = Date.now()`. `removeTask` (`:109`) tìm bằng `findIndex(t => t.id === task.id)` → xoá nhầm cái đầu tiên. Chỉ đạt được bằng script/paste, không phải thao tác tay.

🟡 **"Sắp xếp" là tự động, không kéo thả được.** `sortedTasks` (`:46-64`) sắp cứng theo done → pin → wish → `created_at`. Không có cơ chế thứ tự thủ công (khác với danh sách project, vốn có `reorderProjects`). Nếu use case mong đợi kéo thả thì đây là tính năng thiếu, không phải bug.

🟠 **`tasksProject` giữ tham chiếu object có thể bị thay thế.** `useProjectTasks.js:13` `tasksProject.value = project` giữ nguyên object trong mảng `projects`. Nhưng `applyProjectConfig` (`remoteActions.js:175`) làm `projects.value[index] = { ...plain }` — **thay object mới**. Nếu modal Tasks đang mở và người dùng (hoặc phone) lưu config project đó, `tasksProject.value` trở thành object mồ côi: mọi tick/sửa sau đó vẫn `applyTaskEdit(id, {tasks: tasksProject.value.tasks})` với mảng cũ → ghi đè mất các thay đổi vừa đến từ mirror. Cùng lớp bug với "task note reverts" đã được ghi trong CHANGELOG, chỉ khác đường vào.

---

### E15. Icon project — project thêm giữa phiên

**Flow**
Cache icon nằm ở Rust: `system.rs:300-304` `PROJECT_ICONS: OnceLock<Mutex<HashMap>>`, được nạp **duy nhất** bởi `load_and_cache_project_icons` (`system.rs:306-371`), và nó chỉ được gọi từ `projects.rs:141` — tức chỉ trong `load_projects`.
Host render icon qua protocol `aki-devsync-icon://` (`projectIcon.js:17`), phục vụ bởi `lib.rs:33-56` đọc thẳng cache đó.
Companion render qua `projectIcons` map (`projectStore.js:37`), điền bởi `refreshProjectIcons()` (`projectStore.js:40-46`) → `get_project_icons_map` (`web_server.rs:963-985`) — cũng đọc cùng cache.
`refreshProjectIcons()` được gọi **chỉ một lần**, ở `App.vue:79` (boot).

**Phát hiện**

🔴 **Project thêm giữa phiên không có icon trên cả hai màn hình, cho tới khi khởi động lại app.**
- Đường tạo project: `createNewProject` (`useProjectConfig.js:197`) → `openConfig` → `saveConfig` (`:164`) → `applyProjectConfig` (`remoteActions.js:166-201`). Hàm này `projects.value.push({...plain})` (`:195`) rồi `saveProjectsList()` (`:198`) → `invoke('save_projects')` (`useProjectConfig.js:139`).
- `save_projects` (`projects.rs:145-152`) **chỉ ghi file**, không gọi `load_and_cache_project_icons`.
- `applyProjectConfig` gọi `refreshProject(saved)` (`remoteActions.js:200`) — chỉ git/diff/stack — **không** gọi `refreshProjectIcons()` (grep chỉ ra đúng 1 call site: `App.vue:79`).
→ Cache Rust không có id mới → protocol trả 404 (`lib.rs:50-54`) → `<img @error>` set `failedIcons[p.id] = true` (`ProjectTable.vue:80`) và **không bao giờ thử lại**; `projectIcons` mirror sang phone cũng không có key đó.
Đây đúng là hành vi mà plan mô tả ("chỉ fill lúc boot") — code xác nhận, và nó là lỗi thật đối với người dùng, không phải giới hạn tài liệu.

**Hệ quả phụ**: đổi `local_path` của project sẵn có (icon repo thay đổi) cũng không cập nhật icon vì cùng lý do. `iconTimestamp` (`useProjectConfig.js:107`) chỉ bust cache **trình duyệt**, không bust cache Rust — nên nó không cứu được trường hợp này.

---

### E16. Enable SSH Terminal Color

**Flow**
`AppHeader.vue:41` → `enableSshTerminalColor()` (`:517-528`) → `invoke('install_ssh_terminal_color')` → `system.rs:215-258`.

**Mong đợi vs thực tế**
- `spawn_blocking` ✅ (`system.rs:217`).
- Idempotent: strip vùng giữa `SSH_COLOR_MARKER_BEGIN`/`END` (`system.rs:222-236`) rồi ghi lại → chạy nhiều lần không nhân bản. ✅
- Backup `~/.zshrc.aki-bak` (`:247-252`), chỉ tạo nếu chưa có. ✅
- UI phản hồi đúng, có nhắc "Open a new terminal" (`AppHeader.vue:520-523`). ✅

**Phát hiện**

🟠 **Đọc `.zshrc` thất bại → ghi đè `.zshrc` bằng nội dung chỉ có snippet.** `system.rs:220` `fs::read_to_string(&zshrc_path).unwrap_or_default()`. Nếu đọc lỗi (quyền, I/O, file đang khoá) chứ không phải "không tồn tại", `existing` thành `""` → `kept_lines` rỗng → `new_content` chỉ còn marker + snippet → `fs::write` (`:253`) xoá sạch `.zshrc`. Có backup che chắn ở lần đầu, nhưng nếu `.zshrc.aki-bak` **đã tồn tại** từ lần cài trước, điều kiện `if !backup_path.exists()` (`:249`) bỏ qua backup và bản `.zshrc` hiện tại mất luôn. Đúng lớp bug "unwrap_or_default che lỗi thật".

🟡 **Chỉ vá `~/.zshrc`.** `system.rs:219` hard-code; người dùng bash/fish bấm xong thấy Toast "enabled" mà không có tác dụng gì.

🟡 **Hàm `ssh()` override áp cho cả shell không tương tác.** Snippet (`system.rs:204-210`) `printf` OSC 11 vô điều kiện; trong script không phải TTY nó rắc escape sequence vào stdout.

---

### E17. Logger `usage.log` — xoay vòng / giới hạn kích thước

**Flow**
`lib.rs:18` `logger::init(app.handle())` → `logger.rs:23-56`.
Trim: `maybe_truncate_log` (`logger.rs:60-85`) — nếu > 1 MB thì giữ 512 KB cuối, cắt tại biên `\n`.
Ghi: `write_line` (`logger.rs:132-143`), append + `eprint!`.
Gates: `error` luôn ghi (`:146`), `info`/`debug` chỉ khi `--debug`/`AKI_DEBUG=1` (`:150-164`).

**Mong đợi vs thực tế**

🟠 **Trim CHỈ chạy một lần lúc khởi động, không bao giờ trong lúc chạy.** `maybe_truncate_log` được gọi duy nhất tại `logger.rs:40`, bên trong `init`. `write_line` (`:132`) không kiểm tra kích thước lần nào. Chạy `--debug` (hoặc `AKI_DEBUG=1`) trong một phiên dài với usage monitor poll mỗi 30s + statusline probe theo host, file tăng không giới hạn cho tới lần khởi động lại kế tiếp. Comment đầu file (`logger.rs:10`) tự mô tả đúng giới hạn này ("on startup") nhưng đây vẫn là cap không thực sự chặn.

🟡 **Trim đọc toàn bộ file vào RAM.** `logger.rs:72` `std::fs::read(path)` — nếu file đã phình lên vài trăm MB từ phiên trước, lần khởi động sau nạp trọn vào bộ nhớ trên máy 16 GB.

🟡 **Không xoay vòng theo file** (`usage.log.1`, `.2`…). Cắt đầu file là mất lịch sử vĩnh viễn, không thể truy dấu ngược một sự cố cũ.

---

### E18. Máy sleep 8 tiếng rồi wake

**Flow / trạng thái từng hệ**

| Hệ | Cơ chế | Hồi phục sau wake |
|---|---|---|
| Usage monitor | `usageMonitor.js:374` `hostInterval` + wake self-heal | ✅ `visibilitychange`/`focus` (`:101-104`) + watchdog 7s (`:106-113`) + `onWake` restart interval (`:395`) |
| Git / remote-diff timer | `useBackgroundRefresh.js:48,65` `hostInterval` | ❌ không có wake handler nào |
| WebSocket companion | `bridge.js` | ✅ một phần, xem dưới |

**Phát hiện**

🟠 **Timer git/diff không có đường hồi phục** — xem chi tiết ở E11 (`useBackgroundRefresh.js` vs `usageMonitor.js:85-113`). Đây là hệ quả rõ nhất của kịch bản sleep 8 tiếng.

🟡 **Ping watchdog của WebSocket chỉ phát hiện chết sau khi timer lại chạy.** `bridge.js:171-180`: `pingTimer` 15s, `pingTimeoutTimer` 5s. Trong lúc sleep cả hai đều bị đình chỉ. Khi wake, socket thực tế đã chết từ lâu nhưng phải chờ tick ping kế tiếp (≤15s) + timeout (5s) mới `ws.close()` (`:177`) rồi mới vào `scheduleReconnect` — tối đa ~20s + backoff. Backoff bắt đầu 1s và trần 10s (`bridge.js:38-39`), reset về 1s khi open (`:252`) → cuối cùng phục hồi. Không có listener `online`/`visibilitychange` để ép reconnect ngay.

🟡 **`pingTimeoutTimer` bị ghi đè không clear.** `bridge.js:175` gán timeout mới mỗi tick mà không `clearTimeout` handle cũ. Với 15s > 5s thì bình thường timeout cũ đã bắn xong; nhưng sau wake, nếu vài tick bị dồn (timer coalescing), handle cũ mất tham chiếu và `clearPingTimers` (`:164-169`) không dọn được nó.

Không phát hiện vấn đề: host luôn `connect()` (`services/index.js:41`) và mirror `broadcastFull()` lại toàn bộ state mỗi khi socket mở lại (`mirror.js:150-152`) hoặc khi có companion join (`mirror.js:160-162`) → sau reconnect, phone không bị state lệch.

---

### E19. Mở app khi ổ đĩa chứa project (`/Volumes/...`) chưa mount

**Flow**
`load_projects` (`projects.rs:131`) đọc `projects.json` bình thường (nằm ở app-data, không phải `/Volumes`) → OK.
`load_and_cache_project_icons` (`system.rs:306-371`) — mọi `Path::exists()` false → không icon, không panic.
`check_project_stack` (`system.rs:527-573`) — tất cả false → `dev_cmd`/`build_cmd` rỗng.
`get_git_info` (`git.rs:70-107`) — `!path.join(".git").exists()` → trả `status: "No Git"`.
`validate_project` (`projects.rs:100-107`) — **không** kiểm tra `local_path` tồn tại.

**Phát hiện**

🟠 **Không phân biệt được "ổ chưa mount" với "thư mục không phải git repo".** `git.rs:74-81` trả về `status: "No Git"`, `log: "Not a git repository."`; UI (`ProjectTable.vue:113,117`) hiển thị badge `git-no-repo` với tooltip "No Git repository". Người dùng thấy toàn bộ project trên ổ ngoài đột nhiên "mất git" và không có manh mối nào rằng nguyên nhân là ổ chưa mount. Chỉ cần một `Path::exists(&local_path)` ở đầu `get_git_info` để trả trạng thái riêng.

🟠 **Không có guard trước khi sync.** `validate_project` (`projects.rs:100`) chỉ chặn `..`, ký tự điều khiển, và `remote_host` rỗng. Với ổ chưa mount, PULL (`delete_on_pull` mặc định true, `projects.rs:90`) chạy rsync vào một đường dẫn không tồn tại; PUSH chạy rsync từ một thư mục trống hoặc không tồn tại. rsync sẽ báo lỗi, nhưng đây là loại thao tác mà một kiểm tra `exists()` một dòng trước khi chạy đáng giá hơn nhiều so với việc dựa vào exit code của rsync — đặc biệt vì `/Volumes/DEV` xuất hiện trong chính config của repo này.

🟡 **`failedIcons` nhớ vĩnh viễn.** `ProjectTable.vue:80` set `failedIcons[p.id] = true` khi `<img>` lỗi; không có chỗ reset ngoài việc load lại trang. Mount ổ lên rồi Refresh cũng không trả icon về.

---

### E20. Chọn text — select-none ở chrome, select-on ở nội dung

**Flow**
Mặc định: `main.css:47-48` `body { user-select: none }`.
Opt-in: `main.css:61-64` `input, textarea, [contenteditable], .u-select-text, .u-select-text *` → `user-select: text`.
Chống kéo ảnh/link: `main.css:69-72` `img, a { -webkit-user-drag: none }`.

**Đối chiếu các bề mặt cần copy được:**
- Console/log: `AppConsole.vue:75` `class="console-output u-select-text"` ✅
- Pair code: `AppHeader.vue:75` `class="remote-code u-select-text"` ✅ (và có comment giải thích vì sao hàng này khác các hàng URL click-to-copy)
- Git status/diff, changelog, release notes, đường dẫn, email: theo CHANGELOG dòng 69 đã được phủ.

**Không phát hiện vấn đề.** Quy tắc đảo (mặc định off, opt-in bằng đúng một class) được áp đúng, không còn `user-select: none` rải rác theo component.

🟡 Duy nhất một ghi chú: `main.css:61` opt-in `input, textarea` **toàn cục** — kể cả các input chỉ-đọc trong chrome (ví dụ ô số trong Statusline Customizer) đều chọn được. Đây là đánh đổi có chủ ý, ghi ở comment `main.css:52-58`.

---

## Tổng hợp phát hiện

**🔴 (1)**
- E15 — `src/store/projectStore.js:40` + `src/store/remoteActions.js:166-201` + `src-tauri/src/projects.rs:141`: project thêm giữa phiên không bao giờ có icon; `refreshProjectIcons()` chỉ được gọi ở `src/App.vue:79`.

**🟠 (13)**
- E2 — `src/composables/useAppWindow.js:46-55` + `src/components/AppHeader.vue:443`: "Stick Top-Left" được nhớ khôi phục sai chiều cao (đo DOM trước khi project load).
- E5 — `src-tauri/src/statusline.rs:355-362`: Apply cả hai target không nguyên tử (`set -e` cắt giữa chừng), báo lỗi không nói rõ nửa nào đã ghi.
- E5 — `src-tauri/src/statusline.rs:313-320` và `:330-338`: `jq` là phụ thuộc bắt buộc không kiểm tra; fail để lại script đã ghi nhưng settings chưa patch.
- E6 — `src-tauri/src/statusline.rs:310` và `:329`: `[ ! -f "$FILE.aki-bak" ]` → chỉ backup một lần; sửa tay sau lần Apply đầu bị ghi đè không backup.
- E7 — `src-tauri/src/ssh.rs:75-92`: xoá/đổi tên host đang được project dùng, không cảnh báo, không migrate.
- E8 — `src/store/claudeModeStore.js:12` (chỉ gọi từ `src/App.vue:78`): đổi profile không refresh `claudeMode`, usage native vẫn hiện sau khi chuyển proxy.
- E11/E18 — `src/composables/useBackgroundRefresh.js:42-70`: thiếu wake self-heal cho timer git/diff (đối chiếu `src/composables/usageMonitor.js:85-113`).
- E12 — `src/store/logStore.js:3-4` + `src/components/AppConsole.vue:80`: log không có cap, DOM phình vô hạn.
- E12 — `src/services/mirror.js:14,136-147`: mỗi dòng log mới phát lại toàn bộ mảng log qua WebSocket.
- E14 — `src/store/remoteActions.js:175` vs `src/composables/useProjectTasks.js:13`: `tasksProject` trỏ object bị thay thế khi lưu config → ghi đè mất thay đổi.
- E16 — `src-tauri/src/system.rs:220,249,253`: `unwrap_or_default()` khi đọc `.zshrc` + backup chỉ tạo một lần → có đường mất `.zshrc`.
- E17 — `src-tauri/src/logger.rs:40,132`: trim chỉ chạy lúc `init`, không cap trong lúc chạy.
- E19 — `src-tauri/src/git.rs:74-81` + `src-tauri/src/projects.rs:100-107`: ổ chưa mount hiện thành "No Git", không có guard `exists()` trước sync.

**🟡 (21)** — chi tiết trong từng case: E1 (2), E2 (2), E3 (1), E4 (1), E5 (1), E6 (1), E7 (2), E8 (2), E9 (3), E10 (2), E11 (1), E12 (1), E13 (2), E14 (2), E16 (2), E17 (2), E18 (2), E19 (1), E20 (1).

**Không phát hiện vấn đề**: E4 (capabilities — 13/13 API đều được cấp quyền), E10 (audit toàn bộ `#[tauri::command]` — không lệnh nào vi phạm NEVER BLOCK THE UI), E13 (kiến trúc lưu/mirror global note), E20 (quy tắc select), phần bảng màu ANSI của E5 (8 màu + 5 tier khớp 100% giữa JS / Rust / shell), và `#[serde(default)]` trên `SyncProject`/`ProjectTask` (`src-tauri/src/projects.rs:23-88` — mọi field mới đều có default).
