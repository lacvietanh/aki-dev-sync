# Share: Aki StatusLine cho Claude Code

**Status:** DONE - 2026-07-15 (commit `e461460`). `share/aki-statusLine/` đã publish (statusline.sh + demo.png), README đã có section giới thiệu. Đồng thời statusline customizer đã tích hợp vào app (1.10.0 `f4be8a9`, auto-install 1.10.1 `3e52a99`, hook v3 1.12.0 `f121b27`).
**Ngày:** 2026-07-15

---

## Bối cảnh

Qua nhiều vòng chỉnh tay, đã hoàn thiện 1 bộ statusline script cho Claude Code CLI (`~/.claude/statusline-command.sh`) hiển thị đầy đủ thông tin hữu dụng trên 1 dòng: identity, workspace, model+effort, context window %, rate limits 5h/7d, session duration/lines/cost. Có dynamic color 4 bậc theo ngưỡng % (green/yellow/orange/red) và cache rate_limits qua `aki-rlcache v2` để không mất data khi Claude bỏ block `rate_limits` (429 quirk).

Script này vốn phát triển song song với tính năng quota monitoring của app, dùng chung cơ chế `aki-rlcache v2` mà `provision-claudecode.sh` đã tiêm. Nay tách ra thành asset share độc lập để cộng đồng dùng được mà không cần cài Aki Dev Sync.

## Nội dung đã có

```
share/aki-statusLine/
├── statusline.sh    # Script hoàn chỉnh, drop-in replacement cho statusline-command.sh
└── demo.png         # Ảnh infographic giải thích từng field + color scheme
```

### statusline.sh - output mẫu

```
aki@Aki-M | kinhdich.akinet.me | opus 4.8 med | ctx 5% 56.9k/1M | 5h:38%  7d:52% | 3h21m +148/-24 $8.08
```

### Cài đặt (user hướng dẫn)

```bash
# 1. Copy script vào đúng vị trí Claude Code mong đợi
cp share/aki-statusLine/statusline.sh ~/.claude/statusline-command.sh
chmod +x ~/.claude/statusline-command.sh

# 2. Bật statusLine trong Claude Code settings
# Thêm vào ~/.claude/settings.json:
#   "statusLine": { "command": "$HOME/.claude/statusline-command.sh" }
```

Yêu cầu: `jq` phải có trên PATH (macOS: `brew install jq`, Ubuntu: `apt install jq`).

## Liên hệ với codebase hiện tại

- **`provision-claudecode.sh`**: Tiêm block `aki-rlcache v2` vào `statusline-command.sh` trên remote. Script share này **đã tích hợp sẵn** block đó (dòng 4-13), nên nếu user cài script share → provision sẽ nhận marker và skip (idempotent, không conflict).
- **`statusline-customizer.md`** (plan): Kế hoạch xây UI customizer trong app. Script share này chính là **default preset** mà customizer sẽ dùng làm baseline.

## Việc cần làm để share

1. **README.md** - Thêm section ngắn (dưới `## 📦 Install` hoặc cuối) giới thiệu statusline share, link tới `share/aki-statusLine/`, nhúng `demo.png`.
2. **Commit & push** - `share/` hiện untracked, cần `git add share/` rồi commit.

## Việc KHÔNG làm

- Không sửa `provision-claudecode.sh` - script share đã compatible, không cần thay đổi gì.
- Không sửa `statusline-customizer.md` - plan đó vẫn đúng, script share là input cho nó.
