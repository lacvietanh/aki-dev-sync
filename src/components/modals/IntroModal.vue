<template>
  <BaseModal :show="showIntroModal" @close="closeIntroModal">
    <template #title>
      <i class="fa-solid fa-book-open mr-1" style="color: #6ee7b7;"></i> Hướng Dẫn Sử Dụng - Aki Dev Sync
    </template>

    <div class="modal-body scrollable intro-content">
      <div class="intro-header mb-3">
        <h3>🚀 Aki Dev Sync</h3>
        <p>
          Command Center để đồng bộ code giữa <strong>Máy Local</strong> và <strong>Máy Remote</strong> qua SSH/rsync  - 
          không lo treo giao diện (UI freeze), không làm bẩn lịch sử Git bằng các commit tạm.
        </p>
      </div>

      <!-- Mental Model: Local <-> Remote -->
      <div class="model-flow mb-3">
        <div class="model-node local">
          <div class="model-role">LOCAL</div>
          <div class="model-title">Source of Truth</div>
          <div class="model-meta">Git · Antigravity</div>
        </div>
        <div class="model-arrows">
          <span class="arrow-push"><i class="fa-solid fa-arrow-right"></i> PUSH</span>
          <span class="arrow-pull">PULL <i class="fa-solid fa-arrow-left"></i></span>
        </div>
        <div class="model-node remote">
          <div class="model-role">REMOTE</div>
          <div class="model-title">AI Workspace</div>
          <div class="model-meta">Claude Code · Tác vụ nặng</div>
        </div>
      </div>

      <!-- Use Cases -->
      <div class="alert-box info mb-3">
        <h4 class="alert-title"><i class="fa-solid fa-earth-americas"></i> Ai cần dùng?</h4>
        <ul class="custom-list">
          <li><strong>Máy yếu ↔ Server cấu hình cao:</strong> Viết code nhẹ nhàng ở máy Local, chuyển các tác vụ build nặng / chạy AI lên Server.</li>
          <li><strong>Bảo mật mã nguồn:</strong> Tách biệt môi trường công việc, lưu giữ mã nguồn cốt lõi (core code) trên Remote riêng.</li>
          <li><strong>Đồng bộ đa thiết bị:</strong> Đồng bộ nhanh giữa PC, Laptop và Server mà không tạo commit nháp trên GitHub.</li>
          <li><strong>AI Workspace:</strong> Đẩy toàn bộ project (kèm <code>.git/</code>) lên Remote để AI đọc hiểu đầy đủ ngữ cảnh.</li>
        </ul>
      </div>

      <!-- SYNC features -->
      <div class="subgroup-label mb-1">⚡ SYNC</div>
      <div class="features-grid mb-3">
        <div class="feature-card">
          <div class="feature-icon"><i class="fa-solid fa-arrow-up"></i></div>
          <div class="feature-text">
            <strong>PUSH</strong>
            <span>Đẩy code Local → Remote, kèm mọi thứ không nằm trong <code>push_excludes</code>, cùng cấu hình "Force Delete" riêng từng dự án. Thư mục chỉ có trong <code>pull_excludes</code> là <strong>push-only</strong> - đẩy lên nhưng không kéo về và không tính là thay đổi (mặc định <code>.git/</code>: AI có đủ history, badge PUSH không sáng vì git tự dọn nội bộ).</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #f59e0b;"><i class="fa-solid fa-bolt"></i></div>
          <div class="feature-text">
            <strong>SELECT (Push Special)</strong>
            <span>Mở hộp thoại chọn file native của macOS (multi-select). Nếu file đã tồn tại ở Remote, hiển thị bảng so sánh mtime Local / Remote trước khi xác nhận ghi đè.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #3b82f6;"><i class="fa-solid fa-arrow-down"></i></div>
          <div class="feature-text">
            <strong>PULL</strong>
            <span>Lấy file (Pull) sau khi code trên Remote về lại Local để kiểm tra và commit.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #22c55e;"><i class="fa-solid fa-shield-halved"></i></div>
          <div class="feature-text">
            <strong>DRY RUN</strong>
            <span>Xem trước (Dry Run) các file sẽ thay đổi - không ghi đè dữ liệu thực tế cho đến khi tắt chế độ này.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #a78bfa;"><i class="fa-solid fa-circle-dot"></i></div>
          <div class="feature-text">
            <strong>Sync Status</strong>
            <span>Tự động kiểm tra - nút PUSH/PULL sáng lên để thông báo khi dữ liệu giữa hai phía có sự chênh lệch.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #f97316;"><i class="fa-solid fa-code"></i></div>
          <div class="feature-text">
            <strong>Pre / Post Hooks</strong>
            <span>Script chạy trước/sau mỗi lần sync (build, restart service, notify...), chạy Local hoặc Remote tuỳ chọn.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #ef4444;"><i class="fa-solid fa-clone"></i></div>
          <div class="feature-text">
            <strong>Mirror / Delete</strong>
            <span>Bật <code>--delete</code> để mirror chính xác. Mặc định Push không xóa; nếu kích hoạt, việc push ghi đè lên thay đổi mới ở Remote sẽ yêu cầu xác nhận.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #14b8a6;"><i class="fa-solid fa-layer-group"></i></div>
          <div class="feature-text">
            <strong>Exclude Presets</strong>
            <span>Cấu hình rsync exclude riêng cho Push/Pull, hỗ trợ cấu hình nhanh chỉ với 1-click: Nuxt 4, Tauri v2, Aki Default.</span>
          </div>
        </div>
      </div>

      <!-- TOOLS & MONITOR features -->
      <div class="subgroup-label mb-1">🛠 TOOLS & MONITOR</div>
      <div class="features-grid">
        <div class="feature-card">
          <div class="feature-icon" style="color: #00d2ff;"><i class="fa-solid fa-list-check"></i></div>
          <div class="feature-text">
            <strong>Project Tasks & Notes</strong>
            <span>Quản lý task: Pin 📌, Wish 🕒, và Done (tự động gỡ ghim). Tích hợp thẻ ghi chú Project Notes có khả năng co giãn chiều cao native bằng CSS (`field-sizing: content`) và tự động trim khoảng trắng thừa.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #06b6d4;"><i class="fa-solid fa-grip"></i></div>
          <div class="feature-text">
            <strong>Open Popup & Stack Launcher</strong>
            <span>Mở nhanh IDEs Local và Remote. Nút <strong>DEV</strong> (xanh) và <strong>BUILD</strong> (vàng) tự động theo stack, tooltip hiển thị lệnh thực thi. Quét lockfile để gọi đúng Package Manager. Mọi cửa sổ Terminal app mở ra đều tự snap 124 cột vào góc trên-phải màn hình chính.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #f59e0b;"><i class="fa-solid fa-note-sticky"></i></div>
          <div class="feature-text">
            <strong>Global Note</strong>
            <span>Tấm ghi chú toàn cục trên titlebar - icon chuyển màu vàng khi có nội dung. Không gắn với project nào, tự lưu sau 500ms vào <code>appDataDir</code>.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #818cf8;"><i class="fa-solid fa-chart-bar"></i></div>
          <div class="feature-text">
            <strong>Agent Usage</strong>
            <span>Quota thực tế: <strong>Claude Code</strong> đọc <code>rate_limits</code> Anthropic (5H + 7D) - cục bộ trên Mac này hoặc trên SSH host đang chọn - hiển thị plan tier, email, org name. <strong>Antigravity</strong> hỗ trợ trọn vẹn 3 môi trường: <code>AG</code> (Desktop App bản trắng), <code>IDE</code> (bản VS Code extension), và <code>CLI</code> (bản CLI Terminal) với cơ chế Smart Deduplicate gộp 1-pass gọn nhẹ (~40ms). Tách bạch 2 Domain Auth (IDE SQLite vs Gemini Core shared <code>~/.gemini/</code>) cùng nút Log Out theo ngữ cảnh. Hai panel hiển thị độc lập, tự khóa không cho hiện trùng nguồn. Đồng hồ đếm ngược reset real-time.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #f59e0b;"><i class="fa-solid fa-cloud"></i></div>
          <div class="feature-text">
            <strong>Sync Check & CC Remote - 2 công tắc độc lập</strong>
            <span>Icon nguồn ở cột SYNC bật/tắt riêng PUSH/PULL/SELECT + mục IDE Remote trong popup Open + check sync diff (nền + thủ công). Icon nguồn cạnh ô chọn host (tab REMOTE của Agent Usage) bật/tắt riêng monitor Claude Code Remote. Tắt một công tắc không còn tắt luôn công tắc kia.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #94a3b8;"><i class="fa-solid fa-key"></i></div>
          <div class="feature-text">
            <strong>SSH Config</strong>
            <span>Quản lý <code>~/.ssh/config</code> có undo/redo, kiêm chọn Remote Host cho quota & logs - không cần mở terminal.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #ec4899;"><i class="fa-solid fa-rotate"></i></div>
          <div class="feature-text">
            <strong>Background Refresh</strong>
            <span>Tự động kiểm tra Git status, sync diff, agent usage theo chu kỳ tùy cấu hình. Vòng đếm ngược (countdown ring) hiển thị tiến trình trực tiếp trên tiêu đề cột GIT và ACTIONS.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #fbbf24;"><i class="fa-solid fa-arrows-rotate"></i></div>
          <div class="feature-text">
            <strong>Refresh - 1 unit of work</strong>
            <span>Nút Refresh riêng-project và nút Refresh tổng ở titlebar đều gọi chung <code>refreshProject()</code> (git status + sync diff + stack detection, chạy song song) - không còn reload toàn app. Trạng thái xoay icon nằm ở chính từng check, nên vòng nền cũng làm icon xoay đúng lúc.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #f87171;"><i class="fa-brands fa-git-alt"></i></div>
          <div class="feature-text">
            <strong>Git & Changelog Visual Preview</strong>
            <span>Modal Git hiển thị log/status dạng màu sắc Terminal ANSI, hỗ trợ tiếng Việt có dấu (quotepath=false), stage & commit, fetch, push, và hiển thị Visual Changelog Preview của dự án.</span>
          </div>
        </div>
        <div class="feature-card">
          <div class="feature-icon" style="color: #10b981;"><i class="fa-solid fa-cloud-arrow-down"></i></div>
          <div class="feature-text">
            <strong>App Update Check</strong>
            <span>Tự động kiểm tra bản cập nhật ngầm khi khởi động hoặc click thủ công trong Logo menu, hiển thị badge số hiệu và toast thông báo phiên bản mới.</span>
          </div>
        </div>
      </div>

      <!-- Engineering Highlights -->
      <div class="alert-box tech mt-3">
        <h4 class="alert-title"><i class="fa-solid fa-flask"></i> Điểm Nhấn Công Nghệ</h4>
        <ul class="custom-list">
          <li><strong>Màu sắc Git Terminal & Unicode:</strong> Ép Git xuất màu ANSI thô (`color.status=always`), tự parse bằng Regex sang thẻ HTML span màu sắc trực quan (Red/Green/Yellow/Cyan/Bold) và tắt quotepath (`core.quotepath=false`) giúp hiển thị hoàn hảo tên file tiếng Việt có dấu.</li>
          <li><strong>Stack Detector & Lockfile Analyzer:</strong> Tự nhận dạng Tauri vs Node và phân tích lockfiles (`pnpm`, `yarn`, `bun`, `npm`) để tự chạy lệnh chạy thử nghiệm (dev/preview) mà không cần cấu hình thủ công.</li>
          <li><strong>Co giãn Textarea Native (CSS-only):</strong> Sử dụng thuộc tính CSS mới `field-sizing: content` giúp co giãn chiều cao của Tasks & Notes tự động theo nội dung thực tế, không tốn dù chỉ một dòng JS resize hay gây giật giao diện.</li>
          <li><strong>Changelog Modal kế thừa:</strong> Bổ sung tham số `projectId` và tùy biến title/content giúp tái sử dụng component xem Changelog dùng chung cho cả Changelog nội bộ của từng dự án.</li>
          <li><strong>Quota thực tế:</strong> Đọc trực tiếp `rate_limits` do server Anthropic trả về qua `statusLine` hook, không chắp vá hay giả lập request - an toàn tuyệt đối.</li>
          <li><strong>Hybrid Patching:</strong> Khi quota chạm mốc 100%, Claude CLI ẩn `rate_limits`. Ứng dụng tự động ước lượng thời gian reset để đảm bảo giao diện luôn hiển thị chính xác.</li>
          <li><strong>Hạn ngạch đa luồng (v1.3.0):</strong> Truy vấn song song hai endpoint Connect RPC để kéo đồng thời hạn ngạch 5H và hạn ngạch tuần (Weekly) cho cả Gemini và Claude/GPT pools, phân cụm bằng fieldset tinh gọn.</li>
          <li><strong>Antigravity Native RPC:</strong> Bỏ qua API Google (thường trả dữ liệu trống) - quét native process + dò cổng bằng `lsof` để truy vấn Connect RPC tới local proxy, tốc độ cực nhanh (~40ms).</li>
          <li><strong>Antigravity Log Out đúng nghĩa:</strong> Xóa Cookies/Local Storage không đủ - token đăng nhập được mã hóa bởi `safeStorage` của Electron, khóa AES nằm ở đúng một mục Keychain macOS (`"Antigravity IDE Safe Storage"`). Log Out đóng app rồi xóa đúng mục đó (không quét/dump Keychain), khiến token bị mã hóa vĩnh viễn không đọc được nữa - settings/extension/rule nằm ở file riêng nên không bị ảnh hưởng.</li>
          <li><strong>Claude Code - một nguồn dữ liệu duy nhất:</strong> App không tự chạy <code>claude</code> để lấy usage. Luồng active cũ (force-sync, probe Haiku) đã bị xoá hẳn: nó từng làm rò 19 session mồ côi (6GB RAM + 4GB swap) trên một máy remote, và đo thật cho thấy một turn headless chỉ trả về mốc reset chứ không có phần trăm. Cache hook <code>statusLine</code> giờ là nguồn duy nhất.</li>
          <li><strong>Statusline hai CLI, MỘT script:</strong> Cùng một file được cài y hệt vào cả Claude Code (<code>~/.claude/statusline-command.sh</code>) lẫn AGY CLI (<code>~/.gemini/antigravity-cli/statusline.sh</code>), và <strong>mỗi CLI đều được trỏ tới file đó qua khoá <code>statusLine</code> trong <code>settings.json</code> của chính nó</strong> - ghi file thôi là cài một nửa, CLI sẽ không chạy gì; nó tự nhận ra mình đang chạy cho CLI nào qua chính đường dẫn được gọi (<code>$0</code>), không đoán theo payload. Chọn target nào ghi ngay trên tiêu đề modal, đẩy song song ra nhiều host. Dòng luôn mở đầu bằng tag màu <code>CC</code>/<code>AG</code> kèm tên account để biết CLI nào và tài khoản nào in ra nó. Cache <code>rate_limits</code> nay gắn với đúng account đã ghi và tự loại mục đã hết hạn, không còn treo quota ma của phiên/account khác. Mỗi field có độ cắt riêng, các khối phân tách bằng nền sáng-tối xen kẽ (zebra) thay cho dấu <code>|</code>, và mọi lựa chọn trên modal đều có test tự động kiểm chứng là nó thật sự ra tới file. Mỗi host hiện tag <code>CC</code>/<code>AG</code> cho đúng những CLI nó có: tô đặc là statusline đang chạy được, viền rỗng là có CLI nhưng chưa nối gì.</li>
          <li><strong>App-icon menu & window presets:</strong> Menu ☰ ở titlebar gom link GitHub/Release, check update thủ công, SSH Config, Enable SSH Terminal Color, Statusline Customizer, Claude Code Profile, cài AkiClaudeDoc, cùng 4 preset cửa sổ trong mục <code>AppWindow:</code> - Narrow (420px), Wide (768px), Stick Top-Left (auto-fit chiều cao theo danh sách project), Center Primary. <code>⌘1</code> = Narrow + Stick Top-Left, <code>⌘2</code> = Wide + Center Primary (bấm được cả khi chưa mở menu). Tick <strong>remember</strong> để lần mở app sau tự áp lại chiều rộng và vị trí bạn đã chọn (áp chiều rộng trước, vì Stick Top-Left tính chiều cao theo chiều rộng hiện tại).</li>
          <li><strong>Khắc phục lỗi mtime của `.git/`:</strong> Loại bỏ sự thay đổi mtime của thư mục khi Git dọn dẹp nội bộ khỏi kết quả dry-run, tránh việc kích hoạt nút PUSH không chính xác.</li>
          <li><strong>Phân tách EC-3 hai chiều (Baseline Manifest):</strong> rsync không phân biệt được "remote tạo file X" vs "Local xóa file X", hay "Mac tạo file Y" vs "remote xóa file Y". Sau mỗi lần sync đầy đủ, app ghi snapshot danh sách file local vào <code>appDataDir/baselines/</code>. Lần check tiếp theo: file trong pull_list + có trong baseline + không còn ở Local → Local đã xóa → cộng vào push_count; file trong push_list + có trong baseline → remote đã xóa → loại khỏi push_count. Giải quyết hoàn toàn badge PUSH sáng nhầm khi code chủ yếu trên remote.</li>
          <li><strong>Narrow Mode (v1.14.0):</strong> Cửa sổ dùng được xuống tới 400px (<code>minWidth</code> trong <code>tauri.conf.json</code>), một breakpoint dùng chung duy nhất (700px) và hai utility class toàn cục (<code>.u-narrow-hide</code>/<code>.u-wide-hide</code>) - không component nào tự định nghĩa breakpoint riêng. Nhãn chữ ẩn đi vẫn giữ nguyên trong tooltip <code>title</code>, không nút nào bị mất cả icon lẫn nhãn.</li>
        </ul>
      </div>

      <!-- Origin Story - moved to bottom, preserved -->
      <div class="alert-box origin mt-3">
        <h4 class="alert-title"><i class="fa-solid fa-bullseye"></i> Bối Cảnh Ra Đời</h4>
        <p class="mb-1">Ứng dụng phát triển để phục vụ chính nhu cầu của tác giả (Lạc Việt Anh) trong việc tối ưu luồng code hàng ngày:</p>
        <ul class="custom-list">
          <li><strong>Local - Source of Truth:</strong> code an toàn, giữ Git, dùng <em>Antigravity Pro</em> cá nhân.</li>
          <li><strong>Remote - AI Workspace:</strong> đẩy code lên cho <em>Claude Code / MAX</em> (tài khoản riêng) sinh code hàng loạt qua Terminal.</li>
          <li><strong>Reverse Engineering Quota:</strong> Đo lường hạn mức Antigravity bằng cách phân tích ngược IDE - quét native process, dùng <code>lsof</code> dò cổng Connect RPC và truy vấn trực tiếp local proxy.</li>
        </ul>
      </div>
    </div>

    <div class="modal-footer" style="justify-content: flex-end;">
      <button class="btn-tech btn-tech-primary" @click="closeIntroModal">
        <i class="fa-solid fa-check"></i> ĐÃ HIỂU
      </button>
    </div>
  </BaseModal>
</template>

<script setup>
import BaseModal from './BaseModal.vue';
import { useIntro } from '../../composables/useIntro';

const { showIntroModal, closeIntroModal } = useIntro();
</script>

<style scoped>
.intro-content {
  font-size: 14px;
  line-height: 1.5;
  color: #d1d5db;
}
.intro-header h3 {
  margin: 0 0 8px 0;
  color: #f3f4f6;
  font-size: 18px;
}
.intro-header p {
  margin: 0;
  color: var(--text-muted);
}
.alert-box {
  padding: 14px;
  border-radius: 6px;
  background: rgba(5, 7, 12, 0.4);
  border: 1px solid rgba(255, 255, 255, 0.05);
}
.alert-box.info {
  background: rgba(59, 130, 246, 0.05);
  border-color: rgba(59, 130, 246, 0.2);
}
.alert-box.origin {
  background: rgba(110, 231, 183, 0.04);
  border-color: rgba(110, 231, 183, 0.15);
}
.alert-box.tech {
  background: rgba(167, 139, 250, 0.05);
  border-color: rgba(167, 139, 250, 0.2);
}
.alert-box.tech .alert-title {
  color: #a78bfa;
}
.alert-title {
  margin: 0 0 10px 0;
  font-size: 14px;
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 8px;
}
.alert-box.info .alert-title {
  color: #60a5fa;
}
.alert-box.origin .alert-title {
  color: #6ee7b7;
  opacity: 0.8;
}
.model-flow {
  display: flex;
  align-items: stretch;
  gap: 8px;
}
.model-node {
  flex: 1;
  padding: 10px 12px;
  border-radius: 6px;
  background: rgba(5, 7, 12, 0.6);
  border: 1px solid rgba(255, 255, 255, 0.06);
}
.model-node.local {
  border-color: rgba(34, 211, 238, 0.25);
  background: rgba(34, 211, 238, 0.04);
}
.model-node.remote {
  border-color: rgba(245, 158, 11, 0.25);
  background: rgba(245, 158, 11, 0.04);
}
.model-role {
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.08em;
  color: rgba(255, 255, 255, 0.45);
}
.model-node.local .model-role { color: #22d3ee; }
.model-node.remote .model-role { color: #f59e0b; }
.model-title {
  font-size: 13px;
  font-weight: 700;
  color: #e5e7eb;
  margin: 2px 0;
}
.model-meta {
  font-size: 10px;
  color: var(--text-muted);
}
.model-arrows {
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 6px;
  flex-shrink: 0;
}
.arrow-push, .arrow-pull {
  font-size: 9px;
  font-weight: 700;
  letter-spacing: 0.05em;
  white-space: nowrap;
}
.arrow-push { color: #6ee7b7; }
.arrow-pull { color: #60a5fa; }
.subgroup-label {
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.1em;
  color: rgba(255, 255, 255, 0.35);
  text-transform: uppercase;
}
.custom-list {
  margin: 0;
  padding-left: 20px;
}
.custom-list li {
  margin-bottom: 6px;
}
.custom-list li:last-child {
  margin-bottom: 0;
}
.features-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
}
.feature-card {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  background: rgba(5, 7, 12, 0.6);
  padding: 10px 12px;
  border-radius: 6px;
  border: 1px solid rgba(255, 255, 255, 0.05);
}
.feature-icon {
  font-size: 16px;
  color: #a78bfa;
  margin-top: 2px;
  flex-shrink: 0;
  width: 16px;
  text-align: center;
}
.feature-text strong {
  display: block;
  color: #e5e7eb;
  font-size: 12px;
  font-weight: 700;
  margin-bottom: 3px;
}
.feature-text span {
  font-size: 11px;
  color: var(--text-muted);
}
.mb-1 { margin-bottom: 4px; }
</style>
