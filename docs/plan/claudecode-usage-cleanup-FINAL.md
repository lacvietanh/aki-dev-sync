# Plan FINAL — dọn dẹp Claude Code usage flow

> **Trạng thái:** chờ thực thi. Soạn 2026-07-20.
> Nền tảng nghiên cứu: `docs/research/claudecode-usage-FINAL.md` (đọc §5 nhật ký trước).
> Kiến trúc đích: `docs/arch/usage-claudecode.md`.
>
> Nguyên tắc xuyên suốt: **xoá nhiều hơn thêm**. Mỗi bước dưới đây đều làm codebase nhỏ đi.

## 0. Vì sao dọn

Một tính năng đọc phần trăm quota đang chiếm ~2.400 dòng tài liệu và một luồng active tự gọi
`claude` 3 lần mỗi lượt. Luồng đó gây tràn RAM một máy remote đến mức phải bỏ máy (research §4).

Phần lớn độ phức tạp là **di sản của một hiểu lầm đã được đính chính**: người ta tin probe
headless fire statusLine hook, nên dựng vũ điệu 3 lượt gọi để đi vòng (research §5.3).

Đo thật 2026-07-20 khép lại câu chuyện: turn headless **chỉ trả mốc reset, không có phần trăm**
(§1.5). Toàn bộ luồng active vì thế đổi lấy gần như không gì — nên xoá hẳn, không rút gọn.

Hai thứ được thêm vào ngày 2026-07-20 (`FORCE_SYNC_TIMEOUT_SECS = 180`, sửa pattern
`ORPHAN_PATTERNS`) là **vá chống đỡ cho code lẽ ra phải xoá**. Plan này gỡ chính chúng.

---

## 1. Code — thứ tự thực thi

Quyết định chốt 2026-07-20: **xoá hẳn luồng active**, không rút gọn. Căn cứ: đo thật cho thấy
turn headless chỉ trả mốc reset, không có phần trăm (research §1.5) → đổi lấy quá ít so với việc
nó là nguồn gốc của cả ba lỗi gây tràn RAM.

### B1. Xoá toàn bộ force-sync

**Xoá file:** `scripts/force-sync-claudecode.sh`, `scripts/force-sync-parse.py`

**`src-tauri/src/agent_usage.rs`** — xoá lệnh `force_sync_agent_usage`, `force_sync_agent_usage_sync`,
`FORCE_SYNC_TIMEOUT_SECS`, `run_remote_script_long()`, mọi tag log `FORCE_SYNC`.

**`src/composables/useAgentUsage.js`** — xoá `forceSync()`, `isSyncing`, `forceSyncFailCount`,
`MAX_FORCESYNC_RETRIES`, `initialSyncDone`, `staleResetSyncDone`, `showForceSyncDebugAlert()`,
hằng `FORCE_SYNC_PROMPT`, và nhánh gọi force-sync trong `checkUsage()`.

**`src/components/AgentUsage.vue`** — xoá nút Force Sync và emit `force-sync`.
Xoá luôn chuỗi truyền emit ở component cha.

*Gỡ theo:* `src-tauri/capabilities/default.json` nếu có entry riêng cho lệnh này.

### B2. Qua mốc reset → giữ số cũ thay vì xoá trắng

**`src/composables/useAgentUsage.js`**

Hiện tại `|||STALE_RESET|||` → `data = null` → UI trắng. Đổi thành: giữ nguyên `data`, bật
`isCached = true`, `cachedAt = lastFetchedAt` — **đúng cơ chế AG đang dùng**, không viết mới.

**`src/components/AgentUsage.vue`** — ở trạng thái cached hiện thêm một dòng:

> `Waiting for next Claude Code session`

Một dòng, thay đúng vào chỗ nút Force Sync vừa bị xoá. Không thêm phần tử nào so với hiện tại
(tuân thủ nguyên tắc Extreme Narrow trong CLAUDE.md).

### B3. Xoá cơ chế dọn rác

**`src-tauri/src/agent_usage.rs`** — xoá `cleanup_orphan()`, `ORPHAN_PATTERNS`,
`wait_with_timeout()`, `CLEANUP_TIMEOUT_SECS`, lời gọi cleanup trong nhánh timeout.

Sau B1 app không còn tự chạy `claude` ở đâu → không có rác để dọn.

### B4. Gỡ khối OAuth HTTP

**`scripts/get-claudecode-usage.sh`** — xoá khối Python gọi
`GET https://api.anthropic.com/api/oauth/usage`, marker `aki-oauth-last-attempt`, gate 60s.

Lý do đầy đủ: research §3.

Sau bước này poll = đọc `rate-limits-cache.json` + `claude auth status`. Không gọi mạng.

**Đánh đổi ghi vào CHANGELOG:** mất khả năng thấy quota tiêu bởi Claude app/Cowork. Có chủ đích,
không phải hồi quy.

### B5. Giữ nguyên

- `AKI_CLAUDE_TMO` (bound `claude` phía remote) — vẫn cần cho `claude auth status`, lệnh `claude`
  duy nhất còn lại.
- `haltPolling()` sau 5 lần hỏng liên tiếp.
- `ConnectTimeout` / `ServerAlive` / `BatchMode`.
- `provision-claudecode.sh` — cài statusLine hook, giờ là nguồn dữ liệu **duy nhất**.

## 2. Docs — trạng thái đích

Còn đúng **3 file** về chủ đề này:

| File | Vai trò |
|---|---|
| `docs/arch/usage-claudecode.md` | Kiến trúc **đang chạy**. Viết lại gọn, xoá 5 chỗ đính chính/gạch bỏ inline (giữ kết luận đúng, bỏ dấu vết tranh cãi) |
| `docs/research/claudecode-usage-FINAL.md` | Sự thật đã kiểm chứng + nhật ký đã thử/đã bỏ + ràng buộc bất biến |
| `docs/plan/claudecode-usage-cleanup-FINAL.md` | File này. Chuyển `docs/plan/done/` sau khi xong |

**Xoá 8 file** — nội dung đã được cô đọng vào research §5 (nhật ký) và §4 (bài học):

```
docs/research/claude-usage-1.2.x-analyze.md                    (281)
docs/research/claude-usage-dash-pipefail-regression.md         (166)
docs/research/claude-headless-rate-limit-event-2026-07-09.md   (108)
docs/research/claude-app-usage-measurement.md                  (172)
docs/research/ssh-process-leak-remote-ram-overflow.md          (128)
docs/plan/done/claudecode-oauth-usage-p3.md                    (485)
docs/plan/done/fix-claude-flow-24jun.md                        (318)
docs/plan/done/fix-usage-monitor-freeze.md                     (287)
```

**Giữ lại, không đụng:**

- `docs/research/aki-dev-sync-ag-cc-usage-flow.md` — bài đối ngoại cho dev.akitao.com, thể loại
  khác, không phải nhật ký nội bộ. **Nhưng phải sửa một chỗ:** nó quảng cáo tiêu chí "An toàn —
  không vi phạm ToS" trong khi code khi đó đang gọi endpoint nội bộ. Sau B5 câu đó mới thành
  đúng; kiểm lại lúc dọn.
- `docs/plan/done/fix-to-1.3.1.md` — audit toàn codebase, không riêng CC usage. Xoá sẽ mất phần
  không liên quan.

Ước tính: **~2.400 → ~400 dòng**.

---

## 3. Kiểm chứng

Ràng buộc môi trường: **không tự build, không tự chụp hình** — user tự test.

| # | Kiểm | Cách |
|---|---|---|
| V1 | Rust compile | `cargo check` |
| V2 | Frontend build | `npx vite build` |
| V3 | Script còn là POSIX sh hợp lệ | `sh -n scripts/*.sh` (research §6.1) |
| V4 | Không còn tham chiếu treo | `grep -rn "force-sync-parse\|cleanup_orphan\|ORPHAN_PATTERNS\|oauth/usage\|run_remote_script_long"` → rỗng |
| V5 | Không doc nào trỏ tới file đã xoá | `grep -rn` từng tên file đã xoá trong `docs/` và mã nguồn (chú ý header `// @docs`) |
| V6 | Qua mốc reset hiện số cũ + dòng chờ | **User quan sát** sau lần reset kế tiếp |
| V7 | App không còn tự spawn `claude` | `grep -rn "claude --model\|claude -p" scripts/ src-tauri/` → chỉ còn `auth status` |

---

## 4. Việc còn nợ, cố ý không làm trong plan này

1. **`bien` đã chết vì tràn RAM, không cứu.** Không còn máy remote để kiểm chứng đường SSH —
   đường remote sẽ ship mà chưa ai chạy thật. Ghi rõ ở đây, không giấu.
2. **Điểm mù Claude app.** Đóng lại như đánh đổi có chủ đích (research §3), không phải TODO.

---

## 5. Thứ tự

Docs là SSOT: viết/sửa docs trước, code sau, code phản ánh docs.

```
D1  research FINAL          ✅ xong
D2  plan FINAL (file này)   ✅ xong
D3  arch doc viết lại       → mô tả trạng thái sau khi dọn
D4  xoá 8 file + sửa tham chiếu
────────────────────────────────
B1..B5  code                → khớp đúng D3
V1..V5  tự kiểm
V6..V7  user kiểm
────────────────────────────────
CHANGELOG (ghi rõ đánh đổi B5) → plan này chuyển sang docs/plan/done/
```
