# Ship 1.26.0 — tách lại section CHANGELOG bị gộp, bổ sung mục thiếu, thêm phím tắt cửa sổ

## Mục tiêu

`[1.25.0]` trong CHANGELOG hiện đang chứa cả việc đã ship lẫn việc làm sau khi ship. Kế hoạch này tách đôi: `[1.25.0]` chỉ còn đúng nội dung có trong bản DMG đã phát hành, mọi thứ sau tag chuyển sang `[1.26.0]` mới, cộng thêm một việc mới (phím tắt cửa sổ F1/F2/F3 + preset bề rộng 1400px).

## Ranh giới release — cơ sở của việc tách

| Sự kiện | Giá trị |
|---|---|
| Tag `1.25.0` | commit `7835cf3` (2026-08-16 15:22 +0700) |
| GitHub Release `v1.25.0` | publish 2026-08-16T08:27Z, đang là `Latest` |
| Sau tag | 26 commit + working tree chưa commit |
| Manifest hiện tại | `package.json` 1.25.0 · `Cargo.toml` 1.25.0 · `tauri.conf.json` trỏ `../package.json` |

Mọi phân loại "shipped / post-tag" dưới đây được xác định bằng cách kiểm tra sự hiện diện của từng marker trong cây tại `7835cf3`, không suy đoán từ commit message.

**Cảnh báo cần biết trước khi sửa:** hai commit gắn nhãn không phải feature lại mang thay đổi hành vi.

- `8a6e7df` `style(composables): clean wrap and yap comments…` — chứa fix Gboard double-insert (nhánh `deleteContentBackward` → `term.input('\x7f', true)` trong `useTerminalTextDrain.js`). Tại tag chưa có nhánh này.
- `1adf86b` `docs(plan): move wish-terminal-split-simpleview to done…` — chứa fix `.u-wide-hide` / `.u-narrow-hide` **và** revert `MAIN_VIEW_MAX_WIDTH` 440 → 420 ở cả `useRightDockLayout.js` lẫn `main.css`. Chuỗi thực tế: `f1b03db` đặt 440 → `1adf86b` revert về 420 → working tree đặt lại 440.

---

## T1 — CHANGELOG: nội dung GIỮ LẠI trong `[1.25.0]`

Đây là những gì thật sự có trong bản DMG đã phát hành. Giữ nguyên câu chữ hiện có, trừ một sửa đổi bắt buộc.

**Added** (giữ cả 5 bullet)
- Right-side dock column above 900px window width — **sửa `440px` → `420px`**: bản ship dùng 420, con số 440 là việc làm sau tag và thuộc về 1.26.0.
- External terminals now say whether they were launched from a project…
- External-terminal badge counts now honour spawn origin…
- Per-project disable toggle to reduce background load.
- Terminal chrome visibility menu.

**Changed** (giữ)
- New maintenance script `scripts/fix-ssh-agent-leak.sh`…
- `TerminalCell.vue` replaced by `TerminalScopeButton.vue`…
- Internal: hard-wrap sweep on the code buckets (commit `38854b9`)…
- Idle GPU/CPU cleanup: notification dot no longer animates forever, `transition: all` narrowed…
- Design System & Tokenization Cleanup — giữ, nhưng bổ sung theo T4.

**Fixed** (giữ)
- Global event log expand/collapse inversion in right-dock mode.
- Responsive breakpoint query mismatch in AppHeader and modals.
- State mirroring over WebSocket.
- Toast no longer covers the ProjectTable ACTIONS column or a modal footer.
- ProjectTable empty state CTA.

---

## T2 — CHANGELOG: nội dung CHUYỂN sang `[1.26.0]`

Cắt nguyên văn khỏi `[1.25.0]`, dán vào section mới.

**Changed**
- Unified Narrow Mode & Right-Dock main-view cap SSoT to 440px — 6 vị trí: `useRightDockLayout.js` (`MAIN_VIEW_MAX_WIDTH`), `useAppWindow.js` (`NARROW_WIDTH`), `tauri.conf.json` (`minWidth`), `main.css` (`--main-view-max-width` fallback), `ClaudeCleanupModal.vue`, `GeminiAllowlistModal.vue`.
- Repo-wide `[WRAP]` / `[YAP]` formatting sweep — chuyển nguyên khối, kèm sửa số liệu theo T3.

**Fixed**
- Container Query vs Media Query dual-hiding collision on `.u-narrow-hide` / `.u-wide-hide`.
- Android/Gboard IME double-insert in the in-app terminal — **bullet này đang nằm sai ở 1.25.0**; tại tag `7835cf3` nhánh `deleteContentBackward` chưa tồn tại trong `useTerminalTextDrain.js`.
- Claude Code reset timer lifecycle in AgentUsage (`ccClockTimer`).

---

## T3 — Sửa số liệu sai trong bullet sweep

| Đang ghi | Đúng phải là |
|---|---|
| `132 files` | `131 files` (dải `7835cf3..HEAD`) |
| `-6,330 lines net: +2,405 insertions, -8,735 deletions` | giữ nguyên — khớp chính xác dải committed |
| `Composables Layer (27 files…)` | `26 files` |
| danh sách composables có `useRightDockLayout.js` | **bỏ tên này khỏi danh sách** — nó không nằm trong dải sweep committed, chỉ đổi ở working tree |

Sáu bucket còn lại đã kiểm và khớp: docs 15 · rust 15 · store+services 22 · components+css 28 · scripts 14 · constants+utils 11.

**Không giữ nguyên câu "strictly preserving 100% of architecture invariants"** — sai thực tế: các commit `style(*)` cộng lại chứa khoảng 1.000 dòng thay đổi phi-comment, và hai commit nêu ở phần Ranh giới đã mang cả fix lẫn revert. Viết lại thành mô tả trung thực, không tuyên bố bất biến.

---

## T4 — Bổ sung mục thiếu (code có, CHANGELOG chưa nói)

### Thêm vào `[1.25.0]` (đã ship, chỉ là chưa ghi)

**Fixed**
- `open_remote_subprocess` và `install_akiclaudedoc` chuyển từ `fn` đồng bộ sang `async fn` + `tauri::async_runtime::spawn_blocking` — hết đơ cửa sổ khi mở terminal remote / cài AkiClaudeDoc. Đây đúng lớp bug mà `CLAUDE.md` đánh dấu ABSOLUTE, đáng một dòng riêng.
- Nút Copy trong event log không còn tắt trạng thái "copied" sớm khi bấm liên tục (`useLogs.js`, dọn timer cũ trước khi hẹn timer mới).

**Changed**
- Event log đổi nhãn `RAW CONSOLE` → `PROJECT LOG`, icon `fa-terminal` → `fa-list-ul`.
- Surface lệnh Tauri: bỏ `count_external_terminals`, `count_external_terminals_global`, `list_external_terminals`; thêm `list_terminal_sessions`, `describe_terminal_sessions` (kèm state `TerminalOwnership`).
- Tab strip: nút pin và close có vùng bấm rộng hơn, `aria-label`, và kích hoạt được bằng Enter; tooltip pin đổi thành "Unpin — keep in this group only"; icon close hover màu đỏ.
- Chuyển `@media (max-width: 700px)` → `@container main-view (max-width: 700px)` ở `AgentUsage.vue`, `AgentUsageSection.vue`, `AgentUsageSlot.vue`, `UsageCircle.vue`, `ProjectTable.vue`. Lưu ý: bullet "Responsive breakpoint query mismatch" hiện chỉ mô tả chiều ngược lại (container → media ở AppHeader/TaskListPanel), đọc riêng sẽ hiểu nhầm là cả app đi một hướng — cần nói rõ hai nhóm đi hai hướng vì nằm trong/ngoài container `main-view`.
- Vùng hover của OPEN popup mở rộng thêm 12px xuống dưới (`.open-popup::before`).
- Splitter đổi trigger highlight từ class tổ tiên `.is-dragging` sang `:active` của chính nó.
- Copy trong IntroModal: "Force Delete" → "Mirror / Delete", "Logo menu" → "App-icon menu".
- Bổ sung vào bullet "Design System & Tokenization Cleanup": một số hex **đổi màu hiển thị thật**, không phải swap cùng giá trị — `#FFF`→`#F3F4F6`, `#a5f3fc`→`#00d2ff`, donate `#f87171`→`#ef4444`, note-sticky `#f59e0b`→`#ff8c00`, popup icon `#fbbf24`/`#38bdf8`→`#ff8c00`/`#00d2ff`, pin tab `#60a5fa`→`#0088ff`.

### Thêm vào `[1.26.0]` (sau tag)

**Changed**
- Agent Usage: bỏ prop `remote` khỏi `AgentUsage.vue` và binding tương ứng ở `AgentUsageSlot.vue`; xoá rule `.zone-fieldset:hover`.
- Agent Usage: bỏ class `src-tab` trên nút tab; xoá style `.tab:disabled` — tab bị vô hiệu không còn mờ và không còn con trỏ `not-allowed`.
- Agent Usage: bỏ sàn `Math.max(tierCount, 1)` ở `AgentUsageSection.vue` và `AgentUsageSlot.vue` — khi `tierCount === 0` giờ render 0 hàng thay vì 1.
- AppHeader: link Remote HTTPS không còn bị chặn bởi `remoteHttpsAvailable`, chỉ còn phụ thuộc `remoteHttpsEnabled && remoteHttpsUrl`.
- `main.css`: xoá toàn bộ rule set `.btn-ui-action` (base, `:hover`, `:active`, `.error-state`, `.error-state:hover`), không có thay thế.
- ProjectTable ở narrow mode: cột TASKS `2.5rem` → `2.0rem`, `.last-action` `7px` → `9px` + `line-height: 1.2`, `.col-terminal` `margin-left: -6px` → `0`.

---

## T5 — Sửa mâu thuẫn tài liệu

1. **Bullet right-dock nói quá.** Câu "redundant minimize/collapse buttons and drag splitters are hidden" không đúng hoàn toàn: `main.css:203` chỉ ẩn `.dashboard-bottom .dock-splitter`. Ở right-dock, LogStack nằm trong `.dashboard-left` nên splitter của nó **vẫn hiển thị** — đúng như bullet "Global event log expand/collapse inversion" mô tả. Sửa thành "splitter của bottom dock bị ẩn; log stack trong cột phải vẫn kéo được".
2. **`IntroModal.vue:206`** đang nói right-dock "with its own resize splitter", ngược với README ("the column has no drag splitter"). Sửa IntroModal cho khớp mô tả đúng ở mục 1.
3. **`docs/feat/right-dock.md` đang untracked** dù CHANGELOG trỏ tới nó — `git add docs/feat/right-dock.md` trước khi commit.
4. **Cấp heading:** `[1.25.0]` hiện là `##` trong khi 1.24.0 trở xuống đều `###`. Hạ `[1.25.0]` về `###` và viết `[1.26.0]` cũng ở `###` — theo đúng cấp mà toàn bộ lịch sử file đang dùng, không đụng các version cũ.

---

## T6 — Version bump

`release.A5` cho phép bump manifest khi mở version mới với app đóng gói (Tauri): guard đã pass vì manifest hiện tại `1.25.0` **đã có tag tương ứng**.

- `package.json` → `1.26.0`
- `src-tauri/Cargo.toml` → `1.26.0` (bắt buộc cùng commit — `CLAUDE.md`)
- `src-tauri/tauri.conf.json` → không đụng (`"version": "../package.json"`)
- Không tạo tag ở bước này. Tag + build + GitHub Release là sự kiện riêng, chạy sau khi T7 xong.

Ghi chú phân loại: phần sau tag chủ yếu là fix + dọn nội bộ, theo `release.A4` nghiêng về patch (`1.25.1`). Chọn `1.26.0` theo yêu cầu trực tiếp — hợp lý vì T7 thêm khả năng mới và tổng khối lượng chạm 131 file.

---

## T7 — Phím tắt cửa sổ: F1 / F2 / F3 + preset bề rộng 1400px

### T7.1 — `src/composables/useAppWindow.js`

**Thêm hằng số**, cạnh `NARROW_WIDTH` / `WIDE_WIDTH` (dòng 42-43):

```js
const ULTRAWIDE_WIDTH = 1400;
```

Đặt tên theo vai trò, cùng họ với `narrow` / `wide` — không đặt `WIDTH_1400` (`pattern.A7`).

**Sửa `setWidthPreset(widthLogical)`** (dòng 116) để clamp bề rộng theo work area. Hiện tại hàm gọi `appWindow.setSize()` **trước** khi đọc `monitor`, nên chỉ nudge được vị trí x chứ không giới hạn được bề rộng — với 1400px trên màn nhỏ hơn thì cửa sổ tràn. Đảo thứ tự:

1. `await Promise.all([...])` như cũ (đã có `currentMonitor()` trong đó).
2. Tính `waSize` **trước**, rồi `const targetWidth = monitor ? Math.min(widthLogical, waSize.width) : widthLogical;`
3. `await appWindow.setSize(new LogicalSize(targetWidth, heightLogical));`
4. Phần nudge x giữ nguyên, nhưng dùng `targetWidth` thay cho `widthLogical`.

Sửa ở đây chứ không sửa riêng cho preset mới: một chỗ clamp dùng chung cho cả ba preset (`pattern.A1`). Với `narrow`/`wide` thì clamp là no-op nên không đổi hành vi cũ.

**Thêm preset** vào `VIEWS.width` (dòng ~143):

```js
ultrawide: () => setWidthPreset(ULTRAWIDE_WIDTH),
```

`VIEW_COMBOS` giữ nguyên (`1` = narrow + stick, `2` = wide + center) — F1 vẫn dùng combo 1.

Chiều cao và vị trí không đổi: `setWidthPreset` vốn đã giữ nguyên height và chỉ chạm x khi tràn mép phải.

### T7.2 — `src/components/AppHeader.vue`

**Sửa `onViewShortcut`** (dòng 570). Hàm hiện chỉ nhận `⌘1`/`⌘2` và chỉ áp combo; phím F không có modifier nên guard phải đảo lại:

```js
function onViewShortcut(e) {
  if (e.metaKey || e.ctrlKey || e.shiftKey || e.altKey || e.repeat) return;
  if (e.key === 'F1') { e.preventDefault(); applyViewComboSafe(1); return; }
  if (e.key === 'F2') { e.preventDefault(); applyViewSafe('width', 'ultrawide'); return; }
  if (e.key === 'F3') { e.preventDefault(); applyViewSafe('place', 'center'); }
}
```

`applyViewSafe` và `applyViewComboSafe` đã có sẵn trong file (dòng ~565), `applyView` / `applyViewCombo` đã được destructure từ `useAppWindow()` (dòng 363-377) — không cần thêm import.

Sửa luôn comment dòng 569 (`// Global shortcuts for window views (⌘1 / ⌘2).`) cho khỏi rot.

**Cập nhật nhãn trong dropdown** (dòng 216-222). Hiện có 2 chip `⌘1` / `⌘2` với class `view-combo-key col-1` / `col-2`. Cần 3 chip:

| Chip | title | hành động |
|---|---|---|
| `F1` | `F1 - Narrow + Stick Top-Left` | `applyViewComboSafe(1)` |
| `F2` | `F2 - Width 1400px` | `applyViewSafe('width', 'ultrawide')` |
| `F3` | `F3 - Center Primary` | `applyViewSafe('place', 'center')` |

Grid hiện chia 2 cột theo `col-1` / `col-2`; thêm chip thứ ba cần một quy tắc `col-3` hoặc đổi grid sang 3 cột trong `<style scoped>`. **Không thêm hàng mới** — nguyên tắc Extreme Narrow trong `CLAUDE.md`.

**Nút preset bề rộng thứ ba.** Hàng width hiện có `Narrow | Wide`, thêm nút thứ ba cùng hàng: nhãn `Ultra`, `title="Resize window width to 1400px, keeping height and position"`, `:class="{ 'is-active': savedView.width === 'ultrawide' }"`, `@click="applyViewSafe('width', 'ultrawide')"`. Không có nút thì preset này không hiện được trạng thái `is-active` từ `savedView.width` như hai preset kia.

`⌘1`/`⌘2` bị **thay thế hẳn**, không chạy song song: nhánh `e.metaKey` cũ trong `onViewShortcut` bị xoá, menu chỉ còn nhãn F1/F2/F3.

### T7.3 — Tài liệu đi kèm

- `README.md`: hàng **App-icon menu** đang ghi `⌘1 applies Narrow + Stick Top-Left, ⌘2 applies Wide + Center Primary` và liệt kê presets `Narrow (440px)`, `Wide (768px)` — cập nhật sang F1/F2/F3 và thêm preset 1400px.
- `src/components/modals/IntroModal.vue`: kiểm tra và cập nhật nếu có nhắc `⌘1`/`⌘2`.
- CHANGELOG `[1.26.0]` → **Added**: một bullet cho preset 1400px + bộ phím tắt F1/F2/F3, nêu rõ preset bị clamp theo bề rộng work area của màn hình hiện tại.

---

## Verify checklist

Đọc tĩnh là đủ cho phần lớn — chỉ escalate khi có nghi ngờ cụ thể (`coding.B3`).

**Kiểm bằng đọc / lệnh, không cần chạy app**

- [ ] `grep -nE '^#{2,3} \[' CHANGELOG.md` — thứ tự version đơn điệu, không trùng, không hụt; `1.26.0` nằm trên `1.25.0`; cấp heading nhất quán.
- [ ] Không còn bullet nào trong `[1.25.0]` mà marker của nó vắng mặt tại `7835cf3`. Cách kiểm một mục bất kỳ: `git show 7835cf3:<file> | grep <marker>`.
- [ ] `node -p "require('./package.json').version"` và `grep -m1 '^version' src-tauri/Cargo.toml` cùng ra `1.26.0`.
- [ ] `grep -n '"version"' src-tauri/tauri.conf.json` vẫn là `"../package.json"`.
- [ ] `git status --short` không còn `?? docs/feat/right-dock.md`.
- [ ] `grep -rn "ULTRAWIDE_WIDTH\|ultrawide" src/composables/useAppWindow.js src/components/AppHeader.vue` — hằng số, preset trong `VIEWS.width`, và cả 2 (hoặc 3) call site khớp tên.
- [ ] `grep -n "⌘1\|⌘2" src/components/AppHeader.vue README.md` — rỗng nếu chọn hướng thay thế.
- [ ] `setWidthPreset` gọi `setSize` **sau** khi đã clamp theo `waSize.width`.
- [ ] `npm run build` (hoặc `vite build`) pass — bắt lỗi cú pháp / import thiếu.

**Cần chạy app mới kết luận được — bàn giao cho người test, không tự đánh dấu Done**

- [ ] F2 trên màn hẹp hơn 1400px: cửa sổ khít work area, không tràn mép phải, chiều cao giữ nguyên.
- [ ] Grid 3 chip trong dropdown không vỡ ở narrow mode (440px).
- [ ] `savedView.width === 'ultrawide'` hiển thị đúng trạng thái active và sống sót qua lần khởi động sau khi bật **remember**.

---

## Ghi chú để lần sau khỏi lặp

Nguyên nhân gốc của việc phải tách lại: công việc sau khi tag được ghi tiếp vào section của version đã phát hành. `release.A5` giải quyết bằng một thói quen duy nhất — ngay sau khi tag và publish, mở `## [Unreleased]` ở đầu CHANGELOG và mọi thứ mới rơi vào đó cho tới sự kiện release kế tiếp.

Nguyên nhân gốc thứ hai: fix hành vi bị chôn trong commit gắn nhãn `style(*)` / `docs(*)`. Hệ quả là không phân loại được bằng commit message, phải dò từng marker trong cây tại tag. Commit dọn định dạng nên giữ đúng phạm vi định dạng.
