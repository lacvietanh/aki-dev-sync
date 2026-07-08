# Plan: Nút COPY path SSH (open popup) lag vài giây + đơ UI — ✅ FIXED 2026-07-08 (code tĩnh, cần Mac-verify)

## Triệu chứng
Bấm nút **COPY** ở khối "☁️ REMOTE (SSH)" trong open-popup (`ProjectTable.vue:170`) — tưởng chỉ là
copy một field có sẵn — nhưng **lag vài giây và đơ (freeze) toàn UI** mỗi lần bấm.

## Root cause (đọc code tĩnh)
Không phải "copy field có sẵn". Chuỗi thực tế:

```
COPY → copyRemotePath(p)                                   ProjectTable.vue:416
     → await resolveRemoteFullPath(host, remote_path)      ProjectTable.vue:418
        → path bắt đầu '~/' → LUÔN đúng (default remote_path = "~/")
        → await invoke('resolve_remote_path', {host,path}) ProjectTable.vue:392
           → Rust: mở SSH mới, chạy `bash -c "echo $HOME/…"`  system.rs:266-270
           → command.output()  ← SSH round-trip ĐỒNG BỘ    system.rs:272
```

Hai lỗi cộng dồn:
1. **Thao tác sai bản chất**: copy chỉ cần chuỗi `remote_path`, nhưng code lại RE-FETCH `$HOME` qua
   mạng bằng SSH mỗi lần bấm. Vì mọi project khởi tạo `remote_path: "~/"` (`useProjectConfig.js`),
   nhánh resolve **luôn** chạy → full SSH handshake (TCP+auth) mất vài giây. Không cache gì.
2. **Đơ UI**: `resolve_remote_path` là `pub fn` **không async** → Tauri chạy trên main thread →
   `command.output()` giữ main thread suốt round-trip mạng (đúng pitfall CLAUDE.md "async fn +
   blocking subprocess", nhưng ở đây tệ hơn vì là sync command trên main thread).

`resolveRemoteFullPath` cũng dùng cho **mở IDE remote** (`openIdeRemote`, `ProjectTable.vue:428`) —
nên bug đơ UI này còn ảnh hưởng cả luồng mở VSCode/terminal remote, không riêng COPY.

## Fix đã áp dụng (2026-07-08)
1. **`ProjectTable.vue` `copyRemotePath`** — copy thẳng `project.remote_path` verbatim (giống
   `copyLocalPath`). `~` là path hợp lệ/portable trên remote (shell/scp/rsync tự expand) → 0 network,
   tức thì. Đây mới đúng "copy field có sẵn" như kỳ vọng.
2. **`ProjectTable.vue` `resolveRemoteFullPath`** — thêm cache `Map` keyed `(host, path)`. `$HOME`
   remote bất biến trong 1 session → SSH tối đa 1 lần/host+path; chỉ cache khi resolve thành công
   (không pin fallback thất bại). Giờ chỉ còn mở-IDE-remote dùng hàm này.
3. **`system.rs` `resolve_remote_path`** — chuyển `pub fn` → `pub async fn` + bọc SSH trong
   `tauri::async_runtime::spawn_blocking` → ra khỏi main thread, UI không bao giờ đơ (kể cả lần
   resolve đầu của IDE-open).

Kết quả: COPY instant; mở IDE remote không đơ UI + nhanh sau lần đầu.

## Còn lại
- ⚠️ **Cần rebuild + verify trên Mac** (máy dev remote chỉ viết code, không build). Kịch bản verify:
  (a) COPY path remote → dán ra thấy đúng `remote_path` verbatim, không delay;
  (b) mở VSCode Remote lần đầu → UI không freeze trong lúc resolve;
  (c) mở lại IDE cùng host → instant (cache hit).
- Tooltip nút vẫn ghi "Copy full path" — giờ copy path dạng `~` chứ không phải absolute. Cân nhắc
  đổi thành "Copy remote path" cho khớp (chưa đổi, để user quyết — dính UI-Principle "extreme narrow").
