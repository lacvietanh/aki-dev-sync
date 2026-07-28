# Tham chiếu: các bucket quota của Claude (schema `rate_limits`)

> Tham chiếu ngắn cho `docs/arch/usage-claudecode.md` §4.1. Đọc cái này trước khi đụng vào bất kỳ chỗ nào xử lý `rate_limits`.
>
> Ghi rõ **[CHÍNH THỨC]** (Anthropic công bố) vs **[CỘNG ĐỒNG]** (quan sát được từ payload thật / tổng hợp của cộng đồng, không có tài liệu chính thức, có thể sai hoặc hết hạn bất cứ lúc nào).

## Nguồn dữ liệu

App chỉ đọc `~/.claude/rate-limits-cache.json` do statusLine hook ghi ra (§1 của arch doc). Cục `rate_limits` trong đó **cùng hình dạng** với cục mà endpoint OAuth usage của Anthropic trả về.

⚠️ **[CỘNG ĐỒNG]** Endpoint OAuth usage là **không công bố (unofficial)**. App **không** gọi nó (ràng buộc bất biến §5.2) - nhưng schema mô tả dưới đây là schema của nó, nên nó thay đổi theo nó. Đã đổi hình **4 lần trong 7 tuần**. Đừng coi bảng dưới là hợp đồng.

## Các key đã biết

| key | nghĩa | trạng thái |
|---|---|---|
| `five_hour` | cửa sổ trượt 5 giờ, dùng chung mọi model | [CHÍNH THỨC] có tài liệu về khái niệm cửa sổ 5h |
| `seven_day` | pool tuần **dùng chung** | [CHÍNH THỨC] khái niệm weekly limit |
| `seven_day_opus` | pool tuần riêng cho lớp Opus | [CỘNG ĐỒNG] tên key |
| `seven_day_sonnet` | pool tuần riêng cho lớp Sonnet | [CỘNG ĐỒNG] tên key |
| `seven_day_oauth_apps` | pool tuần cho app dùng OAuth ngoài CLI | [CỘNG ĐỒNG] tên key |
| `extra_usage` | phần dùng vượt trả bằng credit | [CỘNG ĐỒNG] tên key, hình dạng khác các key trên |

Chưa từng thấy trong payload thật tính tới 2026-07: `seven_day_fable`, `seven_day_mythos`. App đã có sẵn nhãn cho hai key này để nếu chúng xuất hiện thì không phải sửa code (arch §4.2).

## Hình dạng một entry

```json
{ "used_percentage": 42, "resets_at": 1782034800, "seen_at": 1782030000 }
```

- **[CỘNG ĐỒNG]** Tuỳ phiên bản, phần trăm đến dưới tên `used_percentage` **hoặc** `utilization`. Payload của statusLine hook mà app đọc dùng `used_percentage`.
- `resets_at`: Unix epoch giây, UTC. `0`/thiếu = "không rõ mốc reset".
- `seen_at`: **không phải của Anthropic** - do script `aki-rlcache v5` của app stamp vào lúc merge (arch §3), dùng để loại entry CLI không còn gửi nữa.
- **`null` = không áp dụng cho gói này**, không phải lỗi và không phải 0%. Bỏ qua, đừng vẽ.

## Quy tắc khi viết code đụng vào đây

1. **Parse như map mở.** Duyệt `Object.keys()` / `as_object()`, không bao giờ đọc key theo tên trừ khi logic thực sự nói về đúng bucket đó (ví dụ: `five_hour` cho hợp đồng stale, `seven_day` cho quy tắc làm mờ - hai trường hợp duy nhất trong repo này, cả hai đều có comment giải thích).
2. **Không đếm cứng cardinality.** "Hai thanh" là bug đã sửa, không phải thiết kế.
3. **Key lạ vẫn phải hiển thị được**, với nhãn suy ra từ key (arch §4.1), không in key thô.
4. **Không tự bịa giá trị** cho bucket vắng mặt.
