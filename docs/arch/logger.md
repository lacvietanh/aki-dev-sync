# Kiến trúc Logger - `logger.rs`

> updated 2026-08-16 · v1.24.0

Module logging dùng chung cho toàn bộ usage-data pipeline. Ghi vào file cố định trên disk; không phụ thuộc thư viện ngoài.

---

## Triết lý thiết kế

**Production im lặng, debug đầy đủ.**

Log file mặc định chỉ chứa lỗi thật và session boundary - không có verbose output trong chế độ bình thường. Developer bật `--debug` khi cần truy vết chi tiết.

---

## Ba level

| Level | Ghi file | Ghi stderr | Dùng cho |
|-------|----------|------------|----------|
| `error` | luôn | luôn | Lỗi thật: shell chết sớm, parse fail, write fail, SSH fail, data loss risk |
| `info` | debug-only | debug-only | Key lifecycle: start, done, STALE_RESET, force-sync outcome |
| `debug` | debug-only | debug-only | Per-poll detail, parse internals, shell stderr lines |

`[STARTUP]` là ngoại lệ: **luôn ghi file** (session boundary marker) nhưng stderr chỉ khi debug.

---

## Kích hoạt debug mode

```bash
# Flag
/Applications/Aki\ Dev\ Sync.app/Contents/MacOS/aki-dev-sync --debug

# Env var
AKI_DEBUG=1 /Applications/Aki\ Dev\ Sync.app/Contents/MacOS/aki-dev-sync
```

---

## Log file location

```
# macOS
~/Library/Application Support/aki.devsync/usage.log

# Linux
~/.local/share/aki.devsync/usage.log
```

Cùng thư mục với `projects.json`. Path chính xác được in vào DevTools F12 lúc startup:

```
[YYYY-MM-DD HH:MM:SS.mmm][USAGE:init] log_file=<path>
```

---

## Auto-truncate

Khi file vượt **1 MB**: giữ lại **512 KB** gần nhất, tìm newline boundary để không cắt giữa dòng, ghi đè file. Không tạo file `.old` - log cũ bị xóa hoàn toàn.

Kiểm tra chạy **lúc startup và trong khi ghi**, không chỉ startup. Một phiên `--debug` dài không bao giờ restart, nên nếu chỉ kiểm lúc khởi động thì file cứ thế phình ra. Để không phải `stat` mỗi dòng, `append_line` đếm số byte đã ghi và chỉ kiểm khi vượt ngưỡng 64 KB kể từ lần kiểm trước.

```
mỗi lần ghi → cộng dồn byte → qua 64KB thì kiểm → file > 1MB → giữ 512KB cuối → cắt tại newline → ghi đè
```

---

## API

```rust
logger::error(tag, msg)  // luôn ghi
logger::info(tag, msg)   // chỉ khi debug
logger::debug(tag, msg)  // chỉ khi debug
```

Ba IPC command cho frontend:
- `is_debug_mode()` → `bool`
- `get_log_path()` → `String`
- `log_frontend(level, tag, msg)` → forward log từ frontend vào cùng pipeline (usage.log + stderr)

---

## Frontend Logging (`usageMonitor.js`)

Mỗi `ulog(event, fields, level)` thực hiện **hai hành động song song**:

1. **Print ngay ra Webview DevTools console** (trước khi gửi IPC) - giữ nguyên source-line link trong Chrome DevTools.
   - `'error'` → `console.error` - **luôn** hiện, kể cả khi tắt debug
   - `'info'` → `console.info` - chỉ khi `_isDebugMode = true`
   - `'debug'` → `console.log` - chỉ khi `_isDebugMode = true`

2. **Forward về Rust qua `invoke('log_frontend')` (fire-and-forget)** → ghi vào `usage.log` + in ra stderr khi `--debug`, xen kẽ đúng thứ tự thời gian với các log entry Rust.

Trong production (không có `--debug`): Webview console im lặng hoàn toàn, chỉ error thật mới hiện.

---

## Format

```
[YYYYMMDD.HHMMSS.mmm][TAG] message
```

Timestamp UTC (Rust), local time (JS). Compact format - ~10 bytes saved per line vs old `YYYY-MM-DD HH:MM:SS.mmm`.
Tag = `GET_USAGE` / `PROVISION` (Rust, cộng `SHELL:*` relay từ stderr của script) or `USAGE:<monitorId>` (frontend) - `monitorId` = `<agentId>@<host>`, ví dụ `USAGE:claudecode@local`, `USAGE:antigravity@hostB`.

---

## Level map hiện tại (`agent_usage/` + `remote_shell.rs`)

**GET_USAGE** (mỗi poll tick ~30s):
- `debug`: start, ssh_result, stdout_preview, parse steps, rate_limits summary, done
- `info`: no cache file (null), STALE_RESET
- `error`: shell exit≠0, MTIME delimiter missing, json_parse fail, auth_parse fail

**PROVISION** (một lần per host session):
- `debug`: skip (not claudecode)
- `info`: start, exit/ok
- `error`: provision failed + stderr

---

## Liên quan

- `src-tauri/src/logger.rs` - implementation: `error`, `info`, `debug`, `log_frontend`
- `src-tauri/src/remote_shell.rs` + `src-tauri/src/agent_usage/` — primary callers
- `src/composables/usageMonitor.js` - frontend logger (`makeLogger`, `ulog`, dual-path); the tag carries the monitor's full identity (`USAGE:claudecode@hostB`) so two hosts are separable in one log
- `docs/arch/usage-claudecode.md` - §“Cách đọc log khi debug”
