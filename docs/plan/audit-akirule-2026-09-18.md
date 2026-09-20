# Audit akirule — 2026-09-18

## Mục tiêu

Lập kế hoạch xử lý các vi phạm akirule đã được quét cơ học trên toàn repo và rà soát độc lập. Audit chỉ ghi nhận và lên lịch; không sửa code/config/docs đang được audit.

## Baseline và phạm vi

- Baseline audit: working tree ngày 2026-09-18, app version `1.30.0`.
- Tracked changes đã tồn tại trong lúc audit phải được bảo toàn và không được tự phân loại: `docs/plan/backlog.md`, `src/composables/useTerminalTabs.js`; các thay đổi xuất hiện sau đó cũng phải được coi là owner work cho tới khi được xác nhận.
- Phạm vi: tracked Vue/CSS/JS/Rust, cấu trúc docs, Tauri version/capabilities.
- Quy tắc trọng tâm: `ui.A1`, `ui.A2`, `ui.C1`, `coding.A1`, `coding.B4`, `docs.A2`, `docs.A3`, project Tauri rules.

## Kết luận rà soát

| # | Finding | Verdict | Mức | Bằng chứng |
|---|---|---|---|---|
| 1 | Static inline style | CONFIRMED | P1 | Detector tìm 61 vị trí trong tracked Vue. `ui.A1` chỉ cho phép inline style cho giá trị tính ở runtime; cần phân loại từng vị trí vì count gồm cả HTML được tạo động và prop `container-style`. Ví dụ: `src/components/DialogHost.vue:3`, `src/components/modals/ChangelogModal.vue:4`, `src/components/modals/ProjectConfigModal.vue:69`. |
| 2 | Hardcoded visual colors ngoài token source | CONFIRMED | P0 | Detector tìm 383 literal hex/rgb trong tracked Vue/CSS/JS ngoài token/theme path. Đây là breach SSoT theo `ui.A2`; cần tách palette có contract riêng như terminal ANSI khỏi màu UI trước khi thay. Ví dụ: `src/components/PairingGate.vue:82`, `src/components/TerminalView.vue:235`, `src/components/AgentUsage.vue:759`. |
| 3 | Kiến trúc CSS bị đảo tầng | CONFIRMED | P0 | CSS trong SFC `<style>` là 5,579 dòng, stylesheet chung là 1,502 dòng. `ui.A1/C1` định nghĩa scattered CSS lớn hơn shared CSS là inversion ở cấp project, không phải nợ cục bộ từng file. |
| 4 | Comment không dùng English | PARTIAL | P1 | Detector ban đầu nêu 5, scan rộng tìm 7 candidate. Năm vi phạm chắc chắn nằm tại `scripts/test/monitor-ag-proxy.js:37`, `:46`, `:59`, `:65`, `:68`. Hai comment còn lại chứa ký tự Unicode trong prose English (`src-tauri/src/pty.rs:39`, `src/composables/useTerminalChrome.js:3`) nên không được tính là “comment tiếng Việt”, nhưng vẫn thuộc finding YAP bên dưới. |
| 5 | Comment YAP | CONFIRMED | P1 | Hai comment dài/narrative cần xử lý theo `coding.B4`: `src-tauri/src/pty.rs:39` và `src/composables/useTerminalChrome.js:3`. Giữ constraint/why hoặc reference doc; bỏ phần kể lại flow/code. |
| 6 | Thiếu business backbone | CONFIRMED | P1 | `docs/biz/` không tồn tại trong một sản phẩm có business/public distribution dimension. Vi phạm `docs.A2/A3` và `biz.A3`; cần tạo SSoT tối thiểu thay vì suy diễn positioning từ README. |
| 7 | 11 arbitrary-value findings | FALSE POSITIVE | — | 11 match đều đến từ `:class` array/object indexing như `value?.[key]`, không phải Tailwind arbitrary value dạng `w-[…]`/`text-[…]`. Không đưa vào remediation count; detector phải giới hạn literal `class="…"`. |
| 8 | Tauri version/capabilities | COMPLIANT | — | `package.json` và `src-tauri/Cargo.toml` cùng `1.30.0`; `src-tauri/tauri.conf.json:4` dùng `../package.json`; `src-tauri/capabilities/default.json:12-14` có minimize, close, start-dragging. |

## Nguyên tắc remediation

- Không biến 383 màu hoặc 61 inline styles thành 444 patch độc lập. Sửa nguồn token/pattern/component trước, rồi thay call sites theo batch.
- Không đụng đồng thời vào file đang có owner work nếu chưa tách được hunk an toàn.
- Mỗi batch chỉ xử lý một pattern; sau thay thế phải chạy type/lint/build phù hợp và kiểm tra visual/runtime cho phần mà static reading không chứng minh được.
- Terminal ANSI palette, màu dữ liệu runtime và browser-anchor styles phải được phân loại như contract riêng; không ép chúng thành token UI nếu semantics khác.
- Không sửa `:class` bindings thuộc false positive.

## Kế hoạch thực thi

### P0 — Khôi phục single source of truth cho UI

#### Batch P0.1 — Inventory token và phân loại 383 màu

1. Xác định token source hiện tại trong shared stylesheet và liệt kê mọi literal màu theo semantic role: surface/text/border/accent/status/terminal/runtime preview.
2. Nhóm literal trùng hoặc gần nhau; snap về role hiện hữu trước khi tạo token mới.
3. Đánh dấu ngoại lệ contract: xterm ANSI palette, parser/rendered preview và giá trị thực sự do runtime cung cấp.
4. Tạo mapping `literal → token/contract`, không sửa call site trong batch inventory.

Hoàn thành khi mọi literal có một classification duy nhất, không còn mục “unknown”, và không tạo token theo tên hue/value.

Verify:

```bash
git grep -n -E '#[0-9a-fA-F]{3,8}\b|rgb\([^)]+\)' -- '*.vue' '*.css' '*.js'
```

#### Batch P0.2 — Củng cố shared token layer

1. Thêm hoặc hợp nhất semantic tokens tại một source duy nhất.
2. Không sao chép token vào SFC; không tạo parallel scale.
3. Ghi lại global pattern/token theo convention hiện có của project.

Hoàn thành khi mỗi visual role có một definition và không có duplicate token definitions theo load order.

#### Batch P0.3 — Thay màu theo cụm component

Thực hiện tuần tự theo cụm để giới hạn blast radius:

1. Shell/common controls và base components.
2. Modal family.
3. Project/terminal/task surfaces.
4. Usage/agent surfaces.
5. Pairing/remote surfaces.

Hoàn thành khi scan màu chỉ còn token source và danh sách ngoại lệ contract đã ghi nhận; mọi giá trị UI còn lại đều tham chiếu semantic token.

Verify:

```bash
git grep -n -E '#[0-9a-fA-F]{3,8}\b|rgb\([^)]+\)' -- '*.vue' '*.css' '*.js'
npm run lint:scripts
npm run lint:simpleview
```

#### Batch P0.4 — Đảo lại tỷ lệ shared/SFC CSS

1. Chạy subtraction pass: delete → inherit → hoist.
2. Tìm pattern lặp repo-wide; chuyển pattern có bằng chứng sang shared stylesheet/base component/variant API.
3. Giữ trong SFC chỉ keyframes, selector phức tạp, third-party override hoặc markup không do project author.
4. Không đặt mục tiêu số dòng tùy ý; mục tiêu cơ học là shared layer không còn bị scattered layer áp đảo, đồng thời từng block còn lại phải có lý do hợp lệ.

Hoàn thành khi SFC style total không lớn hơn shared CSS total hoặc mọi phần vượt còn lại được phân loại là resident hợp lệ theo `ui.A1`.

Verify:

```bash
find . -path ./node_modules -prune -o -name '*.css' -print | xargs cat | wc -l
find . -path ./node_modules -prune -o -name '*.vue' -print | xargs awk '/<style/{f=1} f{n++} /<\/style>/{f=0} END{print n+0}' | awk '{s+=$1} END{print s+0}'
```

### P1 — Inline styles, comments và docs backbone

#### Batch P1.1 — Phân loại và loại static inline styles

1. Re-run count trên tracked Vue và chia 61 vị trí thành: static template style, `container-style`, generated HTML style, runtime-computed style.
2. Static template/prop: chuyển sang class/token/variant.
3. Generated HTML: dùng semantic classes nếu markup được kiểm soát; giữ inline chỉ khi giá trị được tính runtime và ghi rõ contract.
4. Các CSS anchor properties chỉ giữ inline nếu thực sự cần runtime scoping; nếu hằng, chuyển sang class.

Hoàn thành khi không còn static `style=`/`container-style`; mọi inline còn lại đều là binding runtime có lý do kiểm chứng được.

Verify:

```bash
git grep -n -E 'style="[^":][^"]*"|container-style="[^"]+"' -- '*.vue'
```

#### Batch P1.2 — Comment hygiene

1. Dịch 5 comment trong `scripts/test/monitor-ag-proxy.js` sang English, đồng thời áp dụng deletion test thay vì dịch nguyên văn nếu code đã tự nói được.
2. Rút `src-tauri/src/pty.rs:39` thành constraint/why ngắn hoặc reference doc; không kể lại toàn bộ constant derivation trong comment.
3. Rút `src/composables/useTerminalChrome.js:3` thành invariant/reference ngắn; tên và cấu trúc phải mang phần WHAT/HOW.
4. Chạy scythe trên changed scope và repo.

Hoàn thành khi source comments dùng English và detector không còn YAP/WRAP trong các file đã xử lý.

Verify:

```bash
python3 /Users/aki/.claude-prx/skills/akiflow/scripts/scythe.py .
git grep -n -E '//.*[À-ỹ]|/\*.*[À-ỹ]' -- '*.js' '*.mjs' '*.rs' '*.vue' '*.css'
```

#### Batch P1.3 — Tạo business backbone tối thiểu

Tạo `docs/biz/` với một canonical doc ngắn, trả lời được:

- Primary audience và job-to-be-done.
- Falsifiable USP hoặc ghi rõ chưa có moat.
- Revenue path, kể cả câu trả lời “none/personal tool/ecosystem support”.
- Product boundaries dùng để giải quyết tradeoff.

Cập nhật `docs/index.md`; thêm anchor stamp `updated 2026-09-18 · v1.30.0`. Không chép marketing prose từ README nếu chưa được chứng minh.

Hoàn thành khi `docs/biz/` là SSoT được index và README/feature docs không mâu thuẫn với nó.

### P2 — Guard chống tái phát

#### Batch P2.1 — Detector chính xác

1. Thêm scan static inline style và hardcoded visual values vào lint/audit script phù hợp.
2. Sửa arbitrary-value regex để chỉ xét literal class string, không match `:class` object/array indexing.
3. Cho phép allowlist hẹp cho contract palette/runtime-generated markup; mỗi ngoại lệ cần owner và lý do.

Hoàn thành khi fixture chứa 11 false positives trả zero, còn fixture `w-[123px]` và static `style="color:red"` bị bắt.

#### Batch P2.2 — UI architecture regression gate

1. Ghi số dòng shared/SFC CSS trong audit output.
2. Gate không dựa vào một con số mù: fail khi scattered vượt shared mà không có classification report cho legitimate residents.
3. Thêm duplicate selector/token checks để tránh một name nhiều definitions.

Hoàn thành khi CI/local audit báo rõ count, exceptions và file origins; không trả verdict “clean” chỉ từ heuristic.

## Verification cuối

```bash
npm run lint:scripts
npm run lint:simpleview
cargo check --manifest-path src-tauri/Cargo.toml
python3 /Users/aki/.claude-prx/skills/akiflow/scripts/scythe.py .
git diff --check
git status --short
```

Runtime/visual ledger sau khi mọi batch UI hoàn tất: mở app một lần ở trạng thái cuối, kiểm tra modal family, terminal theme/ANSI colors, project table states, usage/status surfaces và pairing/remote screens. Đây là phần không thể xác nhận chỉ bằng static reading; không báo Done cho UI refactor trước lượt này.

## Không thuộc plan

- Không sửa 11 `:class` false positives.
- Không thay Tauri version/capabilities đang compliant.
- Không commit, push, release hoặc thay đổi git state trong audit.
- Không tự phân loại/ghi đè các hunk đang có của owner.

## Trạng thái bàn giao — 2026-09-20 (đo lại trên HEAD `d73d0e7`, cây sạch)

Đo bằng `npm run audit:ui`: **583 hit** = 524 màu literal trong Vue/CSS + 34 hex trong JS + 22 inline style + 3 khác; `npm run build` xanh; `npm run test:ui-audit` 4/4.

### Đã xong (kèm commit)

| Finding | Kết quả | Commit |
|---|---|---|
| 6 — business backbone | `docs/biz/backbone.md`, đã vào `docs/index.md` | `e7d0a93` |
| 4, 5 — comment | English hoá `scripts/test/monitor-ag-proxy.js`; `pty.rs:39` và `useTerminalChrome.js:3` rút gọn | `0e908cf` |
| 2 (một phần) — màu | ~30 primitive vào `:root` của `main.css`; mọi hex có token 1:1 trong `<style>` của 27 SFC và `main.css` đã thành `var()` | `0e908cf` |
| 1 — inline style | detector bỏ qua `:style`/`@style` (79 → 22); style tĩnh của 6 modal và `DialogHost` thành class; pattern dùng chung `.modal-title-row/-icon/-glyph`, `.row-gap-8` ở `main.css` | `0e908cf`, `d73d0e7` |
| P2.1, P2.2 — guard | `scripts/audit-ui-architecture.mjs` (đếm dòng shared/SFC, exception cần owner+reason, duplicate selector/token) + fixture + test | `0e908cf` |

### Còn lại — theo thứ tự làm

1. **Chốt vocab alpha (cần owner, chặn bước 2).** 524 hit còn lại gần như toàn `rgba()`; 10 giá trị đầu chiếm phần lớn: `rgba(255,255,255,.1)` ×24, `.08` ×19, `.05` ×12, `rgba(0,0,0,.5)` ×12, `.07` ×11, `.04` ×11, `.06` ×10, `.2` ×7, `.02` ×7, `rgba(0,210,255,.3)` ×7, `.15` ×7. `ui.A2` cấm đặt tên theo giá trị (`--white-a10`) và `ui.A4` yêu cầu snap về bậc gần nhất: đề xuất một thang theo vai trò (ví dụ `--overlay-faint/subtle/soft/strong`, `--scrim`, `--accent-cyan-soft/-strong`) gộp `.04/.05/.06/.07/.08` thành 2–3 bậc. Snap làm đổi render nhẹ, nên chỉ làm sau khi owner duyệt thang.
2. **Migrate rgba → token theo cụm** (shell/controls → modal → project/terminal/task → usage → pairing/remote), mỗi cụm chạy `npm run build` + `npm run audit:ui`. Chỉ đụng `<style>`/CSS; không đụng literal màu trong JS.
3. **34 hex trong JS** (xterm theme, `statuslineColors.js`, `useSync.js`, `remoteActions.js`, `projectStore.js`, `UsageCircle`, `TerminalView`): là contract riêng (`var()` không chạy trong xterm/canvas). Ghi từng nhóm vào `scripts/ui-audit.config.json` `exceptions` (file, rule, owner, reason); không allowlist cả thư mục.
4. **Finding 3 — cần owner chốt.** `<style scoped>` co-location (Vue idiom, token layer + utility dùng chung thỏa `ui.C1`) hay ép chuyển nghĩa đen ~5.6k dòng SFC CSS vào shared (rủi ro cao, giá trị thấp). Khuyến nghị: chấp nhận diễn giải; sau đó điền `sfcResidents` cho 37 SFC trong `scripts/ui-audit.config.json` để gate hết đỏ. Nếu chưa chốt, không điền.
5. **22 inline style còn lại là hợp lệ, cần ghi exception:** HTML sinh lúc chạy (bảng ANSI `GitModal:127-133`, span màu `ClaudeSettingModal:795-907`), prop `container-style` của `BaseModal` (6 modal — API prop, cân nhắc đổi sang prop `width`), `anchor-name` của `TerminalChromeMenu`, fixture dương tính.
6. **Sửa detector cho khớp thực tế (P2.1 chưa trọn):** báo nhầm `.logo-section h1` trong `@media` và keyframe `0%/50%` là selector trùng — cần bỏ qua khi nằm trong `@media`/`@keyframes`.
7. **Visual ledger (bắt buộc trước khi đóng plan)**: mở app một lần ở trạng thái cuối và duyệt modal family (đặc biệt `ProjectConfigModal`, `GitModal`, `ChangelogModal`, `UpdateModal`, `RefreshSettingsModal`, `SshConfigModal` — vừa đổi từ inline sang class), màu terminal/ANSI, bảng project, usage/status, pairing/remote. Xong thì chuyển plan này sang `done/`.

### Cổng đóng

`npm run audit:ui` exit 0 với mọi exception có owner+reason, `npm run build`, `npm run lint:scripts`, `npm run lint:simpleview`, `cargo check`, visual ledger. Không báo Done trước visual ledger (`coding.B3`).

### Quyết định đã chốt (/akithink 2026-09-20)

Chạy màu→token 1:1 trước, giữ CSS scoped, không patch mù 444 chỗ, không đụng literal màu trong JS. Reopen khi visual ledger phát hiện lệch render.
