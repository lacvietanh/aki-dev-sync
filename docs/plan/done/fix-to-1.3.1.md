# Fix & Improve Plan → v1.3.1+

> Tổng hợp từ audit toàn codebase ngày 2026-06-24.  
> Phân loại: **[CRITICAL] [HIGH] [MEDIUM] [LOW] [FEATURE]**

---

## 1. Bugs & Flow Integrity

### [HIGH] `useAgentUsage.js` — `checkUsage()` không có concurrency guard

**File:** `src/composables/useAgentUsage.js:57–58`

`forceSync()` được gọi fire-and-forget (không `await`) bên trong `checkUsage`. Ngoài ra `checkUsage` bản thân không có `isSyncing`-style guard — nhiều poll tick hoặc `manualRefreshCount` watch có thể trigger đồng thời, dữ liệu bị interleave.

**Fix:**
```js
let isChecking = false
async function checkUsage() {
  if (isChecking) return
  isChecking = true
  try { ... } finally { isChecking = false }
}
```
Thay `forceSync()` thành `await forceSync()` ở chỗ gọi trong `checkUsage`.

---

### [HIGH] `useLogs.js:68` — `copyLogs` nuốt lỗi clipboard hoàn toàn im lặng

**File:** `src/composables/useLogs.js:68`

```js
} catch (err) {}
```

`navigator.clipboard.writeText()` có thể fail (permission, focus). User không có feedback.

**Fix:** Thêm `Toast.fire({ icon: 'error', title: 'Copy failed' })` trong catch.

---

### [HIGH] `sync.rs` — `.unwrap()` trên poisoned Mutex có thể crash app

**File:** `src-tauri/src/sync.rs:240, 255`

```rust
let mut map = versions_map.lock().unwrap();
```

Nếu thread khác panic trong khi giữ `RSYNC_VERSIONS` lock → mutex poisoned → `unwrap()` panic → toàn bộ Tauri process sập.

**Fix:**
```rust
let mut map = versions_map.lock().unwrap_or_else(|e| e.into_inner());
```

---

### [HIGH] `ssh.rs:78` — `.expect()` có thể crash app

**File:** `src-tauri/src/ssh.rs:78`

```rust
let ssh_dir = config.parent().expect("~/.ssh has no parent");
```

**Fix:** Dùng `ok_or_else(|| anyhow!("..."))` và propagate với `?`.

---

### [HIGH] `provision-claudecode.sh` — `/tmp/patch.sh` không cleanup + bak file rác

**File:** `scripts/provision-claudecode.sh`

- Nếu `sed` fail: `/tmp/patch.sh` không bị xóa.
- `sed -i.bak` tạo `statusline-command.sh.bak` trên remote mỗi lần provision, không bao giờ dọn.

**Fix:**
```sh
trap 'rm -f /tmp/patch.sh' EXIT
# sau sed thành công:
rm -f "${FILE}.bak"
```

---

### [HIGH] Shell scripts thiếu `set -e` / `set -o pipefail`

**Files:** `scripts/get-claudecode-usage.sh`, `scripts/force-sync-claudecode.sh`

Python parse fail silently. `run_usage` fail không chặn tiếp tục chạy probe. `2>/dev/null` khắp nơi che khuất lỗi thực sự.

**Fix:** Thêm `set -e` đầu script. Điều chỉnh các `[ ... ] 2>/dev/null` guards để tương thích.

---

### [MEDIUM] `useAgentUsage.js` — `manualRefreshCount` watch không debounce

**File:** `src/composables/useAgentUsage.js:132–140`

User spam click refresh → nhiều `checkUsage()` đồng thời (chỉ `forceSync` có `isSyncing`, `checkUsage` thì không). Fix: dùng guard `isChecking` từ issue trên, hoặc debounce 300ms.

---

### [MEDIUM] `get-claudecode-usage.sh` — `auth-cache.json` corrupt → `cat` exit 0, Rust parse fail im lặng

**File:** `scripts/get-claudecode-usage.sh:36–40`

File tồn tại nhưng JSON truncated → `cat` thành công, Rust `serde_json::from_str` fail → không có email/orgName, không có lỗi rõ ràng.

**Fix:** Validate JSON trước khi dùng:
```sh
AUTH_INFO=$(python3 -m json.tool "$AUTH_CACHE" 2>/dev/null) || AUTH_INFO='{}'
```

---

### [MEDIUM] `agent_usage.rs` — `eprintln!` debug output còn trong production

**File:** `src-tauri/src/agent_usage.rs:82`

```rust
eprintln!("[force_sync] remote stderr: {}", err.trim());
```

Xóa hoặc convert sang Tauri log plugin.

---

## 2. Vue Pattern Issues

### [MEDIUM] `AgentUsage.vue` — 2 props khai báo nhưng không dùng

**File:** `src/components/AgentUsage.vue:212–213`

```js
locationType: String,  // không dùng trong template hay script
hostName: String,      // không dùng trong template hay script
```

Xóa cả 2 props và chỗ pass từ `AgentUsageSection.vue` (lines 15, 39).

---

### [MEDIUM] `AgentUsage.vue:334` — Clock timer `setInterval` chạy cả cho `antigravity`

`ccNow` chỉ dùng bởi CC reset lines. Timer 1 phút/lần chạy vô ích trên mọi instance.

**Fix:**
```js
onMounted(() => {
  if (props.agentId === 'claudecode') {
    ccClockTimer = setInterval(...)
  }
})
```

---

### [MEDIUM] `useSsh.js`, `useProjects.js`, `useLogs.js` — Module-scope `ref` là anti-pattern

Tất cả state khai báo ngoài factory function. Trong Tauri single-window OK ở production, nhưng Vite HMR re-evaluate module → refs bị recreate → reactive connection cũ bị đứt, phải full reload. Là DX issue nghiêm trọng khi dev.

**Note:** Refactor này tương đối lớn, cần cân nhắc riêng.

---

## 3. Template & Accessibility

### [LOW] `ProjectTable.vue` — 2 icon-only buttons thiếu `aria-label`

**File:** `src/components/ProjectTable.vue:123, 161`

`.btn-action-git` (Git Actions) và gear button có `title` nhưng thiếu `aria-label`. Vi phạm CLAUDE.md rule.

**Fix:** Thêm `aria-label="Git Actions"` và `aria-label="Project Settings"`.

---

### [LOW] `ProjectTable.vue:374` — Dead scoped CSS class `.popup-divider`

Khai báo trong `<style scoped>` nhưng không có element nào trong template dùng.

---

## 4. Changelog Modal — Refactor theo `BaseModal` pattern

**File:** `src/components/AppHeader.vue:63–77`

Hiện tại changelog dùng `Swal.fire()` với inline HTML string — rời rạc, không nhất quán với tất cả modal khác trong app đang dùng `BaseModal.vue` (SshConfigModal, ProjectConfigModal, GitModal, SpecialPushModal, IntroModal...).

**Vấn đề cụ thể:**
- Style hardcode inline (`style="text-align: left; font-size: 13px; ..."`) — không dùng CSS variables
- Background/color riêng biệt không sync với theme
- Không có `Teleport to="body"` pattern → z-index risk
- Không có Esc-to-close qua `BaseModal` handler
- `runMermaid()` gọi trong `didOpen` Swal hook — fragile
- Không tái dùng được `modal-header`, `modal-body`, `modal-footer` pattern

**Fix:** Tạo `src/components/modals/ChangelogModal.vue`:
```vue
<BaseModal :show="show" @close="$emit('close')" container-class="changelog-modal">
  <template #title>Changelog</template>
  <div class="modal-body changelog-body" v-html="renderedChangelog" ref="bodyRef" />
</BaseModal>
```
- `renderedChangelog` = `renderMarkdown(changelogText)` computed một lần
- `runMermaid()` trong `watch(show, ...)` khi `true`
- CSS class `changelog-modal` + `changelog-body` trong scoped style
- State: `showChangelog ref(false)` trong `AppHeader.vue`, trigger bởi click version badge

---

## 5. [FEATURE] Countdown Ring cho Git & Remote Diff refresh

**Inspired by:** SVG `stroke-dashoffset` ring trên reload button của usage (AgentUsage.vue) — cực kỳ elegant và WKWebView-safe.

**Ý tưởng:** Hiển thị passive countdown ring trên bảng project table để báo hiệu khi nào sẽ auto-refresh tiếp theo cho:
1. **Git status** (interval: `refreshSettings.git_interval_s`, default 60s)
2. **Remote diff / sync check** (interval: `refreshSettings.remote_diff_interval_s`, default 60s)

**Không cần nút chủ động** — chỉ là visual indicator display-only.

**Vị trí đặt:** Header row của `ProjectTable.vue`, góc phải hoặc bên cạnh từng cột GIT / LAST SYNC, hoặc trong `AppHeader.vue` gần nút REFRESH.

**Cơ chế:**

```
refreshStore.git_interval_s = 60
↓
useBackgroundRefresh.js → gitTimer setInterval(60s)
↓
gitRefreshKey tăng mỗi lần timer fire (tương tự drainKey)
↓
SVG ring reset animation → fill lại từ đầu
```

**Implementation sketch:**

`useBackgroundRefresh.js` export thêm:
```js
export const gitRefreshKey = ref(0)   // tăng mỗi lần gitTimer fire
export const diffRefreshKey = ref(0)  // tăng mỗi lần diffTimer fire
```

Trong `restartGitTimer()`:
```js
gitTimer = setInterval(() => {
  projects.value.forEach(p => fetchGitStatus(p.id, true))
  gitRefreshKey.value++   // trigger ring reset
}, s * 1000)
```

SVG ring component (tái sử dụng pattern từ AgentUsage):
```vue
<svg class="refresh-ring" viewBox="0 0 36 36" aria-hidden="true">
  <circle class="ring-fill" :key="gitRefreshKey"
    cx="18" cy="18" r="15" fill="none" stroke="rgba(0,210,255,0.5)" stroke-width="2"
    :style="{ animationDuration: refreshSettings.git_interval_s + 's' }" />
</svg>
```

CSS animation giống hệt AgentUsage (`stroke-dasharray: 94.25`, `@keyframes drain`).

**Khi `git_interval_s = 0`** (disabled): ring ẩn bằng `v-if`.

**Khả năng tái dùng:** Extract thành `<RefreshRing :interval-s="..." :refresh-key="..." :color="..." />` — dùng cho cả usage, git, và diff mà không duplicate animation code.

---

## Tóm tắt ưu tiên

| Priority | Issue | File |
|---|---|---|
| 🔴 Fix ngay | Mutex `unwrap()` panic | `sync.rs:240,255` |
| 🔴 Fix ngay | `provision-claudecode.sh` trap + bak cleanup | `provision-claudecode.sh` |
| 🟠 Quan trọng | `checkUsage` concurrency guard | `useAgentUsage.js` |
| 🟠 Quan trọng | `set -e` cho shell scripts | `get-claudecode-usage.sh`, `force-sync-claudecode.sh` |
| 🟡 Cleanup | Dead props `locationType`/`hostName` | `AgentUsage.vue:212–213` |
| 🟡 Cleanup | Clock timer guard `agentId === 'claudecode'` | `AgentUsage.vue:334` |
| 🟡 Cleanup | `copyLogs` silent catch | `useLogs.js:68` |
| 🟡 Cleanup | `eprintln!` debug output | `agent_usage.rs:82` |
| 🟡 Cleanup | `aria-label` 2 buttons + dead `.popup-divider` | `ProjectTable.vue` |
| 🟡 Cleanup | `auth-cache.json` JSON validation | `get-claudecode-usage.sh` |
| 🔵 Refactor | Changelog modal → `BaseModal` pattern | `AppHeader.vue` → `ChangelogModal.vue` |
| 🟢 Feature | Countdown ring cho git/diff refresh | `useBackgroundRefresh.js` + `ProjectTable.vue` |
