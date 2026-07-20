# Điều tra: Tràn RAM remote do tích lũy SSH process — vòng lặp dương tính của usage polling

> Thời điểm điều tra: **2026-07-20**. Phiên bản: `1.14.0`.
> Máy remote bị ảnh hưởng: `bien` (SSH host trong SSH config).
> Tài liệu này ghi lại toàn bộ phân tích để xác định liệu app Aki-Dev-Sync có phải thủ phạm gây tràn RAM trên remote thông qua tích lũy SSH/child process.

---

## 1. Triệu chứng

Máy remote `bien` bị tràn RAM, phát hiện có **quá nhiều process liên quan SSH + claude** đang chạy đồng thời. Log production của app tại thời điểm phát hiện:

```
[20260720.104717.303][USAGE:claudecode] IPC error err="script timed out after 30s (local killed, cleanup fired) host=bien"
[20260720.104747.355][USAGE:claudecode] IPC error err="script timed out after 30s (local killed, cleanup fired) host=bien"
[20260720.104817.369][USAGE:claudecode] IPC error err="script timed out after 30s (local killed, cleanup fired) host=bien"
[20260720.104847.402][USAGE:claudecode] IPC error err="script timed out after 30s (local killed, cleanup fired) host=bien"
  ... (liên tục mỗi 30s, kéo dài 24+ phút)
```

Kèm theo chuỗi provision retry, force-sync retry, và `ssh: connect to host ... Operation timed out` — tất cả cho thấy remote đã bão hoà đến mức không còn accept được connection mới.

---

## 2. Tất cả các điểm spawn SSH trong app

App spawn `ssh` ở **3 điểm** trong `agent_usage.rs`, cộng thêm `sync.rs`/`system.rs` (không polling):

### 2a. Polling SSH (chạy liên tục, mỗi `usage_interval_s` giây)

| Nguồn | File | Dòng | Lệnh | Khi nào |
|--------|------|------|-------|---------|
| `get_agent_usage` (CC) | `agent_usage.rs` | L52 | `ssh host sh` + stdin script | Mỗi poll tick (mặc định 30s) |
| `get_agent_usage` (AG) | `agent_usage.rs` | L62 | `ssh host node` + stdin script | Mỗi poll tick |
| `provision_agent_usage` | `agent_usage.rs` | L52 | `ssh host sh` + stdin script | Khởi động + retry khi fail |
| `force_sync_agent_usage` | `agent_usage.rs` | L52 | `ssh host sh` + stdin script | Khi cache null/stale + retry |
| `cleanup_orphan` | `agent_usage.rs` | L93 | `ssh host pkill -f "claude -p"` | Mỗi khi timeout xảy ra |

### 2b. Non-polling SSH (chỉ chạy khi user trigger)

| File | Mục đích |
|------|----------|
| `sync.rs` L105, L427, L468 | rsync sync / mkdir / version check — user-triggered |
| `system.rs` L122, L377 | Terminal SSH / remote command — user-triggered |

**Kết luận:** chỉ nhóm 2a gây tích lũy tự động. Nhóm 2b chỉ chạy khi user click.

---

## 3. Frontend polling — bao nhiêu instance, tần suất nào

`src/composables/useAgentUsage.js` được khởi tạo **3 lần** đồng thời (xem `AgentUsageSection.vue`):

| Instance | Agent | Host | SSH? |
|----------|-------|------|------|
| 1 | `antigravity` | local hoặc remote | Có nếu remote |
| 2 | `claudecode` | local | Có nếu remote |
| 3 | `claudecode` | remote (e.g. `bien`) | **CÓ — đây là instance gây vấn đề** |

Mỗi instance tạo `setInterval` với period = `usage_interval_s` (settings, mặc định 30s):

```javascript
// useAgentUsage.js L619-622
pollTimer = setInterval(() => {
  checkUsage();
}, s * 1000);
```

Ngoài poll timer, còn có **wake self-heal triggers** (L208-211):
- `visibilitychange` → `checkUsage()` ngay lập tức
- `focus` → `checkUsage()` ngay lập tức
- Watchdog heartbeat (7s interval) → `checkUsage()` nếu gap > 2×interval

---

## 4. Timeout/cleanup chain — hoạt động thế nào, hở ở đâu

### 4a. Happy path (remote responsive, script < 30s)

```
Local                          Remote
  |                              |
  |── ssh host sh ──────────────>|
  |   stdin: script              |── sh process
  |                              |   ├── python3 (oauth)
  |                              |   ├── claude auth status
  |                              |   └── cat cache.json
  |<──── stdout (data) ─────────|
  |   SSH exits cleanly          |   sh exits → children reaped
  ✓ Done                         ✓ Clean
```

### 4b. Timeout path (remote slow/hung) — ĐÂY LÀ VẤN ĐỀ

```
Local                                     Remote
  |                                         |
  |── ssh host sh ─────────────────────────>|
  |   stdin: script                         |── sh process
  |                                         |   ├── python3 (oauth, 8s HTTP)
  |   [30s elapsed]                         |   ├── bash -lc "claude auth status" ← CHƯA XONG
  |                                         |   └── python3 -c "..." ← CHƯA XONG
  |── child.kill() ─┐                      |
  |── child.wait()  │                      |   ← SSH connection bị cắt
  |                  │                      |
  |   ✓ Local SSH    │                      |   remote sshd gửi SIGHUP
  |     process      │                      |   nhưng:
  |     reaped       │                      |   - sh có thể đã thoát
  |                  │                      |   - python3 có thể ignore SIGHUP
  |                  │                      |   - bash -lc "claude..." có thể orphan
  |                  │                      |   - claude process ← ORPHAN
  |                  │                      |
  |── cleanup_orphan │ (fire-and-forget)    |
  |   ssh host       │                      |
  |   pkill -f       │                      |
  |   "claude -p"    │                      |   ← THÊM 1 SSH connection
  |   (NO TIMEOUT)   │                      |   ← thread treo nếu remote chậm
  |                  │                      |
  |   [30s later]    │                      |
  |── POLL TICK      │                      |
  |── ssh host sh ──────────────────────────>   ← THÊM 1 SSH connection nữa
  |   ...            │                      |
```

### 4c. Phân tích cụ thể từng điểm hở

#### Hở 1: `cleanup_orphan` — fire-and-forget, KHÔNG có timeout

```rust
// agent_usage.rs L84-98
fn cleanup_orphan(self, host: &str, local: bool) {
    // ...
    std::thread::spawn(move || {
        let _ = Command::new("ssh")
            .args([host_cleanup.as_str(), "pkill", "-f", "claude -p"])
            .output();       // ← .output() = blocking VÔ HẠN
    });
}
```

Mỗi timeout spawn 1 thread chạy `ssh host pkill ...` **không có timeout**. Nếu remote chậm:
- Thread treo → không bao giờ join → tích lũy threads
- SSH connection treo → tích lũ sshd process trên remote
- `pkill -f "claude -p"` chỉ kill `claude -p`, **KHÔNG kill** `python3`, `bash -lc`, `sh`, `node`

#### Hở 2: SIGHUP không chắc chắn kill toàn bộ process tree trên remote

Khi SSH connection đứt, remote `sshd` gửi `SIGHUP` đến process group. Nhưng:

1. `python3` inline scripts (`get-claudecode-usage.sh` L46-182) — đang chạy HTTP request, có thể bị stuck trong syscall, nhận SIGHUP muộn hoặc không
2. `bash -lc "'$CLAUDE_BIN' auth status"` — bash login shell, `claude` CLI bên trong có thể đã `fork` + `setsid`
3. `zsh -lc "cd ... && '$CLAUDE_BIN' --model haiku -p /usage"` (force-sync) — Claude CLI tự spawn subprocess, có thể orphan

Bao nhiêu process 1 SSH session có thể để lại trên remote:

| Script | Processes spawned trên remote |
|--------|-------------------------------|
| `get-claudecode-usage.sh` | `sh` → `python3` (oauth) → `bash -lc "claude auth status"` → `python3 -c ...` (resets_at check) → `python3 -c ...` (auth parse) |
| `force-sync-claudecode.sh` | `sh` → `zsh -lc "claude --model haiku -p /usage"` → `zsh -lc "claude --model haiku -p respond_with_ok"` → `python3 -c ...` (parse) → `python3 /tmp/.claude_sync_parse.py` |
| `provision-claudecode.sh` | `sh` → `bash -lc "claude auth status"` |

Worst case: **5-8 process per SSH session** có thể orphan trên remote khi SSH bị kill sớm.

#### Hở 3: Force-sync retry cascade

```javascript
// useAgentUsage.js L535-538
if (forceSyncFailCount < MAX_FORCESYNC_RETRIES) {
  initialSyncDone = false;      // ← RESET guard
  staleResetSyncDone = false;   // ← RESET guard
}
```

Khi force-sync fail (timeout), code reset guards → poll tick tiếp theo nhận `null` từ `get_agent_usage` → trigger `forceSync()` lại → thêm SSH. Force-sync bản thân nó chạy **3 lệnh SSH nặng**: run_usage + probe + run_usage lần 2.

#### Hở 4: Volume tích lũy thực tế

Trong 1 phút, với remote bị chậm, CC remote instance có thể spawn:

| Thời điểm | Hành động | SSH connections |
|-----------|-----------|----------------|
| t=0s | Poll tick → `get_agent_usage` | +1 |
| t=30s | Timeout → `cleanup_orphan` | +1 (fire-and-forget) |
| t=30s | Poll tick → `get_agent_usage` (mới) | +1 |
| | **Subtotal/phút** | **3** |

Nếu force-sync trigger (mỗi 30s retry khi null):

| Thời điểm | Hành động | SSH connections |
|-----------|-----------|----------------|
| t=0s | force_sync → run_remote_script | +1 |
| t=30s | Timeout → cleanup | +1 |
| t=30s | Poll → get_agent_usage (null) → forceSync lại | +1 |
| t=60s | Timeout → cleanup | +1 |
| | **Subtotal/phút** | **4+** |

Qua 24 phút (khoảng thời gian log thực tế): **48-96 SSH connections** từ app.

Mỗi connection có thể để lại **2-8 orphan process** trên remote → **96-768 orphan processes** trên remote.

---

## 5. Bằng chứng trực tiếp từ log production

### 5a. Chuỗi timeout liên tục (get_agent_usage)

```
[20260720.104717] IPC error err="script timed out after 30s host=bien"
[20260720.104747] IPC error err="script timed out after 30s host=bien"
[20260720.104817] IPC error err="script timed out after 30s host=bien"
[20260720.104847] IPC error err="script timed out after 30s host=bien"
```

Khoảng cách: **30s đúng** = poll interval = timeout → mỗi tick spawn 1 SSH, hết timeout ngay lúc tick mới.

### 5b. Provision retry storm

```
[20260720.104932] provision err err="script timed out after 30s host=bien" n=1
[20260720.105147] provision err err="script timed out after 30s host=bien" n=2
```

Provision retry 2 lần trước khi give up (max=3), mỗi lần thêm 1 SSH connection.

### 5c. Force-sync failure cascade

```
[20260720.105003] fs err err="script timed out after 30s host=bien"
[20260720.105217] fs err err="script timed out after 30s host=bien"
```

Force-sync retry, mỗi lần 30s timeout = thêm SSH connection.

### 5d. Remote hoàn toàn bão hoà

```
[20260720.110419] FORCE_SYNC shell stderr: ssh: connect to host 100.102.111.75 port 22: Operation timed out
```

Cuối cùng remote **từ chối luôn TCP connection** — sshd đã quá tải hoặc kernel hết resource.

### 5e. Cho dù mất kết nối, app vẫn TIẾP TỤC spawn

```
[20260720.110541] IPC error err="script timed out after 30s host=bien"
[20260720.110611] IPC error err="script timed out after 30s host=bien"
[20260720.110641] IPC error err="script timed out after 30s host=bien"
[20260720.110711] IPC error err="script timed out after 30s host=bien"
[20260720.110741] IPC error err="script timed out after 30s host=bien"
[20260720.110811] IPC error err="script timed out after 30s host=bien"
[20260720.110841] IPC error err="script timed out after 30s host=bien"
[20260720.110911] IPC error err="script timed out after 30s host=bien"
[20260720.110941] IPC error err="script timed out after 30s host=bien"
[20260720.111011] IPC error err="script timed out after 30s host=bien"
[20260720.111041] IPC error err="script timed out after 30s host=bien"
[20260720.111111] IPC error err="script timed out after 30s host=bien"
```

**Không có circuit breaker** — dù remote đã chết hẳn, app vẫn cứ 30s spawn 1 SSH connection mới. Đây chính là vòng lặp dương tính: remote overloaded → timeout → spawn thêm → remote thêm overloaded.

---

## 6. Cơ chế gây tràn RAM — mô hình vòng lặp dương tính (positive feedback loop)

```
┌──────────────────────────────────────────────────────────────────┐
│                    POSITIVE FEEDBACK LOOP                        │
│                                                                  │
│  Remote chậm (bất kỳ lý do gì ban đầu)                          │
│       ↓                                                          │
│  SSH scripts chạy lâu hơn bình thường                            │
│       ↓                                                          │
│  Timeout 30s trigger → local kill SSH                            │
│       ↓                                                          │
│  Remote: SIGHUP không kill hết child processes                   │
│  (python3, claude, bash -lc... orphan)                           │
│       ↓                                                          │
│  + cleanup_orphan spawn thêm SSH (không timeout)                 │
│  + poll tick spawn SSH mới ngay lập tức                          │
│       ↓                                                          │
│  Remote tích lũy orphan processes → dùng thêm RAM/CPU           │
│       ↓                                                          │
│  Remote CHẬM HƠN → script timeout NHIỀU HƠN                     │
│       ↓                                                          │
│  (quay lại đầu vòng lặp)                                        │
│       ↓                                                          │
│  Kết quả: RAM exhaustion → OOM → remote treo hoàn toàn          │
└──────────────────────────────────────────────────────────────────┘
```

Đặc điểm của loop này: **không cần trigger ban đầu lớn**. Chỉ cần remote bị chậm nhẹ (VD: đang build lớn, swap nhiều) → bắt đầu timeout → cascade → tràn.

---

## 7. Các điểm yếu cần sửa (sắp theo mức độ nghiêm trọng)

> **Trạng thái 2026-07-20 — cả P0–P4 đã sửa trong 1.14.0.** Xem CHANGELOG 1.14.0 và
> `useAgentUsage.js` (`restartPollTimer` / `nextPollDelayMs`) + `agent_usage.rs`
> (`polling_ssh` / `wait_with_timeout` / `ORPHAN_PATTERNS`). Phần dưới giữ nguyên như lúc
> điều tra để làm bối cảnh cho từng quyết định.
>
> - **P0** → chained `setTimeout` + exponential backoff sau 3 fail liên tiếp, trần 10 phút,
>   reset khi host trả lời hoặc user bấm Reload. Watchdog lấy ngưỡng gap từ chính delay đã
>   backoff (nếu không nó bắn lại probe mỗi 7s, phá đúng cái backoff vừa dựng).
> - **P1** → `cleanup_orphan` có bound 8s (`wait_with_timeout`), gộp mọi pattern vào 1 SSH.
> - **P2** → mở rộng pattern sang `claude auth status` + `claude --model haiku`. **Cố ý KHÔNG**
>   `pkill python3`/`node` diện rộng: trên dev box dùng chung việc giết nhầm process của người
>   khác tệ hơn hẳn so với rò rỉ vài orphan.
> - **P3** → `ConnectTimeout=10 ServerAliveInterval=5 ServerAliveCountMax=3 BatchMode=yes`.
> - **P4** → được giải quyết cấu trúc bởi P0: chained scheduling khiến probe không bao giờ chồng
>   nhau, không cần điều kiện `interval > timeout` nữa.


### P0: Không có circuit breaker cho liên tiếp timeout

**Hiện trạng:** dù timeout 100 lần liên tiếp, poll vẫn tiếp tục spawn SSH mỗi 30s.

**Cần:** sau N lần timeout liên tiếp (ví dụ 5), dừng poll hoặc exponential backoff (30s → 60s → 120s → ...). Bắt buộc phải có ceiling (ví dụ 10 phút).

### P1: `cleanup_orphan` không có timeout

**Hiện trạng:** `Command::new("ssh").output()` blocking vô hạn trên thread riêng.

**Cần:** route qua `run_interpreter_timeout` với timeout ngắn (5-10s), hoặc ít nhất dùng `tokio::time::timeout`.

### P2: Remote-side orphan cleanup chỉ kill `claude -p`

**Hiện trạng:** `pkill -f "claude -p"` chỉ nhắm `claude -p`. Không cleanup `python3`, `bash -lc`, `sh`, `node` do script spawn.

**Cần:** kill cả process tree. Có thể dùng `pkill -P <sshd_child_pid>` hoặc set `setsid` + kill process group. Hoặc dùng SSH `-o ServerAliveInterval` + `-o ServerAliveCountMax` để SSH tự ngắt sạch.

### P3: SSH options thiếu cho polling connections

**Hiện trạng:** `Command::new("ssh").args([host, "sh"])` — không có connection timeout, TCP keepalive, hay ServerAlive options.

**Cần:** thêm options cho mọi polling SSH:
```
-o ConnectTimeout=10
-o ServerAliveInterval=5
-o ServerAliveCountMax=3
-o BatchMode=yes
```

`ConnectTimeout` đặc biệt quan trọng — hiện tại SSH có thể mất 30-60s chỉ để TCP handshake khi remote bão hoà, trong khi `REMOTE_SCRIPT_TIMEOUT_SECS` đã đếm ngược.

### P4: Poll interval = timeout = 30s — chồng chéo

**Hiện trạng:** poll interval 30s, timeout 30s → SSH cũ timeout đúng lúc SSH mới spawn. Không bao giờ có khoảng nghỉ.

**Cần:** interval > timeout, hoặc guard không spawn SSH mới nếu SSH cũ chưa xong (hiện có `isChecking` ở JS nhưng Rust side không có).

---

## 8. Lệnh kiểm tra trên remote

Chạy trên remote `bien` để xác nhận và đo mức độ:

```bash
# Đếm tất cả process liên quan
ps aux | grep -E 'sshd|claude|python3|bash.*claude|node' | grep -v grep | wc -l

# Xem chi tiết từng process, sorted by memory
ps aux --sort=-%mem | grep -E 'sshd|claude|python3|bash.*claude' | grep -v grep | head -30

# Xem process tree (Linux)
pstree -ap $(whoami) 2>/dev/null | head -50

# Kill tất cả orphan claude processes
pkill -f "claude -p"
pkill -f "claude.*--model.*haiku"
pkill -f "claude auth status"

# Kill orphan python3 inline scripts (cẩn thận — chỉ kill nếu đúng là từ app)
# Kiểm tra trước:
ps aux | grep python3 | grep -E 'rate-limits|oauth|parse' | grep -v grep
```

---

## 9. Kết luận

**App Aki-Dev-Sync CÓ khả năng cao là nguyên nhân chính gây tràn RAM trên remote**, thông qua cơ chế vòng lặp dương tính:

1. Remote chậm → SSH timeout → orphan process tích lũy
2. Poll tiếp tục spawn SSH mỗi 30s (không circuit breaker)
3. Cleanup SSH cũng spawn thêm SSH (fire-and-forget, không timeout)
4. Mỗi SSH session có thể để lại 2-8 orphan process trên remote
5. 24+ phút liên tục = 48-96 SSH connections = hàng trăm orphan processes

Bản chất là thiếu **circuit breaker** + **remote-side process lifecycle management**.

---

## 10. Tham chiếu chéo

- Kiến trúc usage polling: `docs/arch/usage-claudecode.md`
- Post-mortem dash/pipefail (liên quan SSH script delivery): `docs/research/claude-usage-dash-pipefail-regression.md`
- Code SSH spawn: `src-tauri/src/agent_usage.rs` (L48-98, L140-204)
- Code polling frontend: `src/composables/useAgentUsage.js` (L613-636)
- Remote scripts: `scripts/get-claudecode-usage.sh`, `scripts/force-sync-claudecode.sh`, `scripts/provision-claudecode.sh`

---

## 11. Đính chính (2026-07-20, sau khi điều tra lại theo flow)

Phần 1–10 ở trên **quy sai thủ phạm**. Giữ lại nguyên văn để thấy suy luận sai ở đâu.

### Bằng chứng phản bác

**Force-sync KHÔNG hề cascade.** Cả ngày 20/07 chỉ có 5 dòng `fs err`, và nó tự dừng đúng thiết kế:
`fs finally outcome="giveup" n=3`. Cap `MAX_FORCESYNC_RETRIES` đã hoạt động. "Hở 3" trong §4c
là hư cấu.

**Poll 30s không hề chồng lấn.** 12 lần timeout cuối cách nhau **đúng 30.0s**, đều tăm tắp —
nếu có chồng lấn thì nhịp phải loạn và ngắn hơn 30s. Guard `isChecking` bên JS vốn đã tuần tự
hoá probe. §4b vẽ cảnh SSH chồng SSH là sai.

**Quan sát quyết định (không có trong log, do user cung cấp):** trên remote có **19 phiên
`claude` từ NHIỀU NGÀY TRƯỚC**, chiếm 6GB RAM + 4GB swap. Sự cố poll 24 phút không thể sinh ra
process nhiều ngày tuổi. Đây mới là dữ kiện phá vỡ toàn bộ giả thuyết §6.

### Nguyên nhân thật

1. **`force-sync-claudecode.sh` chạy 3 lệnh `claude -p` nối tiếp** (run_usage → probe →
   run_usage) nhưng đi qua `run_remote_script` = **cùng ngân sách 30s với một lần poll đơn**.
   Mỗi lệnh là một round-trip API thật. Force-sync **về mặt cấu trúc không thể xong trong 30s**
   → gần như lần nào cũng bị cắt giữa chừng.
2. **Không lệnh `claude` nào bị giới hạn thời gian phía remote** (`grep timeout` trong
   force-sync: 0 kết quả). Local cắt SSH, nhưng SIGHUP không xuyên qua `zsh -lc` tới `claude`.
   Một `claude` đang treo ở API → sống mãi mãi, giữ vài trăm MB.
3. **`pkill -f "claude -p"` CHƯA TỪNG MATCH GÌ.** Cmdline thật là
   `claude --model haiku -p /usage`; chuỗi con `"claude -p"` không tồn tại trong đó. Kiểm chứng:
   pattern match `claude -p foo` nhưng KHÔNG match `claude --model haiku -p /usage`. Cleanup
   im lặng không làm gì suốt từ ngày viết, mà vẫn mở thêm 1 SSH mỗi lần.
4. **Tích luỹ theo số lần khởi động app**, không theo thời gian. Force-sync chạy mỗi lần app
   start (`initialSyncDone = false`). Riêng 20/07 app restart 15 lần → khớp với 19 phiên.

Poll 30s ồn ào trong log nên trông giống thủ phạm. Nó có góp phần (mỗi lần cắt để lại
`claude auth status`, nhẹ) nhưng **không phải cái ăn 6GB**.

### Đã sửa (1.14.0)

| Sửa | Chỗ |
|-----|-----|
| Force-sync có ngân sách riêng 180s | `FORCE_SYNC_TIMEOUT_SECS`, `run_remote_script_long` |
| Mọi lệnh `claude` tự giới hạn 45s **trên remote** | `AKI_CLAUDE_TMO` trong preamble → 6 call site |
| Fallback `perl -e 'alarm shift; exec @ARGV'` | macOS không có `timeout`/`gtimeout` (đã verify) |
| Sửa pattern `pkill` sai | `ORPHAN_PATTERNS` |
| Cleanup có bound 8s, gộp 1 SSH | `wait_with_timeout` |
| Dừng hẳn poll sau 5 lần hỏng liên tiếp | `haltPolling()` trong `useAgentUsage.js` |
| `ConnectTimeout`/`ServerAlive`/`BatchMode` | `polling_ssh()` |

Điểm mấu chốt: **process tự kết thúc thì không cần dọn**. `pkill` giờ chỉ là lưới an toàn cho
host không có cả `timeout` lẫn `perl`, không còn là tuyến phòng thủ chính.

### Chưa kiểm chứng

Con số "2–8 orphan mỗi session" và "96–768 process" ở §4c/§9 là **suy diễn, chưa ai `ps` trên
`bien` để đo**. Con số đo thật duy nhất là 19 phiên / 6GB do user báo.
