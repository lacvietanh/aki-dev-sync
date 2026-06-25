# Post-mortem: Lỗi "load mãi / no data" của Claude Usage — `set -o pipefail` chết trên `dash`

> Thời điểm chốt nguyên nhân: **2026-06-25**. Phiên bản dính lỗi: từ sau commit refactor `98fa2b7` cho tới `1.3.3`.
> Tài liệu này ghi lại trọn vẹn vụ bug đã ngốn "bao nhiêu phiên bản và ngày đêm" để không bao giờ phải điều tra lại từ đầu.

---

## 1. Triệu chứng

Ở **đúng một** kịch bản:

- Đã **qua thời điểm reset quota** (`five_hour.resets_at` nằm trong quá khứ), VÀ
- **Không có session Claude nào đang hoạt động** để statusLine hook tự ghi `resets_at` mới.

→ Panel Claude Usage **load mãi, không bao giờ có data**. Sau 3 lần tự poll vẫn `no data`. Và **không có log nào đáng kể** chỉ ra lỗi.

Trong khi đó, ở trạng thái bình thường (còn session, cache còn hạn) thì **mọi thứ chạy hoàn hảo** — đây chính là cái bẫy khiến lỗi sống sót qua nhiều phiên bản.

---

## 2. Nguyên nhân gốc (root cause)

**Dòng 2 của `scripts/force-sync-claudecode.sh`:**

```sh
set -o pipefail 2>/dev/null || true
```

Script này được giao tới remote qua `ssh host sh` (xem `agent_usage.rs::run_remote_script`), và trên hầu hết remote Linux `sh` là **dash**, không phải bash.

Chuỗi sự kiện chí mạng trên dash:

1. `set` là một **POSIX special built-in**.
2. `-o pipefail` là cú pháp của bash/zsh — dash **không** có. Đây là một **usage error của special built-in**.
3. Theo chuẩn POSIX, khi special built-in gặp usage error, shell **non-interactive phải thoát NGAY LẬP TỨC** với exit status `2`.
4. Cái bẫy: shell thoát **trước khi** chạy tới `|| true` → `|| true` **không cứu được gì**.
5. `2>/dev/null` nuốt mất thông báo lỗi → **không một byte stderr nào** được ghi.

Kết quả: `force-sync` chết ngay ở dòng 2, **exit=2, stdout rỗng, stderr rỗng**, không chạy bất cứ dòng `_log` nào của thân script.

### Bằng chứng tái hiện (trên chính máy remote "bien")

```
$ printf 'set -o pipefail 2>/dev/null || true\necho REACHED' | dash ; echo "exit=$?"
exit=2                       # dừng ở dòng 1, KHÔNG in REACHED

$ printf 'set -o pipefail 2>/dev/null || true\necho REACHED' | bash ; echo "exit=$?"
REACHED
exit=0                       # bash thì không sao
```

Trace bằng `sh -x` cho combined script: chỉ in đúng **1 dòng** `+ set -o pipefail` rồi chết.

Khớp tuyệt đối với log production:

```
[FORCE_SYNC] ssh_result: exit=2 stdout_bytes=0 stderr_bytes=0
```

---

## 3. Tại sao lỗi này "ẩn mình" qua nhiều phiên bản

Bốn yếu tố cộng hưởng khiến nó cực khó tìm:

| # | Yếu tố | Hệ quả |
|---|--------|--------|
| A | **Thất bại im lặng** | `2>/dev/null` nuốt lỗi, exit code bị diễn giải thành `parsed:false` vô hại → không có log báo động |
| B | **Bị che lấp ở luồng thường** | Khi cache còn hạn, `get-usage` đã trả data hợp lệ; `force-sync` thất bại cũng **không ai thấy** vì UI vẫn có số |
| C | **Chỉ lộ ở trạng thái hiếm** | Chỉ kịch bản "sau reset + no session" mới biến `force-sync` thành đường phục hồi **duy nhất** — và nó đã chết |
| D | **Do refactor không liên quan gây ra** | Bug không nằm trong logic usage → diff code usage **không thấy gì** |

### Yếu tố D — regression thực sự đến từ đâu

Commit `98fa2b7` ("refactor: split god-module into domain modules") đã đổi cách chạy script remote:

| Trước `98fa2b7` | Sau `98fa2b7` (tới 1.3.3) |
|-----------------|---------------------------|
| `ssh host <cmd>` → chạy bằng **login shell** của remote (bash/zsh) | `ssh host sh` (script qua stdin) → ép dùng **dash** |

`set -o pipefail` **chạy được** trên bash/zsh nên trước đó không lộ. Khi refactor ép sang `sh`/dash, lỗi tiềm ẩn (latent bug) đã có sẵn trong script mới phát tác. Vì đây là thay đổi ở tầng "cách gọi shell", không phải tầng "logic usage", nên mọi nỗ lực soi diff phần usage đều vô ích.

### Đối chứng: vì sao `get-usage` không chết

`scripts/get-claudecode-usage.sh` mở đầu bằng `set -e` (hợp lệ trên dash), **không** dùng `set -o pipefail`. Nên nó sống khỏe trên dash. Chỉ duy nhất `force-sync-claudecode.sh` dính.

---

## 4. Luồng bị đứt (flow breakdown)

```
get-usage → cache có resets_at trong QUÁ KHỨ → |||STALE_RESET||| → Rust trả None   ✓ đúng
   ↓
JS: hadData=false, !initialSyncDone → gọi forceSync()                              ✓ đúng
   ↓
force_sync → ssh host sh → dash chết ở dòng 2 → exit 2, RỖNG                        ✗ ĐỨT
   ↓
cache KHÔNG BAO GIỜ được refresh → resets_at vẫn ở quá khứ
   ↓
mọi get-usage tiếp theo → STALE_RESET → None → MÃI MÃI no data
```

**Tầng khoá kép (làm kẹt vĩnh viễn):** sau lần `forceSync` đầu thất bại, JS đã set `initialSyncDone=true` / `staleResetSyncDone=true`, cộng nhánh `hadData=false` không bao giờ trigger lại `forceSync`. Nên dù poll mỗi 30s vẫn **không thử lại** → kẹt cứng.

### Quan sát bổ sung từ log "trạng thái bình thường"

Ngay cả khi cache còn hạn, `forceSync` **vẫn** được gọi lúc khởi động (qua `refreshAll()` → `triggerManualRefresh()` → watcher `manualRefreshCount` → `claudecode` luôn `forceSync`) và **vẫn chết exit=2**. Nhưng vì `get-usage` đã trả data hợp lệ nên thất bại đó **vô hình**. Đây là bằng chứng đanh thép nhất cho yếu tố B: bug đã xảy ra **mỗi lần khởi động**, suốt nhiều phiên bản, mà không ai biết.

---

## 5. Cách sửa (đã áp dụng)

`scripts/force-sync-claudecode.sh` dòng 2 → probe trong subshell, chỉ bật pipefail ở shell thực sự hỗ trợ:

```sh
( set -o pipefail ) 2>/dev/null && set -o pipefail
```

- **dash**: subshell chết (status 2 bị nuốt) nhưng **shell cha sống tiếp** → bỏ qua, không bật pipefail.
- **bash/zsh**: subshell thành công → bật pipefail ở shell cha như mong muốn.

Kiểm chứng end-to-end (chạy combined script qua pipe + stub `claude`, đúng như `ssh host sh`):

| | Trước fix | Sau fix |
|--|-----------|---------|
| exit | `2` | `0` |
| stdout | rỗng | JSON diagnostic `parsed:true written:true` |
| stderr | rỗng | đầy đủ dòng `[SHELL:force-sync]` |
| cache | không đổi | được ghi `resets_at` mới |

### 5b. Xác nhận thực địa tại giao thoa reset (2026-06-25 14:10, build đã fix)

Bắt đúng kịch bản hiểm nhất — **qua mốc reset mà không có session active** — trên build đã có fix:

```
14:09:31  poll: resets_at còn 29s → cache valid, pct=47          (load thường, không sync)
14:10:01  poll: NOW > resets_at (overdue 1s) → STALE → null
          ├─ forceSync fire (đúng trigger: chỉ khi null)
          ├─ /usage lần 1: has_resets=NO (chưa có session local)
          ├─ probe_decision: resets_is_future=0 → probe=YES
          ├─ probe: tạo session haiku → exit=0 (3s)
          └─ /usage lần 2: has_resets=YES future → cache ghi pct=0 resets_at=+5h  (tổng 9s)
14:10:12  get_usage: cache valid pct=0 → Ok(Some)                (UI: 47% → 0%, không flash data sai)
```

Ba điểm xác nhận: (1) **không** có forceSync vô điều kiện lúc startup 14:09 — fix watcher `manualRefreshCount → checkUsage` ăn; (2) force-sync chạy `exit=0` kèm 27 dòng `[SHELL:force-sync]` — bug dash chết hẳn; (3) probe future-check (`resets_is_future`) chặn đúng cái bẫy v1.3.2 (echo `resets_at` cũ). Hiện tượng "đỏ lòm 100%" ở log cũ chỉ là artifact của binary **trước fix** (forceSync vô điều kiện trúng bug dash → nút reload bật `.error-state` đỏ + `RefreshRing` overlay đầy vòng), không tái diễn trên build mới.

---

## 6. Bài học & nguyên tắc phòng ngừa (để không tái diễn)

1. **Mọi script giao qua `ssh host sh` phải là POSIX sh thuần.** Không bashism (`set -o pipefail`, `[[ ]]`, `local` ngoài hàm, mảng, `function name()`...). Verify bằng `dash -n` trước khi ship.
2. **Không bao giờ nuốt lỗi shell im lặng.** `2>/dev/null` + `|| true` ở đầu script là anti-pattern — nó che mất chính cái lỗi cần thấy. Lỗi phải lên tới log/diagnostic.
3. **Exit code phải được phân loại đúng:** `exit != 0 && stdout rỗng` là **trạng thái LỖI cần báo động**, không phải `parsed:false` vô hại.
4. **Mỗi script tự khai báo interpreter** ở dòng log đầu tiên (`interpreter=$0`) để log luôn cho biết shell nào đã chạy.
5. **Cảnh giác với "cái thường thì chạy tốt".** Một thất bại bị che lấp ở luồng phổ biến vẫn là một quả bom hẹn giờ ở luồng hiếm.

Xem kiến trúc phòng ngừa chi tiết (lint build-time, surfacing lỗi, retry, timeout) tại `docs/arch/usage-claudecode.md` §6.

---

## 7. Tham chiếu chéo

- Kiến trúc & sơ đồ luồng update: `docs/arch/usage-claudecode.md`
- Phân tích usage 1.2.x: `docs/research/claude-usage-1.2.x-analyze.md`
- Code liên quan: `scripts/force-sync-claudecode.sh`, `scripts/get-claudecode-usage.sh`, `scripts/force-sync-parse.py`, `src-tauri/src/agent_usage.rs`, `src/composables/useAgentUsage.js`
