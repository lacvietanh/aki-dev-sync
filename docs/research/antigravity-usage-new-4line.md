# Nghiên cứu: Cơ Chế Bộ Đếm Quota Mới (4 Chỉ Số) Trên Antigravity IDE 2.1.1+

* **Tác giả:** Antigravity / Lạc Việt Anh Workflow
* **Ngày thực hiện:** 24/06/2026
* **Bối cảnh:** Google Antigravity IDE phiên bản 2.1.1 nâng cấp cơ chế quản lý quota, phân chia tường minh các hạn ngạch theo hai nhóm mô hình độc lập (Gemini Models và Claude/GPT Models), mỗi nhóm chia sẻ một giới hạn tuần (Weekly Limit) và giới hạn 5 giờ (Five Hour Limit) riêng biệt.

---

## 📌 1. Mục Tiêu Nghiên Cứu
* Xác định nguồn cấp dữ liệu gốc (Authoritative Source) cho 4 thanh hạn ngạch mới hiển thị trên giao diện của Antigravity IDE 2.1.1+.
* Thực hiện các thử nghiệm Connect RPC cục bộ trên tiến trình Language Server để truy vấn chính xác các giá trị hạn ngạch tuần và hạn ngạch 5 giờ.
* Đề xuất phương án thiết kế giao diện dạng vòng tròn tiến trình (radial) siêu gọn để đưa cả 4 thanh của Antigravity và 2 thanh của Claude Code lên giao diện Aki Dev Sync.

---

## 🔬 2. Quá Trình Thử Nghiệm Thực Tế

### Thử nghiệm 1: Phân tích tiến trình và Endpoint Connect RPC cũ
* **Thời gian:** 13:28 ICT - 24/06/2026
* **Phương pháp:** Gọi endpoint Connect RPC cũ `/GetUserStatus` với csrf token và port quét được qua tiến trình `language_server_macos_arm`.
* **Kết quả:** Endpoint chỉ trả về thông số `quotaInfo` lồng bên trong từng cấu hình mô hình (`clientModelConfigs`), chứa một `remainingFraction` và `resetTime` duy nhất.
  * Các mô hình Gemini có chung `remainingFraction` khoảng `0.444` và `resetTime` trong vòng 2.5 giờ tới.
  * Các mô hình Claude/GPT có chung `remainingFraction` khoảng `0.010` và `resetTime` trong vòng 3 giờ tới.
  * **Hạn chế:** Endpoint cũ hoàn toàn không trả về các thông số liên quan đến Weekly Limit (giới hạn tuần).

### Thử nghiệm 2: Điều tra tĩnh và quét RPC Services trong tệp nhị phân
* **Thời gian:** 13:33 ICT - 24/06/2026
* **Phương pháp:** Sử dụng công cụ phân tích tĩnh `strings` quét tệp nhị phân `/Applications/Antigravity IDE.app/Contents/Resources/app/extensions/antigravity/bin/language_server_macos_arm` để tìm kiếm các hàm handler Connect RPC mới.
* **Kết quả:** Phát hiện ra một handler mới:
  `7_LanguageServerService_RetrieveUserQuotaSummary_Handler`
  tương ứng với endpoint:
  `/exa.language_server_pb.LanguageServerService/RetrieveUserQuotaSummary`

### Thử nghiệm 3: Gọi thử nghiệm RPC `RetrieveUserQuotaSummary`
* **Thời gian:** 13:33 ICT - 24/06/2026
* **Phương pháp:** Viết script Node.js tạm thời gửi yêu cầu POST đến cổng LS cục bộ và gọi endpoint `/RetrieveUserQuotaSummary` đính kèm CSRF token bảo mật.
* **Kết quả:** Gọi thành công (Status 200 OK), trả về cấu trúc dữ liệu JSON sạch sẽ và đầy đủ cho cả 4 chỉ số:
  ```json
  {
    "response": {
      "groups": [
        {
          "displayName": "Gemini Models",
          "description": "Models within this group: Gemini Flash, Gemini Pro",
          "buckets": [
            {
              "bucketId": "gemini-weekly",
              "displayName": "Weekly Limit",
              "description": "You have used some of your weekly limit, it will fully refresh in 3 days, 16 hours.",
              "window": "weekly",
              "remainingFraction": 0.21379915,
              "resetTime": "2026-06-27T23:40:29Z"
            },
            {
              "bucketId": "gemini-5h",
              "displayName": "Five Hour Limit",
              "description": "You have used some of your 5-hour limit, it will fully refresh in 2 hours, 18 minutes.",
              "window": "5h",
              "remainingFraction": 0.4300308,
              "resetTime": "2026-06-24T09:10:11Z"
            }
          ]
        },
        {
          "displayName": "Claude and GPT models",
          "description": "Models within this group: Claude Opus, Claude Sonnet, GPT-OSS",
          "buckets": [
            {
              "bucketId": "3p-weekly",
              "displayName": "Weekly Limit",
              "description": "You have used some of your weekly limit, it will fully refresh in 6 days, 3 hours.",
              "window": "weekly",
              "remainingFraction": 0.30314785,
              "resetTime": "2026-06-30T10:39:59Z"
            },
            {
              "bucketId": "3p-5h",
              "displayName": "Five Hour Limit",
              "description": "You have used some of your 5-hour limit, it will fully refresh in 2 hours, 47 minutes.",
              "window": "5h",
              "remainingFraction": 0.0107236,
              "resetTime": "2026-06-24T09:39:17Z"
            }
          ]
        }
      ]
    }
  }
  ```

---

## 📈 3. Kết Luận & Hướng Triển Khai
1. **Nguồn dữ liệu:** RPC `/RetrieveUserQuotaSummary` là nguồn dữ liệu chuẩn nhất, cung cấp trực tiếp 4 buckets hạn ngạch của 2 nhóm mô hình.
2. **Hợp nhất dữ liệu:** Script backend cần gọi đồng thời `/GetUserStatus` để lấy email tài khoản và `/RetrieveUserQuotaSummary` để lấy chi tiết quota.
3. **Cách hiển thị UI:** Chuyển đổi hiển thị dạng thanh tiến trình ngang truyền thống thành các vòng tròn tiến trình (radial indicator) để tiết kiệm không gian.
   * Antigravity hiển thị 4 vòng tròn (Gemini 5h, Gemini Wk, Claude 5h, Claude Wk).
   * Claude Code hiển thị 2 vòng tròn (5h, 7d/Weekly), trong đó Weekly hiển thị màu xám và chữ `N/A` nếu thiếu dữ liệu (fallback cho môi trường remote của người dùng).
4. **Cách tính %:** Dùng **Used %** (`(1 - remainingFraction) * 100`) để nhất quán với Claude Code hiện tại.
