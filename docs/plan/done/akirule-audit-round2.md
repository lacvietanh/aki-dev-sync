# Audit Round 2 — akirule full-pass (post v1.1.0)

> **✅ HOÀN TẤT — 15/15 items · Released: v1.1.1**

> Nguồn: phân tích độc lập bởi Claude Opus 4.8, sau đó được review và đánh giá lại bởi Sonnet 4.6.
> Phạm vi: toàn bộ bộ akirule (RULE-coding, RULE-agent-behavior, METHOD-flow-audit).
> Tất cả mục đều Verified (đọc trực tiếp source). Build chỉ có thể verify trên macOS.

---

## Thứ tự thực hiện

`B1 → A1+A2+C2+D2+E1 (sync.rs rewrite) → D1 → A5 (projects.rs rewrite) → A3+C1 (git.rs rewrite) → A4+F1 (ssh.rs rewrite) → F2 (agent_usage.rs) → F1 (system.rs rewrite)`

---

## Checklist

### B — C6 làm dở
- [x] **B1** Tách `get_claudecode_usage` shell script → `scripts/get-claudecode-usage.sh` + `include_str!`.
      Tất cả 4 script giờ đều là external files, C6 thực sự hoàn tất.

### A — DRY
- [x] **A1** `spawn_and_stream(cmd, window, project_id, label)` — gộp pattern spawn→take→stream→join→wait→check.
      `execute_hook` và rsync block trong `run_sync` đều dùng chung.
- [x] **A2** `run_hook_phase(window, project, cmd, dry_run, dry_prefix, phase_name)` — gộp pre/post hook block.
      `run_sync` gọi 2 lần với `pre_cmd`/`post_cmd`.
- [x] **A3** `git_capture(path, args) -> Option<String>` — private helper trong git.rs.
      5 lần lặp `Command::new("git")...output()` → 1 helper.
- [x] **A4** `ssh_config_path() -> Result<PathBuf, String>` — private helper trong ssh.rs.
      4 lần `home.join(".ssh").join("config")` → 1 helper.
- [x] **A5** `pub fn validate_path_segment(label, s) -> Result<(), String>` trong projects.rs.
      `validate_project` và `validate_specific_paths` (sync.rs) đều dùng chung.

### D — Error handling
- [x] **D1** `load_projects`: `if let Ok(...)` → `serde_json::from_str(...).map_err(...)` propagate thật.
      Corrupt JSON giờ trả về Err rõ ràng thay vì Ok(vec![]) im lặng.
- [x] **D2** `ssh mkdir` exit status được check; `create_dir_all` propagate error.
      Cả 2 nhánh push/pull nhất quán: fail → Err rõ ràng.

### C — SRP
- [x] **C1** `get_project_files` move từ projects.rs sang git.rs (cùng chỗ với git porcelain parsing).
      `lib.rs` cập nhật: `git::get_project_files` thay vì `projects::get_project_files`.
- [x] **C2** `build_rsync_args(project, is_push, dry_run, specific_paths, sync_git, src, dest)` extracted.
      `run_sync` từ ~150 dòng → ~60 dòng, args-building riêng testable.

### E — Comment kể lịch sử
- [x] **E1** sync.rs: xóa 2 comment kể lịch sử refactor. Code tự mô tả hành vi hiện tại.

### F — Convention / correctness
- [x] **F1** `#[cfg(test)]` modules xuống đáy: projects.rs, sync.rs, system.rs đều đúng thứ tự.
      (ssh.rs không có tests — không cần.)
- [x] **F2** JSON mutation: `content.pop()` + `format!` → `serde_json::Value` insert field.
      An toàn với values chứa dấu `"`.

### Ghi chú — KHÔNG làm
- **A6** `if !path.exists() { Err }`: Rust idiom, bỏ qua.
- `unwrap_or_default()` trong `swap_ssh_state`: thêm comment giải thích intentional.
- Enum Agent: giữ magic string "claudecode"/"antigravity".
