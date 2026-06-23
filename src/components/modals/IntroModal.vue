<template>
  <BaseModal :show="showIntroModal" @close="closeIntroModal">
    <template #title>
      <i class="fa-solid fa-book-open mr-1" style="color: #6ee7b7;"></i> Hướng Dẫn Sử Dụng — Aki Dev Sync
    </template>

    <div class="modal-body scrollable intro-content">
      <div class="intro-header mb-3">
        <h3>🚀 Aki Dev Sync</h3>
        <p>
          Command Center để đồng bộ code giữa <strong>Máy Local</strong> và <strong>Máy Remote</strong> qua SSH/rsync —
          không cần commit rác lên Git, không lo UI freeze.
        </p>
      </div>

      <!-- Use Cases -->
      <div class="alert-box info mb-3">
        <h4 class="alert-title"><i class="fa-solid fa-earth-americas"></i> Ai cần dùng?</h4>
        <ul class="custom-list">
          <li><strong>Máy yếu ↔ Server mạnh:</strong> Code nhẹ ở laptop, đẩy build nặng / chạy AI lên Server.</li>
          <li><strong>Bảo mật source code:</strong> Máy công ty bị kiểm soát? Giữ core code ở VPS cá nhân.</li>
          <li><strong>Đa thiết bị:</strong> Đồng bộ nhanh giữa PC, Laptop, Server mà không commit rác lên GitHub.</li>
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
            <span>Đẩy code Local → Remote. Toggle <code>.git/</code> để AI đọc full context.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #f59e0b;"><i class="fa-solid fa-bolt"></i></div>
          <div class="feature-text">
            <strong>PUSH SPECIAL</strong>
            <span>Chọn từng file từ danh sách Git để push — không scan cả thư mục.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #3b82f6;"><i class="fa-solid fa-arrow-down"></i></div>
          <div class="feature-text">
            <strong>PULL</strong>
            <span>Rút file AI vừa sửa từ Remote về Local để review & commit.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #22c55e;"><i class="fa-solid fa-shield-halved"></i></div>
          <div class="feature-text">
            <strong>DRY RUN</strong>
            <span>Preview danh sách file sẽ thay đổi — không ghi thật cho đến khi tắt toggle.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #a78bfa;"><i class="fa-solid fa-circle-dot"></i></div>
          <div class="feature-text">
            <strong>Sync Status</strong>
            <span>Background check — nút PUSH/PULL tự sáng lên khi hai phía lệch nhau.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #f97316;"><i class="fa-solid fa-code"></i></div>
          <div class="feature-text">
            <strong>Pre / Post Hooks</strong>
            <span>Script chạy trước/sau mỗi lần sync (build, restart service, notify...).</span>
          </div>
        </div>
      </div>

      <!-- TOOLS & MONITOR features -->
      <div class="subgroup-label mb-1">🛠 TOOLS & MONITOR</div>
      <div class="features-grid">
        <div class="feature-card">
          <div class="feature-icon" style="color: #06b6d4;"><i class="fa-solid fa-grip"></i></div>
          <div class="feature-text">
            <strong>Project Hub</strong>
            <span>Hover icon project → mở Finder, Terminal, VSCode, Antigravity cho Local & Remote SSH.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #818cf8;"><i class="fa-solid fa-chart-bar"></i></div>
          <div class="feature-text">
            <strong>Agent Usage</strong>
            <span>Theo dõi mức dùng Antigravity (Local) và Claude Code (Remote) theo thời gian thực.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #94a3b8;"><i class="fa-solid fa-key"></i></div>
          <div class="feature-text">
            <strong>SSH Config</strong>
            <span>Quản lý <code>~/.ssh/config</code> có undo/redo — không cần mở terminal.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #ec4899;"><i class="fa-solid fa-rotate"></i></div>
          <div class="feature-text">
            <strong>Background Refresh</strong>
            <span>Tự động poll Git status, sync diff, agent usage theo interval tuỳ chỉnh.</span>
          </div>
        </div>
      </div>

      <!-- Origin Story — moved to bottom, preserved -->
      <div class="alert-box origin mt-3">
        <h4 class="alert-title"><i class="fa-solid fa-bullseye"></i> Mục Đích Khởi Thủy</h4>
        <p class="mb-1">Ứng dụng sinh ra từ bài toán thực tế của tác giả (Lạc Việt Anh):</p>
        <ul class="custom-list">
          <li>
            <strong>Local (Source of Truth):</strong> Nơi code an toàn, lưu trữ Git. Sử dụng <em>Antigravity Pro</em> cá nhân.
          </li>
          <li>
            <strong>Remote (AI Workspace):</strong> Đẩy code lên để AI đọc hiểu toàn bộ project. Tận dụng
            <em>Claude Code / Claude MAX</em> tài khoản riêng qua Terminal để sinh code hàng loạt một cách bảo mật.
          </li>
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
  color: #9ca3af;
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
  color: #9ca3af;
}
.mb-1 { margin-bottom: 4px; }
.mb-2 { margin-bottom: 8px; }
.mb-3 { margin-bottom: 16px; }
.mt-3 { margin-top: 16px; }
</style>
