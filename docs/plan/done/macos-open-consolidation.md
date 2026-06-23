# Plan: macOS Open Consolidation

**Status:** Done  
**Created:** 2026-06-23  
**Completed:** 2026-06-23

---

## Vấn đề

`system.rs` hiện có nhiều Tauri IPC commands chỉ là thin wrapper quanh lệnh `open` của macOS:

| Command | Thực ra làm gì |
|---|---|
| `open_url(url)` | `open <url>` |
| `open_local_dir(path)` | `open -R <path>` |
| `open_in_terminal(path)` | `open -a Terminal <path>` |
| `open_antigravity_app()` | `open -a Antigravity` |
| `open_ide_local("finder", path)` | `open <path>` |
| `open_ide_local("terminal", path)` | `open -a Terminal <path>` |
| `open_ide_local("vscode", path)` | `open -a "Visual Studio Code" <path>` |
| `open_ide_local("vscode_insiders", path)` | `open -a "Visual Studio Code - Insiders" <path>` |
| `open_ide_local("antigravity", path)` | `open -a Antigravity <path>` |
| `open_ide_remote("vscode", host, path)` | `open "vscode://vscode-remote/ssh-remote+..."` |
| `open_ide_remote("vscode_insiders", host, path)` | `open "vscode-insiders://vscode-remote/ssh-remote+..."` |

Mỗi case chỉ build arguments khác nhau cho cùng một binary `open`. Đây là logic thuần JS, không cần ở Rust.

---

## Thiết kế

### Nguyên tắc phân loại

**Giữ ở Rust** — khi có complexity thực sự cần Rust:
- AppleScript string construction + escaping (injection risk)
- Subprocess CLI không phải `open` (`antigravity-ide --remote`)
- Filesystem check (`check_ide_availability`)

**Chuyển về JS** — khi chỉ là build args cho `open`:
- Tất cả `open -a <app> <path>`, `open <url>`, `open <path>`
- VSCode Remote URL scheme (`vscode://...`) — chỉ là string building

### Rust sau refactor

**Giữ nguyên:**
- `check_ide_availability()` — filesystem check
- `open_remote_terminal(host, path)` → đổi tên thành `open_ide_remote` hoặc giữ nguyên
- `open_ide_remote("terminal", ...)` — AppleScript SSH
- `open_ide_remote("antigravity", ...)` — `antigravity-ide --remote` CLI

**Thêm mới:**
```rust
#[tauri::command]
pub fn macos_open(args: Vec<String>) -> Result<(), String> {
    Command::new("open")
        .args(&args)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}
```

**Xóa:**
- `open_url`
- `open_local_dir`
- `open_in_terminal`
- `open_antigravity_app`
- `open_ide_local` (toàn bộ)
- `open_ide_remote` arms: `vscode`, `vscode_insiders` (chuyển về JS)

Nếu `open_ide_remote` còn lại chỉ có `terminal` + `antigravity`, đổi tên thành `open_remote_subprocess` cho rõ nghĩa hơn là "mở subprocess không phải dạng open URL/path".

### JS sau refactor

`ProjectTable.vue` — thay `openIdeLocal` và `openIdeRemote`:

```js
// Toàn bộ LOCAL cases — chỉ build args
const IDE_LOCAL_ARGS = {
  finder:          path => [path],
  terminal:        path => ['-a', 'Terminal', path],
  vscode:          path => ['-a', 'Visual Studio Code', path],
  vscode_insiders: path => ['-a', 'Visual Studio Code - Insiders', path],
  antigravity:     path => ['-a', 'Antigravity', path],
}

async function openIdeLocal(ideName, path) {
  const args = IDE_LOCAL_ARGS[ideName]?.(path)
  if (args) await invoke('macos_open', { args })
}

// REMOTE — split: URL scheme về macos_open, subprocess về Rust
async function openIdeRemote(ideName, host, path) {
  if (ideName === 'vscode') {
    await invoke('macos_open', { args: [`vscode://vscode-remote/ssh-remote+${host}${path}`] })
  } else if (ideName === 'vscode_insiders') {
    await invoke('macos_open', { args: [`vscode-insiders://vscode-remote/ssh-remote+${host}${path}`] })
  } else {
    // terminal (AppleScript) + antigravity (CLI) — vẫn cần Rust
    await invoke('open_remote_subprocess', { ideName, host, path })
  }
}

// URL chung — dùng macos_open thay vì invoke open_url
async function openUrl(url) {
  await invoke('macos_open', { args: [url] })
}
```

`AgentUsage.vue` — thay `invoke("open_antigravity_app")`:
```js
await invoke('macos_open', { args: ['-a', 'Antigravity'] })
```

`GitModal.vue` — thay `invoke('open_url', { url })`:
```js
await invoke('macos_open', { args: [url] })
```

---

## Files thay đổi

| File | Thay đổi |
|---|---|
| `src-tauri/src/system.rs` | Thêm `macos_open`; xóa 5 commands; đổi tên/trim `open_ide_remote` |
| `src-tauri/src/lib.rs` | Update `generate_handler!` |
| `src/components/ProjectTable.vue` | `openIdeLocal`, `openIdeRemote`, `openUrl` rewrite |
| `src/components/modals/GitModal.vue` | `openUrl` → `macos_open` |
| `src/components/AgentUsage.vue` | `open_antigravity_app` → `macos_open` |

---

## Phạm vi KHÔNG nằm trong plan này

- Cross-platform support (Windows/Linux) — chỉ macOS, đúng với hiện tại
- `tauri-plugin-opener` — không dùng (adds dependency, cross-platform overhead không cần thiết)
- Bất kỳ thay đổi logic sync, git, SSH nào

---

## Verification

- `cargo build` trong `src-tauri/` (hoặc `rustfmt --check` nếu môi trường thiếu GTK)
- Kiểm tra `generate_handler!` khớp với functions còn lại trong `system.rs`
- Click "Finder", "Terminal", "VSCode" trong hub → mở đúng app
- Click icon Antigravity trong AgentUsage panel → mở Antigravity
- Click Remote Git URL trong GitModal → mở browser
- Click "Open Production Site" trong hub → mở browser
