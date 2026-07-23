# Unified Statusline Specification

> Tài liệu tham chiếu quy chuẩn kỹ thuật cho kịch bản hiển thị dải trạng thái (Statusline) dùng chung cho cả Antigravity (AGY CLI) và Claude Code CLI.

---

## 1. Luồng truyền dữ liệu JSON (Payload Flow Source)

Kịch bản không đọc file từ đĩa cứng mà nhận dữ liệu JSON trực tiếp từ tiến trình CLI qua luồng `stdin` (`input=$(cat)`):

- **Luồng AGY CLI:** Tiến trình binary `agy` (Go) (đọc `statusLine.command` từ `~/.gemini/antigravity-cli/settings.json`) ──(stdin)──► `~/.gemini/antigravity-cli/statusline.sh`
- **Luồng Claude Code CLI:** Tiến trình Node.js CLI (kích hoạt hook `statusLine` từ `~/.claude/settings.json`) ──(stdin)──► `~/.claude/statusline-command.sh`

---

## 2. Bảng ánh xạ JSON Payload (stdin) & Fallback Email

| Thông số | Nguồn AGY CLI | Nguồn Claude Code CLI | Cú pháp `jq` Unified |
|---|---|---|---|
| **CWD** | `.cwd` / `.workspace.current_dir` | `.cwd` | `.cwd // .workspace.current_dir // ""` |
| **Model** | `.model.id` / `.model.display_name` | `.model.id` / `.model.display_name` (**object**, xác minh 2026-07-23 trên CC 2.1.217 — không phải string) | `.model.display_name // .model.id // .model // ""` |
| **Effort** | `.model.effort` | `.effort.level` | `.model.effort // .effort.level // ""` |
| **Context** | `.context_window` | `.context_window` | `.context_window.total_input_tokens + .context_window.total_output_tokens` |
| **Cache** | `.context_window.current_usage` | `.context_window.current_usage` | `.cache_read_input_tokens` / `total_cache` |
| **Account** | `.account.email` | `.account.email` / `.user.email` | `.account.email // .user.email // .email // ""` |

### Cơ chế Fallback Account Email:
Không CLI nào đặt email vào payload, nên khi `JSON_ACCOUNT_EMAIL` rỗng phải fallback xuống đĩa —
**tách hẳn 2 nhánh theo CLI, không thử lần lượt**: máy có cả 2 file thì file nào tồn tại sẽ thắng và
tag hiện nhầm account của CLI kia.

| CLI | Nguồn fallback |
|---|---|
| Claude Code | `~/.claude.json` → `.oauthAccount.emailAddress` |
| AGY | `~/.gemini/google_accounts.json` → `.active` |

### Thời điểm reset quota — 2 CLI dùng 2 thang khác nhau:

| CLI | Field | Đơn vị |
|---|---|---|
| Claude Code | `.rate_limits.five_hour.resets_at` / `.seven_day.resets_at` | epoch tuyệt đối |
| AGY | `.quota[*].reset_in_seconds` | đếm ngược tương đối |

Script phải quy CC về cùng thang tương đối (`resets_at - now`) trước khi format. Bỏ sót bước này
chính là lý do statusline CC **chưa bao giờ** hiện ETA reset (bug phát hiện 2026-07-23).

---

## 3. Quy tắc phân nhánh Quota (Dynamic Quota Routing)

Phân loại đối tượng Quota dựa trên hai tầng kiểm tra:

```text
[Payload JSON từ stdin]
  │
  ├── 1. NẾU có .rate_limits (Claude Code CLI thật)
  │      ├─► 5h: .rate_limits.five_hour.used_percentage
  │      └─► 7d: .rate_limits.seven_day.used_percentage
  │
  └── 2. NẾU có .quota (AGY CLI)
         │
         ├── Tên Model chứa "gemini"
         │     ├─► 5h: .quota["gemini-5h"]
         │     └─► 7d: .quota["gemini-weekly"]
         │
         └── Tên Model KHÔNG chứa "gemini" (Model 3P: Claude/Llama...)
               ├─► 5h: .quota["3p-5h"]
               └─► 7d: .quota["3p-weekly"]
```

---

## 4. Thang màu động (Dynamic Color Ladder)

- **Context Window:** **Không in `%`.** Tô màu thẳng vào số token theo mốc **200,000 token (200k = 100%)**, 5 nấc: Xanh lơ ➔ Xanh lá ➔ Vàng ➔ Cam ➔ Đỏ.
- **RAM %:** Xám tĩnh (`$GREY`), **không** theo ladder — RAM toàn máy là bối cảnh, không phải thứ để leo thang cảnh báo giữa phiên.
- **Quota %:** Tô màu dynamic theo tỷ lệ Quota đã dùng (`color_for_pct`).
- **Cost USD:** Tô màu dynamic theo mức chi tiêu USD.
- **Cache %:** Sử dụng màu xám tĩnh cố định (`$GREY`), không tô màu dynamic.

---

## 5. Quy chuẩn hiển thị & ANSI Reset Pattern

- **Quy tắc ANSI Reset Pattern:**
  - Trong hàm `colored()`: Dùng `$RESET_FG` (`\033[22;39m`) để chỉ reset màu chữ và kiểu chữ (unbold), giữ nguyên màu nền.
  - Kết thúc mỗi khối: Dùng `$RESET_ALL` (`\033[00m`) để đưa terminal về trạng thái mặc định.
- **Cụm Tag & Account (Nền liền):**
  - AGY: `[Nền 255] AG (Blue 33 bold) [Nền 252] lacv (Xám 241 normal)`
  - Claude: `[Nền 255] CC (Orange 208 bold) [Nền 252] lacv (Xám 241 normal)`
  - Cụm này **không bao giờ** được đệm khoảng trắng, kể cả khi `SEPARATE_BLOCKS` bật.
- **Identity:** User (Trắng `97`), `@` (Xám `90`), Host (Trắng `97`).
- **CWD:** Bỏ màu nền, Magenta Bold (`\033[01;35m`), truncate theo `TRUNC_CWD` (default 10) — xem §8.2.
- **Model & Effort:** Bỏ chữ thương hiệu (`Gemini`, `Claude`) và ghi chú `(...)` ở cuối, tên gọn **dính liền** effort, không khoảng trắng (VD: `3.6Flashmed`). Bỏ một chữ ra khỏi giữa một id thô sẽ để lại dấu phân cách mồ côi (`gemini-2.5-flash` → `-2.5-flash`), nên `compact_model()` cắt luôn dấu thừa ở hai đầu và gộp các dấu bị dính đôi — một chỗ duy nhất lo việc này, không rải khắp nơi.
- **Session (`ss`):** Đọc thời gian vận hành của tiến trình Terminal (`$PPID`) trực tiếp từ Kernel OS (`ps -o etime=`). Không dùng file tạm.
- **Git Branch:** Truncate theo `TRUNC_BRANCH` (default 10) — xem §8.2.
- **RAM:** Biểu tượng `⚅`, tô màu theo % RAM hệ thống.
- **Cache:** Ký tự `↬` kèm % cache hit, màu xám tĩnh, đứng **ngay sau `context`** (xem §8.2).
- **Phân cách:** ~~dấu `|` màu vàng~~ — đã bỏ. Thay bằng nền zebra 2 sắc độ xám xen kẽ, xem §8.2.

---

## 6. Đường dẫn lưu trữ kịch bản trên máy local

- **AGY CLI Target:** `~/.gemini/antigravity-cli/statusline.sh` + khoá `statusLine` trong `~/.gemini/antigravity-cli/settings.json`
- **Claude Code Target:** `~/.claude/statusline-command.sh` + khoá `statusLine` trong `~/.claude/settings.json`

---

## 7. Hiện trạng thực địa trên máy dev (xác minh 2026-07-23, đọc runtime output — không đọc `statusline.rs`)

`diff` trực tiếp 2 file đã cài (342 dòng CC vs 330 dòng AGY) cho thấy **chỉ khác đúng 1 khối 12 dòng**
— chính khối `aki-rlcache` (persist `rate_limits`, chỉ CC cần). Toàn bộ phần còn lại — màu ANSI,
`cli_tag`, bảng jq trích field, quota branching, session/cache/git/account/RAM, assemble output —
**giống hệt nhau, byte-for-byte**.

> **Lịch sử — bảng dưới mô tả generator ở thời điểm 2026-07-23, TRƯỚC Phase 2.2.** Kiến trúc
> `StatuslineTarget` Adapter (mỗi target sinh một nội dung script riêng) đã bị xoá: giờ chỉ còn **một
> body duy nhất** ghi ra cả 2 đường dẫn, sinh bằng cách patch template — xem §8.3b. Giữ bảng này vì nó
> là bằng chứng thực địa dẫn tới quyết định đó.

Nội dung 2 file cài trên máy khi đó **không khớp** với hành vi mà generator (`StatuslineTarget`
Adapter trong `statusline.rs`) sinh ra:

| Điểm khác biệt | File đang cài trên máy | Generator hiện tại (theo docs) |
|---|---|---|
| Header comment | Cả 2 file đều ghi `(AGY CLI Target)`, kể cả file CC | Mỗi target phải tự ghi đúng tên mình |
| `aki-rlcache` | `v3` — không có 2 gate account+expiry | `v4` (DESIGN LOCK) — xem `docs/plan/1.18.0-statusline-apply-correctness.md` §P0-5 |
| Account fallback | **Cả 2 file dùng chung 1 nhánh** `~/.gemini/google_accounts.json` (kể cả file CC) | CC phải fallback `~/.claude.json` → `.oauthAccount.emailAddress` (P0-2); AGY fallback `google_accounts.json` (P0-3) — 2 nhánh khác nhau theo target |
| `cli_tag` | **Tự nhận diện lúc chạy** (comment tiếng Việt "Nhận diện tự động CLI"): `$JSON_MODEL` chứa `"claude"` hoặc `$JSON_CLAUDE_5H_USED != -1` → `CC`, ngược lại → `AG` | Field pin cứng theo target lúc generate, không tự suy luận từ payload (xem `docs/feat/statusline-customizer.md` §"pinned `cli_tag` field") |

**Kết luận:** file cài trên máy này thuộc dòng **sửa tay tiền-1.18.0** (đúng hiện tượng "3 bản script
khác nhau" mà `1.18.0-statusline-apply-correctness.md` §0 đã ghi nhận), **chưa từng được Apply lại**
bằng generator mới — khớp đúng với mục còn mở trong plan đó: *"step 5 (real Apply on the Mac +
`cargo test`) still pending"*.

**Hệ quả đang chạy thật trên máy, cho tới khi Apply lại:**
- Field Account trên statusline **Claude Code** (nếu bật) đang đọc `~/.gemini/google_accounts.json`
  thay vì `~/.claude.json` → hiển thị email tài khoản Google/AGY đang active, không phải tài khoản
  Claude — P0-2 coi như chưa có hiệu lực trên máy này dù plan ghi là đã fix ở tầng code.
- `aki-rlcache v3` không có 2 gate (account scope + expiry) → bug quota ma P0-5 (`7d 45%` từ account/
  session khác, đã hết hạn từ lâu) **vẫn có thể tái diễn** trên máy này.

---

## 8. Gộp thành 1 file vật lý — ĐÃ CHỐT LÀM CHUẨN, chờ port vào generator (Phase 2)

*Đã implement và verify sống bằng payload thật ngày 2026-07-23. Reference implementation:*
*`src-tauri/src/statusline-unified.sh`. Chưa đụng `statusline.rs` — việc port là Phase 2.*

### 8.1. Tín hiệu gate: đường dẫn gọi (`$0`), KHÔNG phải `.model | type`

```sh
case "$0" in
  */.gemini/*) CLI="AG" ;;
  *)           CLI="CC" ;;
esac
```

`CLI` quyết định 3 chỗ: (1) chạy hay bỏ khối `aki-rlcache`, (2) chọn nguồn fallback account, (3) nhãn
`CC`/`AG` trên tag.

**Vì sao bỏ đề xuất `.model | type == "object"` (nội dung cũ của mục này):** payload thật của Claude
Code 2.1.217 cho thấy `.model` của **CC cũng là object** — gate đó sẽ nhận nhầm CC thành AGY và tắt
rlcache đúng lúc CC cần. Đường dẫn cài đặt là dữ kiện chắc chắn, không phụ thuộc field nào trong
payload, nên không thể sai kiểu này.

Gate `$0` cũng thay luôn đoạn tự nhận diện cũ (`$JSON_MODEL` chứa `"claude"` → CC), vốn gắn nhãn sai
thành `CC` khi chạy model Claude **bên trong** AGY.

### 8.2. Default color & thứ tự field — giữ nguyên hiện trạng

Bảng dưới là **default đã chốt**, đúng bằng giá trị đang chạy — Phase 1 không đổi màu, không đổi vị
trí field nào.

Thứ tự khối: `tag` → `identity` → `cwd` → `model` → `context` → `cache` → `quota` → `session` → `git` → `ram`.

`cache` đứng ngay sau `context` vì hai số này đọc cùng nhau (bao nhiêu token, trong đó bao nhiêu %
đến từ cache). Model và effort **dính liền, không có khoảng trắng** (`Opus4.8med`).

| Thang | Ngưỡng | Màu |
|---|---|---|
| `color_for_pct` | `< 20` | `BOLD_BLUE` `\033[01;34m` |
| | `20-50` | `BOLD_GREEN` `\033[01;32m` |
| | `51-74` | `BOLD_YELLOW` `\033[01;33m` |
| | `75-89` | `BOLD_ORANGE` `\033[01;38;5;208m` |
| | `>= 90` | `BOLD_RED` `\033[01;31m` |

| Vai trò | Màu |
|---|---|
| Tag `CC` | nền `48;5;255`, chữ `1;38;5;208` (cam) |
| Tag `AG` | nền `48;5;255`, chữ `1;38;5;33` (xanh royal) |
| Tên account cạnh tag | nền `48;5;252`, `22m` (tắt bold) rồi màu người dùng chọn — default `GREY` `\033[90m` |
| Label (`ctx`, `5h:`, `ss`, `↬`, `⚅`) | `WHITE` `\033[97m` |
| Qualifier (effort, ETA, %cache, %ram) | `GREY` `\033[90m` |
| CWD | Magenta `\033[35m` |
| Model | Cyan `\033[36m` |
| Git branch | Magenta `\033[35m` |
| Nền zebra khối A | `48;5;16` (đen tuyệt đối) |
| Nền zebra khối B | `48;5;235` (xám rất tối) |

**Không còn ký tự phân cách.** Các khối tô nền xen kẽ 2 sắc độ (`16` / `235`), ranh giới
do chính chỗ đổi sắc độ vẽ ra — không `|`, không khoảng trắng đệm giữa 2 khối.

Chọn từ **dải xám trung tính 232-255**: xám không có hue nên không bao giờ chọi hue với màu chữ người
dùng tự đặt; chỉ còn phải canh độ sáng, tức việc kiểm tra rút về **một chiều** thay vì phải duyệt mọi
cặp nền×chữ. Lưu ý `BOLD_BLUE` (`01;34`, nấc thấp nhất của ladder) khá tối — nếu sau này nâng nền lên
sáng hơn `237` thì phải đổi nấc đó sang `38;5;33`.

Mốc thang: context tô màu theo **200k = 100%** (không hiện %), cost theo `COST_FULL_USD=30`.

`fmt_k` làm tròn về số nguyên, không phần thập phân: `126k`, `250k`, `1M` — không phải `125.2k` /
`250.0k` / `1.0M`.

#### Độ dài truncate — mỗi field một tham số riêng

Mỗi giá trị dưới đây là **một ô nhập số trên UI Customizer**. Sàn là 3 cho tất cả, nhưng
**trần không đồng nhất**: tên thư mục và tên branch cần nhiều chỗ hơn mới còn nhận ra được, nên 2 field
đó lên 15, còn lại giữ 12. Script giữ chúng thành biến riêng (`TRUNC_*`) ở đầu file, không nhúng thẳng
số vào chuỗi cắt, để generator chỉ phải thay 1 con số; script tự clamp phòng khi nhận giá trị hỏng.

| Field | Biến | Default | Miền |
|---|---|---|---|
| Account | `TRUNC_ACCOUNT` | **4** | 3..12 |
| User | `TRUNC_USER` | **5** | 3..12 |
| Host | `TRUNC_HOST` | **6** | 3..12 |
| CWD | `TRUNC_CWD` | **12** | 3..**15** |
| Git branch | `TRUNC_BRANCH` | **10** | 3..**15** |

**Account phải bỏ domain TRƯỚC khi cắt** (`${email%%@*}`). AGY gửi địa chỉ đầy đủ trong payload, nên
cắt thô 4 ký tự của `lva@akitao.com` ra `lva@` — vô nghĩa. Cắt local part trước rồi mới truncate:
`lva`.

### 8.3. Xác minh sống (2026-07-23, máy remote Linux, payload thật)

```
CC ntu_  guest@roscy-  Aki-Dev-Sync  Opus4.8med   ctx288k/1M    ↬100%  5h:19%0h5m               ss8h14m +840/-255 $29.18  master  ⚅24%
AG lva   guest@roscy-  Aki-Dev-Sync  Sonnet4.6     ctx125k/250k  ↬99%   5h:0%4h57m 7d:56%6d8h    ss0m                     master  ⚅23%
AG lva   guest@roscy-  Aki-Dev-Sync  3.6Flashmed   ctx133k/1M    ↬0%    5h:0%4h56m 7d:52%5d13h   ss0m                     master  ⚅23%
```

4 ca đã chạy bằng **payload thật cả hai CLI** (fixture §8.4-8.6):

| Ca | Kết quả |
|---|---|
| CC có `rate_limits` | ETA reset hiện đúng (`5h:6% 1h34m`) |
| CC thiếu `rate_limits` | Khôi phục đúng từ `rate-limits-cache.json` |
| AGY model 3P (Sonnet 4.6) | `7d:56%` ← `3p-weekly` (0.4397) ✓ routing đúng nhánh |
| AGY model Gemini | `7d:52%` ← `gemini-weekly` (0.4818) ✓, effort `med` từ `.model.effort` |

Đã kiểm chứng bằng md5 trước/sau: chạy nhánh AGY **không** đụng `~/.claude/rate-limits-cache.json`.

**Fixture AGY thật (§8.5-8.6) xác nhận thêm 2 điều quyết định:**
- **AGY cũng gửi `.model` dạng object.** Nên gate `.model | type` cũ không chỉ sai chiều mà **không
  phân biệt được gì cả** — cả 2 CLI đều object → luôn ra AG → rlcache không bao giờ chạy. Đây là bằng
  chứng cuối cùng cho việc phải gate bằng `$0`.
- **AGY có `email` ngay trong payload root** → nhánh AGY thực tế không cần đọc
  `google_accounts.json`; file đó chỉ còn là dự phòng. Chỉ CC mới thật sự phụ thuộc fallback đĩa.

### 8.3b. Hợp đồng generator (Phase 2.2 — ĐÃ CHỐT)

`src-tauri/src/statusline-unified.sh` **vừa là bản tham chiếu vừa là template**. `statusline.rs` nhúng nó
bằng `include_str!` và khi Apply chỉ thay **đúng vùng** giữa 2 marker:

```
# >>> AKI-GENERATED-CONFIG >>>
# <<< AKI-GENERATED-CONFIG <<<
```

Ngoài vùng đó, từng byte của script được ship nguyên vẹn. Hệ quả — và đây là lý do chọn kiểu này:

- **Không thể drift.** Không còn "script trong docs" và "script sinh ra từ Rust" là 2 thứ khác nhau.
  Test `generated_defaults_match_template` sinh script từ default của Vue và so **byte-for-byte** với
  chính file này; lệch một ký tự là fail.
- **Rust không giữ default nào.** `default_config()` và command `get_default_statusline_config` đã bị
  xoá. SSOT là `defaultLocalConfig()` trong `ClaudeSettingModal.vue`; vùng config trong file .sh chỉ
  là bản chép của chính giá trị đó để file vẫn chạy/test được độc lập.
- **Một body, hai đích.** Cùng một chuỗi script được ghi vào cả 2 đường dẫn, và **mỗi đích đều phải
  patch `settings.json` của CLI đó** — ghi file thôi là cài một nửa, CLI không chạy gì cho tới khi
  settings trỏ tới file. AGY mặc định có `statusLine: {type: "", command: "", enabled: true}` (sẵn
  sàng nhưng không trỏ vào đâu); bỏ nửa này chính là lý do Apply cho AGY từng ra statusline không
  bao giờ hiện. AGY nhận đường dẫn tuyệt đối, không dùng `~/…` vì không có gì bảo đảm nó expand tilde.

Vùng generated chứa đúng 6 nhóm biến, không có logic:

| Nhóm | Biến | Nguồn từ UI |
|---|---|---|
| Bật/tắt | `EN_<key>` (18 key) | checkbox từng field, **đã resolve dependency** |
| Màu | `COLOR_<key>` (6 key) | picker màu — đúng 6 key có picker thật |
| Ladder | `THRESH_GREEN/YELLOW/ORANGE/RED` | 4 ô ngưỡng, generator sort tăng dần |
| Truncate | `TRUNC_*` (5) | 5 ô số |
| Zebra | `BG_ZEBRA_A/B`, `SEPARATE_BLOCKS` | 2 swatch + checkbox separate |
| Thứ tự | `BLOCK_ORDER` | thứ tự kéo-thả hàng, gộp về tên khối |

**Gated fieldset — resolve ở generator, không ở shell.** `EN_effort`, `EN_rate_reset_*`,
`EN_cache_*` được Rust tính sẵn theo bảng `DEPENDS` (bản sao của `DEPENDS` trong Vue): cha tắt thì con
ra `0`. Config lưu của con **không bị sửa** — bật cha lại là con trở về đúng như cũ. Không gate nào
trong shell phải hỏi lại cha.

**Bất biến được test khoá lại** (cách chạy bộ test này trên máy không build được Tauri:
`docs/research/statusline-generator-test-suite.md`): mọi `EN_`/`COLOR_` mà thân script đọc đều
phải nằm trong `EN_KEYS`/`COLOR_KEYS` và ngược lại. Đây chính là cái bắt được bug `COLOR_cwd` (khai báo nhưng khối vẽ nhúng
cứng màu) — một picker chết trên UI.

### 8.4. Fixture payload thật — Claude Code 2.1.217

```json
{
  "cwd": "/home/guest/aki/app/Aki-Dev-Sync",
  "effort": { "level": "medium" },
  "model": { "id": "claude-opus-4-8[1m]", "display_name": "Opus 4.8 (1M context)" },
  "workspace": { "current_dir": "/home/guest/aki/app/Aki-Dev-Sync" },
  "version": "2.1.217",
  "cost": { "total_cost_usd": 0.2447, "total_duration_ms": 226383,
            "total_lines_added": 0, "total_lines_removed": 0 },
  "context_window": { "total_input_tokens": 39787, "total_output_tokens": 263,
                      "context_window_size": 1000000,
                      "current_usage": { "input_tokens": 2, "output_tokens": 263,
                                         "cache_creation_input_tokens": 862,
                                         "cache_read_input_tokens": 38923 } },
  "rate_limits": { "five_hour": { "used_percentage": 2, "resets_at": 1784767800 } }
}
```

Ca "CC thiếu `rate_limits`" = đúng payload trên, bỏ key `rate_limits`. Lưu ý khi bắt fixture mới:
dòng dump phải nằm **trước** khối rlcache merge, nếu không mọi dump đều có `rate_limits` kể cả turn
CLI không gửi.

---

## 8.5. Fixture payload thật — AGY CLI 1.1.5 (Mac, 2026-07-23)

```json
{
  "cwd": "/Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync",
  "session_id": "ad77823f-916e-41cc-ae6f-f496e76e0939",
  "model": { "id": "Claude Sonnet 4.6 (Thinking)", "display_name": "Claude Sonnet 4.6 (Thinking)" },
  "version": "1.1.5",
  "context_window": {
    "total_input_tokens": 92138,
    "total_output_tokens": 33015,
    "context_window_size": 250000,
    "used_percentage": 36.86,
    "remaining_percentage": 63.14,
    "current_usage": {
      "input_tokens": 601,
      "output_tokens": 179,
      "cache_creation_input_tokens": 0,
      "cache_read_input_tokens": 102680
    }
  },
  "quota": {
    "3p-5h":       { "remaining_fraction": 1,          "reset_time": "2026-07-23T09:36:50Z", "reset_in_seconds": 17848 },
    "3p-weekly":   { "remaining_fraction": 0.43969467, "reset_time": "2026-07-29T12:57:54Z", "reset_in_seconds": 548312 },
    "gemini-5h":   { "remaining_fraction": 1,          "reset_time": "2026-07-23T09:36:50Z", "reset_in_seconds": 17848 },
    "gemini-weekly":{ "remaining_fraction": 0.4817584, "reset_time": "2026-07-28T17:59:58Z", "reset_in_seconds": 480036 }
  },
  "agent_state": "tool_use",
  "plan_tier": "Google AI Pro",
  "email": "lva@akitao.com",
  "vcs": { "type": "git" },
  "sandbox": { "enabled": false },
  "terminal_width": 152
}
```

**Điểm khác biệt so với fixture CC (8.4):**
- `email` có mặt ở root payload → **không cần fallback file** (khác CC)
- Không có trường `effort` (model `Claude Sonnet 4.6 (Thinking)` không gửi effort)
- `quota` dùng `reset_in_seconds` (tương đối), CC dùng `resets_at` (epoch tuyệt đối)
- Không có `rate_limits` (CC dùng), thay bằng `quota` phân theo `3p-*` / `gemini-*`
- Model là object `{id, display_name}` — xác nhận thêm rằng cả 2 CLI đều gửi model dạng object

---

## 8.6. Fixture payload thật — AGY CLI 1.1.5 với Model Gemini + Effort (2026-07-23)

```json
{
  "cwd": "/Volumes/DEV/Frameworks/Tauri/Aki-Dev-Sync",
  "session_id": "ad77823f-916e-41cc-ae6f-f496e76e0939",
  "model": {
    "id": "Gemini 3.6 Flash (Medium)",
    "display_name": "Gemini 3.6 Flash (Medium)",
    "effort": "medium"
  },
  "version": "1.1.5",
  "context_window": {
    "total_input_tokens": 97441,
    "total_output_tokens": 35195,
    "context_window_size": 1048576,
    "used_percentage": 9.29,
    "remaining_percentage": 90.71,
    "current_usage": {
      "input_tokens": 91499,
      "output_tokens": 372,
      "cache_creation_input_tokens": 0,
      "cache_read_input_tokens": 0
    }
  },
  "quota": {
    "3p-5h":        { "remaining_fraction": 1,          "reset_time": "2026-07-23T09:36:50Z", "reset_in_seconds": 17765 },
    "3p-weekly":    { "remaining_fraction": 0.43969467, "reset_time": "2026-07-29T12:57:54Z", "reset_in_seconds": 548229 },
    "gemini-5h":    { "remaining_fraction": 1,          "reset_time": "2026-07-23T09:36:50Z", "reset_in_seconds": 17765 },
    "gemini-weekly": { "remaining_fraction": 0.4817584, "reset_time": "2026-07-28T17:59:58Z", "reset_in_seconds": 479953 }
  },
  "agent_state": "tool_use",
  "plan_tier": "Google AI Pro",
  "email": "lva@akitao.com",
  "vcs": { "type": "git" },
  "sandbox": { "enabled": false },
  "terminal_width": 152
}
```

**Điểm xác nhận quan trọng:**
- Dòng model Gemini gửi thêm trường `"effort": "medium"` nằm bên trong `.model` object: `.model.effort`.
- Tên `display_name` chứa cả chuỗi `(Medium)` ở cuối: `"Gemini 3.6 Flash (Medium)"`.

---

## 8-cũ. (lưu vết) Đề xuất ban đầu — đã bị bác bỏ

*Giữ lại để hiểu vì sao gate đổi. Kết luận của mục này SAI, xem 8.1.*

Vì 2 file hiện tại giống nhau ~97% (mục 7), việc gộp thành **đúng 1 file** dùng chung cho cả 2 đường
dẫn cài đặt là khả thi. Rào cản duy nhất còn lại: khối `aki-rlcache` ghi **không điều kiện** vào
`$HOME/.claude/rate-limits-cache.json` — nếu dùng chung 1 file vật lý, mỗi lần AGY gọi script cũng sẽ
đụng vào cache của Claude Code (vô hại về mặt dữ liệu vì payload AGY không có `.rate_limits`, nhưng
sai về mặt ngữ nghĩa: 1 process AGY không nên ghi file thuộc về CC).

**Tín hiệu gate đề xuất — kiểm tra `.model | type`:**
- AGY luôn gửi `.model` dạng **object** (`.model.id` / `.model.display_name`).
- Claude Code luôn gửi `.model` dạng **string** (theo bảng ánh xạ mục 2).
- → `if (.model | type) == "object")` → đang chạy dưới AGY → **bỏ qua** khối rlcache; ngược lại (string
  hoặc null) → đang chạy dưới CC → chạy rlcache như cũ.

**Vì sao không gate theo `.rate_limits` (trực giác ban đầu nhưng sai):** CC chỉ gửi `.rate_limits`
"trên một số turn" (xem `docs/arch/usage-claudecode.md` §1) — chính những turn **thiếu** `.rate_limits`
lại là lúc rlcache cần chạy nhất (để phục hồi số liệu cũ từ file cache). Gate theo sự hiện diện của
`.rate_limits` sẽ tắt đúng khối cần bật nhiều nhất. `.model` type ổn định qua mọi turn của cả 2 CLI,
không phụ thuộc việc quota có mặt hay không — đây là lý do nó là tín hiệu đáng tin hơn.

Việc này **không** thay đổi kiến trúc `StatuslineTarget` Adapter (vẫn 1 config Rust, vẫn có khái niệm
2 target) — chỉ thay đổi ở khâu cuối: thay vì sinh 2 nội dung script khác nhau cho 2 target, sinh
**1 nội dung duy nhất** (đã gate rlcache theo `.model` type) rồi ghi ra cả 2 đường dẫn.

> **Kết cục (Phase 2.2):** ý "1 nội dung duy nhất ghi ra 2 đường dẫn" đã được giữ lại và làm tới cùng —
> nhưng gate là `$0` chứ không phải `.model | type` (payload thật cho thấy **cả 2 CLI** đều gửi `.model`
> dạng object nên gate đó không phân biệt được gì), và trait `StatuslineTarget` đã bị xoá hẳn: hai
> target giờ chỉ khác nhau ở đoạn cài đặt (đường dẫn + patch `settings.json`), không còn khác ở nội dung
> script. Xem §8.1 và §8.3b.
