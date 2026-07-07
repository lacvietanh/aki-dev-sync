# Nghiên cứu: Đo usage tài khoản Claude khi chỉ dùng Claude app (không dùng Claude Code)

> Soạn ngày 2026-07-07. Nguồn: web research (subagent) + đối chiếu kiến trúc hiện tại của app.
> Bối cảnh: incident thực tế cùng ngày — user dùng đồng thời Claude Code và Claude app (Cowork);
> khi chỉ dùng app, usage % trên Aki Dev Sync **không cập nhật** vì không có turn Claude Code nào
> fire statusLine hook. Đây là điểm mù freshness của kiến trúc P1 hiện tại
> (xem `docs/arch/usage-claudecode.md` §2 Lỗi C).

## 1. Fact nền tảng: pool usage là CHUNG toàn tài khoản

Anthropic xác nhận chính thức: gói Pro/Max dùng **một pool usage chung** cho claude.ai web app,
Claude Desktop, mobile, Cowork và Claude Code — mọi hoạt động đều trừ vào cùng limit.

> "Both Pro and Max plans offer usage limits that are shared across Claude and Claude Code —
> all activity in both tools counts against the same usage limits"

Nguồn: https://support.claude.com/en/articles/11145838-use-claude-code-with-your-pro-or-max-plan

**Hệ quả cho app này:**
- Con số `rate_limits` mà statusLine hook nhận được từ server **đã bao gồm app usage** —
  vấn đề không phải scope mà là **freshness**: nó chỉ được ghi mới khi có một turn Claude Code.
- Không cần đo app usage "riêng" — chỉ cần một nguồn account-level cập nhật được mà
  **không phụ thuộc turn Claude Code**.

## 2. UI chính thức của claude.ai

`https://claude.ai/settings/usage` (Settings → Usage) hiển thị với Pro/Max/Team:
- Progress bar cửa sổ **5 giờ** (session) kèm giờ reset
- Progress bar **weekly** (7 ngày), gồm limit riêng cho Opus trên Max
- **Extra usage** (credit mua thêm) nếu bật

Warning banner xuất hiện trong chat khi gần chạm limit. Đây là "server truth" — cùng con số mà
mọi tool bên thứ ba scrape. Chỉ hữu ích cho user xem tay; scrape UI này là phương án tệ nhất.

Nguồn: https://support.claude.com/en/articles/11647753-how-do-usage-and-length-limits-work

## 3. Phương án A — OAuth endpoint (họ P3, khuyến nghị)

Endpoint không tài liệu hóa nhưng ổn định trong thực tế — chính là backend của lệnh `/usage`
"thật" phía server (đã được xác nhận chéo trong
`docs/ref/deepresearch-claudecode-antigravity-quota-measurement.md` §2, ba nguồn độc lập):

```
GET https://api.anthropic.com/api/oauth/usage
Authorization: Bearer <oauth_access_token>       # token Claude Code (~/.claude/.credentials.json hoặc OS keychain)
anthropic-beta: oauth-2025-04-20
User-Agent: claude-code/<version>                # BẮT BUỘC — thiếu sẽ dính bucket rate-limit riêng → 429 liên tục
```

Response JSON:

| Field | Nội dung |
|---|---|
| `five_hour` | `{ utilization: 0–100, resets_at: ISO8601 }` |
| `seven_day` | weekly limit tổng, cùng cấu trúc |
| `seven_day_opus` / `seven_day_sonnet` | limit theo model (có thể `null`) |
| `extra_usage` | `{ is_enabled, monthly_limit, used_credits, utilization }` |

Đặc điểm quyết định: đây là **trạng thái server-side cấp tài khoản** — phản ánh cả app usage,
cập nhật realtime, **không tốn quota** (khác probe session), không cần turn Claude Code nào.

Nguồn:
- https://github.com/Maciek-roboblog/Claude-Code-Usage-Monitor/issues/202
- https://github.com/anthropics/claude-code/issues/31021 (429 khi thiếu User-Agent)

## 4. Phương án B — endpoint nội bộ claude.ai + sessionKey (fallback)

```
GET https://claude.ai/api/organizations/{org_id}/usage
Cookie: sessionKey=sk-ant-sid01-...
```

- `org_id` lấy từ `GET https://claude.ai/api/organizations` (cùng cookie).
- Response tương tự: `five_hour` / `seven_day` / `seven_day_opus` (`utilization` %, `resets_at`),
  cùng `extra_usage` (`current_spending`, `budget_limit`).
- claude.ai đứng sau Cloudflare — gọi từ app native (ngoài browser context) hay bị bot-block;
  các app native đều yêu cầu user dán `sessionKey` và giả User-Agent browser.

Tool cộng đồng dùng cách này:

| Tool | Dạng |
|---|---|
| https://github.com/sshnox/Claude-Usage-Tracker | Chrome extension, zero-config (cookie sẵn có), MIT |
| https://github.com/hamed-elfayome/Claude-Usage-Tracker | macOS menu bar (Swift/SwiftUI), sessionKey |
| https://github.com/f-is-h/Usage4Claude | macOS menu bar, browser nhúng hoặc dán sessionKey, tự lấy org ID |
| https://github.com/lugia19/Claude-Usage-Extension | Firefox/Chrome extension lâu đời |
| https://github.com/jonis100/claude-quota-tracker | VS Code status bar, config sessionKey + orgId |
| https://github.com/jens-duttke/usage-monitor-for-claude | Windows tray app |
| https://github.com/niccolo-sabato/claude-usage-widget | Windows 11 taskbar widget |

## 5. So sánh & rủi ro

| Phương án | Auth | Ưu | Nhược/Rủi ro |
|---|---|---|---|
| **A. `api.anthropic.com/api/oauth/usage`** | OAuth token Claude Code sẵn có trên máy | Bao gồm app usage (pool chung); JSON sạch; không tốn quota; chính Claude Code dùng | Undocumented → có thể đổi schema; bắt buộc `User-Agent: claude-code/x.y.z`; cần refresh token khi hết hạn; bản CC mới lưu token trong OS keychain thay vì `.credentials.json` |
| **B. `claude.ai/api/organizations/{org}/usage`** | User dán cookie `sessionKey` | Hoạt động cả khi không cài Claude Code | UX kém (dán cookie, cookie rotate); Cloudflare bot-block; sessionKey là credential toàn quyền → nhạy cảm; rủi ro ToS cao hơn |
| **C. Scrape UI `claude.ai/settings/usage`** | Browser session | — | Fragile nhất, không phù hợp desktop app |

Rủi ro ToS chung: A và B đều là API không công bố; Consumer ToS cấm "automated access" không được
cho phép. Thực tế hệ sinh thái tool (ccusage, Usage Monitor, hàng chục extension) dùng nhiều năm
chưa thấy enforcement. Nguyên tắc an toàn: read-only, poll thưa (≥60s), fail gracefully khi schema
đổi, disclaimer "not affiliated with Anthropic".

## 6. Khuyến nghị cho Aki Dev Sync

1. **Phương án A làm nguồn refresh chính/bổ sung** cho panel Claude Code usage: vì pool chung,
   chỉ cần poll `oauth/usage` là các thanh 5h/7d **tự phản ánh cả app usage**, giải quyết đúng
   incident "chỉ dùng app thì số đứng im". Kiến trúc statusLine (P1) vẫn giữ — hai nguồn bổ trợ:
   P1 cho-free theo turn, P3 cho freshness khi không có turn.
2. **Giới hạn chung mọi phương án:** server chỉ trả **% utilization**, không trả token count —
   không thể breakdown token/chi phí phần dùng qua app (app không ghi transcript local).
3. Phương án B chỉ đáng làm nếu cần hỗ trợ máy **không cài Claude Code** — và phải chấp nhận
   xử lý Cloudflare + cookie hết hạn.
4. Nếu triển khai A: chú ý nguồn token — bản Claude Code mới không còn `.credentials.json`
   (đã chuyển OS keychain, xem `docs/arch/usage-claudecode.md` § Subscription Tier Fallback);
   trên remote Linux headless thường vẫn là file. Cần xác minh per-host trước khi code.

## 7. Recon thật trên host Linux remote (2026-07-07, sau khi code Phase 1)

Xác nhận bằng request thật tới endpoint (token hợp lệ dạng nhưng SAI giá trị — không đọc
`.credentials.json` thật của máy, agent bị chặn đọc trực tiếp file credential bởi permission
classifier khi thử recon bằng token thật; xem thêm ghi chú trong
`docs/plan/claudecode-oauth-usage-p3.md` Bước 0):

- [x] **HTTP status khi token sai/hết hạn: `401`** (không phải 404/403) — request thật tới
      `https://api.anthropic.com/api/oauth/usage` với `Authorization: Bearer <token bịa>` +
      `anthropic-beta: oauth-2025-04-20` + `User-Agent: claude-code/2.1.0` trả về `401` ngay,
      không bị chặn ở tầng UA/rate-limit trước khi tới bước auth.
- [x] **UA `claude-code/2.1.0` không bị 429 tiền-auth** — request tới được tầng auth-check (401),
      nghĩa là UA này ít nhất không bị chặn sớm; chưa loại trừ khả năng có UA khác "đúng hơn".
- [ ] **Tên field thật** (`utilization` vs `used_percentage`), **`resets_at`** ISO8601 hay epoch,
      **response khi idle** (window absent) — CHƯA xác nhận được vì cần token thật hợp lệ để thấy
      response 200 thân thật; code (`scripts/get-claudecode-usage.sh`) đã viết để chấp nhận cả hai
      dạng field name và cả hai dạng resets_at (epoch số hoặc ISO8601 string), nhưng chưa có mẫu
      response 200 thật để khóa lại. Cần chạy trên host có token hợp lệ (Mac, hoặc chính user chạy
      tay lệnh recon ở Bước 0 của plan) và dán kết quả vào đây.
- [ ] Vòng đời token, `claude auth status` có refresh không, rotation refresh token — vẫn cần đo
      trên Mac theo plan (B1, không làm được trên Linux remote không có Claude app thật).

## 8. Recon thật trên Mac (2026-07-07, tự user chạy) — finding CHẶN Phase 1 trên Mac

User tự chạy lệnh recon Bước 0 trên Mac (`aki@Aki-MBA16`). Kết quả:

```
Traceback (most recent call last):
  File "<string>", line 1, in <module>
FileNotFoundError: [Errno 2] No such file or directory: '/Users/aki/.claude/.credentials.json'
```

- [x] **Mac KHÔNG có `~/.claude/.credentials.json`** — xác nhận thật, không còn nghi ngờ. Bản Claude
      Code hiện tại trên máy Mac hàng ngày của user lưu credential hoàn toàn ở OS keychain, không có
      fallback file nào. → Oauth block (`scripts/get-claudecode-usage.sh`) sẽ luôn thoát sớm với
      `oauth: no token` trên máy này — an toàn (đúng thiết kế fail-open) nhưng **vô dụng trên chính
      máy cần giải quyết incident gốc**.
- [x] **Response envelope khi Bearer rỗng** (do `$TOKEN` rỗng vì python lỗi trước, curl vẫn chạy với
      header `Authorization: Bearer ` trống): server trả **`rate_limit_error`**, không phải lỗi auth:
      ```json
      {
        "error": {
          "type": "rate_limit_error",
          "message": "Rate limited. Please try again later."
        }
      }
      ```
      Xác nhận thêm: error envelope của endpoint theo đúng dạng chuẩn Anthropic API
      (`{"error": {"type", "message"}}`) — khác hẳn dạng response 401 thô đã thấy trên Linux (chỉ có
      HTTP status, chưa parse body). Rate limit ở đây nhiều khả năng do gọi liên tục nhiều lần trong
      thời gian ngắn (từ test Linux trước đó + lần này), không phải do thiếu UA.

**Quyết định (2026-07-07):** user báo effort test Mac tốn quá nhiều thời gian/công sức, ảnh hưởng công
việc → **tạm dừng** yêu cầu thêm test/điều tra Mac cho Phase 1. Không chase keychain-path ngay bây
giờ. Xem finding + quyết định đầy đủ ở đầu `docs/plan/claudecode-oauth-usage-p3.md`.
