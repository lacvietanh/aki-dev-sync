# Batch 2026-07-21 — 8 hạng mục UX/ARCH/FIX

Status: **✅ DONE — ship 1.15.0 (2026-07-21)**

Ngoài 8 hạng mục chốt ban đầu, quá trình thực thi lòi ra một lỗi kiến trúc sâu hơn mà §1 chỉ làm
nó lộ ra: nút Refresh global thật ra gọi `loadData()` (reload cả app), nên "refresh global" và
"refresh từng project" chưa bao giờ là cùng một cơ chế. Xem `docs/arch/refresh-controller.md`.
§3 (auto-open browser cho DEV) bị **gỡ bỏ hoàn toàn** thay vì vá tiếp — lý do ghi trong chính §3.

Mọi `file:line` dưới đây đã được đọc và đối chiếu thật (không suy đoán). Docs là chân lý: code chỉ
thực thi đúng những gì file này chốt. Nếu khi code phát hiện thực tế lệch với spec → **dừng, báo,
sửa doc trước**, không tự ý code khác spec.

> **Đọc mục 0 trước khi làm bất cứ mục nào.** Đó là danh sách cạm bẫy đã được xác minh — bỏ qua sẽ
> tạo bug im lặng (code chạy, không báo lỗi, nhưng sai).

---

## 0. CẠM BẪY ĐÃ XÁC MINH — đọc trước khi code

| # | Cạm bẫy | Hậu quả nếu bỏ qua |
|---|---------|--------------------|
| P1 | `.popup-item` là `<div>`, **không phải `<button>`** | `:disabled` trên div **không có tác dụng** — vẫn click được. Phải dùng `:class="{ 'popup-disabled': ... }"` (xem `ProjectTable.vue:164,194`) |
| P2 | Khối REMOTE của popup có `v-if="p.remote_host && p.remote_path && remoteModeEnabled"` (`ProjectTable.vue:184`) | Nút Upload chuyển vào đây sẽ **biến mất hoàn toàn** khi remote off / project không có remote — khác hành vi hiện tại (hiện: hiện nhưng disabled + title giải thích). Đây là thay đổi hành vi **có chủ đích**, phải ghi CHANGELOG |
| P3 | jq extraction ở `statusline.rs:152-169` là **1 dòng `IFS read -r` với thứ tự vị trí cố định** | Thêm biến mới phải thêm **đồng thời** vào danh sách `read -r` **và** mảng jq, **đúng cùng thứ tự**. Lệch 1 vị trí → mọi biến sau đó nhận sai giá trị, script vẫn chạy, không báo lỗi |
| P4 | `loadCfg()` (`ClaudeSettingModal.vue:137-144`) trả thẳng config từ localStorage, **không merge key mới** | User cũ đã có config lưu sẵn sẽ **không bao giờ thấy field mới** (cache/reset). Bắt buộc thêm bước merge — xem mục 5.0 |
| P5 | `ClaudeProfileModal.vue:144,232` và `ClaudeSettingModal.vue:295,493` override `.modal-body`/`.modal-footer` trong `<style scoped>` | Scoped style có specificity cao hơn (thêm `[data-v-x]`) → media query narrow global trong `main.css` **sẽ không ăn** vào 2 modal này. Phải thêm override riêng trong từng file |
| P6 | `triggerManualRefresh()` (`refreshStore.js:26`) tăng counter mà **mọi** instance `useAgentUsage` đang watch | Nút refresh riêng-project **tuyệt đối không được gọi hàm này** — gọi là kéo theo toàn bộ usage monitor, đúng thứ user muốn tránh |

---

## 1. ARCH: Tách `remoteModeEnabled` thành 2 công tắc độc lập

### 1.1 Hiện trạng đã verify

`src/store/remoteModeStore.js` — **một** ref duy nhất, key localStorage `aki-remote-mode-enabled`,
mặc định `true` (`!== 'false'`). Comment trong file tự khai nó gánh 3 việc: sync project, background
remote-diff, và Claude Code remote usage monitoring.

**Toàn bộ 11 điểm dùng** (grep đã verify, không thiếu điểm nào):

| File:line | Vai trò | Thuộc nhóm |
|---|---|---|
| `AgentUsageSection.vue:32,81,82` | `ccRemoteHostRef` + `ccRemote.enabled` | **USAGE** |
| `AgentUsageSlot.vue:45,46,47,84` | Nút power + tooltip trong tab REMOTE | **UI công tắc** |
| `useSync.js:5,40` | Guard đầu hàm sync | **SYNC** |
| `useSyncStatus.js:3,7` | Guard đầu `checkProjectSyncStatus` | **SYNC** |
| `ProjectTable.vue:29` | `RefreshRing :interval-s` cột SYNC | **SYNC** |
| `ProjectTable.vue:184` | `v-if` khối REMOTE trong Open popup | **SYNC** |
| `ProjectTable.vue:208` | Nút Upload disabled + title | **SYNC** (nút này dời đi ở mục 2a) |
| `ProjectTable.vue:217` | `fieldset :disabled` + title | **SYNC** |
| `ProjectTable.vue:229,253` | Title nút Push/Pull | **SYNC** |
| `ProjectTable.vue:286` | import | **SYNC** |

→ Chỉ **1 điểm** thuộc USAGE. Toàn bộ phần còn lại thuộc SYNC.

### 1.2 Thiết kế đã chốt

**Nguyên tắc**: `ccRemote` hiện là source **duy nhất** trong 3 source không có công tắc riêng — nó
mượn tạm `remoteModeEnabled`. Sửa đúng cách là cho nó dùng chính `useToggleableSource()` mà 2 source
local đã dùng → cả 3 source đối xứng, xoá được sự bất thường thay vì đắp thêm.

**Bước 1 — `AgentUsageSection.vue`: cho ccRemote công tắc riêng**

Xoá dòng 32 (`import { remoteModeEnabled }`). Thay dòng 77-82 (cả block comment cũ) bằng:

```js
// Remote costs an SSH round trip, so it gets its own switch like the two local sources
// (the power icon in the REMOTE tab) — independent of whether project sync/diff is on.
const ccRemote = useToggleableSource(
  'claudecode',
  () => selectedSshHost.value,
  'aki-src-ccremote-enabled',
  true
);
```

`useToggleableSource` (dòng 46-58) đã tự lo: `enabled` persist theo `storageKey`, `toggle()`,
`hostRef` computed trả `null` khi off (dừng poll). Không cần sửa gì trong hàm đó.

> **Đã verify tương thích cấu trúc**: `AgentUsageSlot.vue:154` (`activeSource` = `props.ccRemote`) và
> template dòng 60-76 đọc `.data/.loading/.error/.stale/.isCached/.cachedAt/.accounts/.refresh/
> .selectAccount/.resetAccount` — tất cả đến từ `...hook` (giữ nguyên). Ngoài ra `useToggleableSource`
> còn thêm `.toggle` và `.locked` (computed `false`) mà bản `reactive({ enabled, ...hook })` cũ
> **không** có — thêm vào là an toàn (chỉ nới rộng, không mất field nào). `:locked="!!activeSource.locked"`
> vẫn ra `false` như trước.

> **⚠ Migration ccRemote — BẮT BUỘC, không được bỏ (đã verify là lỗ hổng thật).**
> `useToggleableSource` đọc **thẳng** `localStorage.getItem(storageKey)` và **không** biết key legacy.
> User đang TẮT remote mode (`aki-remote-mode-enabled = 'false'`) sẽ có key mới `aki-src-ccremote-enabled`
> chưa tồn tại → hàm rơi vào `defaultEnabled = true` → **usage monitor tự bật lại ngoài ý muốn**.
> Phải **seed key mới từ legacy TRƯỚC** khi gọi `useToggleableSource`. Thêm ngay đầu `<script setup>`,
> trước mọi lời gọi `useToggleableSource`:
> ```js
> // One-time seed: the ccRemote monitor used to piggyback on the single `aki-remote-mode-enabled`
> // flag. Now it has its own key — copy the old value across on first run after the split so a user
> // who had remote mode OFF doesn't get the monitor silently re-enabled.
> for (const [newKey, legacy] of [['aki-src-ccremote-enabled', 'aki-remote-mode-enabled']]) {
>   if (localStorage.getItem(newKey) === null) {
>     const old = localStorage.getItem(legacy)
>     if (old !== null) localStorage.setItem(newKey, old)
>   }
> }
> ```

**Bước 2 — `remoteModeStore.js`: đổi tên theo đúng phạm vi**

Đổi tên file → `src/store/syncCheckStore.js`, export `syncCheckEnabled` + `toggleSyncCheck()`,
localStorage key mới `aki-sync-check-enabled`.

> Theo `CLAUDE.md` § Regression Guard: **đặt tên đúng phạm vi thật**. Sau khi tách, biến này chỉ còn
> gate sync/diff — giữ tên `remoteModeEnabled` là tên nói dối, đúng loại lỗi đặt tên đã gây regression 1.9.3.

**Migration (bắt buộc — không được bỏ):**

```js
// Migration: this switch used to be `aki-remote-mode-enabled`, a single flag that also governed
// Claude Code remote usage monitoring. That half now lives in `aki-src-ccremote-enabled`
// (AgentUsageSection). Seed both new keys from the old value so an existing user's setup keeps
// behaving exactly as before the split, then let them diverge.
const LEGACY_KEY = 'aki-remote-mode-enabled'
const KEY = 'aki-sync-check-enabled'

function initialEnabled() {
  const current = localStorage.getItem(KEY)
  if (current !== null) return current !== 'false'
  const legacy = localStorage.getItem(LEGACY_KEY)
  return legacy === null ? true : legacy !== 'false'
}
```

Làm y hệt cho `aki-src-ccremote-enabled` — nếu key mới chưa có mà `aki-remote-mode-enabled` là
`'false'` thì seed `false`. **Không xoá** key legacy (để rollback bản cũ không mất setting).

**Bước 3 — Thay tên tại 8 điểm SYNC**: `remoteModeEnabled` → `syncCheckEnabled` ở `useSync.js:5,40`,
`useSyncStatus.js:3,7`, `ProjectTable.vue:29,184,217,229,253,286`. Sửa text tooltip
`'Remote Mode is off'` → `'Sync check is off'`.

**Bước 4 — Chuyển UI công tắc**

- Xoá `<i class="remote-power">` ở `AgentUsageSlot.vue:43-48`, thay bằng power icon trỏ vào
  `ccRemote.toggle()` / `ccRemote.enabled` — **dùng đúng markup như source local** (dòng 34-35),
  giữ class `src-power` + `is-on`/`is-off`. Xoá import `toggleRemoteMode` dòng 84. Title mới:
  `'Claude Code remote monitoring ON — click to turn off'` (và bản OFF tương ứng).
  Sửa `:disabled="!ccRemote.enabled"` ở `<select>` dòng 49 — giữ nguyên, giờ nó trỏ đúng nghĩa.
  Sau khi bỏ markup `.remote-power`, **xoá luôn rule CSS `.remote-power` chết** (`AgentUsageSlot.vue:231`).
- Thêm công tắc mới vào **header cột SYNC** `ProjectTable.vue:26-31`, đặt trong `.th-with-ring`
  cạnh `RefreshRing`, icon `<i class="fa-solid fa-power-off src-power" :class="syncCheckEnabled ? 'is-on' : 'is-off'" @click="toggleSyncCheck">`.
  Bắt buộc có `title`:
  `syncCheckEnabled ? 'Sync check ON — click to stop all remote diff/push/pull' : 'Sync check OFF — click to enable'`.

  > **⚠ CSS `.src-power` hiện SCOPED trong `AgentUsageSlot.vue:208-231` (đã verify)** — dùng class này
  > ở `ProjectTable.vue` sẽ **không có style nào ăn vào**. Bắt buộc **chuyển** khối rule `.src-power`,
  > `.src-power:hover`, `.src-power.is-on`, `.src-power.is-off` (bỏ `.is-locked` — không cần ở đây)
  > từ scoped của `AgentUsageSlot.vue` ra **`main.css` (global)**, rồi xoá chúng khỏi scoped của
  > `AgentUsageSlot.vue` để tránh khai báo trùng 2 nơi (SSoT). Sau khi chuyển, mở lại tab REMOTE/LOCAL
  > của Usage xác nhận power icon cũ vẫn hiển thị đúng (không được để regression phần cũ). **Không**
  > đặt style inline, **không** copy-paste nhân đôi CSS.

**Bước 5 — Cập nhật `docs/feat/remote-mode.md`** phản ánh kiến trúc 2 công tắc (theo `docs.B2`).
Đổi tên doc thành `sync-check-and-usage-switches.md` nếu tên cũ không còn đúng, và sửa link trong
`docs/index.md`.

### 1.3 Verify bắt buộc sau khi code

Theo `CLAUDE.md` § Regression Guard (test với ≥2 entity):
1. Có **≥2 project trỏ 2 host khác nhau**. Tắt Sync check → xác nhận cả 2 project đều khoá push/pull.
   Bật lại → cả 2 hoạt động lại, **không project nào mất config `remote_host`**.
2. Tắt usage monitor remote → xác nhận push/pull **vẫn chạy bình thường** (đây chính là bug đang sửa).
3. Tắt sync check → xác nhận usage monitor remote **vẫn poll bình thường**.
4. Test migration: set `aki-remote-mode-enabled = 'false'` trong localStorage, xoá 2 key mới, reload
   → cả 2 công tắc phải ở trạng thái OFF.

---

## 2. Upload vào Open popup + nút Refresh riêng-project

### 2a. Chuyển Upload vào Open popup

**Xoá** `ProjectTable.vue:208-210` (nguyên khối `<button class="btn-tech btn-tech-push-special">`).

**Thêm** vào cuối khối REMOTE (sau dòng 201, trước `</div>` đóng khối REMOTE), theo đúng pattern
`.popup-item` — **chú ý P1: là `<div>`, dùng class chứ không dùng `:disabled`**:

```html
<div class="popup-item"
     :class="{ 'popup-disabled': projectRuntime[p.id]?.syncing }"
     @click="!projectRuntime[p.id]?.syncing && openSelectDialog(p)"
     title="Pick specific files/folders (native file picker) and push only those to Remote — bypasses this project's exclude list, unaffected by the DRY toggle">
  <i class="fa-solid fa-upload" style="width:14px; color: #38bdf8;"></i> Upload (select files)
</div>
```

Không cần điều kiện `syncCheckEnabled` trong `:class` — khối REMOTE đã có `v-if` bao ngoài (P2).

**CHANGELOG phải ghi rõ P2**: nút Upload giờ nằm trong Open popup và chỉ hiện khi project có remote
host + sync check đang bật (trước đây luôn hiện, ở trạng thái disabled).

### 2b. Nút Refresh riêng-project (thay đúng vị trí Upload cũ)

**Vấn đề đã verify.** Chỉ có 1 nút refresh global (`AppHeader.vue:110-112` → `handleRefresh()` ở
dòng 307-309 → `loadData(sshHosts, true)`). `loadData()` (`useProjectConfig.js:62-118`) là **full
workspace reload**: invoke lại `load_projects`, chạy lại migration, reset `projectRuntime`, check lại
IDE availability, rồi cuối cùng gọi `refreshAll()` (`useBackgroundRefresh.js:42-46`):

```js
export function refreshAll() {
  projects.value.forEach(p => fetchGitStatus(p.id, true))  // git MỌI project
  checkAllSyncStatus()                                      // SSH diff MỌI project
  triggerManualRefresh()                                    // đánh thức MỌI usage monitor
}
```

→ Muốn refresh 1 project thì phải trả giá cho toàn bộ project khác + cả usage monitor. Đúng như user
phản ánh.

**Giải pháp**: 2 hàm per-project **đã tồn tại sẵn với đúng signature cần dùng** — không refactor,
không tách hàm mới, không code thừa:

- `fetchGitStatus(projectId, silent = false, updateModalLog = true)` — `useGit.js:16`
- `checkProjectSyncStatus(project)` — `useSyncStatus.js:6`

**Thêm** vào đúng chỗ nút Upload vừa xoá (trong `.actions-wrapper`, cuối Cell 5):

```html
<button class="btn-tech btn-tech-secondary"
        @click="refreshProject(p)"
        :disabled="projectRuntime[p.id]?.syncing"
        title="Refresh this project only — git status + remote diff. Does not touch other projects or the usage monitors (unlike the global refresh in the header).">
  <i class="fa-solid fa-arrows-rotate"></i>
</button>
```

Icon `fa-arrows-rotate` **cố ý khác** `fa-rotate-right` của nút global để user phân biệt 2 nút.

Handler, đặt cạnh các hàm action khác trong `<script setup>` của `ProjectTable.vue`:

```js
// Scoped counterpart to the header's global refresh: git + remote diff for this one project.
// Deliberately does NOT call triggerManualRefresh() — that wakes every usage monitor, which is
// exactly the cost this button exists to avoid.
async function refreshProject(p) {
  await Promise.all([fetchGitStatus(p.id), checkProjectSyncStatus(p)]);
}
```

Import `checkProjectSyncStatus` từ `../composables/useSyncStatus` (kiểm tra `fetchGitStatus` đã được
import sẵn chưa, có rồi thì **không import lại**).

**Chi tiết có chủ đích, đừng "tối ưu" đi:**
- Gọi `fetchGitStatus(p.id)` với `silent` mặc định `false` → có log "Checking status for X..." trong
  global log. Đây là thao tác user chủ động bấm nên **cần** feedback; khác `refreshAll()` dùng
  `silent=true` vì nó chạy nền cho N project.
- `checkProjectSyncStatus` tự early-return khi `syncCheckEnabled` off (`useSyncStatus.js:7`) → **không
  cần** guard thêm ở nút. Thêm guard nữa là code thừa.
- **Không** gọi `triggerManualRefresh()` (P6).
- **Không** gọi `loadData()`.

---

## 3. REMOVED: tính năng auto-open browser cho DEV

**Trạng thái: đã gỡ bỏ hoàn toàn (2026-07-21), không sửa nữa.**

### 3.1 Lịch sử — 2 vòng sửa trước khi quyết định gỡ

Vòng 1 chẩn đoán 2 lỗi độc lập (toast hứa vô điều kiện + poll 3s quá ngắn cho cold-start
Vite/Nuxt 5-15s) và đã sửa: đổi `run_project_dev` thành trả `Result<bool>`, nâng poll lên 20s,
JS chờ kết quả thật rồi mới bắn toast đúng nội dung.

Vòng 2 lộ ra lỗi UX khác: `await`-ing cả vòng poll 20s trước khi resolve khiến toast bị giữ lại
suốt thời gian đó — với người dùng, cảm giác y hệt "treo", không phân biệt được với bug thật. Đã
sửa bằng cách tách poll ra một `spawn_blocking` task **detached** (không ai await), command trả về
ngay sau khi mở Terminal.

**Sau cả 2 vòng, browser vẫn không mở được trên thực tế** — root cause thật sự nằm ở việc port
resolution (`extract_port_flag`/`extract_port_field`/`resolve_dev_port`) không đủ tổng quát cho
đa dạng cấu hình dev script thực tế (script tuỳ biến, cổng không chuẩn, thời gian boot monorepo
khác nhau) — chi phí duy trì 3 hàm phân tích cổng + TCP poll + task nền không xứng với lợi ích
không ổn định đó. Quyết định: **gỡ bỏ toàn bộ tính năng**, không tiếp tục vá.

### 3.2 Trạng thái code hiện tại

`run_project_dev` (`system.rs`) giờ chỉ mở Terminal, giống hệt `run_project_command` (BUILD) —
không còn port resolution, không TCP poll, không `open -g`. Người dùng tự mở browser khi thấy dev
server đã sẵn sàng trong Terminal.

`extract_port_flag`, `extract_port_field`, `resolve_dev_port` đã xoá khỏi `system.rs`.

`invokeProjectRun` (`ProjectTable.vue`) không còn cần nhận `successTitle` dạng hàm — chỉ còn
string, giống `runProjectCommand`.

---

## 4. Narrow mode: giảm padding modal

### 4.1 Hiện trạng đã verify

- Narrow mode thuần CSS: `@media (max-width: 700px)` — SSoT tại `main.css:89` (comment tự khai là
  breakpoint duy nhất toàn app). **Không** tạo breakpoint mới.
- `BaseModal.vue` **không có `<style>` block nào** — toàn bộ padding nằm ở `main.css:808-877`:
  `.modal-overlay` padding `20px` (820) · `.modal-header` `12px 16px` (839) · `.modal-body` `16px`
  (865) · `.modal-footer` `12px 16px` (871). Chưa có override narrow nào.

### 4.2 Việc cần làm

**Bước 1 — thêm vào block `@media (max-width: 700px)` sẵn có ở `main.css:89-102`** (thêm vào block
cũ, **không** tạo block `@media` thứ hai):

```css
  /* Modal chrome eats a large share of a narrow window — trim the padding, keep the structure. */
  .modal-overlay { padding: 8px; }
  .modal-header  { padding: 8px 10px; }
  .modal-body    { padding: 10px; }
  .modal-footer  { padding: 8px 10px; }
```

**Bước 2 — P5, bắt buộc**: 2 modal override `.modal-body`/`.modal-footer` trong `<style scoped>`,
specificity cao hơn nên rule global ở trên **không ăn**. Phải thêm media query **trong chính file đó**:

- `ClaudeProfileModal.vue` — `.modal-body` (dòng 144, `14px 16px 10px`), `.modal-footer` (232, `10px 16px 14px`)
- `ClaudeSettingModal.vue` — `.modal-body` (dòng 295, `14px 16px 10px`), `.modal-footer` (493, `10px 16px 14px`)

Mỗi file thêm ở cuối `<style scoped>`:

```css
/* Narrow mode (SSoT 700px, main.css) — this file's scoped padding outranks the global
   narrow rule, so the trim has to be repeated here. */
@media (max-width: 700px) {
  .modal-body   { padding: 10px 10px 8px; }
  .modal-footer { padding: 8px 10px 10px; }
}
```

**Bước 3 — verify bằng mắt**: mở app, thu cửa sổ < 700px, mở lần lượt **từng** modal trong
`src/components/modals/` (12 file). Con số trên là điểm khởi đầu — chỉnh nếu thấy chật/lệch, nhưng
**không thêm phần tử DOM mới** (UI Principle Extreme Narrow).

---

## 5. Statusline Customizer

### 5.0 Kiến trúc + 2 quy tắc bắt buộc

Config là `{ fields: [{key, enabled, color}], thresholds }`, **mirror ở 2 nơi phải luôn khớp**:
- Rust: `statusline.rs` — `default_config()` (dòng 64-82) + vòng lặp match sinh `g_<key>` (dòng 246-288)
- JS: `ClaudeSettingModal.vue` — `CATALOG` (99-107), `defaultLocalConfig()` (123-136), `renderField()` (258-291)

**Quy tắc P3 (jq) — nguy hiểm nhất.** `statusline.rs:152-169` là **một** lệnh đọc theo vị trí:

```
IFS=$'\x1f' read -r cwd model_name cost_usd ... git_branch <<< "$(echo "$input" | jq -r '[ ... ] | join("\x1f")')"
```

Thêm biến mới **bắt buộc** thêm vào **cả hai** danh sách, **cùng một vị trí**. An toàn nhất: **luôn
chèn vào cuối cả hai danh sách** (sau `git_branch` / sau `.workspace.git_branch`). Lệch thứ tự →
biến nhận sai giá trị, shell không báo lỗi, hỏng âm thầm.

**Quy tắc P4 (merge config cũ) — bắt buộc, làm trước 5a/5b.**
`loadCfg()` (dòng 137-144) trả thẳng object từ localStorage. User cũ sẽ không bao giờ thấy field mới.
Sửa thành:

```js
function loadCfg() {
  let saved = null;
  try {
    const raw = JSON.parse(localStorage.getItem(STORAGE_KEY) || 'null');
    if (raw && Array.isArray(raw.fields) && raw.thresholds) saved = raw;
  } catch { /* fall through to default */ }
  const def = defaultLocalConfig();
  if (!saved) return def;
  // Field keys added in a later version aren't in an already-saved config — append them with
  // their default state instead of leaving the user stuck on the old catalog. Order and
  // enabled/color of fields the user already has are left untouched.
  const known = new Set(saved.fields.map(f => f.key));
  saved.fields.push(...def.fields.filter(f => !known.has(f.key)));
  return saved;
}
```

### 5a. Segment Cache — 2 tuỳ chọn độc lập (theo yêu cầu user)

**Nguồn dữ liệu — đã xác nhận từ docs chính thức Claude Code** (code.claude.com/docs/en/statusline):
`context_window.current_usage` có 4 field, phản ánh **lần API call gần nhất** (không cộng dồn session):
`input_tokens`, `output_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens`.
Object này là `null` trước API call đầu tiên và ngay sau `/compact` → **bắt buộc có fallback `// 0`**.

Thêm **2 key riêng** (user yêu cầu "cho option cả 2"), dùng đúng model `fields` phẳng sẵn có, không
đổi struct, không cần serde migration:

| Key | Nhãn UI | Hiển thị mẫu | Ý nghĩa |
|---|---|---|---|
| `cache_pct` | Cache hit % | `cache 78%` | Tỉ lệ % request gần nhất ăn cache |
| `cache_tokens` | Cache tokens (read/total) | `12.4k/45.2k` | Số token đọc từ cache / tổng input |

**Công thức** (mẫu số dùng đúng công thức chính thức của `used_percentage`):
```
cache_total = input_tokens + cache_creation_input_tokens + cache_read_input_tokens
cache_pct   = cache_read_input_tokens / cache_total * 100      (cache_total = 0 → ẩn field)
```
Dùng `cache_read` làm tử số vì đó mới là phần **tận dụng được** cache (rẻ);
`cache_creation` là chi phí **tạo mới** cache — ngược nghĩa, không đưa vào tử số.

**Rust — 3 chỗ sửa:**

1. jq (P3): thêm `cache_read cache_creation cache_input` vào cuối danh sách `read -r`, và thêm **đúng
   thứ tự đó** vào cuối mảng jq:
   ```
   (.context_window.current_usage.cache_read_input_tokens // 0),
   (.context_window.current_usage.cache_creation_input_tokens // 0),
   (.context_window.current_usage.input_tokens // 0)
   ```
2. Thêm 2 match arm vào vòng lặp (dòng 246-288), dùng lại helper sẵn có `fmt_k`, `round_pct`,
   `color_for_pct`, `colored` (`HELPERS` dòng 91-127) — **không viết helper mới**:
   ```sh
   # cache_pct
   g_cache_pct=""
   _cache_total=$(( ${cache_read%.*} + ${cache_creation%.*} + ${cache_input%.*} ))
   if [ "$_cache_total" -gt 0 ]; then
     _cache_pct=$(awk -v r="$cache_read" -v t="$_cache_total" 'BEGIN{printf "%.0f", r/t*100}')
     g_cache_pct="$(colored "$WHITE" "cache") $(colored "$(color_for_pct_inv "$_cache_pct")" "${_cache_pct}%")"
   fi
   ```
   **Chú ý thang màu ngược**: cache cao là **tốt** (xanh), ngược với ctx/rate (cao là xấu). Thêm
   helper `color_for_pct_inv()` vào `HELPERS` — gọi lại `color_for_pct` với `100 - p`, không nhân bản
   logic ngưỡng.
   ```sh
   # cache_tokens
   g_cache_tokens=""
   [ "$_cache_total" -gt 0 ] && g_cache_tokens="$(colored "$COLOR_cache_tokens" "$(fmt_k "$cache_read")")$(colored "$GREY" "/")$(colored "$COLOR_cache_tokens" "$(fmt_k "$_cache_total")")"
   ```
   `_cache_total` tính ở arm `cache_pct` nhưng `cache_tokens` cũng cần → tính nó **một lần, ngoài
   vòng lặp match**, ngay sau khối palette, để bật/tắt field nào cũng có (SSoT, tránh phụ thuộc thứ tự).
3. `default_config()`: thêm `f("cache_pct", false, "white")`, `f("cache_tokens", false, "cyan")` —
   **mặc định `false`** để không đổi hiển thị của user hiện tại khi update.

`field_color_editable()` (dòng 44): thêm `"cache_tokens"` (số token đổi màu được), **không** thêm
`cache_pct` (màu của nó mang nghĩa ngưỡng, phải khoá — giống `context`/`rate_limits`).

**JS — 3 chỗ sửa**: `CATALOG` (+2 entry), `defaultLocalConfig()` (+2, `enabled: false`, **đúng thứ tự
như Rust**), `renderField()` (+2 case), `COLOR_EDITABLE` (+`cache_tokens`), `SAMPLE` (dòng 239-245)
thêm `cachePct: 78, cacheRead: '12.4k', cacheTotal: '45.2k'`.

### 5b. Rate limit — thêm tuỳ chọn "Reset"

**Nguồn dữ liệu đã có sẵn**: `docs/arch/usage-claudecode.md:37-39` xác nhận
`rate-limits-cache.json` chứa `resets_at` (Unix epoch giây, UTC) trong cả `five_hour` và `seven_day`.

**Thiết kế**: `rate_reset` là **field trong `cfg.fields`** (để dùng chung cơ chế persist/serialize,
không đổi struct `StatuslineField`), nhưng **render gộp vào block rate_limits** chứ không thành group
riêng — hiển thị `5h:42% ⟳1h12m  7d:18% ⟳2d3h`.

An toàn về mặt kỹ thuật: vòng join (dòng 303-310) có `[ -z "$g" ] && continue`, nên `$g_rate_reset`
không tồn tại → chuỗi rỗng → bị bỏ qua, không sinh dấu `|` thừa.

**Rust:**
1. jq (P3): thêm `five_reset seven_reset` vào cuối `read -r` + cuối mảng jq:
   `(.rate_limits.five_hour.resets_at // 0), (.rate_limits.seven_day.resets_at // 0)`
2. Thêm helper `fmt_eta()` vào `HELPERS` (epoch → `1h12m` / `2d3h`, trả rỗng nếu `<= now` hoặc `= 0`).
3. Arm `rate_limits`: đọc `config.fields` xem `rate_reset` có `enabled` không (Rust có sẵn cả config
   trong vòng lặp) → nếu có thì truyền thêm đối số reset cho `rate_block`, `rate_block` nối
   `$(colored "$GREY" " ⟳")$(colored "$GREY" "$eta")` vào cuối. Nếu không → sinh y hệt script hiện tại.
4. `default_config()`: `f("rate_reset", false, "grey")`, đặt **ngay sau** `rate_limits`.

**JS:** `CATALOG` +1 entry; `defaultLocalConfig()` +1 (cùng vị trí như Rust); `renderField()` case
`'rate_reset'` trả `''` (không render riêng); case `'rate_limits'` đọc
`cfg.fields.find(f => f.key === 'rate_reset')?.enabled` để nối ETA mẫu vào preview.

**UI — hiển thị dạng sub-row.** Trong `v-for` field-list (dòng 14-30), `rate_reset` render như dòng
con: thụt lề (class phụ `field-row-sub`), **ẩn 2 nút reorder** và ô chọn màu (nó không phải group
độc lập). Không thêm section/label mới (Extreme Narrow).

### 5c. Bỏ chữ Yellow/Orange/Red

Chỉ ở **threshold editor**, `ClaudeSettingModal.vue:35-37`. Preview (`renderField`/`tierHex`) đã đúng
chuẩn — **không đụng vào**.

Thay 3 dòng đó bằng:

```html
<label class="threshold-field" title="Yellow tier starts at this %"><span class="dot dot-yellow"></span>≥ <input type="number" min="0" max="100" v-model.number="cfg.thresholds.yellow" /></label>
<label class="threshold-field" title="Orange tier starts at this %"><span class="dot dot-orange"></span>≥ <input type="number" min="0" max="100" v-model.number="cfg.thresholds.orange" /></label>
<label class="threshold-field" title="Red tier starts at this %"><span class="dot dot-red"></span>≥ <input type="number" min="0" max="100" v-model.number="cfg.thresholds.red" /></label>
```

Tên màu chuyển vào `title` (a11y + vẫn tra cứu được khi hover), chấm màu `.dot-*` (CSS dòng 427-429)
giữ nguyên. Section label dòng 33 giữ nguyên.

### 5d. Verify bắt buộc cho mục 5

1. **Test script sinh ra trước khi Apply**: chạy `bash -n` trên output của `generate_statusline_script`
   (syntax check), rồi feed JSON mẫu:
   ```
   echo '{"context_window":{"current_usage":{"input_tokens":30000,"cache_read_input_tokens":12400,"cache_creation_input_tokens":2800},"context_window_size":200000,"used_percentage":22},"rate_limits":{"five_hour":{"used_percentage":42,"resets_at":9999999999}}}' | bash statusline-command.sh
   ```
2. Test `current_usage` = `null` (trạng thái sau `/compact`) → 2 field cache phải **ẩn**, không in `0%`,
   không lỗi shell.
3. Test P4: giữ config cũ trong localStorage, reload app → 3 field mới phải xuất hiện trong danh sách
   ở trạng thái off, thứ tự/màu field cũ **không đổi**.
4. Đối chiếu preview trong modal với output shell thật — hai bên phải khớp nhau.

---

## 6. Thứ tự thực thi đề xuất

Sắp theo mức độ độc lập, giảm rủi ro xung đột:

| Bước | Mục | Lý do đặt ở vị trí này |
|---|---|---|
| 1 | 5c | Nhỏ, độc lập tuyệt đối, không đụng ai |
| 2 | 4 | Thuần CSS, không đụng logic |
| 3 | 3 | Gói gọn trong `system.rs` + 2 hàm JS |
| 4 | 1 | Rework lớn nhất, đụng 11 điểm — làm khi các mục nhỏ đã xong |
| 5 | 2a + 2b | **Phải sau mục 1** (dùng tên biến `syncCheckEnabled` mới) |
| 6 | 5.0 → 5b → 5a | 5.0 (merge config) làm trước; 5b dễ hơn 5a nên làm trước để quen kiến trúc |

**Sau mỗi bước**: chạy `npm run build` (hoặc `cargo check` cho phần Rust) trước khi sang bước kế —
`coding.B3` "done means verified", không dồn hết rồi mới build.

**Trước khi đóng batch**: chạy audit bắt buộc theo `RULE-stack-tauri` A1:
```
grep -n "#\[tauri::command\]" -A2 src-tauri/src/*.rs
```
kiểm tra mọi command mới/đổi đều `async fn` + `spawn_blocking` nếu chạm subprocess/network.
(Batch này chỉ đổi `run_project_dev` — vốn đã đúng chuẩn, chỉ cần giữ nguyên cấu trúc.)

## 7. CHANGELOG — các điểm bắt buộc nêu rõ

- Nút Upload đổi vị trí + đổi điều kiện hiển thị (P2).
- Công tắc remote tách đôi: nêu rõ **setting cũ được giữ nguyên qua migration**, không ai bị reset
  (theo `CLAUDE.md` § Regression Guard: nói rõ cái gì được **bảo toàn**, không chỉ cái gì được sửa).
- Nút refresh mới là phạm vi hẹp, phân biệt với nút global.
- 3 field statusline mới mặc định **tắt** — không đổi statusline đang chạy của ai.
