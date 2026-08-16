# Terminal input — plan chốt lại, 2026-07-31

**Báo cáo test (đối chiếu):** [`docs/research/terminal-input-jul31.md`](../../research/terminal-input-jul31.md) — cùng tên, cùng ngày. File đó là **quan sát**; file này là **quyết định và việc còn phải làm**. Mọi mục dưới đây trỏ về số mục tương ứng bên đó thay vì chép lại quan sát.

**Trạng thái:** kiến trúc đã đúng hướng. Blocker double space (§2.1) **đã đóng 2026-07-31** sau một phiên hội đồng riêng (`red-team-drain` + `regression-surface`) và xác nhận trên máy thật. Còn **2 defect**: hiển thị latch Ctrl/Shift (§2.3, đã sửa, chờ xác nhận) và Android/Gboard (§2.2, đã có giả thuyết root cause ở mức code, chưa xác nhận trên máy thật, chưa sửa).

**Thay thế / thu hẹp các plan trước:**
- [`docs/plan/done/terminal-input-surface.md`](terminal-input-surface.md) — bảng §6 của nó đã được chạy hết một lượt; kết quả nằm ở research §4 (bảng checklist). Plan đó nay chỉ còn hiệu lực cho phần **UI feedback** (§2 dưới đây). Thứ tự thi công `#3 → #13 → #2 + #8` của nó đã hoàn tất trên thực tế.
- [`docs/research/terminal-vietnamese-ime-root-cause-4.md`](../../research/terminal-vietnamese-ime-root-cause-4.md) — không còn là head của chuỗi. **Hai tiền đề của nó hết hiệu lực**: §5.3 coi khả năng A/B `aki-input-mode='legacy'` là điều kiện tiên quyết, chủ sở hữu đã bác bỏ (§4 dưới đây); và §7's "narrow" keypress veto cùng dòng đầu bảng exclusivity của nó, bị phiên hội đồng double-space lật lại bằng bằng chứng máy thật — xem [`terminal-vietnamese-ime-root-cause-5.md`](../../research/terminal-vietnamese-ime-root-cause-5.md), head mới của chuỗi.

---

## 1. Đã đóng — không mở lại

Xác nhận trên máy thật ngày 2026-07-31 bởi chủ sở hữu (research §4, §5):

- **Kiến trúc "xterm owns keys; the app owns text"** — đúng. Phím chức năng (mũi tên trong `vim`/`less`, Ctrl+C, Ctrl+D, Option+Backspace/mũi tên) hoạt động bình thường sau khi trả `disableStdin` về `false`. Không có hồi quy nào từ việc gỡ overlay textarea.
- **`"ăn gì" → "ăn g"`** — lỗi cắt ký tự cuối, tức defect gốc của cả chuỗi nghiên cứu, **đã hết**.
- **OpenKey auto-restore** — đúng (research §5.4).
- **Sticky Ctrl/Shift, tầng logic** — đúng, có bằng chứng ở hai chương trình độc lập (Shift+Tab trong Claude Code, Ctrl+X trong `nano`, Ctrl+C từ ô compose). Latch object + funnel `emitKey` + `ctrlByteFor` + latch áp dụng trong ô compose: tất cả đóng.
- **Shift+Enter** trong ô compose — đúng, "hoàn hảo".
- **Compose row + key row chỉ hiện trên companion** (`showKeyRow`) — đúng.
- **Font ô compose** — nhất quán, "hoàn hảo".

Hệ quả: việc xoá `useTerminalInput.js` và `useWkImeGuard.js` là đúng và không cần đắn đo thêm.

---

## 2. Blocker và defect còn lại — nội dung cho phiên hội đồng

Ba mục, xếp theo mức chặn. Mục 2.1 phải giải trước vì nó che khuất phần còn lại.

### 2.1 — BLOCKER: double space — ĐÃ ĐÓNG 2026-07-31

Quan sát gốc: research §5.2. Truy nguyên đầy đủ, bằng chứng file:dòng và xác nhận máy thật: [`terminal-vietnamese-ime-root-cause-5.md`](../../research/terminal-vietnamese-ime-root-cause-5.md). Ledger phiên hội đồng: `/Users/aki/.aki/agent-council/aki-dev-sync/2026.07.31-0232-double-space/checklist.md`.

**Nguyên nhân gốc:** `cancel(ev, force)` (`Terminal.ts:1308`) là no-op khi thiếu `force` và khi `cancelEvents` tắt (mặc định `false` — `OptionsService.ts:56`; `TerminalView.vue` không truyền cờ này). `_keyPress` gọi `cancel(ev)` không `force` (`Terminal.ts:1133`). `Keyboard.ts:381` chỉ gán `result.key` khi `keyCode >= 48` — space (32) trượt qua, nên `_keyDown` `return true` mà không cancel thật (`Terminal.ts:1046-1048`). Hệ quả: `_keyPress` gửi ký tự lần một, trình duyệt vẫn chèn vào textarea, drain đọc và gửi lần hai — đúng hai lần, đúng bằng chứng quan sát. Guard chống-gửi-hai-lần có sẵn của xterm (`_keyPressHandled`, `Terminal.ts:1177`) không cứu được: nó sống trong `_inputEvent`, gắn trực tiếp trên textarea của xterm (`Terminal.ts:384`), còn drain gắn ở pha capture trên tổ tiên `term.element` và gọi `stopPropagation()` khiến `_inputEvent` không bao giờ chạy — vô hiệu hoá theo cấu trúc, không theo giá trị cờ. **Corollary chưa từng được đo trước đây:** chữ HOA A-Z đi cùng đường với space qua HACK caps-lock riêng (`Terminal.ts:1052-1056`).

**research §5.4 (đoạn sau OpenKey auto-restore không bị double) được giải thích:** OpenKey gửi cả chuỗi kèm dấu cách cuối trong một sự kiện, Chrome/WKWebView gắn `keyCode 229` cho chuỗi nhiều ký tự, nên carrier đó thoát sớm qua `CompositionHelper` và không bao giờ chạm nhánh space nêu trên.

**Fix (`src/composables/useTerminalTextDrain.js`):** `customKeyEventHandler` giờ veto **mọi** `keypress`, thay vì chỉ veto carrier đa ký tự như trước — bớt một nhánh phân loại, không thêm guard. Invariant đúng theo cấu trúc DOM (`preventDefault` chặn mutation textarea) thay vì theo phân loại phím.

**Xác nhận trên máy thật, chủ sở hữu, 2026-07-31:**
- Trước fix: tiên đoán từ mã nguồn — gõ `TEST` phải ra `TTEÉTT` — đo đúng, xác nhận cả cơ chế lẫn corollary chữ HOA.
- Sau fix: Mac + OpenKey (`tét`/`TÉT`/`báo cáo`) sạch; Chrome mặt remote (`tét TÉT báo cáo`) sạch; `vim` mũi tên và Ctrl+C bình thường.
- Sau fix, Android/Gboard mặt remote: không double space; vẫn đúng hình dạng defect §2.2 cũ — fix này không đụng tới nó.
- **Chưa đo, không tính là PASS:** Option+mũi tên — chủ sở hữu chủ động gác lại lần này. Theo dõi tại [`verify-pending.md` §Terminal T2](verify-pending.md#t2--optionarrow-with-the-double-space-fix-in-place).
- **Không tái hiện được, không phải đã sửa:** `bá o` (dấu cách chen giữa âm tiết) thử lại không ra lại. Không có cơ chế nào trong mã nguồn giải thích được hình dạng đó từ nguyên nhân gốc ở trên (giả thuyết "drain bỏ sót textarea ở nhánh composition" bị bác vì nhánh đó cần `compositionstart` thật, mâu thuẫn với việc bug tái hiện khi không bật bộ gõ) — nên không gộp vào fix này.

### 2.2 — Android / Gboard: ký tự gốc và ký tự đã sửa đều tới PTY — **ĐÃ SỬA 2026-08-16**

Quan sát: research §5.5 (`ăn gì` → `aăn giì`). **Giữ tách khỏi 2.1** — khác nền tảng, khác hình dạng, đã xác nhận không chung nguyên nhân với 2.1 (fix 2.1 đổ bộ lên máy thật, hình dạng defect này y nguyên). Không gộp hai mục này vào một fix chung.

Truy nguyên ở mức code, chưa có capture trên máy Android thật: [`terminal-gboard-double-insert.md`](../../research/terminal-gboard-double-insert.md). Cơ chế nghi ngờ mạnh nhất (chưa xác nhận trên phần cứng): drain đọc-hết-rồi-xoá-rỗng textarea sau mỗi ký tự gốc, nên khi Gboard tự sửa (autocorrect/dấu) bằng cách xoá lùi phần đã gõ rồi chèn bản đã sửa, thao tác xoá chạm vào một textarea đã rỗng sẵn — không có gì để xoá, event xoá bị bỏ qua lặng lẽ, chỉ có phần chèn bản sửa tới được PTY, chồng lên bản gốc đã gửi trước đó. Doc trên nêu một nhánh rẽ chưa loại trừ được chỉ bằng đọc mã nguồn (Gboard bọc phần sửa trong composition event thật hay không) và một phép đo console cụ thể (`__akiTermInput.dump()` trên máy Android) để chốt nhánh nào đúng. **Trạng thái: đã đo trên máy Android/Gboard thật, xác nhận nhánh plain-delete (không bọc composition) — `useTerminalTextDrain.js` nay dịch `deleteContentBackward` rơi vào textarea rỗng thành byte backspace thật gửi PTY, thay vì bỏ qua lặng lẽ. Xem `docs/plan/remaining-1.22.md` (mục GBOARD, Resolved 2026-08-16).**

### 2.3 — Nút Ctrl/Shift không sáng khi đang armed — **ĐÃ SỬA 2026-07-31, chờ xác nhận**

Quan sát: research §5.1. Defect **hiển thị**, không phải input — byte tới PTY đã đúng ngay từ đầu.

**Nguyên nhân:** `const ptyApi = ref(null)` trong `TerminalView.vue`. Một `ref` sâu chạy object được gán qua `reactive()`, mà `reactive()` **bóc (unwrap) các ref nằm trong thuộc tính**. Nên `ptyApi.value.pendingModifiers` trả về thẳng object `{ ctrl, shift }`, và mọi `.pendingModifiers.value` trong file đọc ra `undefined`. Template test `undefined?.[k.arms]` → `is-armed` không bao giờ bật. Latch vẫn đúng vì nó sống trong closure của composable, chỗ mà việc bóc ref không với tới được — đúng như triệu chứng "chạy đúng nhưng không sáng".

**Cách sửa:** `ref` → `shallowRef`. Một từ, không thêm DOM, không thêm state, không đổi CSS (Extreme Narrow giữ nguyên: armed = tô đặc accent trên chính nút đó).

**Cùng một nguyên nhân, sửa luôn trong cùng lần:** hai chỗ khác cũng đang đọc `undefined` — `onComposeKeydown` (`api?.pendingModifiers.value.ctrl`, tức Ctrl latch trong ô compose) và `defineExpose({ alive })` (`ptyApi.value?.alive?.value ?? 'unknown'`, tức tab strip không bao giờ biết PTY đã chết, luôn hiển thị 'unknown'). Không phải việc ngoài phạm vi: đó là **cùng một dòng lỗi**, và sửa `shallowRef` làm cả ba chỗ đúng cùng lúc.

**Cần chủ sở hữu xác nhận trên máy thật:** theo dõi tập trung tại [`verify-pending.md` §Terminal T1](verify-pending.md#t1--ctrlshift-armed-button-display--tab-alive-status).

Ràng buộc đã tuân thủ: `showKeyRow` / `ownsPtySize` phải giữ nguyên là **boolean thuần** trong `usePtyTerminal.js` — nếu biến chúng thành ref thì `v-if="ptyApi?.showKeyRow"` luôn truthy và hàng phím phone sẽ hiện trên Mac, tức phá D9 vốn vừa PASS. Ghi lại thành comment ngay tại `shallowRef`.

---

## 3. Chốt ngoài phạm vi — không nhận là bug, không sửa

- **macOS Telex (bộ gõ dựng sẵn của macOS)** — research §5.3. Quyết định của chủ sở hữu: bỏ qua, vì VS Code cũng hỏng tương tự nên không phải khiếm khuyết riêng của app. **Chỉ hỗ trợ OpenKey.** Một phiên sau bắt gặp `ăn ăn gì, gì` phải đọc mục này trước khi coi đó là hồi quy.
- **Paste nhiều dòng / bracketed paste (C8)** — chủ sở hữu đánh giá quá phức tạp so với giá trị. Hình thức gửi hiện tại (các dòng thành các lệnh riêng) giữ nguyên. Câu hỏi bỏ ngỏ trong `terminal-input-surface.md` §3.4 về việc đọc trạng thái bracketed-paste của xterm 5.x **không cần trả lời nữa**.

---

## 4. Việc phải làm ngay, không cần chờ hội đồng — ĐÃ XONG 2026-07-31

**Gỡ escape hatch `aki-input-mode='legacy'`.** Chủ sở hữu yêu cầu rõ: đường legacy vốn đầy lỗi, giữ lại chỉ làm code rộng, rối, thành rác (research §5.6).

Đã gỡ:
- `TerminalView.vue`: hằng `legacyInput` (IIFE đọc `localStorage.getItem('aki-input-mode')` lúc setup) cùng đoạn comment giải thích nó, và nhánh rẽ `if (!legacyInput) textDrain = …` — `useTerminalTextDrain` giờ cài đặt vô điều kiện.
- `useTerminalTextDrain.js`: đoạn "Escape hatch:" ở cuối comment đầu file, mục `'aki-input-mode'` trong object `flags` trả về từ `status()`, và dòng nhắc `localStorage['aki-input-mode']='legacy'` trong `help()`.

Đã giữ nguyên, không đụng tới: toàn bộ `__akiTermInput` (ring buffer, `status()`, `dump()`, `tail()`, `debug()`, `clear()`, `help()`, các lệnh gọi `record()` rải khắp file) và cờ `aki-term-input-debug` (`debugOn`) — chúng là công cụ chẩn đoán cho chính mục 2.1, không phải nhánh code thứ hai.

Xác minh: `node`/`vue/compiler-sfc` syntax-check cả hai file sạch, và `grep -rn "aki-input-mode\|legacyInput" src/` không còn khớp gì. Hành vi runtime **chưa được chạy lại** sau thay đổi này — đây là gỡ code chết (dead branch), không phải một fix hành vi, nên không cần chạy máy thật, nhưng phiên hội đồng vẫn nên biết điều đó.

Đây là điều kiện tiên quyết của phiên hội đồng: hội đồng nay đọc **một** đường đi, không phải hai.

---

## 5. Nghĩa vụ đồng bộ tài liệu khi các mục ở §2 được đóng

Kế thừa từ `terminal-input-surface.md` §7, thu hẹp lại theo phạm vi thực tế còn lại:

- `docs/feat/in-app-terminal.md` — hành vi gõ trực tiếp tiếng Việt; xoá mô tả compose row là đường chính trên Mac.
- `docs/arch/terminal-stack.md` — thay `useTerminalInput` / `useWkImeGuard` bằng `useTerminalTextDrain`; ghi ranh giới "xterm owns keys; the app owns text".
- `README.md` + `src/components/modals/IntroModal.vue` — chỉ khi §2.3 đổi hành vi nhìn thấy được.
- `CHANGELOG.md` mục `[Unreleased]` — **không đánh số phiên bản** (`RULE-release.md` §A5).
- `docs/plan/done/terminal-input-surface.md` — moved to `done/` after §2.3 closed.
- File này + file research song sinh → `docs/plan/done/` và giữ nguyên chỗ cho research, khi cả §2 đóng hết.

---

## 6. Trạng thái commit — ĐÃ COMMIT HẾT, 2026-07-31

Cây làm việc sạch. Tám commit đã lên cho công việc terminal của phiên này, theo thứ tự:

- `bee7498` — doc nghiên cứu ghost-file audit (đã được index và CHANGELOG trích dẫn từ trước).
- `4851f40` — bản viết lại text drain (`useTerminalTextDrain.js` mới, `useTerminalInput.js` + `useWkImeGuard.js` bị xoá), cùng fix `shallowRef` cho hiển thị nút Ctrl/Shift armed (§2.3).
- `2d0f37e` — cặp research/plan jul31 (file này + file song sinh), `docs/index.md`, `CHANGELOG.md`.
- `0a6d314` — gỡ escape hatch `aki-input-mode='legacy'` (§4).
- `c417f99`, `f2b9fe2` — hai lần sửa tiếp §6 của chính file này cho khớp thực tế (mục này từng bị bỏ sót cập nhật hai lượt liền trước).
- `b99502a` — fix double space (§2.1), veto mọi keypress trong `src/composables/useTerminalTextDrain.js`.
- `5f41817` — đồng bộ tài liệu cho fix double space: `terminal-vietnamese-ime-root-cause-5.md` mới, dòng `Status: superseded by` trên `-4.md`, `docs/arch/terminal-stack.md`, `docs/feat/in-app-terminal.md`, `docs/index.md`, `CHANGELOG.md`, và chính file này (§2.1, §6).

**Mốc lùi cho hội đồng nếu việc truy §2.1 (double space) đi sai hướng vẫn là `4851f40`, không đổi.** Đó là commit mang kiến trúc text-drain đang chạy, và fix hiển thị `shallowRef` (§2.3) đã nằm sẵn TRONG chính commit này nên lùi về đó **vẫn giữ** fix hiển thị. Thứ mất đi là việc gỡ escape hatch (`0a6d314`), fix double space (`b99502a`) và các commit doc nằm trên nó.
