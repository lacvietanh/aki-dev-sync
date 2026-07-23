# Bộ test tự động cho generator statusline

> **Bối cảnh:** Phase 2.2 của [`docs/plan/statusline-true-unify.md`](../plan/statusline-true-unify.md)
> **Nằm ở:** `#[cfg(test)]` trong `src-tauri/src/statusline.rs` — không file phụ nào.
> **Chạy:** `npm run test:statusline`

## 1. Nó thay thế việc gì

Trước đó, cách duy nhất để biết một ô config có thực sự ra file hay không là: mở modal Statusline
Customizer → tick/bỏ tick một ô → Apply → soi file `.sh` hoặc nhìn statusline thật. Mỗi ô một vòng.
18 công tắc + 6 picker màu + 5 độ cắt + 4 ngưỡng + 2 sắc zebra là **quá nhiều để làm bằng tay**, nên
thực tế chỉ vài ô hay dùng được kiểm — và đó chính là cách picker màu CWD ship ra mà **chết hẳn**
(UI có ô chọn, generator không hề phát ra `COLOR_cwd`, không ai phát hiện).

Bộ test làm đúng vòng đó, tự động, cho **mọi** ô:

```
JSON của Vue (nguyên văn defaultLocalConfig)
   → serde deserialize             ← đúng payload IPC gửi sang
   → generate_statusline_script()  ← đúng hàm Apply gọi
   → bash <script> < payload       ← chạy thật, có fixture HOME giả
   → assert trên DÒNG ĐÃ RENDER    ← thứ người dùng nhìn thấy
```

Không mock khâu nào. Khác Apply thật ở đúng hai chỗ: không ghi vào `~/.claude/`, không đẩy qua SSH.

```bash
npm run test:statusline              # tất cả
npm run test:statusline -- toggle    # lọc theo tên
```

## 2. Bảo chứng những gì

### 2.1. Config trên UI → ra đúng file (nhóm chính, đọc JSON thật của Vue)

| Test | Bảo chứng |
|---|---|
| `the_ui_payload_reproduces_the_template` | JSON mặc định của customizer dựng lại **byte-for-byte** đúng script đã commit |
| `every_toggle_flips_its_own_output_and_nothing_else` | **Cả 18 công tắc**, lật từng cái: chữ nó sở hữu chỉ xuất hiện khi bật, biến mất khi tắt, và dòng phải đổi thật |
| `the_numeric_and_color_settings_reach_the_rendered_line` | 5 độ cắt, picker màu, 2 sắc zebra, `separate`, thang ngưỡng — kiểm ngay trên escape code |
| `dragging_a_row_reorders_the_rendered_line` | Kéo RAM lên đầu thì nó in trước `ctx` |
| `a_field_the_backend_does_not_know_is_ignored_not_fatal` | UI thêm key mới không làm gãy Apply của bản Rust cũ |
| `a_missing_section_is_rejected_rather_than_silently_defaulted` | Payload thiếu `trunc` → **fail to lên**, đúng chủ ý bỏ `#[serde(default)]` |

Bảng công tắc có **guard tự mở rộng**: nó duyệt `EN_KEYS` và fail nếu một gate mới thêm vào template
chưa có dòng tương ứng. Thêm công tắc mà quên test là biết ngay, không im lặng lọt.

Kiểm **hai chiều**: ô bật sẵn kiểm theo hướng "tắt thì mất", ô tắt sẵn (`cache_tokens`) kiểm theo
hướng "bật thì hiện" — chỉ kiểm một chiều thì một công tắc chết vẫn núp được sau cái default không
bao giờ render nó.

### 2.2. Bất biến của generator

| Test | Bảo chứng |
|---|---|
| `generated_defaults_match_template` | Vùng generated đã commit khớp đúng output của generator |
| `generated_script_is_valid_shell` | `bash -n` trên script sinh ra |
| `every_gate_the_template_reads_is_generated` | Không có `EN_*`/`COLOR_*` nào template đọc mà generator không phát — và ngược lại, không có biến chết |
| `a_disabled_parent_switches_its_children_off` | `DEPENDS` cho ra `EN_child=0` mà **không** ghi đè state đã lưu của con |
| `block_order_follows_the_field_order` | `BLOCK_ORDER` bám thứ tự field |
| `out_of_range_values_are_clamped_not_defaulted` | Giá trị ngoài dải bị **clamp**, không rơi về default (module này không giữ default nào) |
| `no_target_selected_is_an_error_not_a_silent_agy_write` | Không tick host nào → lỗi, không âm thầm ghi AGY |
| `both_targets_receive_the_same_body` | CC và AGY nhận **cùng một** thân script |

### 2.3. Hành vi runtime của script (chạy bash thật)

| Test | Bảo chứng |
|---|---|
| `renders_a_line` / `agy_renders_a_line` | Cả 2 CLI ra dòng hợp lệ, không có phần trăm âm |
| `the_vendor_word_leaves_no_stray_punctuation_behind` | Bỏ chữ thương hiệu khỏi id thô không để lại dấu mồ côi (`gemini-2.5-flash` → `2.5-flash`) |
| `cc_account_falls_back_to_claude_json` | Account CC lấy từ `~/.claude.json` khi payload không có |
| `agy_account_falls_back_to_google_accounts_object` | Đường fallback tương ứng phía AGY |
| `cc_rate_limits_survive_a_payload_that_omits_them` | Payload thiếu quota không xoá số đang hiển thị |
| `cc_rate_limits_merge_instead_of_overwrite` | Cache merge, không đè |
| `cc_drops_a_cached_quota_whose_reset_has_passed` | Gate hết hạn của `aki-rlcache` v4 (DESIGN LOCK) |
| `cc_ignores_a_cache_written_by_another_account` | Gate theo account của `aki-rlcache` v4 (DESIGN LOCK) |
| `cc_survives_a_corrupt_rate_limits_cache` | Cache hỏng không làm trắng statusline |
| `agy_never_touches_the_claude_rate_limit_cache` | AGY không ghi vào cache của CC |
| `agy_reset_eta_includes_minutes` | 5400s ra `1h30m`, không phải `1h0m` |
| `a_disabled_reset_hides_only_the_eta` | Tắt ETA thì chỉ mất ETA, số quota vẫn còn |

## 3. Nó đã bắt được gì

**Bug thật, đủ sức làm statusline trắng trơn** — lộ ngay lần chạy đầu:

```
$ echo '{"model":"gemini-2.5-flash"}' | jq -r '(.model.display_name // .model.id // .model // "")'
jq: error: Cannot index string with string "display_name"
```

Khi `.model` là **string** (AGY bản cũ, hoặc payload rút gọn), biểu thức này abort **cả chương trình
jq** chứ không chỉ một field → mọi `JSON_*` rỗng → statusline trắng, và trên màn hình không có manh
mối nào để đoán nguyên nhân. Biểu thức đó bị lặp **4 lần** ở các dòng routing quota. Sửa bằng cách
bind một lần rồi mới dùng — hết crash và hết duplicate cùng lúc:

```jq
(if (.model | type) == "object" then (.model.display_name // .model.id // "") else (.model // "") end | tostring) as $model_name
| (if ($model_name | ascii_downcase | contains("gemini")) then "gemini" else "3p" end) as $pool
| (if (.quota | type) == "object" then .quota else {} end) as $quota
| def qfrac($k): $quota[$k] as $v
    | if ($v|type)=="object" then ($v.remaining_fraction // -1) elif ($v|type)=="number" then $v else -1 end;
  def qreset($k): $quota[$k] as $v
    | if ($v|type)=="object" then ($v.reset_in_seconds // 0) else 0 end;
```

`qfrac`/`qreset` đóng luôn biến thể cùng loại: quota entry dạng số trần (`{"gemini-5h": 0.25}`) —
`.remaining_fraction` trên một số cũng abort y hệt.

Đáng chú ý: **không kiểm tra tĩnh nào bắt được bug này.** `cargo check` cũng mù vì lỗi nằm trong
shell. Chỉ *chạy script với payload thật* mới lộ — nên bộ test phải chạy `bash` thật, không mock.

## 4. Nó KHÔNG chứng minh được gì

- **Đường ghi file và SSH của Apply.** Test dừng ở nội dung script; việc đẩy tới host thật ngoài phạm vi.
- **Bản thân UI Vue.** Test khoá cái JSON mà UI *gửi*, không khoá việc UI có gửi đúng cái đó không —
  đó là lý do `VUE_DEFAULT_JSON` phải copy **nguyên văn** từ `defaultLocalConfig()`; sửa default bên
  Vue thì phải sửa chuỗi này, và đó là chủ ý.
- **Quyền IPC** trong `capabilities/default.json`.

> **Ghi chú cho máy remote:** máy Linux này không build được crate `src-tauri` (thiếu
> `webkit2gtk-4.1`/`libsoup-3.0`) nên `npm run test:statusline` chỉ chạy trên Mac. Muốn chạy tạm ở
> remote thì dựng một crate rác **ngoài repo**, symlink `statusline.rs` + `statusline-unified.sh`
> vào, stub 4 ký hiệu Tauri (`logger::info`, `run_remote_script_bounded`, `#[tauri::command]`
> pass-through, `spawn_blocking`) — ~55 dòng. Đó là công cụ tình thế của người sửa code, **không
> phải thứ nên nằm trong repo**.

---

## Tham chiếu chéo

| Tài liệu | Quan hệ |
|---|---|
| [`docs/plan/statusline-true-unify.md`](../plan/statusline-true-unify.md) | Plan sinh ra công việc này; §2.2 là phần bộ test phục vụ |
| [`docs/feat/statusline-customizer.md`](../feat/statusline-customizer.md) | Mô tả tính năng + bảng Verification |
| [`docs/ref/statusline-unified-spec.md`](../ref/statusline-unified-spec.md) | §8.3b hợp đồng generator — thứ `the_ui_payload_reproduces_the_template` khoá lại |
| [`docs/arch/usage-claudecode.md`](../arch/usage-claudecode.md) | `aki-rlcache` phía ghi; 2 gate DESIGN LOCK được §2.3 bảo chứng |
| [`docs/plan/1.18.0-statusline-apply-correctness.md`](../plan/1.18.0-statusline-apply-correctness.md) | Ma trận bug gốc: 4 bug generator mà bộ test này chặn tái diễn |
