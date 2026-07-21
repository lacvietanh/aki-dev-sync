# Deferred: Auto-update

**Status:** DROPPED (won't-do) - 2026-07-19. Auto-update quyết định là không cần thiết, không làm nữa (kể cả bản "notify only").  
**Tách từ:** `improve-jun24.md`  
**Ngày:** 2026-06-24

---

## Self-update (Tauri v2 updater plugin)

**Vấn đề:** App chưa có cơ chế tự cập nhật. User phải download thủ công.

**Blocker:** macOS bắt buộc code-sign + notarize. Cần **Apple Developer Program ($99/năm)**. Không có ký/notarize → updater fail silently trên mọi máy trừ máy build.

**Lộ trình trung gian (không cần ký):** Chỉ gọi `check()` để thông báo có bản mới + mở trang GitHub release qua `opener` plugin - không `downloadAndInstall`. Effort ~2h, không cần Apple cert.

**Lộ trình đầy đủ:** Xem chi tiết tại `improve-jun24.md §5` - plumbing plugin, post-build.js sinh `latest.json`, minisign key management.

**Khi nào làm:** Sau khi có Apple Developer account hoặc quyết định làm bản "notify only".
