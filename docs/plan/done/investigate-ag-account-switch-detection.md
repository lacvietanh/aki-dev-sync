# Plan - Antigravity account switch not detected without IDE restart

Status: **CLOSED 2026-07-30 — no action, and the PID check is no longer worth running.** Lead ruling, closing a split between two specialists: `plan-consolidator` relabelled this UNSTARTED and escalated the dead-or-alive question to the owner; `usage-flow` argued it is closable by reasoning alone. The second is right, because both branches of this plan's own decision table now resolve to no separate work. **PID unchanged** — this plan's own conclusion is that nothing can be fixed from outside (there is no public API to force a Connect RPC server to refresh its internal session without a process restart), and the tooltip it proposed as consolation is refused by CLAUDE.md's Extreme Narrow rule. **PID changed** — this plan names process-detection errors as the follow-up class, and `docs/plan/ag-usage-pin-vs-live.md` fixes exactly that class regardless of which branch is true: a CLI process that cannot authenticate now reports a named miss instead of an anonymous `exit 1`, and the `maxBuffer` asymmetry on the `ps` read is closed. A plan whose every branch leads to work already scheduled elsewhere is finished, not blocked.

The distinction `plan-consolidator` drew is correct and is preserved below rather than dissolved by this closure: this was never the same defect as the pin-versus-live one. It closes because neither of its outcomes produces work, not because it turned out to be a duplicate.

Original status line, kept for the record: **UNSTARTED — blocked on one 3-step PID check that has to run on a Mac** (ghi nhận 2026-07-08, chưa root-cause được vì máy này không cài Antigravity IDE để verify trực tiếp). Relabelled 2026-07-30 by the plan-consolidation pass: `docs.C3` forbids leaving an unexecuted active plan ambiguous between dead and unstarted, and "Open — cần Mac để test" for three weeks reads as neither. Deciding whether it is *dead* is the owner's call, not an audit's (`agent.B5`), and it is escalated as such.

**Not the same defect as `docs/plan/ag-usage-pin-vs-live.md`, despite both being "AG shows the wrong account/quota".** This plan is about a live `language_server` process that keeps returning the *old* account's context after an in-IDE switch, because Antigravity may not restart it and its CSRF token is issued at process start. That one is about a *fresh, correct* reading being discarded downstream by a pin that demands a session type which is not live. Closing either one does not close the other.

**Two of its cited paths have moved under the in-flight probe rework** and must be re-resolved before anyone executes the test protocol: it names `scripts/get-antigravity-usage.sh` and `src-tauri/src/agent_usage/antigravity.rs`, while the working tree currently carries `scripts/get-antigravity-usage.js` and a single-file `src-tauri/src/agent_usage.rs`. The test protocol itself (`ps aux | grep language_server`, compare PID before/after an in-IDE switch) touches neither and is unaffected.

## Vấn đề (báo cáo của user, 2026-07-08)

Sau khi đổi account **ngay trong Antigravity IDE** (không quit app) - ví dụ dropdown chọn tài khoản Google khác - app Aki Dev Sync **không nhận ra** account mới. Chỉ khi **quit hẳn Antigravity rồi mở lại** thì app mới thấy đúng account/quota mới.

## Giả thuyết (chưa verify - cần Mac để xác nhận)

`scripts/get-antigravity-usage.sh` **không cache gì ở phía app mình** - mỗi lần poll (mặc định 30s) đều chạy lại toàn bộ chuỗi từ đầu:
1. `ps auxww` → tìm process `language_server_*` đang chạy → đọc `--csrf_token` + `--extension_server_port` **trực tiếp từ command-line arguments của chính process đó** (`detectOnUnix()`/`parseUnixProcessLine()` trong shell script).
2. Dùng CSRF token đó gọi Connect RPC `GetUserStatus` để lấy email.

Vì bước 1 luôn re-đọc `ps auxww` mỗi 30s (không có state cũ nào bị pin ở tầng Rust/JS), nên nếu Antigravity **spawn process mới** khi đổi account, app phải nhận được ngay ở poll kế tiếp - không cần sửa gì. Vậy khả năng cao nhất: **Antigravity không restart `language_server` process khi đổi account qua UI của chính nó** - process cũ (với CSRF token cũ, cấp lúc process khởi động) tiếp tục chạy, và bản thân RPC endpoint đó trả về **context account cũ** vì chưa từng biết có sự kiện đổi account nào xảy ra ở tầng session nội bộ của nó. Quit hẳn app buộc nó phải spawn process mới → CSRF token mới → app mình đọc được account mới ngay từ poll đầu tiên.

Nếu giả thuyết đúng: đây là **giới hạn từ phía Antigravity (third-party)**, không sửa được từ script polling bên ngoài - không có API public nào để ép một Connect RPC server refresh session nội bộ mà không restart process.

## Test protocol (làm trên Mac, nhẹ - không cần build lại app)

1. Mở Antigravity, đang login account X. Lấy PID: `ps aux | grep language_server`, ghi lại số PID.
2. Trong Antigravity, đổi sang account Y **không quit app**.
3. Lập tức chạy lại `ps aux | grep language_server`, so PID với bước 1.

**Đọc kết quả:**
- **PID KHÔNG đổi** → xác nhận đúng giả thuyết trên. Antigravity tự nó không restart process khi đổi account → **không sửa được từ phía mình**. Đóng plan này ở trạng thái "confirmed external limitation, no fix possible", giữ nguyên hành vi hiện tại (quit+reopen là cách duy nhất) - có thể cân nhắc thêm 1 dòng tooltip/hint nhỏ trong UI kiểu "đổi account? quit & mở lại Antigravity" nếu muốn (không bắt buộc, chỉ là UX polish nhỏ).
- **PID CÓ đổi** (process mới được spawn) → giả thuyết trên SAI, tức Antigravity *có* restart nội bộ nhưng app mình vẫn không nhận ra kịp thời - lúc đó mới là bug thật ở code mình, cần điều tra tiếp: khả năng poll interval quá dài (mặc định 30s, xem `refreshStore.js`), hoặc `discoverPorts`/`probeForConnectAPI` không tìm ra port mới đủ nhanh, hoặc lỗi nào đó trong `antigravity.rs::get_antigravity_usage` (dưới `agent_usage/`) khi xử lý process detection thay đổi giữa 2 poll liên tiếp.

## Việc cần user làm

Chỉ 3 bước ở trên (nhẹ, không cần `--debug`, không cần rebuild) - làm khi rảnh, không phải gấp cho release này. Kết quả (PID đổi hay không) dán lại đây hoặc báo trực tiếp để quyết định bước tiếp theo.

## Cross-refs

- `scripts/get-antigravity-usage.sh` - process detection + Connect RPC probe logic.
- `docs/arch/usage-antigravity.md` - kiến trúc tổng thể AG usage monitoring.
- `src-tauri/src/agent_usage/antigravity.rs::get_antigravity_usage` - nơi Rust gọi script này mỗi poll.

