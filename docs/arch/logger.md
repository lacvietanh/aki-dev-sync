# Kiến trúc Logger — `logger.rs`

Module logging dùng chung cho toàn bộ usage-data pipeline. Ghi vào file cố định trên disk; không phụ thuộc thư viện ngoài.

---

## Triết lý thiết kế

**Production im lặng, debug đầy đủ.**

Log file mặc định chỉ chứa lỗi thật và session boundary — không có verbose output trong chế độ bình thường. Developer bật `--debug` khi cần truy vết chi tiết.

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

Khi file vượt **1 MB** lúc startup: giữ lại **512 KB** gần nhất, tìm newline boundary để không cắt giữa dòng, ghi đè file. Không tạo file `.old` — log cũ bị xóa hoàn toàn.

```
file > 1MB → keep last 512KB → trim to next newline → overwrite
```

---

## API

```rust
logger::error(tag, msg)  // luôn ghi
logger::info(tag, msg)   // chỉ khi debug
logger::debug(tag, msg)  // chỉ khi debug
```

Hai IPC command cho frontend:
- `is_debug_mode()` → `bool`
- `get_log_path()` → `String`

---

## Format

```
[YYYY-MM-DD HH:MM:SS.mmm][TAG] message
```

Timestamp UTC (Rust). Tag = `STARTUP` / `GET_USAGE` / `FORCE_SYNC` / `PROVISION`.

---

## Level map hiện tại (`agent_usage.rs`)

**GET_USAGE** (mỗi poll tick ~30s):
- `debug`: start, ssh_result, stdout_preview, parse steps, rate_limits summary, done
- `info`: no cache file (null), STALE_RESET
- `error`: shell exit≠0, MTIME delimiter missing, json_parse fail, auth_inject fail

**FORCE_SYNC** (chỉ khi data null / STALE_RESET):
- `debug`: launching, ssh_result, diagnostic_raw, done
- `info`: start, diagnostic outcome (parsed/written/pct/resets_at), YEAR_FIX, SUCCESS
- `error`: empty stdout, parse_error, write fail, stdout not valid JSON

**PROVISION** (một lần per host session):
- `debug`: skip (not claudecode)
- `info`: start, exit/ok
- `error`: provision failed + stderr

---

## Liên quan

- `src-tauri/src/logger.rs` — implementation
- `src-tauri/src/agent_usage.rs` — caller duy nhất hiện tại
- `docs/arch/usage-claudecode.md` — §"Cách đọc log khi debug"
