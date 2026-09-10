# State nạp một lần lúc khởi động — audit toàn app

**Start time:** 2026-09-10

## Initial purpose

Note `task-1787801054655` của owner: *"khi thay đổi config/state thì không nên phụ thuộc việc khởi động lại app mới apply được. Việc phải restart app Aki Dev Sync để apply một thay đổi là tối kỵ vì có rất nhiều terminal và task đang dở dang của các dự án khác đang chạy song song."* Yêu cầu: rà mọi luồng giữ state/cache nạp một lần lúc startup mà không tự làm mới khi nguồn dữ liệu thật đổi, và cân nhắc file-watch/hot-reload ở nơi có rủi ro.

Đây là audit — chỉ báo cáo, không sửa (`agent.B5`). Ngoại lệ duy nhất: mục icon được sửa ngay trong cùng phiên vì owner đã giao riêng việc đó (backlog B17), và audit này là thứ giải thích *vì sao* nó hỏng.

## Strategy

1. Tìm mọi điểm nạp state từ nguồn ngoài tiến trình (đĩa, `~/.ssh/config`, tiến trình khác), không đoán theo tên hàm.
2. Với mỗi điểm, tìm **mọi** trigger nạp lại có thật — hoặc chứng minh không có, kèm thứ đã grep.
3. Phân loại theo *sự kiện đời thực* làm nó lệch, không theo cảm giác nghiêm trọng.
4. Kiểm tra app có filesystem watcher nào không, trước khi bàn tới chuyện thêm watcher.

## Checklist

- [x] Truy mọi `invoke()` từ JS và handler Rust tương ứng.
- [x] Liệt kê mọi `OnceLock` trong `src-tauri/src/`, tách loại giữ dữ liệu ngoài vs chỉ giữ khoá/registry nội bộ.
- [x] Xác định `loadData` được gọi từ đâu, mấy lần.
- [x] Grep `notify` / `watch` / `FSEvents` / `inotify` trong `src-tauri/` và `Cargo.toml`.

## Result

### Phát hiện gốc

`loadData` (`src/composables/useProjectConfig.js:138`) được gọi **đúng một lần**, từ `src/App.vue:79`. Nó là nơi duy nhất trong toàn app gọi `load_projects`, và `load_projects` (`src-tauri/src/projects.rs:149`) là nơi duy nhất gọi `load_and_cache_project_icons`. Không có nút "refresh toàn cục" nào nạp lại danh sách project — `requestRefreshAll` (`src/store/remoteActions.js:113`) chỉ làm mới *trạng thái* của từng project (git, diff, lệnh dev/build), không đọc lại `projects.json`.

**App không có filesystem watcher nào.** Grep `src-tauri/` chỉ ra `tokio::sync::Notify` và `Condvar::notify_one` — cơ chế đánh thức thread, không liên quan tới đĩa. `Cargo.toml` không có crate `notify`.

### Bảng trạng thái

| State | Nạp ở đâu | Trigger nạp lại có thật | Sự kiện làm lệch | Mức |
|---|---|---|---|---|
| Icon project | `projects.rs:144` trong `load_projects_blocking` | **Không có** — xem ghi chú dưới | Thêm/đổi favicon của một project | **Cao** |
| Danh sách project (`projects.json`) | `useProjectConfig.js:146` | Không có | Sửa `projects.json` ngoài app | Trung bình |
| Danh sách SSH host | `useProjectConfig.js:144` | `applySshHostsChange` (`remoteActions.js:282`), chỉ chạy khi **chính app** save/undo/redo file SSH | Sửa `~/.ssh/config` bằng editor ngoài | Trung bình |
| `SEEN` antigravity | `agent_usage/antigravity.rs:11` | Không có, một cờ/host cho cả vòng đời app | Cài công cụ lên host remote giữa phiên | Trung bình thấp |
| `RSYNC_VERSIONS` | `sync.rs:14` | Không có, cache một lần/host | Nâng cấp rsync giữa phiên | Thấp |
| Notes từng project | `useProjectConfig.js:183` | `refreshProjectNotes` — khi đổi `local_path`, khi thêm project, khi mở modal | Sửa `notes.json` ngoài app rồi không mở lại modal | Thấp |
| IDE availability | `useProjectConfig.js:192` | TTL 60s (`IDE_AVAILABILITY_TTL_MS`, `useProjectConfig.js:106`) | Cài IDE giữa phiên → lệch tối đa 60 giây | Không cần làm gì |
| Agent usage | `usageMonitor.js:229` | Poll liên tục | — | Không lệch |

### Ghi chú riêng về icon — cái tên nói dối

`refreshProjectIcons()` (`src/store/projectStore.js:71`) được gọi ngay sau khi lưu config project (`src/store/remoteActions.js:248`) và trông như một lần quét lại. Nó không phải: nó chỉ `invoke('get_project_icons_map')`, mà lệnh đó (`web_server.rs:1413`) **đọc cache**, không dựng lại cache. Cache chỉ được dựng trong `load_and_cache_project_icons`, chỉ chạy trong `load_projects`, chỉ chạy một lần lúc boot.

Nên trước bản sửa này, đường duy nhất để app thấy một favicon mới là **restart app** — đúng thứ note gọi là tối kỵ. Đây là `pattern.A7`: hàm được đặt tên theo việc người ta *mong* nó làm, và cái tên đó che mất lỗi trong nhiều tháng.

### Verification

Đọc tĩnh hai đầu là đủ, không cần chạy: đếm call site bằng grep (`load_projects` → 1 call site JS; `loadData` → 1 call site; `load_and_cache_project_icons` → 1 call site Rust), rồi đọc thân `get_project_icons_map` để xác nhận nó không gọi hàm quét. Một hàm không thể làm mới thứ nó không bao giờ gọi.

Không kiểm được ở máy này: hành vi thực tế của TTL 60s và của poll agent-usage khi máy ngủ/thức — không đổi kết luận, vì cả hai đều đã có cơ chế tự làm mới.

## Decision

**Không thêm filesystem watcher.** Thêm crate `notify` là một phụ thuộc mới, một luồng nền, và một lớp trạng thái nữa phải đúng — trong khi mọi mục lệch ở trên đều có một **thời điểm sử dụng rõ ràng** để đọc lại. Đọc-lúc-dùng rẻ hơn theo-dõi-liên-tục và không đẻ ra trạng thái mới (`think.B4` — bớt trước khi gói).

Hình dạng đúng cho từng mục, xếp theo giá trị trên chi phí:

1. **Icon** — đã sửa trong phiên này (B17): thêm lệnh quét lại thật, `refreshProjectIcons` gọi nó trước khi đọc map, cộng một nút RELOAD trong modal config project.
2. **SSH host** — đọc lại `get_ssh_hosts` ngay khi mở modal SSH Config. Một dòng, đúng lúc người dùng đang nhìn vào danh sách.
3. **Danh sách project** — cho nút refresh toàn cục ở header gọi lại `load_projects`. Cần cẩn thận: `loadData` có cờ chống chạy chồng (`isReloading`) và có ba migration chạy trong đó, nên phải kiểm chúng vẫn idempotent trước khi cho chạy nhiều lần. **Chưa làm.**
4. **`SEEN` antigravity** — cờ "công cụ không có trên host" giữ suốt vòng đời app; nên hết hạn theo thời gian như IDE availability đã làm, thay vì vĩnh viễn. **Chưa làm.**
5. **`RSYNC_VERSIONS`, notes** — để nguyên. Sự kiện làm lệch hiếm, và đều đã có đường làm mới thủ công.

**Reopen trigger:** nếu sau này có luồng nào cần biết file đổi *ngay lập tức* mà không có thời điểm sử dụng nào để bám vào, thì lúc đó mới cân nhắc watcher — và cân nhắc lại toàn bảng, không chỉ luồng đó.

## Cross-refs

- `.akidevsync/notes.json` `task-1787801054655` — note gốc của owner.
- `docs/plan/backlog.md` B17 (icon), B18 (các mục còn lại của audit này).
- `src/composables/useProjectConfig.js:138` `loadData` — điểm nạp một lần duy nhất.
- `src/store/projectStore.js:71` `refreshProjectIcons` — hàm có tên nói dối, đã sửa.
