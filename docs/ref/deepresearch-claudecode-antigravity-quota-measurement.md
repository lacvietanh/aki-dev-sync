# Nghiên cứu: Đo lường / giám sát quota Claude Code & Antigravity

> Tài liệu tổng hợp các phương pháp đo quota từ repo công khai, độ ổn định và edge case của
> từng cách, để đối chiếu với 9router. Soạn ngày 2026-06-23.

## Về độ tin cậy của nguồn

Dữ liệu trích từ phase fetch + adversarial-verify của một harness deep-research (104 agent,
~1.43M token). Nhãn độ tin cậy dùng trong tài liệu:

- **[3-0]** — đã qua adversarial verify 3 phiếu thuận.
- **[trích quote]** — có quote gốc từ README/docs nhưng chưa thấy verdict cuối.
- **[không rõ]** — repo tồn tại nhưng phase nghiên cứu không bắt được cơ chế; cần đọc trực tiếp.

Phase synthesize cuối của workflow vỡ vì lỗi schema; bảng dưới đây được ghép tay từ journal.

---

## 1. Năm HỌ phương pháp gốc

Mọi công cụ đều rơi vào 1 trong 5 cơ chế. Đây là khung đánh giá tỉ lệ ổn định.

| # | Họ phương pháp | Cách hoạt động | Ổn định | Vỡ khi nào |
|---|---|---|---|---|
| **P1** | Đọc stream/stdin do chính tool cấp | Claude Code ≥2.1.x tự đẩy `rate_limits.five_hour/seven_day` qua stdin của statusLine hook | ★★★★★ Cao nhất | Chỉ Pro/Max; bản CC cũ không có field; khi 429 CC cắt cụm `rate_limits` (lỗi đã biết) |
| **P2** | Parse file local JSONL (`~/.claude/projects/*.jsonl`) | Cộng token từ log hội thoại, tự suy ra hạn mức | ★★★★ Bền (thuần local) | Là **ước lượng**, không phải số server; lệch khi đổi chính sách giờ/model; không thấy dùng ở máy khác |
| **P3** | OAuth token → gọi usage endpoint | Đọc token local rồi GET endpoint usage chính thức (bán-công khai) | ★★★ Khá | Endpoint không tài liệu hóa, đổi/khóa bất ngờ; rate-limit gắt |
| **P4** | Local Language-Server endpoint (flow gốc Antigravity) | Hỏi LS cục bộ `127.0.0.1` bằng CSRF token | ★★ Đúng nguồn nhưng mong manh | Port động mỗi lần chạy; tên process/flag CSRF/schema JSON đổi theo version |
| **P5** | MITM / proxy intercept HTTPS | Cài root CA, sửa hosts, chặn giữa client↔cloud, đọc token thật từ response | ★★ Số liệu thật nhất, dễ vỡ + rủi ro ToS | Provider đổi domain/proto là gãy; phụ thuộc sudo/CA/hosts; rủi ro khóa tài khoản |

---

## 2. Claude Code — repo công khai

| Repo | Họ | Cơ chế chính xác | Ổn định / Edge case |
|---|---|---|---|
| [ryoppippi/ccusage](https://github.com/ryoppippi/ccusage) **[3-0]** | P2 | Đọc JSONL `~/.claude/projects/*/`, hỗ trợ `CLAUDE_CONFIG_DIR` + `--offline`. Không gọi API quota | Bền; chỉ fetch giá model (LiteLLM) để tính cost trừ khi `--offline`. Token đếm từ JSONL lệch nhẹ |
| [haasonsaas/claude-usage-tracker](https://github.com/haasonsaas/claude-usage-tracker) | P2 | Parse JSONL `~/.claude/projects/` (+ `~/.config/claude/projects/`), quy "giờ/model" theo tier Pro/Max | "Giờ" là ước lượng nội bộ, không phải số chính thức |
| [TylerGallenbeck/claude-code-limit-tracker](https://github.com/TylerGallenbeck/claude-code-limit-tracker) | P2 | Parse JSONL, dựng lại session từ timestamp file. Không API | Thuần local, ước lượng; không phản ánh hạn mức server |
| [aiedwardyi/claude-usage-monitor](https://github.com/aiedwardyi/claude-usage-monitor) **[3-0]** | P3 | Token từ `~/.claude/.credentials.json` (`claudeAiOauth.accessToken`) hoặc env `CLAUDE_CODE_OAUTH_TOKEN` → `GET https://api.anthropic.com/api/oauth/usage` (5h/7d), cache 5'. Header `anthropic-beta: oauth-2025-04-20` + `User-Agent: claude-code/<ver>` | Endpoint "authoritative" mà lệnh `/usage` của CC dùng; không tài liệu hóa |
| [jens-duttke/usage-monitor-for-claude](https://github.com/jens-duttke/usage-monitor-for-claude) **[3-0]** | P3 | OAuth token từ `~/.claude/.credentials.json`, "communicates exclusively with api.anthropic.com" | Vỡ nếu đổi vị trí credentials / endpoint |
| [ohugonnot/claude-code-statusline](https://github.com/ohugonnot/claude-code-statusline) | **P1** (+P3 fallback) | CC ≥2.1.x đẩy `rate_limits.five_hour/seven_day` qua **stdin** → 0 network. Fallback: gọi `oauth/usage` (~200ms, cache 5') | **Bền nhất** cho phần stdin; field chỉ có ở ≥2.1.x & Pro/Max |
| [Maciek-roboblog/Claude-Code-Usage-Monitor](https://github.com/Maciek-roboblog/Claude-Code-Usage-Monitor/issues/202) | P2→P3 | Gốc parse JSONL; issue #202 đề xuất chuyển `oauth/usage` làm "authoritative window state" | Nguồn reverse-engineer **độc lập** xác nhận lại endpoint `oauth/usage` |
| [phuryn/claude-usage](https://github.com/phuryn/claude-usage) · [withLinda/claude-JSONL-browser](https://github.com/withLinda/claude-JSONL-browser) | — | [không rõ] — không bắt được cơ chế trong file nghiên cứu | cần đọc trực tiếp |

---

## 3. Antigravity / Windsurf — repo công khai

| Repo | Họ | Cơ chế chính xác | Ổn định / Edge case |
|---|---|---|---|
| [llegomark/ag-telemetry](https://github.com/llegomark/ag-telemetry) | **P4** | Quét process (`ps -axo pid,args \| grep language_server`) lấy CSRF từ `--csrf_token`, `lsof` ra cổng → `POST https://127.0.0.1:{port}/exa.language_server_pb.LanguageServerService/GetUserStatus` (gRPC-Web, cert self-signed), header `X-Codeium-Csrf-Token`. Đọc `raw.userStatus.cascadeModelConfigData.clientModelConfigs[].quotaInfo.remainingFraction` (0..1) + `resetTime` | Quota **thật** từ IDE; dễ vỡ vì port động + tên process/flag CSRF |
| [tuckiestudio/antigravity-usage-monitor](https://github.com/tuckiestudio/antigravity-usage-monitor) | **P4** | Y hệt flow trên (Connect-RPC, `Connect-Protocol-Version: 1`, metadata `ideName='antigravity'`). Đọc per-model `remainingFraction`, `resetTime`, `isExhausted`, `tierName`, `promptCredits`, `email` | Per-model chính xác; cần CSRF token sống |
| [dwgx/WindsurfAPI](https://github.com/dwgx/WindsurfAPI) | **P5** | Local gRPC proxy chặn giữa Windsurf LS ↔ cloud, đọc proto `CortexStepMetadata.model_usage` (`inputTokens/outputTokens/cacheRead/cacheWrite`) | Token thật; phụ thuộc schema proto Cascade nội bộ |
| [badrisnarayanan/antigravity-claude-proxy](https://github.com/badrisnarayanan/antigravity-claude-proxy) | P3+state | Google OAuth token; tự "detect local session" từ **Antigravity local database**; expose `GET /account-limits?format=table` | Cần app AG cài local / Google account đã add |
| [jlcodes99/cockpit-tools](https://github.com/jlcodes99/cockpit-tools) (vscode-antigravity-cockpit) | P3+state | State ở `~/.antigravity_cockpit` (accounts/config/WebSocket); polling API chính thức 2–10' → remaining quota + reset per model; đa provider (AG, Windsurf, Codex, Copilot, Cursor, Kiro, Gemini-cli) | Khá bền nhưng bề mặt vỡ lớn vì nhiều provider |
| [ink1ing/anti-api](https://github.com/ink1ing/anti-api) | state | Quét credential local ở `~/.antigravity` và `~/.codeium`, biến AG/Codex/Copilot thành server tương thích Anthropic/OpenAI | Chủ yếu proxy auth; không nêu rõ field quota |
| [robinebers/openusage](https://github.com/robinebers/openusage/blob/main/docs/providers/antigravity.md) · [steipete/CodexBar](https://github.com/steipete/CodexBar/blob/main/docs/antigravity.md) · [skainguyen1412/antigravity-usage](https://github.com/skainguyen1412/antigravity-usage) · [Henrik-3/AntigravityQuota](https://github.com/Henrik-3/AntigravityQuota) · [ddarkr/antigravity-token-monitor](https://github.com/ddarkr/antigravity-token-monitor) · [60ke/antigravity-statusline](https://github.com/60ke/antigravity-statusline) · [AGI-is-going-to-arrive/Antigravity-Context-Window-Monitor](https://github.com/AGI-is-going-to-arrive/Antigravity-Context-Window-Monitor) · [wusimpl/AntigravityQuotaWatcher](https://github.com/wusimpl/AntigravityQuotaWatcher) | (đa số P4) | [không rõ chi tiết] — repo tồn tại, doc nhắc "reverse-engineered local endpoint" nhưng file không bắt được claim sống sót | cần đọc trực tiếp |

---

## 4. Flow gốc giám sát quota của AG IDE đã bị đào ngược

**Có, nhiều repo độc lập tái hiện cùng một flow.** Các artifact reverse-engineering trích chính xác:

- **Endpoint giám sát quota gốc:** `POST https://127.0.0.1:{port}/exa.language_server_pb.LanguageServerService/GetUserStatus` (gRPC-Web / Connect-RPC, cert self-signed).
- **Bằng chứng AG = nhánh Codeium/Windsurf:** header `X-Codeium-Csrf-Token`, namespace proto `exa.language_server_pb.*`.
- **Cách lấy port + token (không hardcode):** quét process → trích `--csrf_token` → `lsof -nP -iTCP -sTCP:LISTEN -p [PID]` ra cổng listen.
- **Schema JSON quota:** `raw.userStatus.cascadeModelConfigData.clientModelConfigs[].quotaInfo` → `remainingFraction` (0..1), `resetTime`, `isExhausted`, `tierName`, `promptCredits`, `email`.
- **File/thư mục state local:** `~/.antigravity`, `~/.codeium`, `~/.antigravity_cockpit`, và "Antigravity local database".
- **Windsurf token thật:** proto `CortexStepMetadata.model_usage`.

### Mâu thuẫn cần kiểm chứng

Tài liệu 9router (mục 5) nói AG dùng MITM hosts-redirect `cloudcode-pa.googleapis.com`. Nhưng
**0/104 agent** nhắc tới `cloudcode-pa.googleapis.com` / `~/.gemini`. Bằng chứng cộng đồng nghiêng
mạnh về flow Codeium LS `127.0.0.1` + `exa.language_server_pb`, không phải googleapis. Có thể cả
hai cùng tồn tại (proxy cloud cấp credits vs local IDE state). Cần đọc thẳng source 9router +
ag-telemetry để chốt.

---

## 5. 9router (#6) đặt vào đâu

Repo [`decolua/9router`](https://github.com/decolua/9router) (local proxy/gateway + dashboard;
fork phổ biến [`n9router`](https://github.com/nightwalker89/n9router)). Tài liệu kỹ thuật auto-gen:
[deepwiki.com/decolua/9router](https://deepwiki.com/decolua/9router).

- **Claude Code:** họ **P3** — OAuth login + auto refresh token (buffer 5'), đếm token từ
  **response body** (`normalizeUsage`), tự theo dõi cửa sổ reset 5h/weekly. Lưu credential vào
  `db.json` riêng (KHÔNG đọc `~/.claude`).
- **Antigravity:** họ **P5 (MITM)** — sửa hosts redirect `cloudcode-pa.googleapis.com` /
  `daily-cloudcode-pa` → `127.0.0.1:443`, TLS giả mạo (root CA tự cài), parse cooldown/429 từ
  **error body**, xoay account (round-robin / sticky).
- **Ổn định:** trung bình–thấp; phụ thuộc endpoint OAuth không công khai của Anthropic
  ([claude-code #31637](https://github.com/anthropics/claude-code/issues/31637): 429 không kèm
  `Retry-After`) + hostname/proto cứng cho MITM.

**Khác biệt cốt lõi:** 9router là **proxy nằm trên đường request** (đo bằng cách *đi qua* lưu
lượng), trong khi đa số repo còn lại là **observer thụ động** (đọc file/stdin/LS endpoint, không
chen vào request). Proxy cho token-count thật nhất nhưng đánh đổi độ bền + rủi ro ToS lớn nhất.

---

## 6. Kết luận & khuyến nghị

- **Claude Code — ổn định nhất:** **P1 (stdin `rate_limits.*`)** kiểu `ohugonnot/claude-code-statusline`,
  fallback **P3 `oauth/usage`** (xác nhận chéo 3 nguồn độc lập: aiedwardyi, jens-duttke, Maciek #202).
  Tránh phụ thuộc duy nhất P2 vì chỉ là ước lượng.
- **Antigravity — đúng nguồn nhất:** **P4 local LS `GetUserStatus`** (ag-telemetry, tuckiestudio).
  Cho quota thật per-model, nhưng phải xử lý port động + CSRF + version-drift của schema. MITM (P5,
  như 9router) chỉ nên dùng khi cần *can thiệp* request, không phải chỉ để *đọc* quota.
- **Việc nên làm tiếp:** kiểm chứng mâu thuẫn ở mục 4 — AG thực sự đi qua
  `cloudcode-pa.googleapis.com` (9router) hay LS `127.0.0.1/exa.language_server_pb` (cộng đồng)?
  Đọc trực tiếp source là cách duy nhất chốt.

---

## Phụ lục — nguồn tham chiếu

- https://github.com/ryoppippi/ccusage
- https://github.com/haasonsaas/claude-usage-tracker
- https://github.com/jens-duttke/usage-monitor-for-claude
- https://github.com/aiedwardyi/claude-usage-monitor
- https://github.com/TylerGallenbeck/claude-code-limit-tracker
- https://github.com/ohugonnot/claude-code-statusline
- https://github.com/Maciek-roboblog/Claude-Code-Usage-Monitor/issues/202
- https://github.com/llegomark/ag-telemetry
- https://github.com/tuckiestudio/antigravity-usage-monitor
- https://github.com/dwgx/WindsurfAPI
- https://github.com/badrisnarayanan/antigravity-claude-proxy
- https://github.com/jlcodes99/cockpit-tools
- https://github.com/ink1ing/anti-api
- https://github.com/robinebers/openusage/blob/main/docs/providers/antigravity.md
- https://github.com/steipete/CodexBar/blob/main/docs/antigravity.md
- https://github.com/decolua/9router  ·  https://deepwiki.com/decolua/9router
- https://github.com/anthropics/claude-code/issues/31637
