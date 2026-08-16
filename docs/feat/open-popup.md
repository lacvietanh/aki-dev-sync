# Open Popup

> updated 2026-08-16 · v1.24.0

Một menu popup tập trung giúp hợp nhất các thao tác mở dự án Local và Remote SSH trên nhiều IDE khác nhau. Được tái cấu trúc từ các nút bấm phân tán (xem thêm chi tiết kiến trúc macOS open consolidation).

## Chức năng chính

### 1. Unified Trigger (Nút OPEN)
- Visibility is driven by state (`.is-open` on the wrapper), not a CSS `:hover` rule: a phone companion has no hover, so the popup - and everything only reachable through it - used to be unreachable there.
  - **Hover** opens it (unchanged on the Mac); moving the pointer away closes it again.
  - **Tap / click** on the `OPEN` button toggles it and *pins* it: it then stays open until an Esc, a pointerdown outside the popup, or another click on the same button.
  - At most one popup is open app-wide.
- A 150ms transition delay keeps a fast pointer pass from flickering the menu.
- Position is computed on open (`position: fixed`, centred on the viewport, clamped to an 8px margin) and stored in a **component-local** ref (`popupStyles`), never in `projectRuntime`: that store is mirrored, so a hover on the Mac used to broadcast its own coordinates over the phone's.

### 2. Local IDE Targets
Hiển thị danh sách các lối tắt mở code tại thư mục máy Local:
- **Finder:** Mở folder gốc.
- **Terminal:** Mở tab Terminal native macOS. Phiên này được gắn nhãn spawn-origin (project id) ngay lúc mở — xem `docs/arch/terminal-stack.md` § Spawn-origin ownership — nên modal "Terminal.app sessions" đọc "launched from <project>" thay vì chỉ suy đoán theo thư mục.
- **VSCode & VSCode Insiders:** Mở bằng text editor phổ biến.
- **Antigravity IDE:** Editor mặc định của hệ sinh thái Aki.
- **DEV + BUILD (v1.7.0):** Hai nút inline, **luôn hiển thị** cho mọi project. Mỗi nút có `title` tooltip hiển thị lệnh cụ thể sẽ chạy. Nhấn nút mở/focus một tab **trong terminal của app** (không còn mở cửa sổ `Terminal.app` bên ngoài) — nhờ vậy DEV/BUILD giờ nhìn thấy được từ Remote Control trên điện thoại, đúng thứ mà `docs/feat/in-app-terminal.md` từng nêu là khoảng trống. Tab được gắn nhãn `runKind: 'dev'|'build'`: một tab đang chạy chỉ được **focus**, không bị gõ lại lệnh; một tab đã thoát thì được **respawn** rồi gõ lại lệnh; không có tab nào khớp thì mở tab **mới** (không bao giờ tái sử dụng shell người dùng đang gõ dở). Chi tiết thiết kế + rào chắn PATH cold-start: `docs/plan/done/dev-build-in-app-launch.md`.
  - **Không có lệnh nào resolve được** (project không thuộc stack Tauri/Nuxt/Node và chưa đặt override): nút vẫn nằm đó nhưng bị làm mờ, tooltip "No dev/build command detected - set one in Project Settings". Trước đây cả hàng bị `v-if` xoá khỏi DOM, nên các project ngoài 3 stack đó mất luôn dấu vết của tính năng này.
  - **Tự động nhận diện stack & lệnh mặc định** (`check_project_stack` → `ProjectStackInfo.dev_cmd / build_cmd`):
    - Tauri: DEV = `{pm} tauri dev`, BUILD = `{pm} build:app`
    - Nuxt / Node: DEV = `{pm} dev`, BUILD = `{pm} build`
  - **Tự động cấu hình Package Manager**: Quét lockfile (`pnpm-lock.yaml`, `yarn.lock`, `bun.lockb`) để tự chọn `pnpm`/`yarn`/`bun`/`npm`.
  - **Per-project override**: Có thể ghi đè lệnh DEV/BUILD cho từng project trong Project Settings ("RUN COMMANDS - LOCAL ONLY"). Để trống = dùng mặc định theo stack.

**Missing local folder**: when `local_path_missing` is set (unmounted volume - the same flag that turns the GIT badge amber), every LOCAL item that consumes the path (Finder, In-App Terminal, Terminal, the three IDEs, DEV, BUILD) is dimmed with a "Local folder missing on disk" tooltip. **COPY stays enabled** - copying a path you are about to go fix is legitimate.

### 3. Remote SSH Targets
Với các project có cấu hình Remote, popup hiển thị thêm cột kết nối từ xa. Cột này chỉ cần `remote_host` + `remote_path`; công tắc SYNC **không** ẩn cột nữa - nó chỉ khoá riêng **Upload (select files)** (xem `docs/feat/sync-check-and-usage-switches.md`):
- **SSH Terminal:** Mở Terminal native, tự tạo script `osascript` kết nối SSH thẳng vào Server và cd vào thư mục project (`~` sẽ được tự động resolve thành `/home/user`). Cũng được gắn nhãn spawn-origin như Terminal local — vì thư mục làm việc của phiên SSH là `$HOME` trên máy local, đây là trường hợp mà nhãn "launched from X" trong modal cho biết điều mà cwd không thể.
- **VSCode Remote (và Insiders):** Dùng URL Scheme `vscode://vscode-remote/ssh-remote+...` để điều hướng VSCode mở Remote Extension. Logic JS luôn xử lý ghép chuẩn xác URL (thêm `/` ở đầu absolute path nếu cần).
- **Antigravity Remote:** Chạy CLI `antigravity-ide --remote` kết nối tới Server.

### 4. Dynamic IDE Availability
- Bằng cơ chế IPC, ứng dụng tự động kiểm tra sự tồn tại của các app (`.app`) trong thư mục `/Applications` trên macOS (như Visual Studio Code, Antigravity IDE).
- Các App/IDE chưa được cài đặt trên máy người dùng sẽ tự động bị chuyển sang trạng thái làm mờ (grayscale, độ trong suốt thấp) và khóa click `cursor: not-allowed` mà không báo lỗi câm (silent fail) khi gọi Command.
- The check re-runs on **hover and popup open** (`refreshIdeAvailability`, three `Path::exists()` probes), not once per `loadData` - installing or removing an IDE while the app runs is picked up without a reload. It is **TTL-cached (60s)**: hover fires the same call as opening the popup, so caching it makes sweeping the mouse down the OPEN column free while still catching an install within a minute; a manual reload passes `{ force: true }` to bypass the TTL. A not-yet-loaded (`null`) result reads as **unavailable**, so an early click cannot fire at an IDE we have not confirmed.
- A remote path that cannot be resolved over SSH now raises an error Toast instead of launching a `vscode://…/~/project` URI built from the unresolved path.

---

## Technical Details (Refactor Insights)

Trước bản cập nhật refactor: 
- Có nhiều nút rải rác: `[>_]` Terminal, `[VSCode]`. 
- Nhiều Implicit click (Click ngầm vào đường dẫn thư mục).
- Rất nhiều API dư thừa phía Rust: `open_in_terminal`, `open_ide_local`, v.v... chỉ thuần tuý dùng `open -a`.

**Sau refactor (MacOS Open Consolidation):**
- **JS-side (Frontend):** 
  - Các cấu hình URL URL Schemes (`vscode://`), app params (`-a 'Terminal'`) được chuyển về và quản lý ở `ProjectTable.vue`. 
  - Chỉ gọi duy nhất 1 handler dùng chung `macos_open(args)`.
- **Rust-side (Backend):** 
  - Bỏ đi logic thin-wrapper.
  - `system.rs` giờ đây chỉ có `macos_open(args)`, lệnh chuyên sâu SSH (`open_remote_subprocess`), và check file system (`check_ide_availability`). (`check_is_nuxt`/`run_nuxt_preview` đã bị gỡ khỏi `system.rs`.)
  - Gọn nhẹ, giảm rủi ro bảo mật (như String injection trong Command args).
