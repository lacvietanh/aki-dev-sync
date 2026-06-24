# Plan: Cải tiến đợt 2026-06 (Account info, Table, Notifications, Antigravity Remote, Auto-update)

**Status:** ✅ Done (core items)  
**Started:** 2026-06-24  
**Base version:** 1.2.8  
**Completed:** 2026-06-24 (v1.3.0–v1.3.1)

### Tóm tắt trạng thái
| Mục | Trạng thái |
|---|---|
| §1 Claude account info (email/org/tier) | ✅ Done — hiển thị trong `AgentUsage.vue` |
| §2 Table alignment | ✅ Done — header/body căn chỉnh, col widths tường minh; `table-layout: fixed` đánh giá lại → không cần (1 table HTML, cột không trôi) |
| §3 CSS variables + empty states + saveConfig toast | ✅ Done — biến định nghĩa trong `:root`, `saveConfig()` có Toast, `--color-danger` alias |
| §3 Toast positioning | ⏸ Deferred → `deferred-auto-update-toast-pos.md` |
| §4 Antigravity remote fix | ✅ Done |
| §5 Auto-update | ⏸ Deferred → `deferred-auto-update-toast-pos.md` |

---

Gom 5 hạng mục độc lập thành một plan vì cùng một đợt làm việc. Mỗi phần có thể triển khai riêng, không phụ thuộc nhau. Thứ tự ưu tiên đề xuất ở cuối (§6).

Mỗi phần ghi: Mục tiêu → Hiện trạng (đã verify trong code) → Nguyên nhân gốc → Giải pháp → Các bước → Rủi ro.

---

## 1. Lấy thêm thông tin account Claude Code (email + organization)

### Mục tiêu
Hiển thị email và tên organization của tài khoản Claude Code (như những gì thấy trong `/usage`), tương tự label account đang có sẵn cho Antigravity.

### Hiện trạng (đã verify)
- App đọc usage Claude Code qua SSH: `scripts/get-claudecode-usage.sh` đọc `~/.claude/rate-limits-cache.json` (do statusLine hook ghi), và grep thêm **2 field** từ `~/.claude/.credentials.json`: `subscriptionType`, `rateLimitTier`. Rust merge vào JSON trong [agent_usage.rs](../../src-tauri/src/agent_usage.rs) `get_claudecode_usage()`.
- `~/.claude/.credentials.json` chỉ chứa token (secret) + `subscriptionType` + `rateLimitTier`. **Không có email/org.**
- **Nguồn đúng: `~/.claude.json`** (ở `$HOME`, không phải trong `~/.claude/`), key top-level **`oauthAccount`** chứa đúng dữ liệu `/usage` hiển thị:
  - `emailAddress`, `organizationName`, `organizationType`, `organizationRole`, `organizationRateLimitTier`, `displayName`, `accountUuid`, `profileFetchedAt`.
  - **Không chứa secret** (token chỉ nằm trong `.credentials.json`). Field `profileFetchedAt` cho thấy Claude Code tự fetch profile từ server và cache lại khi auth/đăng nhập lần đầu, refresh định kỳ.
- [AgentUsage.vue](../../src/components/AgentUsage.vue) đã có sẵn slot/CSS render account cho Antigravity (`data.email.split('@')[0]`, ~dòng 15-17) nhưng **chưa có nhánh cho Claude Code**.

### Giải pháp (đề xuất: đọc file local, KHÔNG gọi API)
Đọc thêm `~/.claude.json` → `oauthAccount` qua đúng kênh SSH + delimiter đang dùng. Đây là pattern y hệt cách đang grep `.credentials.json`, rủi ro thấp, không đụng token.

### Các bước
1. Mở rộng `scripts/get-claudecode-usage.sh`: đọc `$HOME/.claude.json`, trích `oauthAccount.{emailAddress, organizationName, displayName, organizationType}`. Script đã gọi `python3` → dùng `python3 -c` parse JSON (file ~52KB, không nên grep). Phát qua delimiter mới, ví dụ `|||ACCOUNT|||`.
2. [agent_usage.rs](../../src-tauri/src/agent_usage.rs) `get_claudecode_usage()`: merge các field mới vào JSON trả về (cùng pattern `subscriptionType`/`rateLimitTier` hiện có). Tên field gợi ý: `email`, `organizationName`, `displayName`.
3. [AgentUsage.vue](../../src/components/AgentUsage.vue): thêm nhánh Claude Code render `data.email.split('@')[0]` + badge org (tier badge đã có). Tận dụng span `agent-account` sẵn có.
4. Cập nhật [docs/arch/usage-claudecode.md](../arch/usage-claudecode.md) — mục "Local File Layout" hiện thiếu `~/.claude.json`.

### Rủi ro / Lưu ý
- **Không bao giờ đọc/log/truyền token** từ `.credentials.json` (giữ nguyên scope grep hiện tại — `subscriptionType`/`rateLimitTier` là non-secret).
- Mỗi field phải null-safe: `seatTier`, `userRateLimitTier`, `workspaceRole` có thể `null`; `organizationName` đôi khi là chuỗi sinh tự động (`"<email>'s Organization"`).
- File có thể vắng trên remote chưa đăng nhập; `oauthAccount` vắng với setup chỉ dùng API key → giữ caveat "Pro/Max only" như doc hiện tại.
- Phương án thay thế (gọi API `/usage` hoặc `api.anthropic.com/api/oauth/usage`): **không chọn** — phải gửi OAuth token, endpoint không chính thức/không ổn định, tăng attack surface cho dữ liệu vốn đã có sẵn local.

---

## 2. Tinh chỉnh giao diện Table (thu hẹp + căn header/body chính xác, nhất quán)

### Mục tiêu
Table gọn hơn một chút; cột header và cột body căn chính xác, nhất quán, không lệch khi nội dung thay đổi.

### Hiện trạng (đã verify)
- Là `<table>` HTML thật ([ProjectTable.vue](../../src/components/ProjectTable.vue) dòng 3-168). CSS layout cột nằm global ở [main.css](../../src/assets/main.css) dòng 264-332 (style scoped trong component chỉ cho popup).
- Header `<th>` và body `<td>` **dùng chung class width** (`col-git-status`, `col-last-sync`, `col-actions`) → cùng 1 table nên cột không thể "trôi" ngang. Nhưng:
  - **`table-layout` chưa set → mặc định `auto`** ([main.css](../../src/assets/main.css) ~dòng 270): các `width:` chỉ là "gợi ý", trình duyệt tự tính theo nội dung rộng nhất → width 220px/140px không được tôn trọng, lệch khi path/text dài.
  - Cột 1 header `<th>` không có class; cột 3-4 (`col-last-sync`, `col-actions`) **không có width** (chỉ `padding-left:24px`) → tự chia phần còn lại theo nội dung.
  - `th { text-align:center }` (dòng ~290) trong khi body content lệch trái + có `padding-left:24px` → header label *trông* lệch so với content dù box cột vẫn căn đúng.
- Cửa sổ rộng **1100px** ([tauri.conf.json](../../src-tauri/tauri.conf.json) dòng 16). Table `width:100%`, không có max-width, không padding ngang ở `.dashboard-top`.

| Cột | Header | Body class | Width hiện tại |
|---|---|---|---|
| 1 | PROJECT / PATH | `col-project-info` | 220px (th không có class) |
| 2 | LOCAL GIT | `col-git-status` | 140px + pl 24px |
| 3 | LAST ACTION | `col-last-sync` | none (flex) + pl 24px |
| 4 | ACTIONS | `col-actions` | none (flex) + pl 24px |

### Nguyên nhân gốc
Không phải lệch cấu trúc (1 table đã chống được), mà do `table-layout:auto` cho nội dung override width + header căn giữa nằm trên body căn trái.

### Giải pháp (đề xuất: giữ `<table>`, thêm `table-layout: fixed`)
Cách sạch và ít rủi ro nhất; KHÔNG chuyển sang CSS grid (sẽ phải viết lại sticky header, colspan empty/loading, hover — nhiều churn vô ích) và KHÔNG tách 2 table (gây lệch scrollbar-gutter).

### Các bước
1. [main.css](../../src/assets/main.css) `.projects-table`: thêm `table-layout: fixed` → width khai báo trở thành quyền lực, header/body khớp pixel.
2. Gán width tường minh cho **cả 4 cột** (qua `<colgroup>` trong [ProjectTable.vue](../../src/components/ProjectTable.vue) hoặc rule cho mỗi `col-*` + cột 1 header). Dưới `fixed`, cột không có width sẽ chia đều phần thừa → phải budget rõ. Đo trước độ rộng hàng ACTIONS (cột nội dung rộng nhất) rồi cấp phần còn lại cho nó.
3. Thu hẹp/compact: giảm `padding-left` liên cột 24px → ~12-16px (dòng 326, 331); cân nhắc thêm `max-width` + center cho `.projects-table` hoặc padding ngang cho `.dashboard-top`. Tận dụng `text-overflow: ellipsis` đã có ở cột 1 (ProjectTable.vue dòng 41, 47) để truncate path ổn định.
4. Sửa cảm giác lệch: đổi `th { text-align:center }` → `left` để header label nằm thẳng trên body content căn trái (hoặc đồng bộ offset).

### Rủi ro / Lưu ý
- `table-layout: fixed` buộc tổng width các cột hợp lý trong ~1100px; cột ACTIONS phải đủ rộng cho toàn bộ nút (Push/Pull/Git/Special/Config) nếu không sẽ wrap/overflow → đo và cấp phát cuối cùng.

---

## 3. Cải thiện UI thông báo / empty state / error / toast

### Mục tiêu
Empty state & error nhất quán, căn giữa, chữ nhỏ, màu phân biệt, icon+text không gãy dòng vô ý. Toast không đè nút/UI.

### Hiện trạng (đã verify)

**Toast = SweetAlert2**, mixin chung ở [projectStore.js](../../src/store/projectStore.js) dòng 4-16: `position: 'bottom-end'` (fixed, góc dưới-phải, z-index ~1060), `timer: 3000`, **chỉ 1 toast tại 1 thời điểm** (toast mới đè toast cũ, không queue).

Toàn bộ call site toast:

| File:line | icon | Message | Ngữ cảnh |
|---|---|---|---|
| useSsh.js:68 | info | Auto-migrated projects to '...' | SSH host rename |
| useSsh.js:104 | success | Projects updated with new hosts | sau prompt rename |
| useSsh.js:118 / 122 | success/error | SSH config saved / Failed to save | nút Save SSH modal |
| useSsh.js:135/139/152/156 | success/error | undone/redone + fail | undo/redo SSH |
| useSync.js:20 | warning | ...is syncing, please wait | Push/Pull khi đang sync |
| useSync.js:94 / 100 | success/error | Sync complete / Sync failed | sau Push/Pull |
| ProjectConfigModal.vue:180 | success | Preset ... applied | nút preset trong Config modal |
| useProjectConfig.js:45 / 48 | success/error | Data Reloaded! / Reload failed | nút REFRESH (AppHeader) |

**Gap:** `saveConfig()` ([useProjectConfig.js](../../src/composables/useProjectConfig.js) dòng 81-113) — nút **"Save Changes"** trong ProjectConfigModal — **không bắn toast nào** (không feedback success/error). Cần bổ sung.

**Empty/info/error inline states:**

| File:line | Class | Text | Styling hiện tại |
|---|---|---|---|
| AgentUsage.vue:55-58 | `usage-empty` | "IDE not running (Open Antigravity to monitor)" / "No data — waiting for next session" | **CSS không tồn tại** → kế thừa body `#F3F4F6` (trắng), trái, font lớn. Icon + `<br>` + text → **đây là Issue A** |
| AppConsole.vue:32-35 | `empty-logs` | "No raw logs yet. Trigger a sync action." / "No global events recorded yet." | **CSS không tồn tại** → kế thừa `.console-output` màu cyan `#a5f3fc`, trái, mono 12px. Icon `fa-ghost` + `<br>` + text → **Issue B** |
| AgentUsage.vue:43-45 | `usage-error` | `{{ error }}` | `color: var(--color-danger)` — **biến chưa định nghĩa** (xem dưới) |
| ProjectTable.vue:26-28 | `empty-state` | "No projects found..." | **Styling chuẩn** (main.css:1007: flex column, center, `#6B7280`, italic, 12px) → **dùng làm reference** |
| SpecialPushModal.vue:7-14 | `loading-state`/`empty-state` | "Scanning..." / "No tracked files..." | OK, dùng `.empty-state` |

**Issue A & B chung nguyên nhân:** class `.usage-empty` và `.empty-logs` **không có rule CSS ở đâu cả** → kế thừa màu sai + không có flex/center; pattern `<i>...</i><br>text` đẩy icon riêng 1 dòng, chuỗi dài wrap tiếp → gãy 2-3 dòng.

**Phát hiện xuyên suốt — CSS variables chưa định nghĩa:** `--text-muted`, `--text-darker`, `--text-light`, `--border-color`, `--bg-secondary`, `--bg-tertiary`, `--color-danger` **không có trong `:root`** (main.css chỉ định nghĩa `--titlebar-h`, `--bg-primary/card`, `--border-card`, `--accent-*` dòng 20-31). Các rule dùng chúng (`.usage-error`, `.agent-name`, card background/border...) resolve rỗng → góp phần làm card/error nhợt nhạt. **Cần fix gốc:** định nghĩa các biến này trong `:root` HOẶC thay bằng `--accent-*`/hex đã dùng.

**Issue D — Toast đè nút:** toast neo bottom-right.
- **CAO:** toast "Sync complete/failed" bắn ngay sau Push/Pull → đè đúng nút action ở **hàng cuối** ProjectTable (cột ACTIONS cũng ở phải).
- **TRUNG BÌNH:** toast "Preset applied"/"SSH config saved" (z 1060) đè footer modal (nút Cancel/Save ở dưới-phải modal).
- **THẤP/không:** REFRESH ở trên-phải → toast dưới-phải → không đè. Titlebar (trên) không bị đụng.

### Giải pháp
1. **Định nghĩa CSS variables thiếu** trong `:root` ([main.css](../../src/assets/main.css) dòng 20-31) — nền tảng cho mọi sửa màu bên dưới.
2. **Chuẩn hóa empty/info/error** về 1 pattern giống `.empty-state`: định nghĩa `.usage-empty` và `.empty-logs` = `display:flex; flex-direction:column; align-items:center; justify-content:center; text-align:center; color:#6B7280 (muted); font-size:11px; gap:6px;` và **bỏ `<br>`** (dùng flex gap) → icon+text căn giữa, không gãy. Issue A dùng màu muted phân biệt với card; Issue B thêm `word-break:normal` override cyan/break-all.
3. **Toast positioning:** chuyển sang `top-end` (dưới titlebar — **bắt buộc tôn trọng `top: var(--titlebar-h)`, không bao giờ `top:0`** theo titlebar sacred boundary) hoặc `bottom-start`. `top-end` tránh cả cột ACTIONS lẫn footer modal. Xác nhận bằng test thực tế trên 5 modal.
4. **Bổ sung toast cho `saveConfig()`** (success "Config saved" / error) — lấp gap feedback.
5. Rà thêm các case empty/error chưa phát hiện (badge stale, N/A, skeleton) — phần lớn OK, chỉ cần đảm bảo dùng biến màu mới.

### Rủi ro / Lưu ý
- Đổi vị trí toast phải test với modal mở (z-index) để không bị modal che ngược lại.
- Tôn trọng titlebar 42px tuyệt đối nếu chọn `top-end`.

---

## 4. Fix "Open Remote Antigravity" (click không tác dụng) — [DONE]

### Mục tiêu
Khôi phục mở Antigravity remote khi bấm.

### Hiện trạng (đã verify — KHÔNG phải regression do commit)
Call chain: [ProjectTable.vue](../../src/components/ProjectTable.vue) dòng 114 `@click="openIdeRemote('antigravity', ...)"` → `openIdeRemote()` dòng 249-268 → `invoke('open_remote_subprocess', ...)` → [system.rs](../../src-tauri/src/system.rs) nhánh `"antigravity"` dòng 79-86:
```rust
Command::new("antigravity-ide")          // dòng 81 — RAW Command
    .args(["--remote", &format!("ssh-remote+{}", host), &expanded])
    .spawn()...
```
- IPC **đã đăng ký** ([lib.rs](../../src-tauri/src/lib.rs) dòng 35-39); custom command không cần capability → **không phải bẫy capability**.
- Git: code này **không đổi từ lúc ship** (giới thiệu ở commit `a47ec54` / CHANGELOG 1.2.1; chỉ thêm `expand_remote_tilde` ở `e25994b`/1.2.3). `git diff 1cacb47 HEAD` cho 2 file này = **rỗng**. → **Không có commit nào làm hỏng**; "đợt từng chạy tốt" gần như chắc chắn là `tauri dev` chạy từ terminal (kế thừa full PATH).

### Nguyên nhân gốc (xếp theo khả năng)
1. **CAO NHẤT — `antigravity-ide` không có trên PATH của GUI process.** Dòng 81 dùng raw `Command::new` **bỏ qua helper `create_command`** (system.rs dòng 6-21) vốn inject PATH Homebrew (`/opt/homebrew/bin:/usr/local/bin`) trên macOS. App chạy dạng `.app` từ Finder → PATH tối thiểu → không tìm thấy binary → `spawn()` trả `Err` → **bị nuốt bởi `console.error`** (xem dưới) → user thấy không có gì xảy ra. Đây đúng class bug đã từng fix cho `rsync` (CHANGELOG 1.2.0) và `ssh` (dùng `create_command` ở dòng 173) — **nhánh antigravity bị bỏ sót.**
2. **TRUNG BÌNH — tên/cài đặt binary.** CLI có thể không tên `antigravity-ide` hoặc chưa cài; `check_ide_availability` (dòng 144-161) chỉ kiểm tra `.app`, không kiểm CLI.
3. **THẤP — format argument** (`--remote ssh-remote+host path` vs `--remote=...` hoặc token gộp kiểu URL VSCode) → spawn OK nhưng không hiệu lực. Cần verify CLI thật.

(Local Antigravity vẫn chạy vì dùng `open -a`, có sẵn trên GUI PATH → khớp việc user chỉ báo *remote* hỏng.)

### Giải pháp
1. Đổi [system.rs](../../src-tauri/src/system.rs) dòng 81: `Command::new("antigravity-ide")` → `create_command("antigravity-ide")` (mirror fix ssh dòng 173). **Đây là fix khả năng cao nhất.**
2. Surface lỗi cho user (in-app log / toast) thay vì chỉ `console.error` ở `openIdeRemote` (ProjectTable.vue dòng 250, 267) — để lần sau spawn fail không im lặng.
3. Verify tên + cú pháp argument CLI `antigravity-ide` thật trên máy đích (cover nguyên nhân #2, #3); cân nhắc mở rộng `check_ide_availability` để detect CLI và làm mờ item remote khi vắng.

### Rủi ro / Lưu ý
- Cần verify runtime thật (chạy dạng `.app` bundle, không phải `tauri dev`) vì lỗi mang tính môi trường.

---

## 5. Quy trình chuẩn Tauri v2 self-update (qua GitHub Releases)

### Mục tiêu
App tự kiểm tra & cập nhật từ GitHub releases.

### Hiện trạng (đã verify)
- [tauri.conf.json](../../src-tauri/tauri.conf.json): **không có block `plugins`**, macOS **chưa ký** (không có `signingIdentity`). [Cargo.toml](../../src-tauri/Cargo.toml) chưa có updater dep. [lib.rs](../../src-tauri/src/lib.rs) **không có `.setup()` closure**. [post-build.js](../../scripts/post-build.js) chỉ rename `.dmg`.
- [AppHeader.vue](../../src/components/AppHeader.vue) dòng 7 có version badge clickable (`@click="showChangelog"`, dùng `__APP_VERSION__`) + đã import Swal → **điểm tích hợp tự nhiên**.

### Quy trình chuẩn (Tauri v2 — đã verify với docs chính thức)
Plugin `tauri-plugin-updater` (Rust) + `@tauri-apps/plugin-updater` + `@tauri-apps/plugin-process` (JS), trỏ tới `latest.json` trên GitHub Releases. Plugin check manifest → verify chữ ký minisign bằng pubkey nhúng trong app → tải artifact → cài → relaunch.

**Setup:**
1. Cargo: `cargo add tauri-plugin-updater --target 'cfg(any(target_os="macos",windows,target_os="linux"))'`. npm: `@tauri-apps/plugin-updater @tauri-apps/plugin-process`.
2. [lib.rs](../../src-tauri/src/lib.rs): thêm `.setup()` đăng ký plugin gated desktop:
   ```rust
   .setup(|app| {
       #[cfg(desktop)]
       app.handle().plugin(tauri_plugin_updater::Builder::new().build())?;
       Ok(())
   })
   ```
3. Sinh key: `npm run tauri signer generate -- -w ~/.tauri/aki-devsync.key`. **Public key → tauri.conf.json; private key KHÔNG commit, backup offline.**
4. Build env (export thật, **không dùng `.env`**): `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Thiếu → không sinh `.sig`.
5. [tauri.conf.json](../../src-tauri/tauri.conf.json): `bundle.createUpdaterArtifacts: true` (**bắt buộc** v2, hiện thiếu) + block `plugins.updater` mới:
   ```jsonc
   "plugins": { "updater": {
     "pubkey": "<pubkey>",
     "endpoints": ["https://github.com/<user>/<repo>/releases/latest/download/latest.json"]
   }}
   ```
6. [capabilities/default.json](../../src-tauri/capabilities/default.json): thêm `"updater:default"`, `"process:allow-restart"` (thiếu = silent no-op).

**`latest.json` (host trên GitHub Releases):**
```json
{ "version": "1.2.9", "notes": "...", "pub_date": "...Z",
  "platforms": {
    "darwin-aarch64": { "signature": "<NỘI DUNG file .sig>", "url": "https://github.com/.../v1.2.9/...aarch64.app.tar.gz" }
  }}
```
`signature` là **nội dung text** của `.sig`, không phải URL. Key platform: `<target>-<arch>` (`darwin-aarch64`, `darwin-x86_64`...). Dùng URL `releases/latest/download/latest.json` để luôn trỏ release mới nhất (static, hợp solo dev).

**Frontend** ([AppHeader.vue](../../src/components/AppHeader.vue), tách `useUpdater.js` composable cho gọn):
```js
import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
const update = await check();           // null nếu đã mới nhất
if (update) { await update.downloadAndInstall(); await relaunch(); }
```
Dùng Swal sẵn có cho dialog confirm/progress.

### Gotchas quan trọng
1. **macOS updater dùng `.app.tar.gz`, KHÔNG dùng `.dmg`.** DMG chỉ để cài lần đầu. [post-build.js](../../scripts/post-build.js) hiện chỉ xử lý DMG → phải bổ sung: đọc `.sig` từ `bundle/macos/*.app.tar.gz.sig`, copy `.app.tar.gz`, sinh `latest.json` (version lấy từ `pkg.version` đã đọc sẵn).
2. **BLOCKER macOS: phải code-sign + notarize.** Chữ ký minisign chỉ chứng thực artifact với plugin, **không qua Gatekeeper**. App chưa ký → khi updater thay binary & relaunch, Gatekeeper chặn trên mọi máy trừ máy build → update âm thầm fail. Cần **Apple Developer Program ($99/năm)** + `signingIdentity` + credentials notarize (`APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`). Tài khoản free **không** notarize được.
3. Namespace khác nhau: rename DMG (`...-arm.dmg`) ≠ key updater (`darwin-aarch64`) — đừng đổi arch trong manifest.
4. Mất private key = vĩnh viễn không update được bản đã cài.

### Đề xuất lộ trình (cho solo dev)
- **Bước trung gian rẻ (~2h, né blocker notarize):** chỉ gọi `check()` để *thông báo* có bản mới + mở trang release qua plugin `opener` đang có (không `downloadAndInstall`). Không cần ký.
- **Bản đầy đủ:** plumbing updater ~3-5h + sửa post-build ~2-4h + **chi phí trội: enroll Apple Developer + setup notarize** (vài giờ + thời gian chờ enroll). Không có ký/notarize → chỉ chạy được trong dev, không self-update cho user thật trên macOS.

### Nguồn
- https://v2.tauri.app/plugin/updater/
- https://v2.tauri.app/distribute/sign/macos/

---

## 6. Thứ tự ưu tiên đề xuất

1. **§4 Antigravity remote fix** — 1 dòng đổi (`create_command`), giá trị/effort tốt nhất, đang là bug.
2. **§3 Notifications/toasts** — định nghĩa CSS vars thiếu (nền tảng) + chuẩn hóa empty state + reposition toast. Ảnh hưởng cảm nhận chất lượng rõ nhất.
3. **§2 Table alignment** — `table-layout: fixed` + width 4 cột.
4. **§1 Claude account info** — feature nhỏ, rủi ro thấp.
5. **§5 Auto-update** — giá trị cao nhưng vướng blocker notarize (chi phí $99/năm + setup). Cân nhắc làm bản "check + mở release page" trước.

## Liên quan
- Titlebar sacred boundary: [docs/ref/titlebar-sacred-boundary.md](../ref/titlebar-sacred-boundary.md) (ảnh hưởng §3 toast top-end).
- Open popup: [docs/feat/open-popup.md](../feat/open-popup.md) (§4).
- Usage Claude Code: [docs/arch/usage-claudecode.md](../arch/usage-claudecode.md) (§1).
